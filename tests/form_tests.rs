use better_fetch::{Client, Result};
use wiremock::matchers::{body_string, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn form_sets_urlencoded_body() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/submit"))
        .and(body_string("name=alice&role=admin"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    assert!(client
        .post("/submit")
        .form([("name", "alice"), ("role", "admin")])
        .send()
        .await?
        .is_success());
    Ok(())
}

