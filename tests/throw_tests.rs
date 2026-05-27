use better_fetch::{Client, Error, Result};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn send_returns_ok_on_404_by_default() -> Result<()> {
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
async fn throw_on_error_returns_err_on_404() -> Result<()> {
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

    assert!(matches!(err, Error::Http { status, .. } if status.as_u16() == 404));
    assert_eq!(err.status_text(), Some("Not Found"));
    Ok(())
}

#[tokio::test]
async fn throw_on_error_false_matches_default_send() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/x"))
        .respond_with(ResponseTemplate::new(500).set_body_string("err"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let response = client.get("/x").throw_on_error(false).send().await?;
    assert_eq!(response.status(), 500);
    Ok(())
}

#[tokio::test]
async fn throw_on_error_success_still_ok() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ok"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    assert!(client
        .get("/ok")
        .throw_on_error(true)
        .send()
        .await?
        .is_success());
    Ok(())
}

#[tokio::test]
async fn throw_on_error_preserves_response_body_in_error() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/bad"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "message": "nope"
        })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/bad")
        .throw_on_error(true)
        .send()
        .await
        .unwrap_err();

    #[derive(serde::Deserialize)]
    struct Msg {
        message: String,
    }
    let body: Msg = err.api_json().expect("body in error");
    assert_eq!(body.message, "nope");
    Ok(())
}
