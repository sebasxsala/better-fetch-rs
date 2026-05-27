//! Transport stack with Tower layers (feature `tower`).
//!
//! Run: `cargo run -p better-fetch --example tower_stack --features tower,json`
//!
//! [`ServiceBackend`] clones the boxed stack per request, so concurrent transport calls
//! run in parallel. Stack [`ConcurrencyLimitLayer`] to cap in-flight wire calls.
//!
//! Optional: wrap the reqwest inner service with [`tower::buffer::Buffer`] when the inner
//! service is not [`Clone`] or is expensive to clone (`Buffer::new` spawns a worker on
//! the Tokio runtime). For typical reqwest-backed stacks, `Buffer` is not required for
//! concurrency — see [`better_fetch::tower::stack::with_buffer`].

use std::time::Duration;

use better_fetch::backend::HttpRequest;
use better_fetch::tower::stack::{self, ConcurrencyLimitLayer, IntoBoxHttpService, ServiceBuilder};
use better_fetch::{ClientBuilder, Error, Result};
use tower::buffer::Buffer;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let service = stack::build(reqwest::Client::new(), |inner| {
        let buffered = Buffer::new(inner, 32);

        ServiceBuilder::new()
            .layer(ConcurrencyLimitLayer::new(8))
            .map_request(|req: HttpRequest| {
                tracing::debug!(url = %req.url, "transport");
                req
            })
            .map_err(|e: tower::BoxError| Error::transport_message(e.to_string()))
            .service(buffered)
            .into_box()
    });

    let client = ClientBuilder::new()
        .base_url("https://jsonplaceholder.typicode.com")?
        .http_service_boxed(service)
        .timeout(Duration::from_secs(30))
        .build()?;

    let body: serde_json::Value = client.get("/todos/1").send_json().await?;
    println!("{body}");

    Ok(())
}
