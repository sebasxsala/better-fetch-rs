#![cfg(feature = "schema-validate")]

use std::sync::Arc;

use better_fetch::schema::SchemaRegistry;
use better_fetch::{ClientBuilder, Error, Result};
use http::Method;
use schemars::JsonSchema;
use serde::Serialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Serialize, JsonSchema)]
struct CreateItem {
    title: String,
}

#[tokio::test]
async fn strict_registry_rejects_invalid_request_body() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/items"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_endpoint(
        "/items",
        Method::POST,
        Some(schemars::schema_for!(CreateItem)),
        None,
    );

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    let err = client
        .post("/items")
        .json(&serde_json::json!({ "title": 1 }))?
        .send()
        .await
        .expect_err("invalid schema body should fail");

    assert!(matches!(
        err,
        Error::SchemaValidation {
            phase: "request",
            ..
        }
    ));
    Ok(())
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct ItemResponse {
    #[allow(dead_code)]
    id: u64,
}

#[tokio::test]
async fn strict_registry_rejects_invalid_response_body() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"id":"not-a-number"}"#))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_endpoint(
        "/items/:id",
        Method::GET,
        None,
        Some(schemars::schema_for!(ItemResponse)),
    );

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    let err = client
        .get("/items/:id")
        .param("id", "1")
        .send()
        .await
        .expect_err("invalid response schema");

    assert!(matches!(
        err,
        Error::SchemaValidation {
            phase: "response",
            ..
        }
    ));
    Ok(())
}

#[tokio::test]
async fn strict_registry_accepts_valid_response_body() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"id":1}"#))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_endpoint(
        "/items/:id",
        Method::GET,
        None,
        Some(schemars::schema_for!(ItemResponse)),
    );

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    let response = client.get("/items/:id").param("id", "1").send().await?;
    assert!(response.is_success());
    Ok(())
}

#[tokio::test]
async fn strict_registry_accepts_valid_request_body() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/items"))
        .respond_with(ResponseTemplate::new(201).set_body_string("ok"))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_endpoint(
        "/items",
        Method::POST,
        Some(schemars::schema_for!(CreateItem)),
        None,
    );

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    let response = client
        .post("/items")
        .json(&CreateItem {
            title: "hello".into(),
        })?
        .send()
        .await?;

    assert!(response.is_success());
    Ok(())
}

#[tokio::test]
async fn non_strict_registry_skips_request_validation() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/items"))
        .respond_with(ResponseTemplate::new(201).set_body_string("ok"))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new();
    registry.register_endpoint(
        "/items",
        Method::POST,
        Some(schemars::schema_for!(CreateItem)),
        None,
    );

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    let response = client
        .post("/items")
        .json(&serde_json::json!({ "title": 1 }))?
        .send()
        .await?;

    assert!(response.is_success());
    Ok(())
}

#[derive(schemars::JsonSchema)]
struct PathIdParams {
    #[allow(dead_code)]
    id: u64,
}

#[tokio::test]
async fn strict_registry_rejects_invalid_path_params() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/x"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_full(
        "/items/:id",
        Method::GET,
        None,
        None,
        None,
        Some(schemars::schema_for!(PathIdParams)),
    );

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    let err = client
        .get("/items/:id")
        .param("id", "not-a-number")
        .send()
        .await
        .expect_err("invalid path param should fail params schema");

    assert!(matches!(
        err,
        Error::SchemaValidation {
            phase: "params",
            ..
        }
    ));
    Ok(())
}

#[derive(schemars::JsonSchema)]
struct ListQuery {
    #[allow(dead_code)]
    limit: u32,
}

#[tokio::test]
async fn strict_registry_accepts_valid_query_coerced_from_string() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_full(
        "/items",
        Method::GET,
        None,
        None,
        Some(schemars::schema_for!(ListQuery)),
        None,
    );

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    let response = client.get("/items").query("limit", "10").send().await?;
    assert!(response.is_success());
    Ok(())
}

#[tokio::test]
async fn strict_registry_accepts_valid_path_params_coerced() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/7"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_full(
        "/items/:id",
        Method::GET,
        None,
        None,
        None,
        Some(schemars::schema_for!(PathIdParams)),
    );

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    let response = client.get("/items/:id").param("id", "7").send().await?;
    assert!(response.is_success());
    Ok(())
}

#[tokio::test]
async fn strict_registry_rejects_invalid_query() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_full(
        "/items",
        Method::GET,
        None,
        None,
        Some(schemars::schema_for!(ListQuery)),
        None,
    );

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    let err = client
        .get("/items")
        .query("limit", "not-a-number")
        .send()
        .await
        .expect_err("invalid query should fail");

    assert!(matches!(
        err,
        Error::SchemaValidation { phase: "query", .. }
    ));
    Ok(())
}

#[tokio::test]
async fn strict_registry_validates_response_on_stream_collect() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"id":"bad"}"#))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_endpoint(
        "/items/:id",
        Method::GET,
        None,
        Some(schemars::schema_for!(ItemResponse)),
    );

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    let err = client
        .get("/items/:id")
        .param("id", "1")
        .send_stream()
        .await?
        .collect()
        .await
        .expect_err("invalid response on collect");

    assert!(matches!(
        err,
        Error::SchemaValidation {
            phase: "response",
            ..
        }
    ));
    Ok(())
}

#[tokio::test]
async fn strict_registry_rejects_non_json_response_on_stream_collect() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/1"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_endpoint(
        "/items/:id",
        Method::GET,
        None,
        Some(schemars::schema_for!(ItemResponse)),
    );

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    let err = client
        .get("/items/:id")
        .param("id", "1")
        .send_stream()
        .await?
        .collect()
        .await
        .expect_err("non-JSON body should fail schema on collect");

    assert!(matches!(
        err,
        Error::SchemaValidation {
            phase: "response",
            ..
        }
    ));
    Ok(())
}

#[tokio::test]
async fn disable_validation_skips_request_and_response_schema_checks() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/items"))
        .respond_with(ResponseTemplate::new(201).set_body_string(r#"{"id":"bad"}"#))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_endpoint(
        "/items",
        Method::POST,
        Some(schemars::schema_for!(CreateItem)),
        Some(schemars::schema_for!(ItemResponse)),
    );

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    let response = client
        .post("/items")
        .json(&serde_json::json!({ "title": 1 }))?
        .disable_validation(true)
        .send()
        .await?;

    assert!(response.is_success());
    Ok(())
}

#[tokio::test]
async fn disable_validation_skips_stream_collect_response_schema() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"id":"not-a-number"}"#))
        .mount(&server)
        .await;

    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_endpoint(
        "/items/:id",
        Method::GET,
        None,
        Some(schemars::schema_for!(ItemResponse)),
    );

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .schema_registry(Arc::new(registry))
        .build()?;

    let stream = client
        .get("/items/:id")
        .param("id", "1")
        .disable_validation(true)
        .send_stream()
        .await?;

    let response = stream.collect().await?;
    assert_eq!(response.bytes().as_ref(), br#"{"id":"not-a-number"}"#);
    Ok(())
}
