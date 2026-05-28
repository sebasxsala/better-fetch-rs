use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use better_fetch::backend::{HttpBackend, HttpRequest, HttpResponse, HttpStreamingResponse};
use better_fetch::{CancellationToken, Client, ClientBuilder, Error, Result};
use bytes::Bytes;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{Future, Stream};
use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn stream_to_file_writes_body() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/file"))
        .respond_with(ResponseTemplate::new(200).set_body_string("file-bytes"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let path = std::env::temp_dir().join(format!("better-fetch-{}.bin", uuid_simple()));
    let written = client
        .get("/file")
        .send_stream()
        .await?
        .stream_to_file(&path, None)
        .await?;
    assert_eq!(written, 10);
    let on_disk = tokio::fs::read(&path).await.unwrap();
    assert_eq!(on_disk, b"file-bytes");
    let _ = tokio::fs::remove_file(path).await;
    Ok(())
}

struct SlowChunks {
    state: u8,
}

impl Stream for SlowChunks {
    type Item = Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.state {
            0 => {
                self.state = 1;
                Poll::Ready(Some(Ok(Bytes::from_static(b"aaaa"))))
            }
            1 => {
                let mut sleep = Box::pin(tokio::time::sleep(Duration::from_millis(200)));
                if sleep.as_mut().poll(cx).is_ready() {
                    self.state = 2;
                    Poll::Ready(Some(Ok(Bytes::from_static(b"bbbb"))))
                } else {
                    Poll::Pending
                }
            }
            _ => Poll::Ready(None),
        }
    }
}

struct ChunkedStreamBackend;

#[async_trait]
impl HttpBackend for ChunkedStreamBackend {
    async fn execute(&self, _request: HttpRequest) -> Result<HttpResponse> {
        Err(Error::Config("use streaming".into()))
    }

    async fn execute_stream(&self, _request: HttpRequest) -> Result<HttpStreamingResponse> {
        Ok(HttpStreamingResponse {
            status: StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: Box::pin(SlowChunks { state: 0 }),
        })
    }
}

#[tokio::test]
async fn stream_to_file_respects_byte_limit() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/large"))
        .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(4096)))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let path = std::env::temp_dir().join(format!("better-fetch-limit-{}.bin", uuid_simple()));
    let err = client
        .get("/large")
        .send_stream()
        .await?
        .stream_to_file(&path, Some(512))
        .await
        .expect_err("over limit");
    let _ = tokio::fs::remove_file(&path).await;
    assert!(err.is_body_too_large());
    Ok(())
}

#[tokio::test]
async fn stream_to_file_cancelled_mid_write() -> Result<()> {
    let token = CancellationToken::new();
    let cancel = token.clone();

    let client = ClientBuilder::new()
        .base_url("http://localhost")?
        .backend(Arc::new(ChunkedStreamBackend))
        .build()?;

    let path = std::env::temp_dir().join(format!("better-fetch-cancel-{}.bin", uuid_simple()));
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancel.cancel();
    });

    let err = client
        .get("/file")
        .cancellation_token(token)
        .send_stream()
        .await?
        .stream_to_file(&path, None)
        .await
        .expect_err("cancelled mid write");

    let _ = tokio::fs::remove_file(&path).await;
    assert!(err.is_cancelled());
    Ok(())
}

fn uuid_simple() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}
