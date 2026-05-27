use async_trait::async_trait;
use reqwest::Client;

use super::exec::send_reqwest;
use super::{HttpBackend, HttpRequest, HttpResponse};
use crate::Result;

/// Reqwest-backed HTTP backend.
#[derive(Debug, Clone)]
pub struct ReqwestBackend {
    client: Client,
}

impl ReqwestBackend {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

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
}
