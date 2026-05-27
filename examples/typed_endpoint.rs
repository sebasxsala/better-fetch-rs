use better_fetch::{Client, Endpoint, Result};
use http::Method;
use serde::Deserialize;

struct GetTodo;

impl Endpoint for GetTodo {
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/todos/:id";
    type Response = Todo;
    type Params = ();
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

    let todo: Todo = client.call::<GetTodo>().param("id", 1).send_json().await?;

    println!("{todo:#?}");
    Ok(())
}
