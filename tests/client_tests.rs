use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use better_fetch::backend::{HttpBackend, HttpRequest, HttpResponse};
use better_fetch::{Client, ClientBuilder, Error, Result};
use bytes::Bytes;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Todo {
    user_id: u64,
    id: u64,
    title: String,
    completed: bool,
}

#[tokio::test]
async fn get_with_path_param_and_json() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/todos/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "userId": 1,
            "id": 42,
            "title": "test",
            "completed": false
        })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let todo: Todo = client.get("/todos/:id").param("id", 42).send_json().await?;

    assert_eq!(todo.id, 42);
    Ok(())
}

#[tokio::test]
async fn params_iter_sets_multiple() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/post/1/hello"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    assert!(client
        .get("/post/:id/:title")
        .params_iter([("id", "1"), ("title", "hello")])
        .send()
        .await?
        .is_success());
    Ok(())
}

#[tokio::test]
async fn query_params_are_sent() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(wiremock::matchers::query_param("q", "rust"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    assert!(client
        .get("/search")
        .query("q", "rust")
        .send()
        .await?
        .is_success());
    Ok(())
}

#[tokio::test]
async fn query_json_serializes_number() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/page"))
        .and(wiremock::matchers::query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    client
        .get("/page")
        .query_json("page", &2u32)?
        .send()
        .await?;
    Ok(())
}

#[tokio::test]
async fn bearer_auth_header_is_set() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/secure"))
        .and(header("authorization", "Bearer secret"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .auth(better_fetch::Auth::bearer("secret"))
        .build()?;

    assert!(client.get("/secure").send().await?.is_success());
    Ok(())
}

#[tokio::test]
async fn json_errors_on_non_success_status() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/missing")
        .send()
        .await?
        .json::<Todo>()
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Http { status, .. } if status.as_u16() == 404));
    Ok(())
}

#[tokio::test]
async fn post_json_body() -> Result<()> {
    let server = MockServer::start().await;
    let body = Todo {
        user_id: 1,
        id: 1,
        title: "t".into(),
        completed: false,
    };
    Mock::given(method("POST"))
        .and(path("/todos"))
        .and(body_json(serde_json::to_value(&body).unwrap()))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "userId": 1,
            "id": 1,
            "title": "t",
            "completed": false
        })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let _: Todo = client.post("/todos").json(&body)?.send_json().await?;
    Ok(())
}

#[tokio::test]
async fn put_patch_delete_methods() -> Result<()> {
    let server = MockServer::start().await;
    for (m, p) in [("PUT", "/put"), ("PATCH", "/patch"), ("DELETE", "/delete")] {
        Mock::given(method(m))
            .and(path(p))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
    }

    let client = Client::new(server.uri())?;
    assert!(client.put("/put").send().await?.is_success());
    assert!(client.patch("/patch").send().await?.is_success());
    assert!(client.delete("/delete").send().await?.is_success());
    Ok(())
}

#[tokio::test]
async fn method_modifier_put_on_path() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    assert!(client.get("@put/user").send().await?.is_success());
    Ok(())
}

#[tokio::test]
async fn absolute_url_ignores_base() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/abs"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new("https://unused.example.com")?;
    assert!(client
        .get(format!("{}/abs", server.uri()))
        .send()
        .await?
        .is_success());
    Ok(())
}

#[tokio::test]
async fn default_headers_applied() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/d"))
        .and(header("x-app", "better-fetch"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .default_header("x-app", "better-fetch")?
        .build()?;

    assert!(client.get("/d").send().await?.is_success());
    Ok(())
}

#[tokio::test]
async fn send_json_convenience() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/todos/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "userId": 1,
            "id": 1,
            "title": "a",
            "completed": true
        })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let todo: Todo = client.get("/todos/1").send_json().await?;
    assert_eq!(todo.id, 1);
    Ok(())
}

struct MockBackend {
    response: HttpResponse,
}

#[async_trait]
impl HttpBackend for MockBackend {
    async fn execute(&self, _request: HttpRequest) -> Result<HttpResponse> {
        Ok(self.response.clone())
    }
}

#[tokio::test]
async fn custom_backend_via_builder() -> Result<()> {
    let backend = Arc::new(MockBackend {
        response: HttpResponse {
            status: StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: Bytes::from_static(b"mocked"),
        },
    });

    let client = ClientBuilder::new()
        .base_url("http://localhost")?
        .backend(backend)
        .build()?;

    let text = client.get("/any").send().await?.text().await?;
    assert_eq!(text, "mocked");
    Ok(())
}

#[tokio::test]
async fn per_request_timeout_applies() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("ok")
                .set_delay(Duration::from_secs(2)),
        )
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/slow")
        .timeout(Duration::from_millis(100))
        .send()
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Timeout));
    Ok(())
}

#[tokio::test]
async fn client_json_parser_strips_bom() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/bom"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            b"\xef\xbb\xbf{\"userId\":1,\"id\":7,\"title\":\"t\",\"completed\":false}",
            "application/json",
        ))
        .mount(&server)
        .await;

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .json_parser(|body: &Bytes| {
            let slice = body.strip_prefix(b"\xef\xbb\xbf").unwrap_or(body);
            serde_json::from_slice(slice).map_err(|e| e.to_string())
        })
        .build()?;

    let todo: Todo = client.get("/bom").send_json().await?;
    assert_eq!(todo.id, 7);
    Ok(())
}

#[tokio::test]
async fn request_json_parser_overrides_client() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/override"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"id":1}"#))
        .mount(&server)
        .await;

    #[derive(Debug, Deserialize, PartialEq)]
    struct IdOnly {
        id: u64,
    }

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .json_parser(|body: &Bytes| serde_json::from_slice(body).map_err(|e| e.to_string()))
        .build()?;

    let parsed: IdOnly = client
        .get("/override")
        .json_parser(|_| Ok(serde_json::json!({ "id": 99 })))
        .send_json()
        .await?;

    assert_eq!(parsed, IdOnly { id: 99 });
    Ok(())
}

#[test]
fn invalid_request_header_returns_error() {
    let client = Client::new("http://localhost").unwrap();
    let err = client
        .get("/")
        .header("x-bad", "\n")
        .err()
        .expect("invalid header should fail");
    assert!(matches!(err, Error::Other(_)));
}
