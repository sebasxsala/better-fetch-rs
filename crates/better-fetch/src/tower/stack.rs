//! Helpers for building transport stacks with [`ServiceBuilder`](tower::ServiceBuilder).

use crate::backend::{HttpRequest, HttpResponse};
use crate::Error;

use super::{BoxHttpService, ReqwestHttpService};

pub use tower::limit::{ConcurrencyLimitLayer, RateLimitLayer};
pub use tower::timeout::TimeoutLayer;
pub use tower::ServiceBuilder;

/// Creates a reqwest-backed inner service for layer stacking.
pub fn reqwest_service(client: reqwest::Client) -> ReqwestHttpService {
    ReqwestHttpService::new(client)
}

/// Builds a type-erased transport stack from a reqwest client and a configuration closure.
///
/// Pass the result to [`ClientBuilder::http_service_boxed`](crate::ClientBuilder::http_service_boxed)
/// or use [`ClientBuilder::transport_stack`](crate::ClientBuilder::transport_stack).
pub fn build<F>(client: reqwest::Client, configure: F) -> BoxHttpService
where
    F: FnOnce(ReqwestHttpService) -> BoxHttpService,
{
    configure(ReqwestHttpService::new(client))
}

/// Extension trait to box a configured service stack.
pub trait IntoBoxHttpService: Sized {
    /// Boxes `self` as [`BoxHttpService`].
    fn into_box(self) -> BoxHttpService;
}

impl<S> IntoBoxHttpService for S
where
    S: tower::Service<HttpRequest, Response = HttpResponse, Error = Error> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    fn into_box(self) -> BoxHttpService {
        BoxHttpService::new(self)
    }
}

/// Convenience: concurrency limit on the transport stack.
pub fn with_concurrency_limit(client: reqwest::Client, max_in_flight: usize) -> BoxHttpService {
    build(client, |inner| {
        ServiceBuilder::new()
            .layer(ConcurrencyLimitLayer::new(max_in_flight))
            .service(inner)
            .into_box()
    })
}

/// Wraps the reqwest inner service with [`tower::buffer::Buffer`](https://docs.rs/tower/latest/tower/buffer/struct.Buffer.html).
///
/// Use when the inner service is not [`Clone`] or cloning it is expensive. `Buffer::new`
/// spawns a worker on the Tokio runtime; lightweight `Buffer` clones enqueue work to that
/// worker. This is optional for typical reqwest-backed stacks — [`ServiceBackend`](crate::tower::ServiceBackend)
/// already clones the boxed stack per request.
pub fn with_buffer(client: reqwest::Client, capacity: usize) -> BoxHttpService {
    build(client, |inner| {
        let buffered = tower::buffer::Buffer::new(inner, capacity);
        ServiceBuilder::new()
            .map_err(|e: tower::BoxError| Error::transport_message(e.to_string()))
            .service(buffered)
            .into_box()
    })
}

/// Logs each transport call at `DEBUG` (wire-level; complements [`LoggerPlugin`](crate::LoggerPlugin)).
pub fn with_request_logging(client: reqwest::Client) -> BoxHttpService {
    build(client, |inner| {
        ServiceBuilder::new()
            .map_request(|req: HttpRequest| {
                tracing::debug!(method = %req.method, url = %req.url, "better-fetch transport");
                req
            })
            .service(inner)
            .into_box()
    })
}
