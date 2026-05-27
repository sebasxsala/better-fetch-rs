//! Standalone Tower extension crate for [better-fetch](https://docs.rs/better-fetch).
//!
//! Re-exports the `tower` module from `better-fetch` with the `tower` feature enabled.
//! Depend on this crate if you want an explicit transport-extension dependency without
//! enabling `better-fetch`'s `tower` feature on the main crate.

pub use better_fetch::tower::*;

pub mod stack {
    pub use better_fetch::tower::stack::*;
}

#[cfg(feature = "tower-http")]
pub mod trace {
    pub use better_fetch::tower::trace::*;
}
