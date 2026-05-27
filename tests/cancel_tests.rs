use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use better_fetch::backend::{HttpBackend, HttpRequest, HttpResponse, HttpStreamingResponse};
use better_fetch::{CancellationToken, Client, ClientBuilder, Error, Result};
use bytes::Bytes;
use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct SlowBackend {
    hits: Arc<AtomicU32>,
    delay: Duration,
}

#[async_trait]
impl HttpBackend for SlowBackend {
    async fn execute(&self, _request: HttpRequest) -> Result<HttpResponse> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
        Ok(HttpResponse {
            status: StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: Bytes::from_static(b"ok"),
        })
    }

    async fn execute_stream(&self, _request: HttpRequest) -> Result<HttpStreamingResponse> {
        Err(Error::Other(
            "streaming not supported in SlowBackend".into(),
        ))
    }
}

#[tokio::test]
async fn cancellation_during_request_returns_cancelled() -> Result<()> {
    let token = CancellationToken::new();
    let cancel = token.clone();
    let backend = Arc::new(SlowBackend {
        hits: Arc::new(AtomicU32::new(0)),
        delay: Duration::from_secs(5),
    });

    let client = ClientBuilder::new()
        .base_url("http://localhost")?
        .backend(backend.clone())
        .build()?;

    let task =
        tokio::spawn(async move { client.get("/slow").cancellation_token(token).send().await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();

    let err = task.await.unwrap().unwrap_err();
    assert!(err.is_cancelled());
    assert_eq!(backend.hits.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn cancellation_during_retry_sleep_returns_cancelled() -> Result<()> {
    let server = MockServer::start().await;
    let counter = Arc::new(AtomicU32::new(0));
    let counter_c = counter.clone();

    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(move |_: &wiremock::Request| {
            counter_c.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(503)
        })
        .mount(&server)
        .await;

    let token = CancellationToken::new();
    let cancel = token.clone();

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .retry(better_fetch::RetryPolicy::linear(
            5,
            Duration::from_secs(10),
        ))
        .build()?;

    let task =
        tokio::spawn(async move { client.get("/flaky").cancellation_token(token).send().await });

    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.cancel();

    let err = task.await.unwrap().unwrap_err();
    assert!(err.is_cancelled());
    assert!(counter.load(Ordering::SeqCst) >= 1);
    Ok(())
}

#[tokio::test]
async fn completed_request_before_cancel_succeeds() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fast"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let token = CancellationToken::new();
    let client = Client::new(server.uri())?;
    assert!(client
        .get("/fast")
        .cancellation_token(token)
        .send()
        .await?
        .is_success());
    Ok(())
}
