//! HTTP client, builder, and shared configuration.
//!
//! Start with [`Client::new`] or [`ClientBuilder`], then:
//!
//! - [`Client::get`] / [`Client::post`] — flexible [`RequestBuilder`] (string paths, `.param("id", 1)`).
//! - [`Client::call`] — typed [`Endpoint`] routes ([`.params()`](EndpointRequestBuilder::params) with structs).
//!
//! See [`crate::request`] for per-request options on [`RequestBuilder`].

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use indexmap::IndexMap;
use tokio::sync::Semaphore;

use http::Method;
use reqwest::Client as ReqwestClient;
use url::Url;

use crate::auth::Auth;
use crate::backend::{HttpBackend, HttpBody};
use crate::endpoint::{Endpoint, EndpointParamsInitial, EndpointRequestBuilder};
use crate::hooks::Hooks;
use crate::plugin::PluginRegistry;
use crate::request::RequestBuilder;
use crate::response::Response;
use crate::retry::RetryPolicy;
use crate::streaming::StreamingResponse;
use crate::Result;

#[cfg(feature = "json")]
use crate::json_parser::JsonParserFn;

#[cfg(feature = "schema")]
use crate::schema::SchemaRegistry;

/// Shared client configuration (returned by [`Client::config`]).
#[derive(Clone)]
pub struct ClientConfig {
    /// Base URL joined with request paths.
    pub base_url: Url,
    /// Default per-request timeout when the builder does not override it.
    pub timeout: Option<Duration>,
    /// Default retry policy for requests that do not set their own.
    pub retry: Option<RetryPolicy>,
    /// Default authentication applied when a request has no per-request auth.
    pub auth: Option<Auth>,
    /// Headers merged into every request unless overridden.
    pub default_headers: http::HeaderMap,
    #[allow(dead_code)]
    pub(crate) hooks: Hooks,
    pub(crate) merged_hooks: Hooks,
    /// Registered plugins (init hooks + merged hook chains).
    pub plugins: Arc<PluginRegistry>,
    /// Limits concurrent in-flight requests for this client (including retries).
    ///
    /// This is separate from Tower's [`ConcurrencyLimitLayer`](crate::tower::stack::ConcurrencyLimitLayer):
    /// the client semaphore applies to the full request lifecycle (hooks + retries), while Tower
    /// limits only transport-layer concurrency. Avoid stacking both without accounting for that.
    pub max_in_flight: Option<Arc<Semaphore>>,
    #[cfg(feature = "schema")]
    /// Optional strict route registry (feature `schema`).
    pub schema_registry: Option<Arc<SchemaRegistry>>,
    #[cfg(feature = "json")]
    /// Client-wide custom JSON parser (feature `json`).
    pub json_parser: Option<JsonParserFn>,
    /// Default maximum response body size for buffered and streaming responses.
    pub max_response_bytes: Option<u64>,
    /// Maximum bytes read from a streaming body when evaluating a custom retry predicate.
    pub retry_body_peek_bytes: u64,
}

impl ClientConfig {
    /// Hooks executed at runtime (client hooks merged with plugin hooks when the client was built).
    ///
    /// This is the chain used for `on_request`, `on_response`, and related hooks. The separate
    /// `hooks` field on [`ClientConfig`] is the client-only configuration snapshot and is not
    /// consulted during requests.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{Client, Result};
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// let client = Client::new("https://api.example.com")?;
    /// let _hooks = client.config().effective_hooks();
    /// # Ok(())
    /// # }
    /// ```
    pub fn effective_hooks(&self) -> &Hooks {
        &self.merged_hooks
    }
}

/// Typed HTTP client built on reqwest.
#[derive(Clone)]
pub struct Client {
    pub(crate) config: Arc<ClientConfig>,
    pub(crate) backend: Arc<dyn HttpBackend>,
}

