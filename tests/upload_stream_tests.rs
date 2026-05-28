use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use better_fetch::backend::{
    HttpBackend, HttpBody, HttpRequest, HttpResponse, HttpStreamingResponse,
};
use better_fetch::{ClientBuilder, Error, Result};
use bytes::Bytes;
use futures_util::{stream, StreamExt};
use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct CountingBackend {
    received: Arc<AtomicUsize>,
}

#[async_trait]
impl HttpBackend for CountingBackend {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse> {
        let size = match request.body {
            HttpBody::Bytes(b) => b.len(),
            HttpBody::Stream(mut s) => {
                let mut total = 0usize;
                while let Some(chunk) = s.next().await.transpose()? {
                    total += chunk.len();
                }
                total
            }
            HttpBody::Empty => 0,
        };
        self.received.fetch_add(size, Ordering::SeqCst);
        Ok(HttpResponse {
            status: StatusCode::OK,
            headers: Default::default(),
            body: Bytes::from_static(b"ok"),
        })
    }

    async fn execute_stream(&self, _request: HttpRequest) -> Result<HttpStreamingResponse> {
        Err(Error::Config("streaming not supported".into()))
    }
}

#[tokio::test]
async fn upload_stream_counts_bytes_without_buffering_full_body() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/upload"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let received = Arc::new(AtomicUsize::new(0));
    let backend = Arc::new(CountingBackend {
        received: Arc::clone(&received),
    });

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .backend(backend)
        .build()?;

    let payload: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
    let len = payload.len();
    let stream: better_fetch::BodyStream = Box::pin(stream::iter(vec![Ok(Bytes::from(payload))]));
    client.put("/upload").body_stream(stream).send().await?;

    assert_eq!(received.load(Ordering::SeqCst), len);
    Ok(())
}

#[tokio::test]
async fn stream_body_with_retry_returns_non_replayable() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/stream-retry"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .retry(better_fetch::RetryPolicy::count(1))
        .build()?;

    let stream: better_fetch::BodyStream =
        Box::pin(stream::iter(vec![Ok(Bytes::from_static(b"x"))]));
    let err = client
        .post("/stream-retry")
        .body_stream(stream)
        .send()
        .await
        .unwrap_err();

    assert!(matches!(err, Error::NonReplayableBody));
    Ok(())
}

#[tokio::test]
async fn upload_stream_cancelled_mid_request() -> Result<()> {
    use better_fetch::CancellationToken;
    use futures_util::stream;
    use std::time::Duration;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/upload-cancel"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("ok")
                .set_delay(Duration::from_secs(2)),
        )
        .mount(&server)
        .await;

    let token = CancellationToken::new();
    let client = ClientBuilder::new().base_url(server.uri())?.build()?;
    let body: better_fetch::BodyStream = Box::pin(stream::iter(vec![
        Ok(Bytes::from_static(b"a")),
        Ok(Bytes::from_static(b"b")),
    ]));

    let handle = tokio::spawn({
        let token = token.clone();
        let client = client.clone();
        async move {
            client
                .post("/upload-cancel")
                .body_stream(body)
                .cancellation_token(token)
                .send()
                .await
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    token.cancel();

    let err = handle.await.unwrap().unwrap_err();
    assert!(err.is_cancelled());
    Ok(())
}
