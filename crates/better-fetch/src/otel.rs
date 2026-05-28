//! OpenTelemetry re-exports when the `otel` feature is enabled.
//!
//! better-fetch does not install a global subscriber or OTLP exporter. In your application,
//! build a tracer (for example via [`TracerProvider`](opentelemetry_sdk::trace::TracerProvider)),
//! then attach [`tracing_opentelemetry::layer`] to a [`tracing_subscriber`](https://docs.rs/tracing-subscriber) registry.
//!
//! ```ignore
//! use opentelemetry::trace::TracerProvider as _;
//! use opentelemetry_sdk::trace::TracerProvider;
//! use tracing_subscriber::layer::SubscriberExt;
//! use tracing_subscriber::Registry;
//!
//! let tracer = TracerProvider::builder().build().tracer("my-app");
//! let subscriber = Registry::default().with(tracing_opentelemetry::layer().with_tracer(tracer));
//! ```

/// OpenTelemetry API (tracers, spans, propagation).
pub use opentelemetry;
/// SDK (`TracerProvider`, exporters, resources).
pub use opentelemetry_sdk;
/// Bridge from [`tracing`](https://docs.rs/tracing) spans to OpenTelemetry.
pub use tracing_opentelemetry;
