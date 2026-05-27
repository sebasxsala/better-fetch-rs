use better_fetch::{Client, Result};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn query_params_follow_insertion_order() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("z", "1"))
        .and(query_param("a", "2"))
        .and(query_param("m", "3"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let response = client
        .get("/search")
        .query("z", "1")
        .query("a", "2")
        .query("m", "3")
        .send()
        .await?;

    assert!(response.is_success());
    let url = response.url().expect("url");
    assert_eq!(url.query(), Some("z=1&a=2&m=3"));
    Ok(())
}
