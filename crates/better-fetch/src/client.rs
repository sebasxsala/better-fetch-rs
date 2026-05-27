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
use crate::backend::{HttpBackend, HttpBody, HttpRequest, ReqwestBackend};
use crate::cancel::execute_or_cancel;
use crate::endpoint::{Endpoint, EndpointParams, EndpointParamsInitial, EndpointRequestBuilder};
use crate::error::Error;
use crate::hooks::{
    ErrorContext, Hooks, RequestContext, ResponseContext, StreamingResponseContext,
    StreamingSuccessContext, SuccessContext,
};
use crate::plugin::{PluginRegistry, PreparedRequest};
use crate::request::RequestBuilder;
use crate::response::Response;
use crate::retry::{sleep_or_cancel, RetryPolicy};
use crate::streaming::{
    body_stream_from_bytes, drain_body_for_retry, wrap_cancellation, wrap_max_bytes,
    StreamingResponse, RETRY_BODY_PEEK_DEFAULT,
};
use crate::url_build::build_url;
use crate::Result;

#[cfg(feature = "tower")]
use crate::backend::HttpResponse;

#[cfg(feature = "json")]
use crate::json_parser::JsonParserFn;

#[cfg(feature = "schema")]
use crate::schema::SchemaRegistry;

fn body_for_context(body: &HttpBody) -> Option<bytes::Bytes> {
    match body {
        HttpBody::Empty => None,
        HttpBody::Bytes(b) => Some(b.clone()),
    }
}

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
    /// Client-level lifecycle hooks (merged with plugin hooks at build time).
    pub hooks: Hooks,
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
    /// Default maximum response body size for [`RequestBuilder::send_stream`](crate::RequestBuilder::send_stream).
    pub max_response_bytes: Option<u64>,
    /// Maximum bytes read from a streaming body when evaluating a custom retry predicate.
    pub retry_body_peek_bytes: u64,
}

