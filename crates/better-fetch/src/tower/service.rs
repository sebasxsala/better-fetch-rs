use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures_util::future::BoxFuture;
use tower::util::BoxCloneService;
use tower::{Service, ServiceExt};

use crate::backend::exec::{send_reqwest, send_reqwest_stream};
use crate::backend::{HttpBackend, HttpRequest, HttpResponse, HttpStreamingResponse};
use crate::{Error, Result};

/// Type-erased clone HTTP [`Service`](tower::Service) for buffered responses.
pub type BoxHttpService = BoxCloneService<HttpRequest, HttpResponse, Error>;

/// Type-erased clone HTTP [`Service`](tower::Service) for streaming responses.
pub type BoxStreamingHttpService = BoxCloneService<HttpRequest, HttpStreamingResponse, Error>;

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

/// Reqwest-backed streaming [`Service`](tower::Service).
#[derive(Clone, Debug)]
pub struct ReqwestStreamingHttpService {
    client: reqwest::Client,
}

impl ReqwestStreamingHttpService {
    /// Creates a streaming service using the given reqwest client.
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl Service<HttpRequest> for ReqwestStreamingHttpService {
    type Response = HttpStreamingResponse;
    type Error = Error;
    type Future = BoxFuture<'static, std::result::Result<HttpStreamingResponse, Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: HttpRequest) -> Self::Future {
        let client = self.client.clone();
        Box::pin(async move { send_reqwest_stream(&client, request).await })
    }
}

/// Wraps Tower [`Service`] stacks as an [`HttpBackend`].
///
/// Both buffered and streaming paths run through Tower middleware when configured via
/// [`ClientBuilder::transport_stack`](crate::ClientBuilder::transport_stack).
pub struct ServiceBackend {
    inner: Arc<Mutex<BoxHttpService>>,
    streaming: Arc<Mutex<BoxStreamingHttpService>>,
}

impl ServiceBackend {
    /// Wraps buffered and streaming Tower services.
    pub fn new<B, S>(buffered: B, streaming: S) -> Self
    where
        B: Service<HttpRequest, Response = HttpResponse, Error = Error> + Clone + Send + 'static,
        B::Future: Send + 'static,
        S: Service<HttpRequest, Response = HttpStreamingResponse, Error = Error>
            + Clone
            + Send
            + 'static,
        S::Future: Send + 'static,
    {
        Self {
            inner: Arc::new(Mutex::new(BoxHttpService::new(buffered))),
            streaming: Arc::new(Mutex::new(BoxStreamingHttpService::new(streaming))),
        }
    }

    /// Wraps already-boxed buffered and streaming transport stacks.
    pub fn from_boxes(buffered: BoxHttpService, streaming: BoxStreamingHttpService) -> Self {
        Self {
            inner: Arc::new(Mutex::new(buffered)),
            streaming: Arc::new(Mutex::new(streaming)),
        }
    }

    /// Buffered Tower stack + plain reqwest streaming (legacy).
    pub fn buffered_with_reqwest_streaming<S>(service: S, client: reqwest::Client) -> Self
    where
        S: Service<HttpRequest, Response = HttpResponse, Error = Error> + Clone + Send + 'static,
        S::Future: Send + 'static,
    {
        Self::new(service, ReqwestStreamingHttpService::new(client))
    }

    /// Wraps a Tower service and a reqwest backend used for streaming responses.
    #[deprecated(note = "use `from_boxes` or `transport_stack` which wires both paths")]
    pub fn new_buffered_only<S>(service: S, client: reqwest::Client) -> Self
    where
        S: Service<HttpRequest, Response = HttpResponse, Error = Error> + Clone + Send + 'static,
        S::Future: Send + 'static,
    {
        Self::buffered_with_reqwest_streaming(service, client)
    }

    /// Wraps an already-boxed transport stack with reqwest-only streaming.
    #[deprecated(note = "use `from_boxes`")]
    pub fn from_box(service: BoxHttpService, client: reqwest::Client) -> Self {
        Self::buffered_with_reqwest_streaming(service, client)
    }

    /// Returns a clone of the inner transport stack (for advanced testing).
    pub fn clone_inner(&self) -> BoxHttpService {
        self.inner
            .lock()
            .expect("ServiceBackend inner mutex poisoned")
            .clone()
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
        let mut service = self
            .streaming
            .lock()
            .expect("ServiceBackend streaming mutex poisoned")
            .clone();
        service
            .ready()
            .await
            .map_err(|e| Error::transport_message(format!("streaming service not ready: {e}")))?;
        service.call(request).await
    }
}
