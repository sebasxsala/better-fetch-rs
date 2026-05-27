//! Tower transport integration (`tower` feature).
//!
//! Bridges [`tower::Service`] to [`HttpBackend`](crate::backend::HttpBackend).
//! Enable with `better-fetch` features `tower` and optionally `tower-http`.
//!
//! Custom stacks are wrapped by [`ServiceBackend`], which serializes transport calls with
//! a mutex. For production, build your stack with [`tower::buffer::Buffer`] and a spawned
//! worker on the Tokio runtime (see `examples/tower_stack`), and use [`stack`](stack) helpers
//! to add layers such
//! as [`ConcurrencyLimitLayer`](stack::ConcurrencyLimitLayer).

mod service;
pub mod stack;

#[cfg(feature = "tower-http")]
pub mod trace;

pub use service::{BoxHttpService, ReqwestHttpService, ServiceBackend};