/// Typed HTTP client built on reqwest.
#[derive(Clone)]
pub struct Client {
    config: Arc<ClientConfig>,
    backend: Arc<dyn HttpBackend>,
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
    /// [`.send_json()`](EndpointRequestBuilder::send_json).
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
    ) -> EndpointRequestBuilder<'_, E, <E::Params as EndpointParams>::BuilderState>
    where
        E::Params: EndpointParamsInitial<E>,
    {
        E::Params::initial(self)
    }

    /// Returns a snapshot of this client's configuration.
    pub fn config(&self) -> &ClientConfig {
        &self.config
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
            max_response_bytes: None,
            retry_body_peek_bytes: None,
        }
    }

    pub(crate) async fn execute_stream(
        &self,
        builder: RequestBuilder<'_>,
    ) -> Result<StreamingResponse> {
        #[cfg(feature = "json")]
        let json_parser = builder
            .json_parser
            .clone()
            .or_else(|| self.config.json_parser.clone());

        let built = build_url(
            &self.config.base_url,
            &builder.path,
            &builder.params,
            &builder.query,
        )?;

        let mut method = builder.method;
        if let Some(override_method) = built.method_override {
            method = override_method;
        }

        #[cfg(feature = "schema")]
        if let Some(registry) = &self.config.schema_registry {
            registry.ensure_route(&builder.path, &method)?;
        }

        let mut url = built.url;
        let mut headers = builder.headers;
        let auth = builder.auth.or_else(|| self.config.auth.clone());
        if let Some(auth) = auth {
            auth.apply(&mut headers).await?;
        }

        let mut prepared = PreparedRequest {
            url: url.clone(),
            path: builder.path.clone(),
            method: method.clone(),
            headers: headers.clone(),
        };
        self.config.plugins.run_init_all(&mut prepared).await?;
        url = prepared.url;
        headers = prepared.headers;
        method = prepared.method;

        let mut req_ctx = RequestContext {
            url: url.clone(),
            method: method.clone(),
            headers: headers.clone(),
            body: body_for_context(&builder.body),
            retry_attempt: 0,
        };

        let merged_hooks = &self.config.merged_hooks;
        req_ctx = merged_hooks.run_on_request(req_ctx).await?;
        url = req_ctx.url.clone();
        headers = req_ctx.headers.clone();
        method = req_ctx.method.clone();

        let timeout = builder.timeout;
        let retry_policy = builder.retry.or_else(|| self.config.retry.clone());
        let throw_on_error = builder.throw_on_error;
        let cancel = builder.cancellation;
        let max_response_bytes = builder
            .max_response_bytes
            .or(self.config.max_response_bytes);
        let retry_body_peek_bytes = builder
            .retry_body_peek_bytes
            .unwrap_or(self.config.retry_body_peek_bytes);

        let backend = self.backend.clone();

        let _in_flight_permit = match &self.config.max_in_flight {
            Some(sem) => Some(
                sem.acquire()
                    .await
                    .map_err(|_| Error::Other("max_in_flight semaphore closed".into()))?,
            ),
            None => None,
        };

        let mut attempt = 0u32;
        let max_attempts = retry_policy.as_ref().map(|p| p.max_attempts()).unwrap_or(0);

        let request_body = builder.body;
        #[cfg(feature = "multipart")]
        let mut multipart_body = builder.multipart;
        #[cfg(feature = "multipart")]
        let had_multipart = multipart_body.is_some();

        let cancel_ref = cancel.as_ref();

        loop {
            req_ctx.retry_attempt = attempt;

            #[cfg(feature = "multipart")]
            if attempt > 0 && had_multipart {
                return Err(Error::Other(
                    "automatic retry is not supported with multipart request bodies".into(),
                ));
            }

            let http_req = HttpRequest {
                method: method.clone(),
                url: url.clone(),
                headers: headers.clone(),
                body: request_body.clone(),
                timeout,
                cancellation: cancel.clone(),
                #[cfg(feature = "multipart")]
                multipart: multipart_body.take(),
            };
            let request_url = http_req.url.clone();

            let result = execute_or_cancel(cancel_ref, backend.execute_stream(http_req)).await;

            match result {
                Ok(http_res) => {
                    let status = http_res.status;
                    let headers = http_res.headers.clone();
                    let peek_limit = max_response_bytes
                        .map(|m| m.min(retry_body_peek_bytes))
                        .unwrap_or(retry_body_peek_bytes);

                    let mut body = http_res.body;
                    if let Some(policy) = retry_policy.as_ref() {
                        if policy.has_custom_should_retry() {
                            let peeked = drain_body_for_retry(body, peek_limit).await?;
                            let stub = Response::new(
                                status,
                                headers.clone(),
                                peeked.clone(),
                                Some(request_url.clone()),
                                #[cfg(feature = "json")]
                                None,
                            );
                            if policy.should_retry_response(&stub, false) && attempt < max_attempts
                            {
                                let stub = Response::new(
                                    status,
                                    headers.clone(),
                                    bytes::Bytes::new(),
                                    Some(request_url.clone()),
                                    #[cfg(feature = "json")]
                                    None,
                                );
                                merged_hooks
                                    .run_on_retry(ResponseContext {
                                        request: req_ctx.clone(),
                                        response: stub,
                                    })
                                    .await;
                                let delay = policy.delay_after_response(attempt, &headers);
                                attempt += 1;
                                sleep_or_cancel(delay, cancel_ref).await?;
                                continue;
                            }
                            body = body_stream_from_bytes(peeked);
                        } else {
                            let stub = Response::new(
                                status,
                                headers.clone(),
                                bytes::Bytes::new(),
                                Some(request_url.clone()),
                                #[cfg(feature = "json")]
                                None,
                            );
                            if policy.should_retry_response(&stub, false) && attempt < max_attempts
                            {
                                let stub = Response::new(
                                    status,
                                    headers.clone(),
                                    bytes::Bytes::new(),
                                    Some(request_url.clone()),
                                    #[cfg(feature = "json")]
                                    None,
                                );
                                merged_hooks
                                    .run_on_retry(ResponseContext {
                                        request: req_ctx.clone(),
                                        response: stub,
                                    })
                                    .await;
                                let delay = policy.delay_after_response(attempt, &headers);
                                attempt += 1;
                                sleep_or_cancel(delay, cancel_ref).await?;
                                continue;
                            }
                        }
                    }

                    let meta = merged_hooks
                        .run_on_response_stream(StreamingResponseContext {
                            request: req_ctx.clone(),
                            status,
                            headers,
                        })
                        .await?;
                    let status = meta.status;
                    let stream_headers = meta.headers;

                    if throw_on_error && !status.is_success() {
                        let http_err = Error::http_with_status_text(
                            status,
                            status.canonical_reason().unwrap_or("request failed"),
                            status.canonical_reason().unwrap_or("request failed"),
                            None,
                        );
                        merged_hooks
                            .run_on_error(ErrorContext {
                                request: req_ctx.clone(),
                                response: None,
                                error: http_err.clone(),
                            })
                            .await;
                        return Err(http_err);
                    }

                    if let Some(limit) = max_response_bytes {
                        body = wrap_max_bytes(body, limit);
                    }
                    if let Some(token) = cancel.clone() {
                        body = wrap_cancellation(body, token);
                    }

                    if status.is_success() {
                        merged_hooks
                            .run_on_success_stream(StreamingSuccessContext {
                                request: req_ctx.clone(),
                                status,
                                headers: stream_headers.clone(),
                            })
                            .await;
                    }

                    return Ok(StreamingResponse::new(
                        status,
                        stream_headers,
                        body,
                        Some(request_url),
                        #[cfg(feature = "json")]
                        json_parser,
                    ));
                }
                Err(err) => {
                    if err.is_cancelled() {
                        merged_hooks
                            .run_on_error(ErrorContext {
                                request: req_ctx.clone(),
                                response: None,
                                error: err.clone(),
                            })
                            .await;
                        return Err(err);
                    }

                    let retry_transport = matches!(&err, Error::Transport { .. } | Error::Timeout);
                    if retry_transport && retry_policy.is_some() && attempt < max_attempts {
                        merged_hooks
                            .run_on_retry(ResponseContext {
                                request: req_ctx.clone(),
                                response: Response::new(
                                    http::StatusCode::SERVICE_UNAVAILABLE,
                                    http::HeaderMap::new(),
                                    bytes::Bytes::new(),
                                    Some(request_url.clone()),
                                    #[cfg(feature = "json")]
                                    None,
                                ),
                            })
                            .await;
                        let delay = retry_policy
                            .as_ref()
                            .map(|p| p.delay_after_response(attempt, &http::HeaderMap::new()))
                            .unwrap_or(Duration::from_secs(1));
                        attempt += 1;
                        sleep_or_cancel(delay, cancel_ref).await?;
                        continue;
                    }

                    merged_hooks
                        .run_on_error(ErrorContext {
                            request: req_ctx.clone(),
                            response: None,
                            error: err.clone(),
                        })
                        .await;

                    if retry_transport && retry_policy.is_some() {
                        return Err(Error::retry_exhausted(attempt + 1, err));
                    }

                    return Err(err);
                }
            }
        }
    }

    pub(crate) async fn execute(&self, builder: RequestBuilder<'_>) -> Result<Response> {
        #[cfg(feature = "json")]
        let json_parser = builder
            .json_parser
            .clone()
            .or_else(|| self.config.json_parser.clone());

        let built = build_url(
            &self.config.base_url,
            &builder.path,
            &builder.params,
            &builder.query,
        )?;

        let mut method = builder.method;
        if let Some(override_method) = built.method_override {
            method = override_method;
        }

        #[cfg(feature = "schema")]
        if let Some(registry) = &self.config.schema_registry {
            registry.ensure_route(&builder.path, &method)?;
        }

        let mut url = built.url;
        let mut headers = builder.headers;
        let auth = builder.auth.or_else(|| self.config.auth.clone());
        if let Some(auth) = auth {
            auth.apply(&mut headers).await?;
        }

        let mut prepared = PreparedRequest {
            url: url.clone(),
            path: builder.path.clone(),
            method: method.clone(),
            headers: headers.clone(),
        };
        self.config.plugins.run_init_all(&mut prepared).await?;
        url = prepared.url;
        headers = prepared.headers;
        method = prepared.method;

        let mut req_ctx = RequestContext {
            url: url.clone(),
            method: method.clone(),
            headers: headers.clone(),
            body: body_for_context(&builder.body),
            retry_attempt: 0,
        };

        let merged_hooks = &self.config.merged_hooks;
        req_ctx = merged_hooks.run_on_request(req_ctx).await?;
        url = req_ctx.url.clone();
        headers = req_ctx.headers.clone();
        method = req_ctx.method.clone();

        let timeout = builder.timeout;
        let retry_policy = builder.retry.or_else(|| self.config.retry.clone());
        let throw_on_error = builder.throw_on_error;
        let cancel = builder.cancellation;

        let backend = self.backend.clone();

        let _in_flight_permit = match &self.config.max_in_flight {
            Some(sem) => Some(
                sem.acquire()
                    .await
                    .map_err(|_| Error::Other("max_in_flight semaphore closed".into()))?,
            ),
            None => None,
        };

        let mut attempt = 0u32;
        let max_attempts = retry_policy.as_ref().map(|p| p.max_attempts()).unwrap_or(0);

        let request_body = builder.body;
        #[cfg(feature = "multipart")]
        let mut multipart_body = builder.multipart;
        #[cfg(feature = "multipart")]
        let had_multipart = multipart_body.is_some();

        let cancel_ref = cancel.as_ref();

        loop {
            req_ctx.retry_attempt = attempt;

            #[cfg(feature = "multipart")]
            if attempt > 0 && had_multipart {
                return Err(Error::Other(
                    "automatic retry is not supported with multipart request bodies".into(),
                ));
            }

            let http_req = HttpRequest {
                method: method.clone(),
                url: url.clone(),
                headers: headers.clone(),
                body: request_body.clone(),
                timeout,
                cancellation: cancel.clone(),
                #[cfg(feature = "multipart")]
                multipart: multipart_body.take(),
            };
            let request_url = http_req.url.clone();

            let result = execute_or_cancel(cancel_ref, backend.execute(http_req)).await;

            match result {
                Ok(http_res) => {
                    let response = Response::new(
                        http_res.status,
                        http_res.headers.clone(),
                        http_res.body,
                        Some(request_url.clone()),
                        #[cfg(feature = "json")]
                        json_parser.clone(),
                    );

                    let response = merged_hooks
                        .run_on_response(ResponseContext {
                            request: req_ctx.clone(),
                            response,
                        })
                        .await?;

                    let should_retry = retry_policy
                        .as_ref()
                        .map(|p| p.should_retry_response(&response, false))
                        .unwrap_or(false);

                    if should_retry && attempt < max_attempts {
                        merged_hooks
                            .run_on_retry(ResponseContext {
                                request: req_ctx.clone(),
                                response: response.clone(),
                            })
                            .await;
                        let delay = retry_policy
                            .as_ref()
                            .map(|p| p.delay_after_response(attempt, response.headers()))
                            .unwrap_or(Duration::from_secs(1));
                        attempt += 1;
                        sleep_or_cancel(delay, cancel_ref).await?;
                        continue;
                    }

                    if response.is_success() {
                        merged_hooks
                            .run_on_success(SuccessContext {
                                request: req_ctx.clone(),
                                response: response.clone(),
                            })
                            .await;
                        return Ok(response);
                    }

                    let status = response.status();
                    let http_err = Error::http_with_status_text(
                        status,
                        status.canonical_reason().unwrap_or("request failed"),
                        status.canonical_reason().unwrap_or("request failed"),
                        Some(response.bytes().clone()),
                    );
                    merged_hooks
                        .run_on_error(ErrorContext {
                            request: req_ctx.clone(),
                            response: Some(response.clone()),
                            error: http_err.clone(),
                        })
                        .await;

                    if throw_on_error {
                        return Err(http_err);
                    }
                    return Ok(response);
                }
                Err(err) => {
                    if err.is_cancelled() {
                        merged_hooks
                            .run_on_error(ErrorContext {
                                request: req_ctx.clone(),
                                response: None,
                                error: err.clone(),
                            })
                            .await;
                        return Err(err);
                    }

                    let retry_transport = matches!(&err, Error::Transport { .. } | Error::Timeout);
                    if retry_transport && retry_policy.is_some() && attempt < max_attempts {
                        merged_hooks
                            .run_on_retry(ResponseContext {
                                request: req_ctx.clone(),
                                response: Response::new(
                                    http::StatusCode::SERVICE_UNAVAILABLE,
                                    http::HeaderMap::new(),
                                    bytes::Bytes::new(),
                                    Some(request_url.clone()),
                                    #[cfg(feature = "json")]
                                    None,
                                ),
                            })
                            .await;
                        let delay = retry_policy
                            .as_ref()
                            .map(|p| p.delay_after_response(attempt, &http::HeaderMap::new()))
                            .unwrap_or(Duration::from_secs(1));
                        attempt += 1;
                        sleep_or_cancel(delay, cancel_ref).await?;
                        continue;
                    }

                    merged_hooks
                        .run_on_error(ErrorContext {
                            request: req_ctx.clone(),
                            response: None,
                            error: err.clone(),
                        })
                        .await;

                    if retry_transport && retry_policy.is_some() {
                        return Err(Error::retry_exhausted(attempt + 1, err));
                    }

                    return Err(err);
                }
            }
        }
    }
}

