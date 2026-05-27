//! Tower transport integration (`tower` feature).
//!
//! Bridges [`tower::Service`] to [`HttpBackend`](crate::backend::HttpBackend).
//! Enable with `better-fetch` features `tower` and optionally `tower-http`.

mod service;
pub mod stack;

#[cfg(feature = "tower-http")]
pub mod trace;

pub use service::{BoxHttpService, ReqwestHttpService, ServiceBackend};
