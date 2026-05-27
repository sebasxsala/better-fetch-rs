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
pub fn build<F>(client: reqwest::Client, configure: F) -> BoxHttpService
where
    F: FnOnce(ReqwestHttpService) -> BoxHttpService,
{
    configure(ReqwestHttpService::new(client))
}

/// Extension trait to box a configured service stack.
pub trait IntoBoxHttpService: Sized {
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
