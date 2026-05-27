#![cfg(feature = "tower")]

use better_fetch::tower::stack::{IntoBoxHttpService, ServiceBuilder};
use better_fetch::{ClientBuilder, Result};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn send_stream_with_transport_stack() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tower-stream"))
        .respond_with(ResponseTemplate::new(200).set_body_string("tower-ok"))
        .mount(&server)
        .await;

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .transport_stack(|inner| ServiceBuilder::new().service(inner).into_box())
        .build()?;

    let text = client
        .get("/tower-stream")
        .send_stream()
        .await?
        .collect()
        .await?
        .into_text()?;
    assert_eq!(text, "tower-ok");
    Ok(())
}