/// Builder for [`Client`].
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
        let name = http::HeaderName::from_bytes(key.as_ref().as_bytes())
            .map_err(|e| Error::Other(format!("invalid header name: {e}")))?;
        let value = http::HeaderValue::from_str(value.as_ref())
            .map_err(|e| Error::Other(format!("invalid header value: {e}")))?;
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

    /// Use a Tower [`Service`](tower::Service) as the HTTP transport (feature `tower`).
    #[cfg(feature = "tower")]
    pub fn http_service<S>(mut self, service: S) -> Self
    where
        S: tower::Service<HttpRequest, Response = HttpResponse, Error = Error>
            + Clone
            + Send
            + 'static,
        S::Future: Send + 'static,
    {
        use crate::backend::ReqwestBackend;
        use crate::tower::ServiceBackend;

        let client = self.reqwest_client.clone().unwrap_or_default();
        let streaming = ReqwestBackend::new(client);
        self.custom_backend = Some(Arc::new(ServiceBackend::new(service, streaming)));
        self
    }

    /// Use a boxed Tower transport stack (feature `tower`).
    #[cfg(feature = "tower")]
    pub fn http_service_boxed(mut self, service: crate::tower::BoxHttpService) -> Self {
        use crate::backend::ReqwestBackend;
        use crate::tower::ServiceBackend;

        let client = self.reqwest_client.clone().unwrap_or_default();
        let streaming = ReqwestBackend::new(client);
        self.custom_backend = Some(Arc::new(ServiceBackend::from_box(service, streaming)));
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
    /// # use better_fetch::tower::stack::{ConcurrencyLimitLayer, IntoBoxHttpService, ServiceBuilder};
    /// let client = ClientBuilder::new()
    ///     .base_url("https://api.example.com")?
    ///     .transport_stack(|inner| {
    ///         ServiceBuilder::new()
    ///             .layer(ConcurrencyLimitLayer::new(32))
    ///             .service(inner)
    ///             .into_box()
    ///     })
    ///     .build()?;
    /// # Ok::<(), better_fetch::Error>(())
    /// ```
    #[cfg(feature = "tower")]
    pub fn transport_stack<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(crate::tower::ReqwestHttpService) -> crate::tower::BoxHttpService,
    {
        use crate::backend::ReqwestBackend;
        use crate::tower::ServiceBackend;

        let client = self.reqwest_client.clone().unwrap_or_default();
        let streaming = ReqwestBackend::new(client.clone());
        let stacked = configure(crate::tower::ReqwestHttpService::new(client));
        self.custom_backend = Some(Arc::new(ServiceBackend::from_box(stacked, streaming)));
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
