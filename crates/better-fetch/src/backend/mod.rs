//! HTTP transport abstraction.
//!
//! The default backend is [`ReqwestBackend`]. Inject a custom [`HttpBackend`] via
//! [`ClientBuilder::backend`](crate::ClientBuilder::backend) for tests or alternate transports.
//!
//! Buffered responses use [`HttpBackend::execute`]. Streaming responses use
//! [`HttpBackend::execute_stream`] (implemented by [`ReqwestBackend`] by default).

pub(crate) mod exec;
mod reqwest;

pub use reqwest::ReqwestBackend;

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use std::time::Duration;

use crate::cancel::CancellationToken;
use crate::streaming::BodyStream;
use crate::Result;

#[cfg(feature = "multipart")]
use crate::multipart::Form as MultipartForm;

/// Request body encoding for the transport layer.
#[derive(Debug, Clone, Default)]
pub enum HttpBody {
    /// No body.
    #[default]
    Empty,
    /// Raw bytes body.
    Bytes(Bytes),
}

/// Prepared HTTP request passed to a backend.
#[derive(Debug)]
pub struct HttpRequest {
    /// HTTP method.
    pub method: Method,
    /// Fully resolved URL.
    pub url: url::Url,
    /// Request headers.
    pub headers: HeaderMap,
    /// Body when not using multipart.
    pub body: HttpBody,
    /// Per-request timeout.
    pub timeout: Option<Duration>,
    /// Cooperative cancellation.
    pub cancellation: Option<CancellationToken>,
    #[cfg(feature = "multipart")]
    /// Multipart form (feature `multipart`).
    pub multipart: Option<MultipartForm>,
}

impl Clone for HttpRequest {
    fn clone(&self) -> Self {
        Self {
            method: self.method.clone(),
            url: self.url.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            timeout: self.timeout,
            cancellation: self.cancellation.clone(),
            #[cfg(feature = "multipart")]
            multipart: None,
        }
    }
}

/// Raw HTTP response from a backend (fully buffered).
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// HTTP status.
    pub status: StatusCode,
    /// Response headers.
    pub headers: HeaderMap,
    /// Response body bytes.
    pub body: Bytes,
}

/// Raw HTTP response with a streaming body from a backend.
pub struct HttpStreamingResponse {
    /// HTTP status.
    pub status: StatusCode,
    /// Response headers.
    pub headers: HeaderMap,
    /// Response body stream.
    pub body: BodyStream,
}

impl std::fmt::Debug for HttpStreamingResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpStreamingResponse")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("body", &"<stream>")
            .finish()
    }
}

/// Pluggable HTTP transport used by [`Client`](crate::Client).
#[async_trait]
pub trait HttpBackend: Send + Sync {
    /// Executes one HTTP request and returns the fully buffered response.
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse>;

    /// Executes one HTTP request and returns a streaming response body.
    async fn execute_stream(&self, request: HttpRequest) -> Result<HttpStreamingResponse>;
}
