//! Typed endpoint example — method, path, params, and response bound at compile time.
//!
//! Compare with `basic.rs`, which uses the flexible `.get()` API with string paths.
//!
//! ```bash
//! cargo run -p better-fetch --example typed_endpoint --features json
//! ```

use better_fetch::{define_params, Client, Endpoint, Result};
use http::Method;
use serde::Deserialize;

define_params!(GetTodoParams for "/todos/:id" { id: u64 });

struct GetTodo;

impl Endpoint for GetTodo {
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/todos/:id";
    type Response = Todo;
    type Params = GetTodoParams;
    type Query = ();
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(dead_code)]
struct Todo {
    user_id: u64,
    id: u64,
    title: String,
    completed: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::new("https://jsonplaceholder.typicode.com")?;

    let todo: Todo = client
        .call::<GetTodo>()
        .params(GetTodoParams { id: 1 })
        .send_json()
        .await?;

    println!("{todo:#?}");
    Ok(())
}
