#![cfg(feature = "json")]

use better_fetch::{ApiResponseExt, Client, Result};
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Debug, Deserialize, PartialEq)]
struct User {
    id: u64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ApiError {
    message: String,
}

#[tokio::test]
async fn into_api_result_success_and_error() -> Result<()> {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/ok"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": 1 })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/err"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({ "message": "missing" })),
        )
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;

    let ok = client
        .get("/ok")
        .send()
        .await?
        .into_api_result::<User, ApiError>()?;
    assert_eq!(ok, Ok(User { id: 1 }));

    let err = client
        .get("/err")
        .send()
        .await?
        .into_api_result::<User, ApiError>()?;
    assert_eq!(
        err,
        Err(ApiError {
            message: "missing".into()
        })
    );

    Ok(())
}
