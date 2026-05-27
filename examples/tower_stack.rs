//! Transport stack with Tower layers (feature `tower`).
//!
//! Run: `cargo run -p better-fetch --example tower_stack --features tower,json`

use std::time::Duration;

use better_fetch::backend::HttpRequest;
use better_fetch::tower::stack::{self, ConcurrencyLimitLayer, IntoBoxHttpService, ServiceBuilder};
use better_fetch::{ClientBuilder, Result};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let service = stack::build(reqwest::Client::new(), |inner| {
        ServiceBuilder::new()
            .layer(ConcurrencyLimitLayer::new(8))
            .map_request(|req: HttpRequest| {
                tracing::debug!(url = %req.url, "transport");
                req
            })
            .service(inner)
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
