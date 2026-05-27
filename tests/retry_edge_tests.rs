use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use better_fetch::{parse_retry_after, Client, ClientBuilder, RetryPolicy, Result};
use http::{HeaderMap, StatusCode};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn respects_retry_after_header_over_policy_delay() -> Result<()> {
    let server = MockServer::start().await;
    let counter = Arc::new(AtomicU32::new(0));
    let counter_c = counter.clone();

    Mock::given(method("GET"))
        .and(path("/rate"))
        .respond_with(move |_: &wiremock::Request| {
            let n = counter_c.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(429).insert_header("retry-after", "1")
            } else {
                ResponseTemplate::new(200).set_body_string("ok")
            }
        })
        .mount(&server)
        .await;

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .retry(RetryPolicy::linear(2, Duration::from_secs(30)).with_jitter(false))
        .build()?;

    let start = Instant::now();
    assert!(client.get("/rate").send().await?.is_success());
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "expected Retry-After ~1s, got {:?}",
        elapsed
    );
    Ok(())
}

#[tokio::test]
async fn retries_408_request_timeout() -> Result<()> {
    let server = MockServer::start().await;
    let counter = Arc::new(AtomicU32::new(0));
    let counter_c = counter.clone();

    Mock::given(method("GET"))
        .and(path("/timeout"))
        .respond_with(move |_: &wiremock::Request| {
            let n = counter_c.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(408)
            } else {
                ResponseTemplate::new(200).set_body_string("ok")
            }
        })
        .mount(&server)
        .await;

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .retry(RetryPolicy::count(2))
        .build()?;

    assert!(client.get("/timeout").send().await?.is_success());
    assert!(counter.load(Ordering::SeqCst) >= 2);
    Ok(())
}

#[test]
fn parse_retry_after_rejects_http_date() {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::RETRY_AFTER,
        "Wed, 21 Oct 2015 07:28:00 GMT".parse().unwrap(),
    );
    assert!(parse_retry_after(&headers).is_none());
}

#[test]
fn parse_retry_after_zero_seconds() {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::RETRY_AFTER, "0".parse().unwrap());
    assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(0)));
}
