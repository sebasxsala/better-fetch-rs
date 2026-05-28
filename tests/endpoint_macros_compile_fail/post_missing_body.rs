use better_fetch::{Client, EndpointDerive, Result};
use http::Method;

#[derive(Debug, Default, serde::Serialize)]
struct TodoBody {
    title: String,
}

#[derive(serde::Serialize, EndpointDerive)]
#[endpoint(method = Method::POST, path = "/todos")]
struct CreateTodo {
    #[response]
    message: String,
    #[body]
    body: TodoBody,
}

fn main() {
    let client = Client::new("http://localhost").unwrap();
    let _ = client.call::<CreateTodo>().send();
}
