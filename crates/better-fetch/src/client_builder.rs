//! [`ClientBuilder`] — configure and build a [`Client`](crate::Client).

use std::sync::Arc;
use std::time::Duration;

use reqwest::Client as ReqwestClient;
use tokio::sync::Semaphore;
use url::Url;

use crate::auth::Auth;
use crate::backend::{HttpBackend, ReqwestBackend};
use crate::client::{Client, ClientConfig};
use crate::error::Error;
use crate::hooks::Hooks;
use crate::plugin::PluginRegistry;
use crate::request::parse_request_header;
use crate::retry::RetryPolicy;
use crate::streaming::RETRY_BODY_PEEK_DEFAULT;
use crate::Result;

#[cfg(feature = "json")]
use crate::json_parser::JsonParserFn;

#[cfg(feature = "schema")]
use crate::schema::SchemaRegistry;

/// Builder for [`Client`].
#[must_use = "call `.build()` to create a `Client`"]
pub struct ClientBuilder {
    base_url: Option<Url>,
    timeout: Option<Duration>,
    retry: Option<RetryPolicy>,
    auth: Option<Auth>,
    default_headers: http::HeaderMap,
    hooks: Hooks,
    plugins: PluginRegistry,
    reqwest_client: Option<ReqwestClient>,
    custom_backend: Option<Arc<dyn HttpBackend>>,
    max_in_flight: Option<usize>,
    max_response_bytes: Option<u64>,
    retry_body_peek_bytes: Option<u64>,
    #[cfg(feature = "schema")]
    schema_registry: Option<Arc<SchemaRegistry>>,
    #[cfg(feature = "json")]
    json_parser: Option<JsonParserFn>,
}

impl ClientBuilder {
    /// Creates an empty builder; [`Self::base_url`] is required before [`Self::build`].
    pub fn new() -> Self {
        Self {
            base_url: None,
            timeout: None,
            retry: None,
            auth: None,
            default_headers: http::HeaderMap::new(),
            hooks: Hooks::default(),
            plugins: PluginRegistry::new(),
            reqwest_client: None,
            custom_backend: None,
            max_in_flight: None,
            max_response_bytes: None,
            retry_body_peek_bytes: None,
            #[cfg(feature = "schema")]
            schema_registry: None,
            #[cfg(feature = "json")]
            json_parser: None,
        }
    }

    /// Sets the base URL (required).
    pub fn base_url(mut self, base_url: impl AsRef<str>) -> Result<Self> {
        self.base_url = Some(Url::parse(base_url.as_ref()).map_err(Error::InvalidBaseUrl)?);
        Ok(self)
    }

