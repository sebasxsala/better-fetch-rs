#[path = "support/mod.rs"]
mod support;

use std::sync::Arc;

use better_fetch::backend::{RecordedBodyKind, RecordingBackend};
use better_fetch::{Client, Result};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn recording_backend_captures_last_request() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items/42"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let inner = Arc::new(better_fetch::ReqwestBackend::new(reqwest::Client::new()));
    let recording = Arc::new(RecordingBackend::new(inner));
    let client = Client::builder()
        .base_url(server.uri())?
        .backend(recording.clone())
        .build()?;

    let _ = client.get("/items/:id").param("id", 42).send().await?;

    assert_eq!(recording.execute_count(), 1);
    let last = recording.last_request().expect("recorded request");
    assert_eq!(last.method, http::Method::GET);
    assert!(last.url.as_str().contains("/items/42"));
    Ok(())
}

#[tokio::test]
async fn recording_backend_records_stream_body_kind() -> Result<()> {
    use futures_util::stream;
    use std::sync::Arc;

    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/upload"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let inner = Arc::new(better_fetch::ReqwestBackend::new(reqwest::Client::new()));
    let recording = Arc::new(RecordingBackend::new(inner));
    let client = Client::builder()
        .base_url(server.uri())?
        .backend(recording.clone())
        .build()?;

    let stream: better_fetch::BodyStream =
        Box::pin(stream::iter(vec![Ok(bytes::Bytes::from_static(b"chunk"))]));
    let _ = client.put("/upload").body_stream(stream).send().await?;

    let recorded = recording.last_recorded().expect("recorded request");
    assert_eq!(recorded.body, RecordedBodyKind::Stream);
    Ok(())
}

#[tokio::test]
async fn support_recording_client_helper() -> Result<()> {
    let (server, client, recording) = support::recording_client().await?;
    Mock::given(method("POST"))
        .and(path("/ingest"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;

    let _ = client
        .post("/ingest")
        .body(bytes::Bytes::from_static(b"x"))
        .send()
        .await?;
    assert_eq!(recording.total_calls(), 1);
    Ok(())
}
