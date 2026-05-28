#![cfg(feature = "validate")]

use better_fetch::{Client, Error, Result};
use garde::Validate;
use serde::Serialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Serialize, Validate)]
struct CreateTodo {
    #[garde(length(min = 1))]
    title: String,
}

#[tokio::test]
async fn json_validated_rejects_before_send() -> Result<()> {
    let server = MockServer::start().await;
    let client = Client::new(server.uri())?;

    let err = match client.post("/todos").json_validated(&CreateTodo {
        title: String::new(),
    }) {
        Ok(_) => panic!("expected validation error"),
        Err(e) => e,
    };

    assert!(matches!(err, Error::RequestValidation { .. }));
    Ok(())
}

#[tokio::test]
async fn json_validated_sends_valid_body() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/todos"))
        .respond_with(ResponseTemplate::new(201).set_body_string("created"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let response = client
        .post("/todos")
        .json_validated(&CreateTodo { title: "ok".into() })?
        .send()
        .await?;

    assert!(response.is_success());
    Ok(())
}

#[derive(Debug, Default, Serialize, Validate)]
struct ListQuery {
    #[garde(range(min = 1))]
    page: u64,
}

better_fetch::define_params!(NoParams for "/items" {});
better_fetch::impl_serde_endpoint_query!(ListQuery);

struct ListItems;
impl better_fetch::Endpoint for ListItems {
    const METHOD: http::Method = http::Method::GET;
    const PATH: &'static str = "/items";
    type Response = serde_json::Value;
    type Params = NoParams;
    type Query = ListQuery;
    type Body = ();
    type Headers = ();
}

#[tokio::test]
async fn endpoint_query_validated_rejects_invalid_query() -> Result<()> {
    let server = MockServer::start().await;
    let client = Client::new(server.uri())?;

    let result = client
        .call::<ListItems>()
        .params(NoParams {})
        .query_validated(ListQuery { page: 0 });
    assert!(matches!(result, Err(Error::RequestValidation { .. })));
    Ok(())
}

#[tokio::test]
async fn endpoint_query_validated_accepts_valid_query() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let _ = client
        .call::<ListItems>()
        .params(NoParams {})
        .query_validated(ListQuery { page: 1 })?
        .send_json()
        .await?;
    Ok(())
}

#[derive(Debug, Default, Validate)]
struct AuthHeaders {
    #[garde(length(min = 1))]
    authorization: String,
}

impl better_fetch::EndpointHeaders for AuthHeaders {
    fn apply_headers(
        self,
        mut builder: better_fetch::RequestBuilder<'_>,
    ) -> Result<better_fetch::RequestBuilder<'_>> {
        builder = builder.header("authorization", self.authorization)?;
        Ok(builder)
    }
}

struct WithHeaders;
impl better_fetch::Endpoint for WithHeaders {
    const METHOD: http::Method = http::Method::GET;
    const PATH: &'static str = "/auth";
    type Response = serde_json::Value;
    type Params = ();
    type Query = ();
    type Body = ();
    type Headers = AuthHeaders;
}

#[tokio::test]
async fn endpoint_headers_validated_rejects_empty() -> Result<()> {
    let client = Client::new("https://example.com")?;
    let result = client
        .call::<WithHeaders>()
        .with_headers_validated(AuthHeaders {
            authorization: String::new(),
        });
    assert!(matches!(result, Err(Error::RequestValidation { .. })));
    Ok(())
}
