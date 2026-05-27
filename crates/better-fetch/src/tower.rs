//! Tower transport integration (`tower` feature).
//!
//! Bridges [`tower::Service`] to [`HttpBackend`](crate::backend::HttpBackend).
//! See also the [`better-fetch-tower`](https://docs.rs/better-fetch-tower) crate for a
//! standalone dependency on this module.

mod service;
pub mod stack;

#[cfg(feature = "tower-http")]
pub mod trace;

pub use service::{BoxHttpService, ReqwestHttpService, ServiceBackend};
