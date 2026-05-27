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
use crate::endpoint::{Endpoint, EndpointRequestBuilder};
use crate::error::Error;
use crate::hooks::{ErrorContext, Hooks, RequestContext, ResponseContext, SuccessContext};
use crate::plugin::{PluginRegistry, PreparedRequest};
use crate::request::RequestBuilder;
use crate::response::Response;
use crate::retry::{sleep_or_cancel, RetryPolicy};
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

/// Shared client configuration.
#[derive(Clone)]
pub struct ClientConfig {
    pub base_url: Url,
    pub timeout: Option<Duration>,
    pub retry: Option<RetryPolicy>,
    pub auth: Option<Auth>,
    pub default_headers: http::HeaderMap,
    pub hooks: Hooks,
    pub(crate) merged_hooks: Hooks,
    pub plugins: Arc<PluginRegistry>,
    /// Limits concurrent in-flight requests for this client (including retries).
    ///
    /// This is separate from Tower's [`ConcurrencyLimitLayer`](crate::tower::stack::ConcurrencyLimitLayer):
    /// the client semaphore applies to the full request lifecycle (hooks + retries), while Tower
    /// limits only transport-layer concurrency. Avoid stacking both without accounting for that.
    pub max_in_flight: Option<Arc<Semaphore>>,
    #[cfg(feature = "schema")]
    pub schema_registry: Option<Arc<SchemaRegistry>>,
    #[cfg(feature = "json")]
    pub json_parser: Option<JsonParserFn>,
}

/// Typed HTTP client built on reqwest.
#[derive(Clone)]
pub struct Client {
    config: Arc<ClientConfig>,
    backend: Arc<dyn HttpBackend>,
}

impl Client {
    pub fn new(base_url: impl AsRef<str>) -> Result<Self> {
        ClientBuilder::new().base_url(base_url)?.build()
    }

    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Builds a client with a custom reqwest instance. [`ClientBuilder::base_url`] is required.
    pub fn with_http_client(reqwest_client: ReqwestClient, base_url: impl AsRef<str>) -> Result<Self> {
        ClientBuilder::new()
            .reqwest_client(reqwest_client)
            .base_url(base_url)?
            .build()
    }

    /// Start a typed request for [`Endpoint`] `E`.
    pub fn call<E: Endpoint>(&self) -> EndpointRequestBuilder<'_, E> {
        EndpointRequestBuilder::new(self.request(E::METHOD, E::PATH))
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    pub fn get(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::GET, path)
    }

    pub fn post(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::POST, path)
    }

    pub fn put(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::PUT, path)
    }

    pub fn patch(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::PATCH, path)
    }

    pub fn delete(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::DELETE, path)
    }

    pub fn head(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::HEAD, path)
    }

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

                    let retry_transport = matches!(&err, Error::Transport(_) | Error::Timeout);
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
    #[cfg(feature = "schema")]
    schema_registry: Option<Arc<SchemaRegistry>>,
    #[cfg(feature = "json")]
    json_parser: Option<JsonParserFn>,
}

impl ClientBuilder {
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
            #[cfg(feature = "schema")]
            schema_registry: None,
            #[cfg(feature = "json")]
            json_parser: None,
        }
    }

    pub fn base_url(mut self, base_url: impl AsRef<str>) -> Result<Self> {
        self.base_url = Some(Url::parse(base_url.as_ref()).map_err(Error::InvalidBaseUrl)?);
        Ok(self)
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = Some(policy);
        self
    }

    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn default_header(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Result<Self> {
        let name = http::HeaderName::from_bytes(key.as_ref().as_bytes())
            .map_err(|e| Error::Other(format!("invalid header name: {e}")))?;
        let value = http::HeaderValue::from_str(value.as_ref())
            .map_err(|e| Error::Other(format!("invalid header value: {e}")))?;
        self.default_headers.insert(name, value);
        Ok(self)
    }

    pub fn hooks(mut self, hooks: Hooks) -> Self {
        self.hooks = hooks;
        self
    }

    pub fn plugin<P: crate::plugin::Plugin + 'static>(mut self, plugin: P) -> Self {
        self.plugins.push(Box::new(plugin));
        self
    }

    pub fn reqwest_client(mut self, client: ReqwestClient) -> Self {
        self.reqwest_client = Some(client);
        self
    }

    /// Use a custom HTTP backend (for testing or alternate transports).
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
        use crate::tower::ServiceBackend;

        self.custom_backend = Some(Arc::new(ServiceBackend::new(service)));
        self
    }

    /// Use a boxed Tower transport stack (feature `tower`).
    #[cfg(feature = "tower")]
    pub fn http_service_boxed(mut self, service: crate::tower::BoxHttpService) -> Self {
        use crate::tower::ServiceBackend;

        self.custom_backend = Some(Arc::new(ServiceBackend::from_box(service)));
        self
    }

    /// Build a Tower transport stack on top of the configured (or default) reqwest client.
    ///
    /// Application hooks and [`RetryPolicy`](crate::RetryPolicy) remain in the core client;
    /// only wire-level behavior is configured here.
    #[cfg(feature = "tower")]
    pub fn transport_stack<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(crate::tower::ReqwestHttpService) -> crate::tower::BoxHttpService,
    {
        use crate::tower::ServiceBackend;

        let client = self.reqwest_client.clone().unwrap_or_default();
        let stacked = configure(crate::tower::ReqwestHttpService::new(client));
        self.custom_backend = Some(Arc::new(ServiceBackend::from_box(stacked)));
        self
    }

    /// Sets a custom JSON parser for all responses from this client.
    ///
    /// See [`crate::json_parser`] for the two-step `Bytes` → `Value` → `T` pipeline vs the
    /// default single-step fast path, and [`Response::into_json_with`](crate::response::Response::into_json_with)
    /// for per-response `Bytes` → `T` without a global parser.
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

    pub fn build(self) -> Result<Client> {
        let base_url = self
            .base_url
            .ok_or(Error::MissingBaseUrl)?;

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
