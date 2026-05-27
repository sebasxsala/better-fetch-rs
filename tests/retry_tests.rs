use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use better_fetch::backend::{HttpBackend, HttpRequest, HttpResponse};
use better_fetch::{default_should_retry, Client, ClientBuilder, Error, Result, RetryPolicy};
use bytes::Bytes;
use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn retries_on_503_then_succeeds() -> Result<()> {
    let server = MockServer::start().await;
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();

    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(move |_: &wiremock::Request| {
            let n = counter_clone.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                ResponseTemplate::new(503)
            } else {
                ResponseTemplate::new(200).set_body_string("ok")
            }
        })
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .retry(RetryPolicy::linear(3, Duration::from_millis(10)))
        .build()?;

    assert!(client.get("/flaky").send().await?.is_success());
    assert!(counter.load(Ordering::SeqCst) >= 3);
    Ok(())
}

#[tokio::test]
async fn retries_on_429() -> Result<()> {
    let server = MockServer::start().await;
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();

    Mock::given(method("GET"))
        .and(path("/rate"))
        .respond_with(move |_: &wiremock::Request| {
            let n = counter_clone.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(429)
            } else {
                ResponseTemplate::new(200).set_body_string("ok")
            }
        })
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .retry(RetryPolicy::count(2))
        .build()?;

    assert!(client.get("/rate").send().await?.is_success());
    Ok(())
}

#[tokio::test]
async fn does_not_retry_404_by_default() -> Result<()> {
    let server = MockServer::start().await;
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();

    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(move |_: &wiremock::Request| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(404)
        })
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .retry(RetryPolicy::count(3))
        .build()?;

    assert_eq!(client.get("/missing").send().await?.status(), 404);
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn exponential_retry_eventually_succeeds() -> Result<()> {
    let server = MockServer::start().await;
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();

    Mock::given(method("GET"))
        .and(path("/exp"))
        .respond_with(move |_: &wiremock::Request| {
            let n = counter_clone.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                ResponseTemplate::new(502)
            } else {
                ResponseTemplate::new(200).set_body_string("ok")
            }
        })
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .retry(RetryPolicy::exponential(
            4,
            Duration::from_millis(5),
            Duration::from_millis(50),
        ))
        .build()?;

    assert!(client.get("/exp").send().await?.is_success());
    Ok(())
}

#[tokio::test]
async fn custom_should_retry_only_404() -> Result<()> {
    let server = MockServer::start().await;
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();

    Mock::given(method("GET"))
        .and(path("/custom"))
        .respond_with(move |_: &wiremock::Request| {
            let n = counter_clone.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(404)
            } else {
                ResponseTemplate::new(200).set_body_string("ok")
            }
        })
        .mount(&server)
        .await;

    let policy = RetryPolicy::linear(2, Duration::from_millis(5))
        .with_should_retry(Arc::new(|r| r.status() == StatusCode::NOT_FOUND));

    let client = Client::builder()
        .base_url(server.uri())?
        .retry(policy)
        .build()?;

    assert!(client.get("/custom").send().await?.is_success());
    assert!(counter.load(Ordering::SeqCst) >= 2);
    Ok(())
}

#[test]
fn default_should_retry_unit() {
    assert!(default_should_retry(StatusCode::SERVICE_UNAVAILABLE));
    assert!(!default_should_retry(StatusCode::BAD_REQUEST));
}

struct FlakyBackend {
    counter: Arc<AtomicU32>,
    fail_until: u32,
}

#[async_trait]
impl HttpBackend for FlakyBackend {
    async fn execute(&self, _request: HttpRequest) -> Result<HttpResponse> {
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        if n < self.fail_until {
            Err(Error::Transport("simulated transport failure".into()))
        } else {
            Ok(HttpResponse {
                status: StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: Bytes::from_static(b"ok"),
            })
        }
    }
}

struct AlwaysFailBackend;

#[async_trait]
impl HttpBackend for AlwaysFailBackend {
    async fn execute(&self, _request: HttpRequest) -> Result<HttpResponse> {
        Err(Error::Transport("always fails".into()))
    }
}

#[tokio::test]
async fn retries_on_transport_error_then_succeeds() -> Result<()> {
    let counter = Arc::new(AtomicU32::new(0));
    let backend = Arc::new(FlakyBackend {
        counter: counter.clone(),
        fail_until: 2,
    });

    let client = ClientBuilder::new()
        .base_url("http://localhost")?
        .backend(backend)
        .retry(RetryPolicy::linear(3, Duration::from_millis(1)))
        .build()?;

    assert!(client.get("/any").send().await?.is_success());
    assert_eq!(counter.load(Ordering::SeqCst), 3);
    Ok(())
}

#[tokio::test]
async fn transport_retry_exhausted_returns_retry_exhausted_error() -> Result<()> {
    let backend = Arc::new(AlwaysFailBackend);
    let client = ClientBuilder::new()
        .base_url("http://localhost")?
        .backend(backend)
        .retry(RetryPolicy::count(2))
        .build()?;

    let err = client.get("/any").send().await.unwrap_err();
    assert!(err.is_retry_exhausted());
    Ok(())
}
