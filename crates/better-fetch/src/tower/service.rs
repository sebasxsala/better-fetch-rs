use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures_util::future::BoxFuture;
use tokio::sync::Mutex;
use tower::util::BoxCloneService;
use tower::{Service, ServiceExt};

use crate::backend::exec::send_reqwest;
use crate::backend::{HttpBackend, HttpRequest, HttpResponse};
use crate::{Error, Result};

/// Type-erased clone HTTP [`Service`](tower::Service).
pub type BoxHttpService = BoxCloneService<HttpRequest, HttpResponse, Error>;

/// Reqwest-backed [`Service`](tower::Service) for stacking Tower layers.
#[derive(Clone, Debug)]
pub struct ReqwestHttpService {
    client: reqwest::Client,
}

impl ReqwestHttpService {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub fn default_client() -> Self {
        Self::new(reqwest::Client::new())
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }
}

impl Default for ReqwestHttpService {
    fn default() -> Self {
        Self::default_client()
    }
}

impl Service<HttpRequest> for ReqwestHttpService {
    type Response = HttpResponse;
    type Error = Error;
    type Future = BoxFuture<'static, std::result::Result<HttpResponse, Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: HttpRequest) -> Self::Future {
        let client = self.client.clone();
        Box::pin(async move { send_reqwest(&client, request).await })
    }
}

/// Wraps a Tower [`Service`] as an [`HttpBackend`].
///
/// Requests are serialized through a [`tokio::sync::Mutex`] because Tower services
/// require `&mut self` for [`Service::call`](tower::Service::call). For high concurrency,
/// prefer [`tower::buffer::Buffer`](https://docs.rs/tower/latest/tower/buffer/struct.Buffer.html)
/// inside the transport stack.
pub struct ServiceBackend {
    inner: Arc<Mutex<BoxHttpService>>,
}

impl ServiceBackend {
    pub fn new<S>(service: S) -> Self
    where
        S: Service<HttpRequest, Response = HttpResponse, Error = Error> + Clone + Send + 'static,
        S::Future: Send + 'static,
    {
        Self {
            inner: Arc::new(Mutex::new(BoxHttpService::new(service))),
        }
    }

    pub fn from_box(service: BoxHttpService) -> Self {
        Self {
            inner: Arc::new(Mutex::new(service)),
        }
    }

    pub async fn lock_inner(&self) -> tokio::sync::MutexGuard<'_, BoxHttpService> {
        self.inner.lock().await
    }
}

impl Clone for ServiceBackend {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[async_trait]
impl HttpBackend for ServiceBackend {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse> {
        let mut service = self.inner.lock().await;
        service
            .ready()
            .await
            .map_err(|e| Error::Transport(format!("service not ready: {e}")))?;
        service.call(request).await
    }
}
