use better_fetch::{Client, ResponseBodyKind, Result};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn json_content_type_parses_as_json() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/json"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({"ok": true})),
        )
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let response = client.get("/json").send().await?;
    match response.body_by_content_type() {
        ResponseBodyKind::Json(v) => assert_eq!(v["ok"], true),
        other => panic!("expected json, got {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn text_content_type_parses_as_text() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/text"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/plain")
                .set_body_string("hello"),
        )
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let response = client.get("/text").send().await?;
    assert!(matches!(
        response.body_by_content_type(),
        ResponseBodyKind::Text(s) if s == "hello"
    ));
    Ok(())
}
