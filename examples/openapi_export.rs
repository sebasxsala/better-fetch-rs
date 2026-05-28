//! Export an OpenAPI 3.0 JSON document from a [`SchemaRegistry`](better_fetch::SchemaRegistry).
//!
//! Demonstrates `ListQuery` as both OpenAPI schema and runtime query via `impl_serde_endpoint_query!`.
//!
//! ```bash
//! cargo run -p better-fetch --example openapi_export --features openapi
//! ```

use better_fetch::{impl_serde_endpoint_query, Endpoint, OpenApiBuilder, SchemaRegistry};
use http::Method;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

struct ListTodos;

impl Endpoint for ListTodos {
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/todos";
    type Response = Vec<Todo>;
    type Params = ();
    type Query = ListQuery;
    type Body = ();
    type Headers = ();
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
struct ListQuery {
    user_id: Option<u64>,
}

impl_serde_endpoint_query!(ListQuery);

#[derive(Debug, Deserialize, JsonSchema)]
#[allow(dead_code)]
struct Todo {
    id: u64,
    title: String,
    completed: bool,
}

fn main() {
    let mut registry = SchemaRegistry::new();
    registry.register_typed::<ListTodos, (), Vec<Todo>>();

    let doc = OpenApiBuilder::new()
        .title("Todos API")
        .version("1.0.0")
        .description("Example OpenAPI export from better-fetch")
        .server("https://jsonplaceholder.typicode.com")
        .from_registry(&registry);

    println!("{}", doc.to_json_pretty().expect("serialize OpenAPI"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use better_fetch::Client;

    #[test]
    fn list_query_applies_via_endpoint_builder() {
        let client = Client::new("https://example.com").unwrap();
        let _builder = client
            .call::<ListTodos>()
            .query(ListQuery { user_id: Some(1) })
            .unwrap();
    }
}
