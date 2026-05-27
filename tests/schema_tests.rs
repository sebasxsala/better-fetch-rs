use std::sync::Arc;

use better_fetch::{ClientBuilder, Error, Result, SchemaRegistry};
use http::Method;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn strict_registry_blocks_unregistered_route() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/allowed"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_endpoint("/allowed", Method::GET, None, None);

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    assert!(client.get("/allowed").send().await?.is_success());

    let err = client.get("/blocked").send().await.unwrap_err();
    assert!(matches!(err, Error::Other(_)));
    assert!(err.to_string().contains("route not in schema registry"));
    Ok(())
}

#[tokio::test]
async fn non_strict_registry_allows_any_route() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/any"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let registry = SchemaRegistry::new();
    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    assert!(client.get("/any").send().await?.is_success());
    Ok(())
}
