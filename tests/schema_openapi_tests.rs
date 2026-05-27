#![cfg(feature = "openapi")]

use better_fetch::{Endpoint, OpenApiBuilder, SchemaRegistry};
use http::Method;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

struct Health;

impl Endpoint for Health {
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/health";
    type Response = HealthResponse;
    type Params = ();
    type Query = ();
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct HealthResponse {
    ok: bool,
}

struct Version;

impl Endpoint for Version {
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/version";
    type Response = VersionResponse;
    type Params = ();
    type Query = ();
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct VersionResponse {
    version: String,
}

#[test]
fn registry_and_openapi_builder() {
    let mut registry = SchemaRegistry::new();
    registry.register_typed::<Health, (), HealthResponse>();
    registry.register_typed::<Version, (), VersionResponse>();

    let doc = OpenApiBuilder::new()
        .title("Test API")
        .version("1.0.0")
        .from_registry(&registry);

    assert_eq!(doc.info.title, "Test API");
    assert!(doc.paths.contains_key("/health"));
    assert!(doc.paths.contains_key("/version"));
}

#[test]
fn strict_registry_rejects_unknown_route() {
    let mut registry = SchemaRegistry::new().strict(true);
    registry.register_endpoint("/health", Method::GET, None, None);

    assert!(registry.ensure_route("/health", &Method::GET).is_ok());
    assert!(registry.ensure_route("/other", &Method::GET).is_err());
}

#[test]
fn register_full_stores_schemas() {
    let mut registry = SchemaRegistry::new();
    registry.register_full(
        "/items",
        Method::POST,
        Some(schemars::schema_for!(CreateItem)),
        Some(schemars::schema_for!(Item)),
        None,
        None,
    );

    let entry = registry
        .entries()
        .iter()
        .find(|e| e.path == "/items")
        .unwrap();
    assert_eq!(entry.method, Method::POST);
    assert!(entry.request_schema.is_some());
    assert!(entry.response_schema.is_some());
}

#[derive(JsonSchema)]
#[expect(dead_code)]
struct CreateItem {
    name: String,
}

#[derive(JsonSchema)]
#[expect(dead_code)]
struct Item {
    id: u64,
    name: String,
}
