#![allow(dead_code)]

use better_fetch::{
    Client, Endpoint, EndpointDerive, EndpointParamsDerive, EndpointQueryDerive, NeedsParams,
    Result,
};
use http::Method;
use serde::{Deserialize, Serialize};

#[derive(Default, EndpointParamsDerive)]
#[endpoint(path = "/posts/:id")]
struct GetPostParams {
    id: u64,
}

#[derive(Debug, Default, Serialize, EndpointQueryDerive)]
struct PostQuery {
    include_comments: bool,
}

struct GetPost;

impl Endpoint for GetPost {
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/posts/:id";
    type Response = Post;
    type Params = GetPostParams;
    type Query = PostQuery;
    type Body = ();
    type Headers = ();
}

#[derive(EndpointDerive)]
#[endpoint(method = GET, path = "/health")]
struct HealthEndpoint {
    #[response]
    status: String,
}

#[tokio::test]
async fn derive_endpoint_send_json() -> Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string("\"ok\""))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let status: String = client.call::<HealthEndpoint>().send_json().await?;
    assert_eq!(status, "ok");
    Ok(())
}

#[derive(Debug, Deserialize, PartialEq)]
struct Post {
    id: u64,
    title: String,
}

#[test]
fn derive_params_builder_state() {
    fn assert_needs<T: better_fetch::EndpointParams<BuilderState = NeedsParams>>() {}
    assert_needs::<GetPostParams>();
}

#[tokio::test]
async fn derive_params_and_query_send_json() -> Result<()> {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/posts/3"))
        .and(query_param("include_comments", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 3,
            "title": "macro"
        })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let post: Post = client
        .call::<GetPost>()
        .params(GetPostParams { id: 3 })
        .query(PostQuery {
            include_comments: true,
        })?
        .send_json()
        .await?;

    assert_eq!(post.id, 3);
    Ok(())
}

#[derive(Debug, Deserialize, PartialEq)]
struct InlineItem {
    id: u64,
    name: String,
}

#[tokio::test]
async fn derive_endpoint_inline_param_fields() -> Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[derive(EndpointDerive)]
    #[endpoint(method = Method::GET, path = "/items/:id")]
    struct GetItemInline {
        #[response]
        item: InlineItem,
        #[param]
        id: u64,
    }

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 9,
            "name": "inline"
        })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let item: InlineItem = client
        .call::<GetItemInline>()
        .params(GetItemInlineParams { id: 9 })
        .send_json()
        .await?;
    assert_eq!(item.id, 9);
    assert_eq!(item.name, "inline");
    Ok(())
}

#[tokio::test]
async fn derive_endpoint_inline_query_fields() -> Result<()> {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[derive(EndpointDerive)]
    #[endpoint(method = Method::GET, path = "/search")]
    struct SearchInline {
        #[response]
        items: Vec<InlineItem>,
        #[query_field]
        q: String,
    }

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("q", "rust"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let items: Vec<InlineItem> = client
        .call::<SearchInline>()
        .query(SearchInlineQuery { q: "rust".into() })?
        .send_json()
        .await?;
    assert!(items.is_empty());
    Ok(())
}

#[test]
fn derive_params_rejects_extra_field() {
    let t = trybuild::TestCases::new();
    t.compile_fail("../../tests/endpoint_macros_compile_fail/extra_param_field.rs");
    t.compile_fail("../../tests/endpoint_macros_compile_fail/missing_param_field.rs");
    t.compile_fail("../../tests/endpoint_macros_compile_fail/duplicate_param.rs");
    t.compile_fail("../../tests/endpoint_macros_compile_fail/invalid_endpoint_path.rs");
    t.compile_fail("../../tests/endpoint_macros_compile_fail/post_missing_body.rs");
    t.compile_fail("../../tests/endpoint_macros_compile_fail/invalid_path_no_colon.rs");
    t.compile_fail("../../tests/endpoint_macros_compile_fail/query_type_duplicate.rs");
}

#[derive(Debug, Default, serde::Serialize)]
struct CreateTodoBody {
    title: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct CreateTodoResponse {
    message: String,
}

#[derive(EndpointDerive)]
#[endpoint(method = Method::POST, path = "/todos")]
struct CreateTodoEndpoint {
    #[response]
    response: CreateTodoResponse,
    #[body]
    body: CreateTodoBody,
}

#[tokio::test]
async fn derive_post_requires_body_before_send() -> Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/todos"))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(serde_json::json!({ "message": "created" })),
        )
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let response: CreateTodoResponse = client
        .call::<CreateTodoEndpoint>()
        .with_body(CreateTodoBody {
            title: "write tests".into(),
        })?
        .send_json()
        .await?;
    assert_eq!(response.message, "created");
    Ok(())
}

#[derive(Debug, Default, Serialize)]
struct SearchQuery {
    q: String,
}

#[derive(EndpointDerive)]
#[endpoint(method = GET, path = "/search")]
struct SearchEndpoint {
    #[response]
    hits: Vec<String>,
    #[query]
    query: SearchQuery,
}

#[tokio::test]
async fn derive_endpoint_emits_endpoint_query_for_nested_query_type() -> Result<()> {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("q", "rust"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(["a"])))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let hits: Vec<String> = client
        .call::<SearchEndpoint>()
        .query(SearchQuery { q: "rust".into() })?
        .send_json()
        .await?;
    assert_eq!(hits, vec!["a".to_string()]);
    Ok(())
}
