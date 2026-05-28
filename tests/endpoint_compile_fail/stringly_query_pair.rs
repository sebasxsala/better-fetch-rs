use better_fetch::{Client, Endpoint, define_params, impl_serde_endpoint_query};
use http::Method;
use serde::Serialize;

define_params!(GetTodoParams for "/todos/:id" { id: u64 });

#[derive(Default, Serialize)]
struct TagQuery {
    tag: String,
}

impl_serde_endpoint_query!(TagQuery);

struct GetTodo;

impl Endpoint for GetTodo {
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/todos/:id";
    type Response = ();
    type Params = GetTodoParams;
    type Query = TagQuery;
    type Body = ();
    type Headers = ();
}

fn main() {
    let client = Client::new("https://example.com").unwrap();
    let _ = client
        .call::<GetTodo>()
        .params(GetTodoParams { id: 1 })
        .query_pair("tag", "a");
}
