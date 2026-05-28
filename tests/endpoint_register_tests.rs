#![cfg(all(feature = "macros", feature = "schema", feature = "json"))]

use better_fetch::schema::SchemaRegistry;
use better_fetch::EndpointDerive;
use http::Method;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, JsonSchema)]
struct RegisterBody {
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[allow(dead_code)]
struct RegisterResponse {
    id: u64,
}

#[derive(EndpointDerive)]
#[endpoint(method = Method::POST, path = "/register", register)]
#[allow(dead_code)]
struct RegisterUser {
    #[response]
    response: RegisterResponse,
    #[body]
    body: RegisterBody,
}

#[test]
fn endpoint_register_populates_registry() {
    let mut registry = SchemaRegistry::new();
    RegisterUser::register(&mut registry);
    let entry = registry
        .entries()
        .iter()
        .find(|e| e.path == "/register" && e.method == Method::POST)
        .expect("route registered");
    assert!(entry.request_schema.is_some());
    assert!(entry.response_schema.is_some());
}
