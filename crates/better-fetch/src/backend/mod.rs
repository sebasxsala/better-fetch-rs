//! HTTP transport abstraction.
//!
//! The default backend is [`ReqwestBackend`]. Inject a custom [`HttpBackend`] via
//! [`ClientBuilder::backend`](crate::ClientBuilder::backend) for tests or alternate transports.

pub(crate) mod exec;
mod reqwest;

pub use reqwest::ReqwestBackend;

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use std::time::Duration;

use crate::cancel::CancellationToken;
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

/// Raw HTTP response from a backend.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// HTTP status.
    pub status: StatusCode,
    /// Response headers.
    pub headers: HeaderMap,
    /// Response body bytes.
    pub body: Bytes,
}

/// Pluggable HTTP transport used by [`Client`](crate::Client).
#[async_trait]
pub trait HttpBackend: Send + Sync {
    /// Executes one HTTP request and returns the raw response.
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse>;
}
