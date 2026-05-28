//! Tower middleware runs on both `send()` and `send_stream()` when using [`ClientBuilder::transport_stack`].
//!
//! ```bash
//! cargo run -p better-fetch --example tower_vs_streaming --features tower,json
//! ```

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use better_fetch::backend::HttpRequest;
use better_fetch::tower::stack::{
    ConcurrencyLimitLayer, IntoBoxHttpService, IntoBoxStreamingHttpService, ServiceBuilder,
};
use better_fetch::{ClientBuilder, Error, Hooks, Result};
use tower::buffer::Buffer;

static TRANSPORT_HITS: AtomicU32 = AtomicU32::new(0);

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let client = ClientBuilder::new()
        .base_url("https://httpbin.org")?
        .transport_stack(|buffered, streaming| {
            let map_hit = |req: HttpRequest| {
                TRANSPORT_HITS.fetch_add(1, Ordering::SeqCst);
                req
            };
            (
                ServiceBuilder::new()
                    .layer(ConcurrencyLimitLayer::new(4))
                    .map_request(map_hit)
                    .map_err(|e: tower::BoxError| Error::transport_message(e.to_string()))
                    .service(Buffer::new(buffered, 8))
                    .into_box(),
                ServiceBuilder::new()
                    .layer(ConcurrencyLimitLayer::new(4))
                    .map_request(map_hit)
                    .map_err(|e: tower::BoxError| Error::transport_message(e.to_string()))
                    .service(Buffer::new(streaming, 8))
                    .into_streaming_box(),
            )
        })
        .hooks(Hooks::new().on_response_stream(|ctx| async move {
            tracing::info!(status = %ctx.status, "on_response_stream");
            Ok(better_fetch::StreamingResponseMeta {
                status: ctx.status,
                headers: ctx.headers,
            })
        }))
        .timeout(Duration::from_secs(30))
        .build()?;

    let before = TRANSPORT_HITS.load(Ordering::SeqCst);
    let _: serde_json::Value = client.get("/json").send_json().await?;
    assert!(TRANSPORT_HITS.load(Ordering::SeqCst) > before);

    let before_stream = TRANSPORT_HITS.load(Ordering::SeqCst);
    let mut stream = client.get("/stream-bytes/512").send_stream().await?;
    use futures_util::StreamExt;
    while stream.bytes_stream().next().await.transpose()?.is_some() {}
    assert!(
        TRANSPORT_HITS.load(Ordering::SeqCst) > before_stream,
        "Tower map_request should run on send_stream too"
    );

    println!("tower_vs_streaming: Tower applies to buffered and streaming paths");
    Ok(())
}
