use async_trait::async_trait;
use better_fetch::backend::{HttpBackend, HttpRequest, HttpResponse, HttpStreamingResponse};
use better_fetch::{Client, ClientBuilder, Error, Result, RetryPolicy};
use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use http::StatusCode;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn streams_body_in_chunks_without_collect() -> Result<()> {
    let server = MockServer::start().await;
    let body = "a".repeat(8_192);
    Mock::given(method("GET"))
        .and(path("/large"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body.clone()))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let mut response = client.get("/large").send_stream().await?;
    let mut total = 0usize;
    while let Some(chunk) = response.bytes_stream().next().await {
        total += chunk?.len();
    }
    assert_eq!(total, body.len());
    Ok(())
}

#[tokio::test]
async fn collect_roundtrip_matches_buffered_body() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"n":42}"#))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let buffered = client.get("/json").send_stream().await?.collect().await?;
    assert_eq!(
        buffered.into_bytes_checked()?,
        Bytes::from_static(br#"{"n":42}"#)
    );
    Ok(())
}

#[tokio::test]
async fn max_response_bytes_returns_body_too_large() -> Result<()> {
    let server = MockServer::start().await;
    let body = "x".repeat(2048);
    Mock::given(method("GET"))
        .and(path("/big"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let mut response = client
        .get("/big")
        .max_response_bytes(1024)
        .send_stream()
        .await?;

    let err = response
        .bytes_stream()
        .next()
        .await
        .expect("at least one chunk")
        .expect_err("should exceed limit");
    assert!(err.is_body_too_large());
    assert_eq!(err.body_too_large_limit(), Some(1024));

    // Must not spin on repeated BodyTooLarge errors.
    assert!(response.bytes_stream().next().await.is_none());
    Ok(())
}

#[tokio::test]
async fn max_response_bytes_allows_body_under_limit() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/small"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let response = client
        .get("/small")
        .max_response_bytes(1024)
        .send_stream()
        .await?;
    let bytes = response.collect().await?.into_bytes_checked()?;
    assert_eq!(bytes, Bytes::from_static(b"ok"));
    Ok(())
}

struct ChunkedSlowBackend;

#[async_trait]
impl HttpBackend for ChunkedSlowBackend {
    async fn execute(&self, _request: HttpRequest) -> Result<HttpResponse> {
        Ok(HttpResponse {
            status: StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: Bytes::from_static(b"ok"),
        })
    }

    async fn execute_stream(&self, request: HttpRequest) -> Result<HttpStreamingResponse> {
        let _ = request;
        let body: better_fetch::BodyStream = Box::pin(futures_util::stream::iter(vec![
            Ok(Bytes::from_static(b"chunk1")),
            Ok(Bytes::from_static(b"chunk2")),
            Ok(Bytes::from_static(b"chunk3")),
        ]));
        Ok(HttpStreamingResponse {
            status: StatusCode::OK,
            headers: http::HeaderMap::new(),
            body,
        })
    }
}

#[tokio::test]
async fn cancellation_mid_stream_returns_cancelled() -> Result<()> {
    let client = ClientBuilder::new()
        .base_url("http://localhost")?
        .backend(Arc::new(ChunkedSlowBackend))
        .build()?;

    let token = better_fetch::CancellationToken::new();
    let cancel = token.clone();
    let mut response = client
        .get("/any")
        .cancellation_token(token)
        .send_stream()
        .await?;

    let first = response.bytes_stream().next().await.unwrap()?;
    assert_eq!(first, Bytes::from_static(b"chunk1"));

    cancel.cancel();

    let err = response
        .bytes_stream()
        .next()
        .await
        .expect("second chunk")
        .expect_err("should cancel");
    assert!(err.is_cancelled());
    Ok(())
}

#[tokio::test]
async fn retries_on_503_before_reading_body() -> Result<()> {
    let server = MockServer::start().await;
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_cb = attempts.clone();

    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(move |_: &wiremock::Request| {
            let n = attempts_cb.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(503).set_body_string("fail")
            } else {
                ResponseTemplate::new(200).set_body_string("ok")
            }
        })
        .mount(&server)
        .await;

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .retry(RetryPolicy::count(1))
        .build()?;

    let response = client.get("/flaky").send_stream().await?;
    let body = response.collect().await?.into_bytes_checked()?;
    assert_eq!(body, Bytes::from_static(b"ok"));
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn throw_on_error_before_body_read() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/missing")
        .throw_on_error(true)
        .send_stream()
        .await
        .expect_err("404 should error");
    assert_eq!(err.status(), Some(StatusCode::NOT_FOUND));
    assert!(err.body().is_none());
    Ok(())
}

