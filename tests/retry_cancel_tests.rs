//! P0 T5: cancellation during retry backoff.

#[path = "support/mod.rs"]
mod support;

use std::sync::atomic::Ordering;
use std::time::Duration;

use better_fetch::{CancellationToken, ClientBuilder, Error, Result, RetryPolicy};

#[tokio::test]
async fn cancellation_during_retry_backoff_returns_cancelled() -> Result<()> {
    let token = CancellationToken::new();
    let cancel = token.clone();
    let (backend, hits) = support::flaky_503_backend();

    let client = ClientBuilder::new()
        .base_url("http://localhost")?
        .backend(backend)
        .retry(RetryPolicy::count(1))
        .build()?;

    let task =
        tokio::spawn(async move { client.get("/flaky").cancellation_token(token).send().await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();

    let err = task.await.unwrap().unwrap_err();
    assert!(matches!(err, Error::Cancelled));
    assert!(hits.load(Ordering::SeqCst) >= 1);
    Ok(())
}

#[tokio::test]
async fn cancellation_during_stream_retry_backoff_returns_cancelled() -> Result<()> {
    let token = CancellationToken::new();
    let cancel = token.clone();
    let (backend, hits) = support::flaky_503_backend();

    let client = ClientBuilder::new()
        .base_url("http://localhost")?
        .backend(backend)
        .retry(RetryPolicy::count(1))
        .build()?;

    let request = async {
        client
            .get("/flaky")
            .cancellation_token(token)
            .send_stream()
            .await
    };
    let fire_cancel = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
    };

    let (result, ()) = tokio::join!(request, fire_cancel);
    let err = result.unwrap_err();
    assert!(matches!(err, Error::Cancelled));
    assert!(hits.load(Ordering::SeqCst) >= 1);
    Ok(())
}
