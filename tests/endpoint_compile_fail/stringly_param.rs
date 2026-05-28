use better_fetch::{Client, Endpoint, define_params};
use http::Method;

define_params!(GetTodoParams for "/todos/:id" { id: u64 });

struct GetTodo;

impl Endpoint for GetTodo {
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/todos/:id";
    type Response = ();
    type Params = GetTodoParams;
    type Query = ();
    type Body = ();
    type Headers = ();
}

fn main() {
    let client = Client::new("https://example.com").unwrap();
    let _ = client.call::<GetTodo>().param("id", 1);
}
