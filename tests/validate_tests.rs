use better_fetch::{Client, Error, Result};
use garde::Validate;
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Debug, Deserialize, Validate, PartialEq)]
struct StrictTodo {
    #[garde(range(min = 1))]
    id: u64,
    #[garde(length(min = 1))]
    title: String,
}

#[tokio::test]
async fn json_validated_rejects_empty_title() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/todo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1,
            "title": ""
        })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/todo")
        .send()
        .await?
        .json_validated::<StrictTodo>()
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Validation { .. }));
    Ok(())
}

#[tokio::test]
async fn send_json_validated_accepts_valid_body() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/todo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1,
            "title": "done"
        })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let todo: StrictTodo = client.get("/todo").send_json_validated().await?;
    assert_eq!(
        todo,
        StrictTodo {
            id: 1,
            title: "done".into()
        }
    );
    Ok(())
}

#[tokio::test]
async fn validate_response_false_skips_garde() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/todo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1,
            "title": ""
        })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let todo: StrictTodo = client
        .get("/todo")
        .validate_response(false)
        .send_json_validated()
        .await?;
    assert_eq!(todo.title, "");
    Ok(())
}

#[tokio::test]
async fn api_json_validated_parses_error_body() -> Result<()> {
    #[derive(Debug, Deserialize, Validate, PartialEq)]
    struct ApiError {
        #[garde(length(min = 1))]
        message: String,
    }

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/bad"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "message": "invalid input"
        })))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/bad")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await
        .unwrap_err();

    let api: ApiError = err.api_json_validated().unwrap();
    assert_eq!(api.message, "invalid input");
    Ok(())
}