    /// Sets the default request timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Sets the default [`RetryPolicy`] for all requests.
    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = Some(policy);
        self
    }

    /// Sets default authentication for all requests.
    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Adds a default header applied to every request.
    pub fn default_header(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Result<Self> {
        let (name, value) = parse_request_header(key, value)?;
        self.default_headers.insert(name, value);
        Ok(self)
    }

    /// Sets client-level lifecycle hooks.
    pub fn hooks(mut self, hooks: Hooks) -> Self {
        self.hooks = hooks;
        self
    }

    /// Registers a [`Plugin`] on this client.
    pub fn plugin<P: crate::plugin::Plugin + 'static>(mut self, plugin: P) -> Self {
        self.plugins.push(Box::new(plugin));
        self
    }

    /// Uses a custom reqwest client for the default [`ReqwestBackend`].
    pub fn reqwest_client(mut self, client: ReqwestClient) -> Self {
        self.reqwest_client = Some(client);
        self
    }

    /// Use a custom HTTP backend (for testing or alternate transports).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{ClientBuilder, Error, HttpBackend, HttpRequest, HttpResponse, HttpStreamingResponse, Result};
    /// # use async_trait::async_trait;
    /// # use bytes::Bytes;
    /// # use http::StatusCode;
    /// # use std::sync::Arc;
    /// # struct MockBackend;
    /// # #[async_trait]
    /// # impl HttpBackend for MockBackend {
    /// #     async fn execute(&self, _req: HttpRequest) -> Result<HttpResponse> {
    /// #         Ok(HttpResponse {
    /// #             status: StatusCode::OK,
    /// #             headers: Default::default(),
    /// #             body: Bytes::from_static(b"{}"),
    /// #         })
    /// #     }
    /// #     async fn execute_stream(
    /// #         &self,
    /// #         _req: HttpRequest,
    /// #     ) -> Result<HttpStreamingResponse> {
    /// #         Err(Error::Other("streaming not supported".into()))
    /// #     }
    /// # }
    /// # fn example() -> Result<()> {
    /// let client = ClientBuilder::new()
    ///     .base_url("https://api.example.com")?
    ///     .backend(Arc::new(MockBackend))
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn backend(mut self, backend: Arc<dyn HttpBackend>) -> Self {
        self.custom_backend = Some(backend);
        self
    }

    /// Limits how many requests this client may have in flight at once (including retries).
    ///
    /// Implemented with a tokio semaphore in the core client. This counts the full request
    /// lifecycle (hooks and retries), not just the transport hop. For wire-level limits only,
    /// use [`Self::transport_stack`] with Tower's [`ConcurrencyLimitLayer`](crate::tower::stack::ConcurrencyLimitLayer)
    /// (feature `tower`) instead of—or deliberately alongside—this setting.
    pub fn max_in_flight(mut self, limit: usize) -> Self {
        self.max_in_flight = Some(limit);
        self
    }

    /// Maximum response body size (in bytes) for [`RequestBuilder::send_stream`](crate::RequestBuilder::send_stream)
    /// when the request does not set its own limit.
    pub fn max_response_bytes(mut self, limit: u64) -> Self {
        self.max_response_bytes = Some(limit);
        self
    }

    /// Maximum bytes read from a streaming body when a custom retry predicate is configured.
    ///
    /// Defaults to 64 KiB. Capped by [`Self::max_response_bytes`] when that is also set.
    pub fn retry_body_peek_bytes(mut self, limit: u64) -> Self {
        self.retry_body_peek_bytes = Some(limit);
        self
    }

    /// Attach a [`SchemaRegistry`] for strict route validation (feature `schema`).
    #[cfg(feature = "schema")]
    pub fn schema_registry(mut self, registry: Arc<SchemaRegistry>) -> Self {
        self.schema_registry = Some(registry);
        self
    }

    /// Use a Tower [`Service`](tower::Service) as the HTTP transport for **buffered** `send()` only.
    ///
    /// `send_stream()` uses the default reqwest streaming transport without your Tower layers.
    /// For middleware on both paths, use [`Self::transport_stack`].
    #[cfg(feature = "tower")]
    pub fn http_service<S>(mut self, service: S) -> Self
    where
        S: tower::Service<
                crate::backend::HttpRequest,
                Response = crate::backend::HttpResponse,
                Error = Error,
            > + Clone
            + Send
            + 'static,
        S::Future: Send + 'static,
    {
        use crate::tower::ServiceBackend;

        let client = self.reqwest_client.clone().unwrap_or_default();
        self.custom_backend = Some(Arc::new(ServiceBackend::buffered_with_reqwest_streaming(
            service, client,
        )));
        self
    }

    /// Use a boxed Tower transport stack for **buffered** `send()` only (streaming uses plain reqwest).
    ///
    /// Prefer [`Self::transport_stack`] when `send_stream()` must see the same middleware.
    #[cfg(feature = "tower")]
    pub fn http_service_boxed(mut self, service: crate::tower::BoxHttpService) -> Self {
        use crate::tower::ServiceBackend;

        let client = self.reqwest_client.clone().unwrap_or_default();
        self.custom_backend = Some(Arc::new(ServiceBackend::new(
            service,
            crate::tower::ReqwestStreamingHttpService::new(client),
        )));
        self
    }

    /// Build a Tower transport stack on top of the configured (or default) reqwest client.
    ///
    /// Application hooks and [`RetryPolicy`](crate::RetryPolicy) remain in the core client;
    /// only wire-level behavior is configured here.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{ClientBuilder, Result};
    /// # use better_fetch::tower::stack::{ConcurrencyLimitLayer, IntoBoxHttpService, IntoBoxStreamingHttpService, ServiceBuilder};
    /// let client = ClientBuilder::new()
    ///     .base_url("https://api.example.com")?
    ///     .transport_stack(|buffered, streaming| {
    ///         (
    ///             ServiceBuilder::new()
    ///                 .layer(ConcurrencyLimitLayer::new(32))
    ///                 .service(buffered)
    ///                 .into_box(),
    ///             ServiceBuilder::new()
    ///                 .layer(ConcurrencyLimitLayer::new(32))
    ///                 .service(streaming)
    ///                 .into_streaming_box(),
    ///         )
    ///     })
    ///     .build()?;
    /// # Ok::<(), better_fetch::Error>(())
    /// ```
    #[cfg(feature = "tower")]
    pub fn transport_stack<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(
            crate::tower::ReqwestHttpService,
            crate::tower::ReqwestStreamingHttpService,
        ) -> (
            crate::tower::BoxHttpService,
            crate::tower::BoxStreamingHttpService,
        ),
    {
        use crate::tower::ServiceBackend;

        let client = self.reqwest_client.clone().unwrap_or_default();
        let (buffered, streaming) = crate::tower::stack::build_dual(client, configure);
        self.custom_backend = Some(Arc::new(ServiceBackend::from_boxes(buffered, streaming)));
        self
    }

    /// Sets a custom JSON parser for all responses from this client.
    ///
    /// See [`crate::json_parser`] for the two-step `Bytes` → `Value` → `T` pipeline vs the
    /// default single-step fast path, and [`Response::into_json_with`](crate::response::Response::into_json_with)
    /// for per-response `Bytes` → `T` without a global parser.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{ClientBuilder, Result};
    /// # use bytes::Bytes;
    /// let client = ClientBuilder::new()
    ///     .base_url("https://api.example.com")?
    ///     .json_parser(|body: &Bytes| {
    ///         let slice = body.strip_prefix(b"\xef\xbb\xbf").unwrap_or(body);
    ///         serde_json::from_slice(slice).map_err(|e| e.to_string())
    ///     })
    ///     .build()?;
    /// # Ok::<(), better_fetch::Error>(())
    /// ```
    #[cfg(feature = "json")]
    pub fn json_parser<F>(mut self, f: F) -> Self
    where
        F: Fn(&bytes::Bytes) -> std::result::Result<serde_json::Value, String>
            + Send
            + Sync
            + 'static,
    {
        self.json_parser = Some(crate::json_parser::json_parser(f));
        self
    }

    /// Sets a custom JSON parser from an existing [`JsonParserFn`].
    #[cfg(feature = "json")]
    pub fn json_parser_fn(mut self, parser: JsonParserFn) -> Self {
        self.json_parser = Some(parser);
        self
    }

    /// Builds the [`Client`]. Requires [`Self::base_url`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{ClientBuilder, Result};
    /// let client = ClientBuilder::new()
    ///     .base_url("https://api.example.com")?
    ///     .build()?;
    /// # Ok::<(), better_fetch::Error>(())
    /// ```
    pub fn build(self) -> Result<Client> {
        let base_url = self.base_url.ok_or(Error::MissingBaseUrl)?;

        let backend: Arc<dyn HttpBackend> = if let Some(b) = self.custom_backend {
            b
        } else {
            let reqwest_client = self.reqwest_client.unwrap_or_default();
            Arc::new(ReqwestBackend::new(reqwest_client))
        };

        let plugins = Arc::new(self.plugins);
        let merged_hooks = self.hooks.clone().merge(plugins.merged_hooks());

        Ok(Client {
            config: Arc::new(ClientConfig {
                base_url,
                timeout: self.timeout,
                retry: self.retry,
                auth: self.auth,
                default_headers: self.default_headers,
                hooks: self.hooks,
                merged_hooks,
                plugins,
                max_in_flight: self.max_in_flight.map(|n| Arc::new(Semaphore::new(n))),
                #[cfg(feature = "schema")]
                schema_registry: self.schema_registry,
                #[cfg(feature = "json")]
                json_parser: self.json_parser,
                max_response_bytes: self.max_response_bytes,
                retry_body_peek_bytes: self
                    .retry_body_peek_bytes
                    .unwrap_or(RETRY_BODY_PEEK_DEFAULT),
            }),
            backend,
        })
    }
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}