impl Client {
    /// Creates a client with default reqwest settings and the given base URL.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{Client, Result};
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// let client = Client::new("https://api.example.com")?;
    /// let _ = client.get("/health").send().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(base_url: impl AsRef<str>) -> Result<Self> {
        ClientBuilder::new().base_url(base_url)?.build()
    }

    /// Returns a [`ClientBuilder`] for advanced configuration.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Builds a client with a custom reqwest instance. [`ClientBuilder::base_url`] is required.
    pub fn with_http_client(
        reqwest_client: ReqwestClient,
        base_url: impl AsRef<str>,
    ) -> Result<Self> {
        ClientBuilder::new()
            .reqwest_client(reqwest_client)
            .base_url(base_url)?
            .build()
    }

    /// Starts a typed request for [`Endpoint`] `E`.
    ///
    /// When `E::Params` is not unit, returns a builder in [`NeedsParams`](crate::NeedsParams) state
    /// that requires [`.params()`](EndpointRequestBuilder::params) before
    /// [`.send_json()`](EndpointRequestBuilder::send_json). Query (`E::Query`) is typed but not
    /// enforced: call [`.query()`](EndpointRequestBuilder::query) when you need query parameters on the wire.
    ///
    /// For ad-hoc requests with string paths, use [`Self::get`] / [`Self::post`] instead.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{Client, Endpoint, Result, define_params};
    /// # use http::Method;
    /// # use serde::Deserialize;
    /// define_params!(GetTodoParams for "/todos/:id" { id: u64 });
    ///
    /// struct GetTodo;
    /// impl Endpoint for GetTodo {
    ///     const METHOD: http::Method = http::Method::GET;
    ///     const PATH: &'static str = "/todos/:id";
    ///     type Response = Todo;
    ///     type Params = GetTodoParams;
    ///     type Query = ();
    ///     type Body = ();
    ///     type Headers = ();
    /// }
    ///
    /// # #[derive(Deserialize)]
    /// # struct Todo { id: u64, title: String }
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// let client = Client::new("https://api.example.com")?;
    /// let todo = client
    ///     .call::<GetTodo>()
    ///     .params(GetTodoParams { id: 1 })
    ///     .send_json()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn call<E: Endpoint>(
        &self,
    ) -> EndpointRequestBuilder<'_, E, <E::Params as EndpointParamsInitial<E>>::State>
    where
        E::Params: EndpointParamsInitial<E>,
    {
        E::Params::initial(self)
    }

    /// Returns a snapshot of this client's configuration.
    ///
    /// Use [`ClientConfig::effective_hooks`] for the hook chain used at runtime (client hooks
    /// merged with plugin hooks at build time).
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    pub(crate) fn backend_arc(&self) -> &Arc<dyn HttpBackend> {
        &self.backend
    }

    /// Starts a `GET` request for `path` (supports `:param` templates).
    pub fn get(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::GET, path)
    }

    /// Starts a `POST` request for `path`.
    pub fn post(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::POST, path)
    }

    /// Starts a `PUT` request for `path`.
    pub fn put(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::PUT, path)
    }

    /// Starts a `PATCH` request for `path`.
    pub fn patch(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::PATCH, path)
    }

    /// Starts a `DELETE` request for `path`.
    pub fn delete(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::DELETE, path)
    }

    /// Starts a `HEAD` request for `path`.
    pub fn head(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::HEAD, path)
    }

    /// Starts a request with an explicit HTTP method and path.
    pub fn request(&self, method: Method, path: impl Into<String>) -> RequestBuilder<'_> {
        RequestBuilder {
            client: self,
            method,
            path: path.into(),
            base_url: None,
            params: HashMap::new(),
            query: IndexMap::new(),
            headers: self.config.default_headers.clone(),
            body: HttpBody::Empty,
            #[cfg(feature = "multipart")]
            multipart: None,
            timeout: self.config.timeout,
            retry: self.config.retry.clone(),
            auth: self.config.auth.clone(),
            cancellation: None,
            throw_on_error: false,
            #[cfg(feature = "json")]
            json_parser: None,
            #[cfg(feature = "validate")]
            validate_response: true,
            #[cfg(feature = "schema-validate")]
            disable_validation: false,
            max_response_bytes: None,
            retry_body_peek_bytes: None,
        }
    }

    pub(crate) async fn execute_stream(
        &self,
        builder: RequestBuilder<'_>,
    ) -> Result<StreamingResponse> {
        let prep = crate::request_pipeline::prepare_execution(self, builder).await?;
        crate::request_pipeline::run_stream_loop(prep).await
    }

    pub(crate) async fn execute(&self, builder: RequestBuilder<'_>) -> Result<Response> {
        let prep = crate::request_pipeline::prepare_execution(self, builder).await?;
        crate::request_pipeline::run_buffered_loop(prep).await
    }
}

pub use crate::client_builder::ClientBuilder;
