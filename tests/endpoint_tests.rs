use better_fetch::{Client, Endpoint, Result};
use http::Method;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
pub struct Todo {
    pub id: u64,
    pub title: String,
}

better_fetch::endpoint!(GetTodo, GET, "/todos/:id", Response = Todo);

#[test]
fn endpoint_constants_match_definition() {
    assert_eq!(GetTodo::METHOD, Method::GET);
    assert_eq!(GetTodo::PATH, "/todos/:id");
}

#[tokio::test]
async fn client_call_uses_endpoint_path() -> Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/todos/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 7,
            "title": "test"
        })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let todo: Todo = client.call::<GetTodo>().param("id", 7).send_json().await?;

    assert_eq!(todo.id, 7);
    Ok(())
}
