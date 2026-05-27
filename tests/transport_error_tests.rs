use std::time::Duration;

use better_fetch::{Client, Error, Result, TransportKind};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn connection_refused_maps_to_connect_transport() -> Result<()> {
    let client = Client::new("http://127.0.0.1:1")?;
    let err = client.get("/").send().await.unwrap_err();

    assert!(err.is_transport());
    assert_eq!(err.transport_kind(), Some(TransportKind::Connect));
    assert!(
        err.transport_detail().is_some_and(|m| !m.is_empty()),
        "expected non-empty transport message"
    );
    Ok(())
}

#[tokio::test]
async fn request_timeout_maps_to_timeout_variant() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("ok")
                .set_delay(Duration::from_secs(2)),
        )
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let err = client
        .get("/slow")
        .timeout(Duration::from_millis(100))
        .send()
        .await
        .unwrap_err();

    assert!(err.is_timeout());
    assert!(!err.is_transport());
    Ok(())
}

#[tokio::test]
async fn transport_message_constructor_uses_other_kind() {
    let err = Error::transport_message("custom backend failure");
    assert_eq!(err.transport_kind(), Some(TransportKind::Other));
    assert_eq!(err.transport_detail(), Some("custom backend failure"));
}
