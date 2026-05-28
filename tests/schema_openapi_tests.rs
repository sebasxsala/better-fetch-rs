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
    type Body = ();
    type Headers = ();
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
    type Body = ();
    type Headers = ();
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct VersionResponse {
    version: String,
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

#[derive(JsonSchema)]
#[expect(dead_code)]
struct TodoParams {
    id: u64,
}

#[derive(JsonSchema)]
#[expect(dead_code)]
struct ListQuery {
    limit: u32,
    #[serde(default)]
    offset: u32,
}

#[test]
fn registry_and_openapi_builder() {
    let mut registry = SchemaRegistry::new();
    registry.register_typed::<Health, (), HealthResponse>();
    registry.register_typed::<Version, (), VersionResponse>();

    let doc = OpenApiBuilder::new()
        .title("Test API")
        .version("1.0.0")
        .server("https://api.example.com")
        .from_registry(&registry);

    assert_eq!(doc.info.title, "Test API");
    assert_eq!(
        doc.servers.as_ref().map(|s| s[0].url.as_str()),
        Some("https://api.example.com")
    );
    assert!(doc.paths.contains_key("/health"));
    assert!(doc.paths.contains_key("/version"));

    let components = doc.components.as_ref().expect("components");
    assert!(components.schemas.contains_key("HealthResponse"));
    assert!(components.schemas.contains_key("VersionResponse"));

    let health_get = &doc.paths["/health"]["get"];
    let response = &health_get.responses.statuses["200"];
    let content = response.content.as_ref().expect("response content");
    let schema = &content["application/json"].schema;
    let ref_path = match schema {
        better_fetch::OpenApiSchemaRef::Ref { ref_path } => ref_path,
        _ => panic!("expected $ref for response schema"),
    };
    assert_eq!(ref_path, "#/components/schemas/HealthResponse");
    assert!(health_get.request_body.is_none());
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

#[test]
fn openapi_post_includes_request_and_response_bodies() {
    let mut registry = SchemaRegistry::new();
    registry.register_full(
        "/items",
        Method::POST,
        Some(schemars::schema_for!(CreateItem)),
        Some(schemars::schema_for!(Item)),
        None,
        None,
    );

    let doc = OpenApiBuilder::new().from_registry(&registry);
    let post = &doc.paths["/items"]["post"];

    let body = post.request_body.as_ref().expect("requestBody");
    assert!(body.required);
    let req_schema = match &body.content["application/json"].schema {
        better_fetch::OpenApiSchemaRef::Ref { ref_path } => ref_path,
        _ => panic!("expected $ref"),
    };
    assert_eq!(req_schema, "#/components/schemas/CreateItem");

    let res = &post.responses.statuses["200"].content.as_ref().unwrap()["application/json"].schema;
    let res_ref = match res {
        better_fetch::OpenApiSchemaRef::Ref { ref_path } => ref_path,
        _ => panic!("expected $ref"),
    };
    assert_eq!(res_ref, "#/components/schemas/Item");
}

#[test]
fn openapi_path_and_query_parameters() {
    let mut registry = SchemaRegistry::new();
    registry.register_full(
        "/todos/:id",
        Method::GET,
        None,
        Some(schemars::schema_for!(Item)),
        Some(schemars::schema_for!(ListQuery)),
        Some(schemars::schema_for!(TodoParams)),
    );

    let doc = OpenApiBuilder::new().from_registry(&registry);
    assert!(doc.paths.contains_key("/todos/{id}"));

    let get = &doc.paths["/todos/{id}"]["get"];
    let names: Vec<_> = get
        .parameters
        .iter()
        .map(|p| (p.name.as_str(), p.location.as_str()))
        .collect();
    assert!(names.contains(&("id", "path")));
    assert!(names.contains(&("limit", "query")));
    assert!(names.contains(&("offset", "query")));
}

#[test]
fn openapi_serializes_to_valid_json() {
    let mut registry = SchemaRegistry::new();
    registry.register_full(
        "/items",
        Method::POST,
        Some(schemars::schema_for!(CreateItem)),
        Some(schemars::schema_for!(Item)),
        None,
        None,
    );

    let doc = OpenApiBuilder::new()
        .title("Items API")
        .version("2.0.0")
        .description("Demo")
        .from_registry(&registry);

    let json = doc.to_json_pretty().expect("json");
    assert!(json.contains("\"openapi\": \"3.0.3\""));
    assert!(json.contains("components"));
    assert!(json.contains("CreateItem"));
    assert!(json.contains("requestBody"));
}
