use better_fetch::{Client, Result};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn per_request_base_url_overrides_client() -> Result<()> {
    let primary = MockServer::start().await;
    let secondary = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string("secondary"))
        .mount(&secondary)
        .await;

    let client = Client::new(primary.uri())?;
    let body = client
        .get("/health")
        .base_url(secondary.uri())?
        .send()
        .await?
        .into_text()?;

    assert_eq!(body, "secondary");
    Ok(())
}
