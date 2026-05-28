//! Tower transport integration (`tower` feature).
//!
//! Bridges [`tower::Service`] to [`HttpBackend`](crate::backend::HttpBackend).
//! Enable with `better-fetch` features `tower` and optionally `tower-http`.
//!
//! Custom stacks are wrapped by [`ServiceBackend`], which clones the boxed service per
//! request (brief lock only — [`BoxCloneService`] is not [`Sync`]) so concurrent transport
//! calls can run I/O in parallel. Use [`tower::buffer::Buffer`] in your stack when the inner
//! in your stack when the inner service is not [`Clone`] or is expensive to clone (see
//! `examples/tower_stack` and [`stack::with_buffer`](stack::with_buffer)). Use [`stack`](stack)
//! helpers to add layers such as [`ConcurrencyLimitLayer`](stack::ConcurrencyLimitLayer).

mod service;
pub mod stack;

#[cfg(feature = "tower-http")]
pub mod trace;

pub use service::{
    BoxHttpService, BoxStreamingHttpService, ReqwestHttpService, ReqwestStreamingHttpService,
    ServiceBackend,
};
