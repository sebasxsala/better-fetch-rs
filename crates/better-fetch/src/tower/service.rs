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
/// Every call through [`HttpBackend::execute`](crate::backend::HttpBackend::execute) acquires
/// a [`tokio::sync::Mutex`] on the inner service for the full `ready` + `call` sequence,
/// because Tower services require `&mut self` for [`Service::call`](tower::Service::call).
/// Concurrent client requests therefore take turns at this lock when using
/// [`ClientBuilder::http_service`](crate::client::ClientBuilder::http_service),
/// [`http_service_boxed`](crate::client::ClientBuilder::http_service_boxed), or
/// [`transport_stack`](crate::client::ClientBuilder::transport_stack).
///
/// **Production transport stacks:** wrap your inner service with
/// [`tower::buffer::Buffer`](https://docs.rs/tower/latest/tower/buffer/struct.Buffer.html)
/// *before* passing the stack to the client (`Buffer::new` spawns its worker on Tokio)
/// (see `examples/tower_stack`). That is the recommended pattern for layered Tower
/// services; the outer mutex here remains a bottleneck until a future release may
/// replace it with an internal buffer.
///
/// When you do not need Tower middleware, prefer the default reqwest
/// [`HttpBackend`](crate::backend::HttpBackend) (no transport mutex).
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
        service.call(request.clone()).await
    }
}
