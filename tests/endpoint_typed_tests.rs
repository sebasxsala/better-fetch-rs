use better_fetch::{
    define_params, impl_serde_endpoint_query, Client, Endpoint, QueryValue, Result,
};
use http::Method;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, PartialEq)]
struct Item {
    id: u64,
}

define_params!(GetItemParams for "/items/:id" { id: u64 });

struct GetItem;

impl Endpoint for GetItem {
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/items/:id";
    type Response = Item;
    type Params = GetItemParams;
    type Query = ();
    type Body = ();
    type Headers = ();
}

#[tokio::test]
async fn endpoint_request_builder_send_json() -> Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": 9 })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let item = client
        .call::<GetItem>()
        .params(GetItemParams { id: 9 })
        .send_json()
        .await?;

    assert_eq!(item, Item { id: 9 });
    Ok(())
}

#[derive(Debug, Default, Serialize)]
struct VerboseQuery {
    verbose: bool,
}

impl_serde_endpoint_query!(VerboseQuery);

#[tokio::test]
async fn endpoint_query_trait_applies_params() -> Result<()> {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/1"))
        .and(query_param("verbose", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": 1 })))
        .mount(&server)
        .await;

    define_params!(GetWithQueryParams for "/items/:id" { id: u64 });

    struct GetWithQuery;
    impl Endpoint for GetWithQuery {
        const METHOD: Method = Method::GET;
        const PATH: &'static str = "/items/:id";
        type Response = Item;
        type Params = GetWithQueryParams;
        type Query = VerboseQuery;
        type Body = ();
        type Headers = ();
    }

    let client = Client::new(server.uri())?;
    let item = client
        .call::<GetWithQuery>()
        .params(GetWithQueryParams { id: 1 })
        .query(VerboseQuery { verbose: true })?
        .send_json()
        .await?;

    assert_eq!(item.id, 1);
    Ok(())
}

#[tokio::test]
async fn endpoint_query_index_map_still_works() -> Result<()> {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/2"))
        .and(query_param("tag", "a"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": 2 })))
        .mount(&server)
        .await;

    define_params!(IndexQueryParams for "/items/:id" { id: u64 });

    struct GetWithIndexQuery;
    impl Endpoint for GetWithIndexQuery {
        const METHOD: Method = Method::GET;
        const PATH: &'static str = "/items/:id";
        type Response = Item;
        type Params = IndexQueryParams;
        type Query = IndexMap<String, QueryValue>;
        type Body = ();
        type Headers = ();
    }

    let mut query = IndexMap::new();
    query.insert("tag".into(), QueryValue::Scalar("a".into()));

    let client = Client::new(server.uri())?;
    let item = client
        .call::<GetWithIndexQuery>()
        .params(IndexQueryParams { id: 2 })
        .query(query)?
        .send_json()
        .await?;

    assert_eq!(item.id, 2);
    Ok(())
}
