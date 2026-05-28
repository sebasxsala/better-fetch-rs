//! HTTP edge cases: 204, HEAD, binary, text/plain, throw_on_error body parity.

use better_fetch::{Client, Error, Result};
use bytes::Bytes;
use futures_util::StreamExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn no_content_204_returns_success_with_empty_body() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/empty"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let response = client.get("/empty").send().await?;
    assert_eq!(response.status(), 204);
    assert!(response.bytes().is_empty());
    Ok(())
}

#[tokio::test]
async fn head_returns_success_with_empty_body() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .and(path("/resource"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let response = client.head("/resource").send().await?;
    assert!(response.is_success());
    assert!(response.bytes().is_empty());
    Ok(())
}

#[tokio::test]
async fn octet_stream_response_bytes() -> Result<()> {
    let server = MockServer::start().await;
    let payload = vec![0u8, 1, 2, 255];
    Mock::given(method("GET"))
        .and(path("/bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(payload.clone())
                .insert_header("content-type", "application/octet-stream"),
        )
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let bytes = client.get("/bin").send().await?.into_bytes_checked()?;
    assert_eq!(bytes.as_ref(), payload.as_slice());
    Ok(())
}

#[tokio::test]
async fn text_plain_request_and_response() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/echo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("plain-ok")
                .insert_header("content-type", "text/plain"),
        )
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let text = client
        .post("/echo")
        .header("content-type", "text/plain")?
        .body(Bytes::from_static(b"hello"))
        .send()
        .await?
        .into_text()?;
    assert_eq!(text, "plain-ok");
    Ok(())
}

#[tokio::test]
async fn throw_on_error_includes_body_buffered() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_string("gone"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/missing")
        .throw_on_error(true)
        .send()
        .await
        .unwrap_err();

    match &err {
        Error::Http { body: Some(b), .. } => assert_eq!(std::str::from_utf8(b).unwrap(), "gone"),
        other => panic!("expected Http with body, got {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn octet_stream_send_stream_respects_max_response_bytes() -> Result<()> {
    let server = MockServer::start().await;
    let payload = vec![0u8; 4096];
    Mock::given(method("GET"))
        .and(path("/bin-stream"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(payload)
                .insert_header("content-type", "application/octet-stream"),
        )
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let mut response = client
        .get("/bin-stream")
        .max_response_bytes(1024)
        .send_stream()
        .await?;
    let err = response
        .bytes_stream()
        .next()
        .await
        .expect("chunk")
        .expect_err("limit exceeded");
    assert!(err.is_body_too_large());
    Ok(())
}

#[tokio::test]
async fn throw_on_error_includes_body_streaming() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_string("gone-stream"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/missing")
        .throw_on_error(true)
        .send_stream()
        .await
        .unwrap_err();

    match &err {
        Error::Http { body: Some(b), .. } => {
            assert_eq!(std::str::from_utf8(b).unwrap(), "gone-stream");
        }
        other => panic!("expected Http with body, got {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn throw_on_error_stream_large_body_errors_body_too_large() -> Result<()> {
    let server = MockServer::start().await;
    let payload = vec![b'x'; 200 * 1024];
    Mock::given(method("GET"))
        .and(path("/large-404"))
        .respond_with(ResponseTemplate::new(404).set_body_bytes(payload))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/large-404")
        .throw_on_error(true)
        .send_stream()
        .await
        .unwrap_err();

    assert!(err.is_body_too_large());
    assert_eq!(err.body_too_large_limit(), Some(64 * 1024));
    Ok(())
}

#[tokio::test]
async fn throw_on_error_stream_http_error_body_is_peek_prefix() -> Result<()> {
    let server = MockServer::start().await;
    let mut payload = vec![0u8; 1024];
    payload[..7].copy_from_slice(b"PEEKED!");
    Mock::given(method("GET"))
        .and(path("/peek-err"))
        .respond_with(ResponseTemplate::new(404).set_body_bytes(payload))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/peek-err")
        .max_response_bytes(1024)
        .throw_on_error(true)
        .send_stream()
        .await
        .unwrap_err();

    match &err {
        Error::Http { body: Some(b), .. } => {
            assert_eq!(b.len(), 1024);
            assert_eq!(&b[..7], b"PEEKED!");
        }
        other => panic!("expected Http with peeked body, got {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn throw_on_error_buffered_large_body_respects_max_response_bytes() -> Result<()> {
    let server = MockServer::start().await;
    let body = "x".repeat(4096);
    Mock::given(method("GET"))
        .and(path("/large-err"))
        .respond_with(ResponseTemplate::new(404).set_body_string(body))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/large-err")
        .max_response_bytes(1024)
        .throw_on_error(true)
        .send()
        .await
        .unwrap_err();

    assert!(err.is_body_too_large());
    assert_eq!(err.body_too_large_limit(), Some(1024));
    Ok(())
}
