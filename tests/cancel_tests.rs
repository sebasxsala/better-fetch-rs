#[path = "support/mod.rs"]
mod support;

use std::sync::atomic::Ordering;
use std::time::Duration;

use better_fetch::{CancellationToken, ClientBuilder, Result};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn cancellation_during_request_returns_cancelled() -> Result<()> {
    let token = CancellationToken::new();
    let cancel = token.clone();
    let (backend, hits) = support::slow_backend(Duration::from_secs(5));

    let client = ClientBuilder::new()
        .base_url("http://localhost")?
        .backend(backend)
        .build()?;

    let task =
        tokio::spawn(async move { client.get("/slow").cancellation_token(token).send().await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();

    let err = task.await.unwrap().unwrap_err();
    assert!(err.is_cancelled());
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn cancellation_during_retry_sleep_returns_cancelled() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let token = CancellationToken::new();
    let cancel = token.clone();

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .retry(better_fetch::RetryPolicy::count(2))
        .build()?;

    let task =
        tokio::spawn(async move { client.get("/flaky").cancellation_token(token).send().await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();

    let err = task.await.unwrap().unwrap_err();
    assert!(err.is_cancelled());
    Ok(())
}
