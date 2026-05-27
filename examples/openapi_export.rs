//! Export an OpenAPI 3.0 JSON document from a [`SchemaRegistry`](better_fetch::SchemaRegistry).
//!
//! ```bash
//! cargo run -p better-fetch --example openapi_export --features openapi
//! ```

use better_fetch::{Endpoint, OpenApiBuilder, SchemaRegistry};
use http::Method;
use schemars::JsonSchema;
use serde::Deserialize;

struct ListTodos;

impl Endpoint for ListTodos {
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/todos";
    type Response = Vec<Todo>;
    type Params = ();
    type Query = ListQuery;
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[expect(dead_code)]
struct ListQuery {
    user_id: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[expect(dead_code)]
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
