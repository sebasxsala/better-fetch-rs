pub(crate) mod exec;
mod reqwest;

pub use reqwest::ReqwestBackend;

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use std::time::Duration;

use crate::Result;

/// Prepared HTTP request passed to a backend.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: url::Url,
    pub headers: HeaderMap,
    pub body: Option<Bytes>,
    pub timeout: Option<Duration>,
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
