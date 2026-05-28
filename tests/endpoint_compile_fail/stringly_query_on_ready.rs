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
    type Body = ();
    type Headers = ();
}

#[derive(Deserialize)]
struct Todo {
    id: u64,
}

fn main() -> Result<()> {
    let client = Client::new("https://example.com")?;
    let _ = client
        .call::<GetTodo>()
        .query("page", "1");
    Ok(())
}
