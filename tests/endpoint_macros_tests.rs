use better_fetch::{
    Client, Endpoint, EndpointParamsDerive, EndpointQueryDerive, NeedsParams, Result,
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
        })
        .send_json()
        .await?;

    assert_eq!(post.id, 3);
    Ok(())
}

#[test]
fn derive_params_rejects_extra_field() {
    let t = trybuild::TestCases::new();
    t.compile_fail("../../tests/endpoint_macros_compile_fail/extra_param_field.rs");
    t.compile_fail("../../tests/endpoint_macros_compile_fail/missing_param_field.rs");
}
