//! Streaming response bodies with [`RequestBuilder::send_stream`].
//!
//! Run: `cargo run -p better-fetch --example streaming`

use better_fetch::{Client, ClientBuilder, Result};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let client = Client::new("https://httpbin.org")?;

    // Incremental download: process each chunk without buffering the full body.
    let mut stream = client.get("/stream-bytes/4096").send_stream().await?;
    let mut total = 0usize;
    while let Some(chunk) = stream.bytes_stream().next().await {
        let chunk = chunk?;
        total += chunk.len();
        tracing::info!(bytes = chunk.len(), total, "chunk received");
    }
    tracing::info!(total, "download complete");

    // Optional size cap (per-request or via ClientBuilder::max_response_bytes).
    let capped = ClientBuilder::new()
        .base_url("https://httpbin.org")?
        .max_response_bytes(512)
        .build()?;

    match capped.get("/bytes/2048").send_stream().await {
        Ok(mut response) => {
            if let Some(Err(err)) = response.bytes_stream().next().await {
                if err.is_body_too_large() {
                    tracing::warn!(
                        limit = err.body_too_large_limit(),
                        "response exceeded max_response_bytes"
                    );
                } else {
                    return Err(err);
                }
            }
        }
        Err(err) => return Err(err),
    }

    // Buffer at the end when you want the familiar Response API (e.g. JSON).
    let buffered = client.get("/json").send_stream().await?.collect().await?;
    let value: serde_json::Value = buffered.json().await?;
    tracing::info!(?value, "collected JSON");

    Ok(())
}
