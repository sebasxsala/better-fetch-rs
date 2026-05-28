//! P0 T6: retry delays are applied between attempts (loose bounds).

use std::time::{Duration, Instant};

use better_fetch::{Client, Result, RetryPolicy};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn linear_retry_waits_between_attempts() -> Result<()> {
    let server = MockServer::start().await;
    let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let counter = attempts.clone();

    Mock::given(method("GET"))
        .and(path("/retry"))
        .respond_with(move |_: &wiremock::Request| {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
        .retry(RetryPolicy::linear(2, Duration::from_millis(200)))
        .build()?;

    let start = Instant::now();
    let body = client.get("/retry").send().await?.into_text()?;
    let elapsed = start.elapsed();

    assert_eq!(body, "ok");
    assert!(attempts.load(std::sync::atomic::Ordering::SeqCst) >= 3);
    assert!(
        elapsed >= Duration::from_millis(350),
        "expected at least ~400ms delay, got {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "retry took too long: {elapsed:?}"
    );
    Ok(())
}
