//! Lifecycle hooks for requests and responses.
//!
//! Buffered responses use [`Hooks::on_response`] / [`Hooks::on_success`]. Streaming responses
//! ([`RequestBuilder::send_stream`](crate::RequestBuilder::send_stream)) use
//! [`Hooks::on_response_stream`] / [`Hooks::on_success_stream`] with status and headers only (no body).
//!
//! [`Hooks::on_request`] and [`Hooks::on_response`] / [`Hooks::on_response_stream`] return [`Result`].
//! To abort the client pipeline intentionally, return `Err(Error::hook("reason"))`.
//! [`Hooks::on_success`], [`Hooks::on_success_stream`], [`Hooks::on_error`], and [`Hooks::on_retry`]
//! cannot return errors.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
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
    ///
    /// Mutations are applied to the outgoing HTTP request after all `on_request` hooks run (since 0.4.0).
    pub body: Option<Bytes>,
    /// Number of times this request has already been retried (`0` on the first HTTP attempt).
    ///
    /// Matches JS [`retryAttempt`](https://better-fetch.vercel.app/docs/fetch-options).
    pub retry_attempt: u32,
}

/// Context after a buffered response is received.
#[derive(Debug, Clone)]
pub struct ResponseContext {
    /// Original request context.
    pub request: RequestContext,
    /// Response from the transport (may be mutated by hooks).
    pub response: Response,
}

/// Context after a streaming response is received (headers only; body not consumed).
#[derive(Debug, Clone)]
pub struct StreamingResponseContext {
    /// Original request context.
    pub request: RequestContext,
    /// HTTP status.
    pub status: StatusCode,
    /// Response headers (hooks may mutate).
    pub headers: HeaderMap,
}

/// Metadata returned from streaming response hooks.
#[derive(Debug, Clone)]
pub struct StreamingResponseMeta {
    /// HTTP status (usually unchanged).
    pub status: StatusCode,
    /// Response headers after hook mutations.
    pub headers: HeaderMap,
}

/// Context after a successful HTTP response (2xx).
#[derive(Debug, Clone)]
pub struct SuccessContext {
    /// Original request context.
    pub request: RequestContext,
    /// Successful response.
    pub response: Response,
}

/// Context after a successful streaming response (2xx, metadata only).
#[derive(Debug, Clone)]
pub struct StreamingSuccessContext {
    /// Original request context.
    pub request: RequestContext,
    /// HTTP status.
    pub status: StatusCode,
    /// Response headers.
    pub headers: HeaderMap,
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

impl ErrorContext {
    /// UTF-8 preview of the buffered response body when [`Self::response`] is set.
    ///
    /// Non-UTF-8 bodies return `None`. Truncates to `max_bytes` (default use: 512).
    pub fn response_body_preview(&self, max_bytes: usize) -> Option<String> {
        let body = self.response.as_ref()?.bytes();
        if body.is_empty() {
            return None;
        }
        let lossy = String::from_utf8_lossy(body);
        if lossy.len() <= max_bytes {
            Some(lossy.into_owned())
        } else {
            let end = lossy
                .char_indices()
                .map(|(i, _)| i)
                .nth(max_bytes)
                .unwrap_or(lossy.len());
            Some(format!("{}…", &lossy[..end]))
        }
    }
}

type RequestHookFn = Arc<
    dyn Fn(RequestContext) -> Pin<Box<dyn Future<Output = Result<RequestContext>> + Send>>
        + Send
        + Sync,
>;

type ResponseHookFn = Arc<
    dyn Fn(ResponseContext) -> Pin<Box<dyn Future<Output = Result<Response>> + Send>> + Send + Sync,
>;

type StreamingResponseHookFn = Arc<
    dyn Fn(
            StreamingResponseContext,
        ) -> Pin<Box<dyn Future<Output = Result<StreamingResponseMeta>> + Send>>
        + Send
        + Sync,
>;

type SuccessHookFn =
    Arc<dyn Fn(SuccessContext) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

type StreamingSuccessHookFn =
    Arc<dyn Fn(StreamingSuccessContext) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

type ErrorHookFn =
    Arc<dyn Fn(ErrorContext) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

type RetryHookFn =
    Arc<dyn Fn(ResponseContext) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Lifecycle hooks for the HTTP client.
#[derive(Clone, Default)]
pub struct Hooks {
    pub(crate) on_request: Vec<RequestHookFn>,
    pub(crate) on_response: Vec<ResponseHookFn>,
    pub(crate) on_response_stream: Vec<StreamingResponseHookFn>,
    pub(crate) on_success: Vec<SuccessHookFn>,
    pub(crate) on_success_stream: Vec<StreamingSuccessHookFn>,
    pub(crate) on_error: Vec<ErrorHookFn>,
    pub(crate) on_retry: Vec<RetryHookFn>,
}

impl Hooks {
    /// Creates an empty hook chain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs before the transport call. Return `Err(Error::hook("…"))` to cancel the request.
    pub fn on_request<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(RequestContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<RequestContext>> + Send + 'static,
    {
        self.on_request.push(Arc::new(move |ctx| Box::pin(f(ctx))));
        self
    }

    /// Runs after a buffered transport returns. Return `Err(Error::hook("…"))` to fail the request.
    pub fn on_response<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(ResponseContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response>> + Send + 'static,
    {
        self.on_response.push(Arc::new(move |ctx| Box::pin(f(ctx))));
        self
    }

    /// Runs after streaming transport returns, before the body is read.
    ///
    /// Use this on [`RequestBuilder::send_stream`](crate::RequestBuilder::send_stream) instead of
    /// [`on_response`](Self::on_response). Return updated [`StreamingResponseMeta`] (e.g. mutate headers).
    pub fn on_response_stream<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(StreamingResponseContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<StreamingResponseMeta>> + Send + 'static,
    {
        self.on_response_stream
            .push(Arc::new(move |ctx| Box::pin(f(ctx))));
        self
    }

    /// Runs after a successful (2xx) buffered response; cannot abort the pipeline.
    pub fn on_success<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(SuccessContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.on_success.push(Arc::new(move |ctx| Box::pin(f(ctx))));
        self
    }

    /// Runs after a successful (2xx) streaming response; cannot abort the pipeline.
    pub fn on_success_stream<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(StreamingSuccessContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.on_success_stream
            .push(Arc::new(move |ctx| Box::pin(f(ctx))));
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
        self.on_response_stream.extend(other.on_response_stream);
        self.on_success.extend(other.on_success);
        self.on_success_stream.extend(other.on_success_stream);
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

    pub(crate) async fn run_on_response_stream(
        &self,
        ctx: StreamingResponseContext,
    ) -> Result<StreamingResponseMeta> {
        let request = ctx.request;
        let mut meta = StreamingResponseMeta {
            status: ctx.status,
            headers: ctx.headers,
        };
        for hook in &self.on_response_stream {
            meta = hook(StreamingResponseContext {
                request: request.clone(),
                status: meta.status,
                headers: meta.headers,
            })
            .await?;
        }
        Ok(meta)
    }

    pub(crate) async fn run_on_success(&self, ctx: SuccessContext) {
        for hook in &self.on_success {
            hook(ctx.clone()).await;
        }
    }

    pub(crate) async fn run_on_success_stream(&self, ctx: StreamingSuccessContext) {
        for hook in &self.on_success_stream {
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
