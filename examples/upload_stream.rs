//! Upload a large body via [`RequestBuilder::body_stream`] without buffering it entirely.
//!
//! ```bash
//! cargo run -p better-fetch --example upload_stream --features json
//! ```

use better_fetch::{BodyStream, ClientBuilder, Result};
use bytes::Bytes;
use futures_util::stream;

#[tokio::main]
async fn main() -> Result<()> {
    let client = ClientBuilder::new()
        .base_url("https://httpbin.org")?
        .build()?;

    let chunks: Vec<std::result::Result<Bytes, better_fetch::Error>> = (0..4)
        .map(|i| Ok(Bytes::from(format!("chunk-{i}"))))
        .collect();

    let response = client
        .put("/put")
        .header("content-type", "application/octet-stream")?
        .body_stream(Box::pin(stream::iter(chunks)) as BodyStream)
        .send()
        .await?;

    println!("status = {}", response.status());
    Ok(())
}
