//! Built-in plugins.
//!
//! [`LoggerPlugin`] emits tracing events for requests, responses, retries, and errors.
//! Your application must install a `tracing` subscriber to see output.

pub mod logger;

pub use logger::LoggerPlugin;
