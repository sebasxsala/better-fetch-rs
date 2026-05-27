use better_fetch::{Client, Error, Result};
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Debug, Deserialize, PartialEq)]
struct ApiError {
    message: String,
}

#[tokio::test]
async fn send_succeeds_on_404() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_string("gone"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let response = client.get("/missing").send().await?;
    assert_eq!(response.status(), 404);
    Ok(())
}

#[tokio::test]
async fn json_fails_on_404_with_http_error() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_string("gone"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/missing")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Http { status, .. } if status.as_u16() == 404));
    assert_eq!(err.status_text(), Some("Not Found"));
    Ok(())
}

#[tokio::test]
async fn api_json_parses_error_body() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/bad"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "message": "invalid input"
        })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/bad")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await
        .unwrap_err();

    let api: ApiError = err.api_json().unwrap();
    assert_eq!(api.message, "invalid input");
    Ok(())
}

#[tokio::test]
async fn json_unchecked_on_error_status() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/err"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({ "code": 1 })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    #[derive(Deserialize)]
    struct Body {
        code: u32,
    }
    let body: Body = client.get("/err").send().await?.json_unchecked().await?;
    assert_eq!(body.code, 1);
    Ok(())
}

#[tokio::test]
async fn deserialize_error_on_invalid_json() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/nope"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/nope")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Deserialize { .. }));
    Ok(())
}

#[tokio::test]
async fn text_succeeds_on_200() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/t"))
        .respond_with(ResponseTemplate::new(200).set_body_string("hello"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let text = client.get("/t").send().await?.text().await?;
    assert_eq!(text, "hello");
    Ok(())
}

#[test]
fn invalid_base_url_on_build() {
    let result = Client::builder().base_url("not a url");
    assert!(matches!(result, Err(Error::InvalidBaseUrl(_))));
}
