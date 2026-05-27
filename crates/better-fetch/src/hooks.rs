//! Lifecycle hooks for requests and responses.
//!
//! [`Hooks::on_request`] and [`Hooks::on_response`] return [`Result`]. To abort the client
//! pipeline intentionally, return `Err(Error::hook("reason"))`. Other [`Error`] variants
//! (`Transport`, `Http`, …) are valid when a hook needs to surface a specific failure.
//! [`Hooks::on_success`], [`Hooks::on_error`], and [`Hooks::on_retry`] cannot return errors.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, Method};
use url::Url;

use crate::error::Error;
use crate::response::Response;
use crate::Result;

/// Context for an outgoing request.
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// Final URL after plugins and hook mutations.
    pub url: Url,
    /// HTTP method.
    pub method: Method,
    /// Request headers.
    pub headers: HeaderMap,
    /// Request body when present.
    pub body: Option<Bytes>,
    /// Number of times this request has already been retried (`0` on the first HTTP attempt).
    ///
    /// Matches JS [`retryAttempt`](https://better-fetch.vercel.app/docs/fetch-options).
    pub retry_attempt: u32,
}

/// Context after a response is received.
#[derive(Debug, Clone)]
pub struct ResponseContext {
    /// Original request context.
    pub request: RequestContext,
    /// Response from the transport (may be mutated by hooks).
    pub response: Response,
}

/// Context after a successful HTTP response (2xx).
#[derive(Debug, Clone)]
pub struct SuccessContext {
    /// Original request context.
    pub request: RequestContext,
    /// Successful response.
    pub response: Response,
}

/// Context when an error occurs.
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Original request context.
    pub request: RequestContext,
    /// Response when the error is HTTP-related.
    pub response: Option<Response>,
    /// Error that occurred.
    pub error: Error,
}

type RequestHookFn = Arc<
    dyn Fn(RequestContext) -> Pin<Box<dyn Future<Output = Result<RequestContext>> + Send>>
        + Send
        + Sync,
>;

type ResponseHookFn = Arc<
    dyn Fn(ResponseContext) -> Pin<Box<dyn Future<Output = Result<Response>> + Send>> + Send + Sync,
>;

type SuccessHookFn =
    Arc<dyn Fn(SuccessContext) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

type ErrorHookFn =
    Arc<dyn Fn(ErrorContext) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

type RetryHookFn =
    Arc<dyn Fn(ResponseContext) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Lifecycle hooks for the HTTP client.
#[derive(Clone, Default)]
pub struct Hooks {
    pub(crate) on_request: Vec<RequestHookFn>,
    pub(crate) on_response: Vec<ResponseHookFn>,
    pub(crate) on_success: Vec<SuccessHookFn>,
    pub(crate) on_error: Vec<ErrorHookFn>,
    pub(crate) on_retry: Vec<RetryHookFn>,
}

impl Hooks {
    /// Creates an empty hook chain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs before the transport call. Return `Err(Error::hook("…"))` to cancel the request.
    ///
    /// # Examples
    ///
    /// ```
    /// use better_fetch::{ClientBuilder, Error, Hooks, Result};
    ///
    /// let hooks = Hooks::new().on_request(|ctx| async move {
    ///     if ctx.url.path().contains("blocked") {
    ///         return Err(Error::hook("path not allowed"));
    ///     }
    ///     Ok(ctx)
    /// });
    ///
    /// let client = ClientBuilder::new()
    ///     .base_url("https://api.example.com")?
    ///     .hooks(hooks)
    ///     .build()?;
    /// # Ok::<(), better_fetch::Error>(())
    /// ```
    pub fn on_request<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(RequestContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<RequestContext>> + Send + 'static,
    {
        self.on_request.push(Arc::new(move |ctx| Box::pin(f(ctx))));
        self
    }

    /// Runs after the transport returns. Return `Err(Error::hook("…"))` to fail the request.
    pub fn on_response<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(ResponseContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response>> + Send + 'static,
    {
        self.on_response.push(Arc::new(move |ctx| Box::pin(f(ctx))));
        self
    }

    /// Runs after a successful (2xx) response; cannot abort the pipeline.
    pub fn on_success<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(SuccessContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.on_success.push(Arc::new(move |ctx| Box::pin(f(ctx))));
        self
    }

    /// Runs when an error occurs; cannot abort the pipeline.
    pub fn on_error<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(ErrorContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.on_error.push(Arc::new(move |ctx| Box::pin(f(ctx))));
        self
    }

    /// Runs before a transport retry is scheduled.
    pub fn on_retry<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(ResponseContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.on_retry.push(Arc::new(move |ctx| Box::pin(f(ctx))));
        self
    }

    pub(crate) fn merge(mut self, other: Hooks) -> Self {
        self.on_request.extend(other.on_request);
        self.on_response.extend(other.on_response);
        self.on_success.extend(other.on_success);
        self.on_error.extend(other.on_error);
        self.on_retry.extend(other.on_retry);
        self
    }

    pub(crate) async fn run_on_request(&self, mut ctx: RequestContext) -> Result<RequestContext> {
        for hook in &self.on_request {
            ctx = hook(ctx).await?;
        }
        Ok(ctx)
    }

    pub(crate) async fn run_on_response(&self, ctx: ResponseContext) -> Result<Response> {
        let request = ctx.request;
        let mut response = ctx.response;
        for hook in &self.on_response {
            response = hook(ResponseContext {
                request: request.clone(),
                response,
            })
            .await?;
        }
        Ok(response)
    }

    pub(crate) async fn run_on_success(&self, ctx: SuccessContext) {
        for hook in &self.on_success {
            hook(ctx.clone()).await;
        }
    }

    pub(crate) async fn run_on_error(&self, ctx: ErrorContext) {
        for hook in &self.on_error {
            hook(ctx.clone()).await;
        }
    }

    pub(crate) async fn run_on_retry(&self, ctx: ResponseContext) {
        for hook in &self.on_retry {
            hook(ctx.clone()).await;
        }
    }
}
