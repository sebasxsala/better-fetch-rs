use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures_util::future::BoxFuture;
use tower::util::BoxCloneService;
use tower::{Service, ServiceExt};

use crate::backend::exec::send_reqwest;
use crate::backend::{
    HttpBackend, HttpRequest, HttpResponse, HttpStreamingResponse, ReqwestBackend,
};
use crate::{Error, Result};

/// Type-erased clone HTTP [`Service`](tower::Service).
pub type BoxHttpService = BoxCloneService<HttpRequest, HttpResponse, Error>;

/// Reqwest-backed [`Service`](tower::Service) for stacking Tower layers.
#[derive(Clone, Debug)]
pub struct ReqwestHttpService {
    client: reqwest::Client,
}

impl ReqwestHttpService {
    /// Creates a service that uses the given reqwest client.
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Creates a service with a default reqwest client.
    pub fn default_client() -> Self {
        Self::new(reqwest::Client::new())
    }

    /// Returns the underlying reqwest client.
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
/// Buffered requests use the Tower stack. [`HttpBackend::execute_stream`] delegates to a
/// separate [`ReqwestBackend`] (same reqwest client as [`ClientBuilder::transport_stack`](crate::ClientBuilder::transport_stack)).
/// Tower request middleware does not apply to the streaming path.
pub struct ServiceBackend {
    inner: Arc<Mutex<BoxHttpService>>,
    streaming: ReqwestBackend,
}

impl ServiceBackend {
    /// Wraps a Tower service and a reqwest backend used for streaming responses.
    pub fn new<S>(service: S, streaming: ReqwestBackend) -> Self
    where
        S: Service<HttpRequest, Response = HttpResponse, Error = Error> + Clone + Send + 'static,
        S::Future: Send + 'static,
    {
        Self {
            inner: Arc::new(Mutex::new(BoxHttpService::new(service))),
            streaming,
        }
    }

    /// Wraps an already-boxed transport stack with a streaming backend.
    pub fn from_box(service: BoxHttpService, streaming: ReqwestBackend) -> Self {
        Self {
            inner: Arc::new(Mutex::new(service)),
            streaming,
        }
    }

    /// Returns a clone of the inner transport stack (for advanced testing).
    pub fn clone_inner(&self) -> BoxHttpService {
        self.inner
            .lock()
            .expect("ServiceBackend inner mutex poisoned")
            .clone()
    }

    /// Returns the reqwest backend used for [`HttpBackend::execute_stream`](crate::backend::HttpBackend::execute_stream).
    pub fn streaming_backend(&self) -> &ReqwestBackend {
        &self.streaming
    }
}

impl Clone for ServiceBackend {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            streaming: self.streaming.clone(),
        }
    }
}

#[async_trait]
impl HttpBackend for ServiceBackend {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse> {
        let mut service = self
            .inner
            .lock()
            .expect("ServiceBackend inner mutex poisoned")
            .clone();
        service
            .ready()
            .await
            .map_err(|e| Error::transport_message(format!("service not ready: {e}")))?;
        service.call(request).await
    }

    async fn execute_stream(&self, request: HttpRequest) -> Result<HttpStreamingResponse> {
        self.streaming.execute_stream(request).await
    }
}
