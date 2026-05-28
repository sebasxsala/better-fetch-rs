//! Cooperative cancellation with [`CancellationToken`](https://docs.rs/better-fetch/latest/better_fetch/struct.CancellationToken.html).
//!
//! Cancellation is checked before transport I/O and during retry backoff sleeps.
//! The second scenario uses a retry policy and cancels while waiting between attempts.
//!
//! ```bash
//! cargo run -p better-fetch --example cancel --features json
//! ```

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use better_fetch::backend::{HttpBackend, HttpRequest, HttpResponse, HttpStreamingResponse};
use better_fetch::{CancellationToken, ClientBuilder, Error, Result, RetryPolicy};
use bytes::Bytes;
use http::StatusCode;

struct SlowBackend;

#[async_trait]
impl HttpBackend for SlowBackend {
    async fn execute(&self, _req: HttpRequest) -> Result<HttpResponse> {
        tokio::time::sleep(Duration::from_secs(5)).await;
        Ok(HttpResponse {
            status: StatusCode::OK,
            headers: Default::default(),
            body: Bytes::from_static(b"ok"),
        })
    }

    async fn execute_stream(&self, _req: HttpRequest) -> Result<HttpStreamingResponse> {
        Err(Error::Config("not used".into()))
    }
}

struct Flaky503 {
    hits: Arc<AtomicU32>,
}

#[async_trait]
impl HttpBackend for Flaky503 {
    async fn execute(&self, _req: HttpRequest) -> Result<HttpResponse> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        Ok(HttpResponse {
            status: StatusCode::SERVICE_UNAVAILABLE,
            headers: Default::default(),
            body: Bytes::new(),
        })
    }

    async fn execute_stream(&self, _req: HttpRequest) -> Result<HttpStreamingResponse> {
        Err(Error::Config("not used".into()))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 1) Cancel during a slow request
    let token = CancellationToken::new();
    let cancel = token.clone();
    let client = ClientBuilder::new()
        .base_url("https://example.com")?
        .backend(Arc::new(SlowBackend))
        .build()?;

    let task =
        tokio::spawn(async move { client.get("/slow").cancellation_token(token).send().await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();
    assert!(task.await.unwrap().unwrap_err().is_cancelled());

    // 2) Cancel during retry backoff
    let token = CancellationToken::new();
    let cancel = token.clone();
    let client = ClientBuilder::new()
        .base_url("https://example.com")?
        .backend(Arc::new(Flaky503 {
            hits: Arc::new(AtomicU32::new(0)),
        }))
        .retry(RetryPolicy::count(5))
        .build()?;

    let task =
        tokio::spawn(async move { client.get("/flaky").cancellation_token(token).send().await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();
    assert!(task.await.unwrap().unwrap_err().is_cancelled());

    println!("cancel works during request and retry backoff");
    Ok(())
}
