use better_fetch::{Client, Result};
use bytes::Bytes;
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn send_max_response_bytes_returns_body_too_large() -> Result<()> {
    let server = MockServer::start().await;
    let body = "x".repeat(2048);
    Mock::given(method("GET"))
        .and(path("/big"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/big")
        .max_response_bytes(1024)
        .send()
        .await
        .expect_err("send should exceed limit");
    assert!(err.is_body_too_large());
    assert_eq!(err.body_too_large_limit(), Some(1024));
    Ok(())
}

#[tokio::test]
async fn send_max_response_bytes_allows_body_under_limit() -> Result<()> {
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
        .send()
        .await?;
    assert_eq!(response.into_bytes_checked()?, Bytes::from_static(b"ok"));
    Ok(())
}

#[derive(Debug, Deserialize)]
struct N {
    n: u32,
}

#[tokio::test]
async fn send_json_max_response_bytes_returns_body_too_large() -> Result<()> {
    let server = MockServer::start().await;
    let body = format!(r#"{{"n":{}}}"#, "9".repeat(2048));
    Mock::given(method("GET"))
        .and(path("/big-json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/big-json")
        .max_response_bytes(1024)
        .send_json::<N>()
        .await
        .expect_err("send_json should exceed limit");
    assert!(err.is_body_too_large());
    assert_eq!(err.body_too_large_limit(), Some(1024));
    Ok(())
}

#[tokio::test]
async fn send_content_length_fast_fail_body_too_large() -> Result<()> {
    let server = MockServer::start().await;
    let body = "x".repeat(2048);
    Mock::given(method("GET"))
        .and(path("/cl"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(body.clone())
                .insert_header("Content-Length", body.len().to_string()),
        )
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/cl")
        .max_response_bytes(1024)
        .send()
        .await
        .expect_err("Content-Length should fail before buffering");
    assert!(err.is_body_too_large());
    assert_eq!(err.body_too_large_limit(), Some(1024));
    Ok(())
}
