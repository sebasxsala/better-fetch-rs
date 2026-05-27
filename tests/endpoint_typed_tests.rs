use better_fetch::{Client, Endpoint, QueryValue, Result};
use http::Method;
use indexmap::IndexMap;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct Item {
    id: u64,
}

struct GetItem;

impl Endpoint for GetItem {
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/items/:id";
    type Response = Item;
    type Params = ();
    type Query = IndexMap<String, QueryValue>;
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
        .param("id", 9)
        .send_json()
        .await?;

    assert_eq!(item, Item { id: 9 });
    Ok(())
}

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

    let mut query = IndexMap::new();
    query.insert(
        "verbose".into(),
        QueryValue::Scalar("true".into()),
    );

    struct GetWithQuery;
    impl Endpoint for GetWithQuery {
        const METHOD: Method = Method::GET;
        const PATH: &'static str = "/items/:id";
        type Response = Item;
        type Params = ();
        type Query = IndexMap<String, QueryValue>;
    }

    let client = Client::new(server.uri())?;
    let item = client
        .call::<GetWithQuery>()
        .param("id", 1)
        .query(query)
        .send_json()
        .await?;

    assert_eq!(item.id, 1);
    Ok(())
}
