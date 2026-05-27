#![cfg(feature = "multipart")]

use better_fetch::{Client, Result};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn multipart_upload_succeeds() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/upload"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let form = reqwest::multipart::Form::new().text("field", "value");
    let client = Client::new(server.uri())?;
    assert!(client
        .post("/upload")
        .multipart(form)
        .send()
        .await?
        .is_success());
    Ok(())
}

#[tokio::test]
async fn multipart_with_retry_policy_errors_on_second_attempt() -> Result<()> {
    use better_fetch::RetryPolicy;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/upload"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let client = better_fetch::ClientBuilder::new()
        .base_url(server.uri())?
        .retry(RetryPolicy::count(1))
        .build()?;

    let form = reqwest::multipart::Form::new().text("x", "y");
    let err = client
        .post("/upload")
        .multipart(form)
        .send()
        .await
        .unwrap_err();

    assert!(matches!(err, better_fetch::Error::Other(_)));
    Ok(())
}