struct BufferedOnlyBackend;

#[async_trait]
impl HttpBackend for BufferedOnlyBackend {
    async fn execute(&self, _request: HttpRequest) -> Result<HttpResponse> {
        Ok(HttpResponse {
            status: StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: Bytes::from_static(b"ok"),
        })
    }

    async fn execute_stream(&self, _request: HttpRequest) -> Result<HttpStreamingResponse> {
        Err(Error::Other(
            "streaming not supported in BufferedOnlyBackend".into(),
        ))
    }
}

#[tokio::test]
async fn on_response_stream_can_mutate_headers() -> Result<()> {
    use better_fetch::hooks::Hooks;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let hooks = Hooks::new().on_response_stream(|ctx| async move {
        let mut headers = ctx.headers;
        headers.insert(
            http::HeaderName::from_static("x-stream-hook"),
            http::HeaderValue::from_static("1"),
        );
        Ok(better_fetch::hooks::StreamingResponseMeta {
            status: ctx.status,
            headers,
        })
    });

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .hooks(hooks)
        .build()?;

    let response = client.get("/hook").send_stream().await?;
    assert_eq!(
        response
            .headers()
            .get("x-stream-hook")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );
    Ok(())
}

#[cfg(feature = "json")]
#[tokio::test]
async fn custom_retry_predicate_reads_streaming_body() -> Result<()> {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    let server = MockServer::start().await;
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_cb = attempts.clone();

    Mock::given(method("GET"))
        .and(path("/retry-json"))
        .respond_with(move |_: &wiremock::Request| {
            let n = attempts_cb.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(503).set_body_string(r#"{"retry":true}"#)
            } else {
                ResponseTemplate::new(200).set_body_string("ok")
            }
        })
        .mount(&server)
        .await;

    let policy = RetryPolicy::count(1).with_should_retry(Arc::new(|res| {
        serde_json::from_slice::<serde_json::Value>(res.bytes())
            .ok()
            .and_then(|v| v.get("retry").and_then(|r| r.as_bool()))
            .unwrap_or(false)
    }));

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .retry(policy)
        .build()?;

    let body = client
        .get("/retry-json")
        .send_stream()
        .await?
        .collect()
        .await?
        .into_bytes_checked()?;
    assert_eq!(body, Bytes::from_static(b"ok"));
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    Ok(())
}

#[cfg(feature = "json")]
#[tokio::test]
async fn custom_retry_no_retry_restores_body_on_stream() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/no-retry"))
        .respond_with(ResponseTemplate::new(400).set_body_string(r#"{"code":"bad"}"#))
        .mount(&server)
        .await;

    let policy = RetryPolicy::count(1).with_should_retry(Arc::new(|_| false));

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .retry(policy)
        .build()?;

    let mut response = client.get("/no-retry").send_stream().await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let mut body = BytesMut::new();
    while let Some(chunk) = response.bytes_stream().next().await {
        body.extend_from_slice(&chunk?);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&body).expect("peeked retry body should be valid JSON");
    assert_eq!(value["code"], "bad");
    Ok(())
}

#[tokio::test]
async fn custom_backend_without_streaming_returns_error() -> Result<()> {
    let client = ClientBuilder::new()
        .base_url("http://localhost")?
        .backend(Arc::new(BufferedOnlyBackend))
        .build()?;

    let err = client
        .get("/any")
        .send_stream()
        .await
        .expect_err("streaming should fail");
    assert!(matches!(err, Error::Other(msg) if msg.contains("streaming not supported")));
    Ok(())
}
