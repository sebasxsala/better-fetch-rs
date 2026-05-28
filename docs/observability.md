# Observability

## LoggerPlugin and tracing

[`LoggerPlugin`](../crates/better-fetch/src/plugins/logger.rs) records HTTP traffic with [`tracing`](https://docs.rs/tracing) spans:

- `http.request` — method, URL, retry attempt
- `http.response` — status, URL

Install a subscriber in your binary:

```rust
tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
```

## OpenTelemetry (`otel` feature)

Enable in your app `Cargo.toml`:

```toml
better-fetch = { version = "0.4", features = ["otel"] }
```

Your application also needs `tracing-subscriber` and an OTLP (or other) exporter crate. Build a tracer, then attach the OpenTelemetry layer:

```rust
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::TracerProvider;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

let provider = TracerProvider::builder().build();
let tracer = provider.tracer("my-app");

let subscriber = Registry::default().with(better_fetch::tracing_opentelemetry::layer().with_tracer(tracer));
tracing::subscriber::set_global_default(subscriber).unwrap();
```

[`LoggerPlugin`](../crates/better-fetch/src/plugins/logger.rs) spans are forwarded when this layer is active.

## miette (`miette` feature)

**Not a client plugin.** Enable the Cargo feature:

```toml
better-fetch = { features = ["miette"] }
```

Wrap errors at the call site (your binary needs `miette` and a reporter):

```rust
use better_fetch::DiagnosticError;

let err = client.get("/x").send().await.unwrap_err();
let diagnostic = DiagnosticError::new(err, Some(&http::Method::GET), None);
eprintln!("{:?}", miette::Report::new(diagnostic));
```
