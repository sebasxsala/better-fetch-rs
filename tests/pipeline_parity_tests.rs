//! Buffered vs streaming paths share the same retry attempt count.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use better_fetch::{
    ClientBuilder, HttpBackend, HttpRequest, HttpResponse, HttpStreamingResponse, Result,
    RetryPolicy,
};
use bytes::Bytes;
use futures_util::stream;
use http::StatusCode;

struct CountingBackend {
    calls: Arc<AtomicU32>,
}

#[async_trait]
impl HttpBackend for CountingBackend {
    async fn execute(&self, _req: HttpRequest) -> Result<HttpResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(HttpResponse {
            status: StatusCode::SERVICE_UNAVAILABLE,
            headers: Default::default(),
            body: Bytes::new(),
        })
    }

    async fn execute_stream(&self, _req: HttpRequest) -> Result<HttpStreamingResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(HttpStreamingResponse {
            status: StatusCode::SERVICE_UNAVAILABLE,
            headers: Default::default(),
            body: Box::pin(stream::once(async { Ok(Bytes::from_static(b"err")) })),
        })
    }
}

#[tokio::test]
async fn buffered_and_streaming_retry_same_attempt_count() -> Result<()> {
    let calls_buffered = Arc::new(AtomicU32::new(0));
    let client_buffered = ClientBuilder::new()
        .base_url("https://example.com")?
        .backend(Arc::new(CountingBackend {
            calls: calls_buffered.clone(),
        }))
        .retry(RetryPolicy::count(2))
        .build()?;

    let response = client_buffered.get("/retry").send().await?;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let calls_stream = Arc::new(AtomicU32::new(0));
    let client_stream = ClientBuilder::new()
        .base_url("https://example.com")?
        .backend(Arc::new(CountingBackend {
            calls: calls_stream.clone(),
        }))
        .retry(RetryPolicy::count(2))
        .build()?;

    let stream = client_stream.get("/retry").send_stream().await?;
    assert_eq!(stream.status(), StatusCode::SERVICE_UNAVAILABLE);

    assert_eq!(
        calls_buffered.load(Ordering::SeqCst),
        calls_stream.load(Ordering::SeqCst),
        "buffered and streaming should perform the same number of transport attempts"
    );
    assert_eq!(calls_buffered.load(Ordering::SeqCst), 3);
    Ok(())
}

#[tokio::test]
async fn buffered_and_streaming_throw_on_error_same_body() -> Result<()> {
    use better_fetch::{Client, Error};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/gone"))
        .respond_with(ResponseTemplate::new(404).set_body_string("parity-body"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;

    let buffered_err = client
        .get("/gone")
        .throw_on_error(true)
        .send()
        .await
        .unwrap_err();
    let stream_err = client
        .get("/gone")
        .throw_on_error(true)
        .send_stream()
        .await
        .unwrap_err();

    let body = |err: &Error| match err {
        Error::Http { body: Some(b), .. } => std::str::from_utf8(b).unwrap().to_string(),
        other => panic!("expected Http with body, got {other:?}"),
    };
    assert_eq!(body(&buffered_err), "parity-body");
    assert_eq!(body(&stream_err), "parity-body");
    Ok(())
}
