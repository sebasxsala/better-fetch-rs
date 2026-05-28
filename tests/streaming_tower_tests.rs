#![cfg(feature = "tower")]

use better_fetch::backend::HttpRequest;
use better_fetch::tower::stack::{IntoBoxHttpService, IntoBoxStreamingHttpService, ServiceBuilder};
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
        .transport_stack(|buffered, streaming| {
            (
                ServiceBuilder::new().service(buffered).into_box(),
                ServiceBuilder::new()
                    .service(streaming)
                    .into_streaming_box(),
            )
        })
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

#[tokio::test]
async fn send_stream_transport_stack_injects_request_header() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tower-stream-header"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .transport_stack(|buffered, streaming| {
            let map_header = |mut req: HttpRequest| {
                req.headers.insert(
                    http::HeaderName::from_static("x-stream-tower"),
                    http::HeaderValue::from_static("1"),
                );
                req
            };
            (
                ServiceBuilder::new()
                    .map_request(map_header)
                    .service(buffered)
                    .into_box(),
                ServiceBuilder::new()
                    .map_request(map_header)
                    .service(streaming)
                    .into_streaming_box(),
            )
        })
        .build()?;

    let _ = client
        .get("/tower-stream-header")
        .send_stream()
        .await?
        .collect()
        .await?;

    let received = server.received_requests().await.expect("wiremock requests");
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0]
            .headers
            .get("x-stream-tower")
            .map(|v| v.to_str().ok()),
        Some(Some("1"))
    );
    Ok(())
}

#[tokio::test]
async fn http_service_does_not_override_send_stream_transport() -> Result<()> {
    use better_fetch::backend::{HttpRequest, HttpResponse};
    use bytes::Bytes;
    use http::StatusCode;
    use tower::service_fn;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/split"))
        .respond_with(ResponseTemplate::new(200).set_body_string("from-wiremock"))
        .mount(&server)
        .await;

    let service = service_fn(|_req: HttpRequest| async {
        Ok::<_, better_fetch::Error>(HttpResponse {
            status: StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: Bytes::from_static(b"from-buffered-service"),
        })
    });

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .http_service(service)
        .build()?;

    let buffered = client.get("/split").send().await?.text().await?;
    assert_eq!(buffered, "from-buffered-service");

    let streamed = client
        .get("/split")
        .send_stream()
        .await?
        .collect()
        .await?
        .into_text()?;
    assert_eq!(streamed, "from-wiremock");
    Ok(())
}

#[tokio::test]
async fn transport_stack_buffered_only_send_stream_uses_reqwest() -> Result<()> {
    use better_fetch::backend::{HttpRequest, HttpResponse};
    use bytes::Bytes;
    use http::StatusCode;
    use tower::service_fn;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/stack-split"))
        .respond_with(ResponseTemplate::new(200).set_body_string("from-wiremock"))
        .mount(&server)
        .await;

    let stub = service_fn(|_req: HttpRequest| async {
        Ok::<_, better_fetch::Error>(HttpResponse {
            status: StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: Bytes::from_static(b"from-buffered-stack"),
        })
    });

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .transport_stack(|_buffered, streaming| {
            (
                ServiceBuilder::new().service(stub).into_box(),
                ServiceBuilder::new()
                    .service(streaming)
                    .into_streaming_box(),
            )
        })
        .build()?;

    let buffered = client.get("/stack-split").send().await?.text().await?;
    assert_eq!(buffered, "from-buffered-stack");

    let streamed = client
        .get("/stack-split")
        .send_stream()
        .await?
        .collect()
        .await?
        .into_text()?;
    assert_eq!(streamed, "from-wiremock");
    Ok(())
}

#[tokio::test]
async fn transport_stack_map_header_only_on_buffered_send() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/stack-header"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .transport_stack(|buffered, streaming| {
            let map_header = |mut req: HttpRequest| {
                req.headers.insert(
                    http::HeaderName::from_static("x-buffered-only"),
                    http::HeaderValue::from_static("1"),
                );
                req
            };
            (
                ServiceBuilder::new()
                    .map_request(map_header)
                    .service(buffered)
                    .into_box(),
                ServiceBuilder::new()
                    .service(streaming)
                    .into_streaming_box(),
            )
        })
        .build()?;

    let _ = client.get("/stack-header").send().await?;
    let _ = client
        .get("/stack-header")
        .send_stream()
        .await?
        .collect()
        .await?;

    let received = server.received_requests().await.expect("wiremock requests");
    assert_eq!(received.len(), 2);
    assert!(received[0].headers.contains_key("x-buffered-only"));
    assert!(!received[1].headers.contains_key("x-buffered-only"));
    Ok(())
}
