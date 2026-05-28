//! Edge-case tests for URL, query, and path parameter handling.

use better_fetch::path_param_names;
use better_fetch::{Client, Error, Result};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn path_param_names_ignores_query_suffix() {
    assert_eq!(
        path_param_names("/items/:id?sort=asc"),
        vec!["id".to_string()]
    );
}

#[test]
fn path_param_names_matches_macro_algorithm_without_query() {
    // Proc-macros in better-fetch-macros duplicate this logic without `?` stripping;
    // endpoint paths should not embed `?query` in the template string.
    assert_eq!(path_param_names("/todos/:id"), vec!["id".to_string()]);
}

#[tokio::test]
async fn unicode_in_path_param_is_percent_encoded() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/users/caf%C3%A9"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    assert!(client
        .get("/users/:name")
        .param("name", "café")
        .send()
        .await?
        .is_success());
    Ok(())
}

#[tokio::test]
async fn special_chars_in_query_value_are_encoded() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("q", "a&b=c"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    assert!(client
        .get("/search")
        .query("q", "a&b=c")
        .send()
        .await?
        .is_success());
    Ok(())
}

#[tokio::test]
async fn plus_and_percent_in_query_value() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("q", "100% done"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    assert!(client
        .get("/search")
        .query("q", "100% done")
        .send()
        .await?
        .is_success());
    Ok(())
}

#[tokio::test]
async fn embedded_query_merged_with_builder_query() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/1"))
        .and(query_param("sort", "desc"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    assert!(client
        .get("/items/1?sort=asc")
        .query("sort", "desc")
        .send()
        .await?
        .is_success());
    Ok(())
}

#[tokio::test]
async fn path_param_with_embedded_query_and_extra_builder_param() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/1"))
        .and(query_param("sort", "asc"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    assert!(client
        .get("/items/:id?sort=asc")
        .param("id", "1")
        .query("page", "2")
        .send()
        .await?
        .is_success());
    Ok(())
}

#[tokio::test]
async fn leftover_path_segment_after_substitution_errors() -> Result<()> {
    let client = Client::new("https://api.example.com")?;
    let err = client
        .get("/items/:id/extra/:missing")
        .param("id", "1")
        .send()
        .await
        .expect_err("unsubstituted :missing should fail");
    assert!(matches!(err, Error::MissingPathParam(name) if name == "missing"));
    Ok(())
}

#[tokio::test]
async fn base_url_with_trailing_slash_joins_relative_path() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let base = format!("{}/", server.uri());
    let client = Client::new(base)?;
    assert!(client.get("v1/health").send().await?.is_success());
    Ok(())
}
