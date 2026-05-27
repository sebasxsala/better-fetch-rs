use std::sync::Arc;

use base64::Engine;
use better_fetch::{Auth, Client, Result, TokenSource};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn bearer_static_token() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/secure"))
        .and(header("authorization", "Bearer my-token"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .auth(Auth::bearer("my-token"))
        .build()?;

    assert!(client.get("/secure").send().await?.is_success());
    Ok(())
}

#[tokio::test]
async fn bearer_fn_none_skips_header() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/open"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .auth(Auth::Bearer {
            token: TokenSource::Fn(Arc::new(|| None)),
        })
        .build()?;

    assert!(client.get("/open").send().await?.is_success());
    Ok(())
}

#[tokio::test]
async fn basic_auth_header() -> Result<()> {
    let server = MockServer::start().await;
    let encoded = base64::engine::general_purpose::STANDARD.encode("user:pass");
    let expected = format!("Basic {encoded}");

    Mock::given(method("GET"))
        .and(path("/basic"))
        .and(header("authorization", expected.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .auth(Auth::basic("user", "pass"))
        .build()?;

    assert!(client.get("/basic").send().await?.is_success());
    Ok(())
}

#[tokio::test]
async fn custom_auth_prefix() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/custom"))
        .and(header("authorization", "Token abc123"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .auth(Auth::Custom {
            prefix: "Token".into(),
            value: TokenSource::Static("abc123".into()),
        })
        .build()?;

    assert!(client.get("/custom").send().await?.is_success());
    Ok(())
}

#[tokio::test]
async fn per_request_bearer_overrides_client() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/x"))
        .and(header("authorization", "Bearer per-request"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .auth(Auth::bearer("client-token"))
        .build()?;

    assert!(client
        .get("/x")
        .bearer_token("per-request")
        .send()
        .await?
        .is_success());
    Ok(())
}

#[tokio::test]
async fn async_bearer_token() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/async"))
        .and(header("authorization", "Bearer async-tok"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .auth(Auth::Bearer {
            token: TokenSource::AsyncFn(Arc::new(|| async { Some("async-tok".into()) })),
        })
        .build()?;

    assert!(client.get("/async").send().await?.is_success());
    Ok(())
}
