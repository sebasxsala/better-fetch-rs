//! Buffered `send()` vs incremental `send_stream()` — when to use each.
//!
//! - **`send()`** — full body in memory; `on_response` / `on_success` hooks; Tower middleware on `transport_stack`.
//! - **`send_stream()`** — chunked reads; `on_response_stream` hooks; same retry policy; with [`ClientBuilder::transport_stack`](https://docs.rs/better-fetch/latest/better_fetch/struct.ClientBuilder.html#method.transport_stack), Tower middleware runs on **both** paths when you wire the streaming stack.
//!
//! ```bash
//! cargo run -p better-fetch --example buffered_vs_streaming --features json
//! ```

use better_fetch::{Client, Result};
use futures_util::StreamExt;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::new("https://httpbin.org")?;

    // Buffered: one shot JSON
    let value: Value = client.get("/json").send_json().await?;
    println!(
        "buffered send_json keys: {:?}",
        value.as_object().map(|m| m.len())
    );

    // Streaming: process chunks (collect() to buffer when needed)
    let mut stream = client.get("/stream-bytes/1024").send_stream().await?;
    let mut total = 0usize;
    while let Some(chunk) = stream.bytes_stream().next().await {
        total += chunk?.len();
    }
    println!("streaming chunks total: {total} bytes");

    Ok(())
}
