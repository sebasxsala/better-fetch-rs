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
    #[default]
    Empty,
    Bytes(Bytes),
}

/// Prepared HTTP request passed to a backend.
#[derive(Debug)]
pub struct HttpRequest {
    pub method: Method,
    pub url: url::Url,
    pub headers: HeaderMap,
    pub body: HttpBody,
    pub timeout: Option<Duration>,
    pub cancellation: Option<CancellationToken>,
    #[cfg(feature = "multipart")]
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
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

#[async_trait]
pub trait HttpBackend: Send + Sync {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse>;
}
