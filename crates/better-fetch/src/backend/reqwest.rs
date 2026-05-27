use async_trait::async_trait;
use reqwest::Client;

use super::exec::{send_reqwest, send_reqwest_stream};
use super::{HttpBackend, HttpRequest, HttpResponse, HttpStreamingResponse};
use crate::Result;

/// Reqwest-backed HTTP backend.
#[derive(Debug, Clone)]
pub struct ReqwestBackend {
    client: Client,
}

impl ReqwestBackend {
    /// Creates a backend that uses the given reqwest client.
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Returns the underlying reqwest client.
    pub fn client(&self) -> &Client {
        &self.client
    }
}

impl Default for ReqwestBackend {
    fn default() -> Self {
        Self::new(Client::new())
    }
}

#[async_trait]
impl HttpBackend for ReqwestBackend {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse> {
        send_reqwest(&self.client, request).await
    }

    async fn execute_stream(&self, request: HttpRequest) -> Result<HttpStreamingResponse> {
        send_reqwest_stream(&self.client, request).await
    }
}
