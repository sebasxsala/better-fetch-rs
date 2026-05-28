#![cfg(feature = "tower")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use better_fetch::backend::{HttpBackend, HttpRequest, HttpResponse};
use better_fetch::tower::stack::{
    self, ConcurrencyLimitLayer, IntoBoxHttpService, IntoBoxStreamingHttpService, ServiceBuilder,
};
use better_fetch::{ClientBuilder, Error, Result};
use bytes::Bytes;
use http::StatusCode;
use tower::service_fn;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn build_succeeds_with_stacked_concurrency_limits() {
    use better_fetch::backend::HttpRequest;
    use tower::service_fn;

    let service = service_fn(|_req: HttpRequest| async {
        Ok::<_, Error>(HttpResponse {
            status: StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: Bytes::from_static(b"ok"),
        })
    });

    let client = ClientBuilder::new()
        .base_url("http://localhost")
        .unwrap()
        .max_in_flight(4)
        .wire_concurrency_limit(4)
        .http_service(service)
        .build();

    assert!(client.is_ok());
}

#[tokio::test]
async fn http_service_with_service_fn() -> Result<()> {
    let service = service_fn(|_req: HttpRequest| async {
        Ok::<_, Error>(HttpResponse {
            status: StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: Bytes::from_static(b"from-tower"),
        })
    });

    let client = ClientBuilder::new()
        .base_url("http://localhost")?
        .http_service(service)
        .build()?;

    let text = client.get("/any").send().await?.text().await?;
    assert_eq!(text, "from-tower");
    Ok(())
}

#[tokio::test]
async fn transport_stack_adds_header_via_map_request() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/echo-header"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .transport_stack(|buffered, streaming| {
            let map_header = |mut req: HttpRequest| {
                req.headers.insert(
                    http::HeaderName::from_static("x-tower-test"),
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

    let status = client.get("/echo-header").send().await?.status();
    assert_eq!(status, StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn concurrency_limit_layer_with_wiremock() -> Result<()> {
    let server = MockServer::start().await;
    static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
    static MAX_SEEN: AtomicUsize = AtomicUsize::new(0);

    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("ok")
                .set_delay(Duration::from_millis(100)),
        )
        .mount(&server)
        .await;

    let stack = stack::build(reqwest::Client::new(), |inner| {
        ServiceBuilder::new()
            .layer(ConcurrencyLimitLayer::new(1))
            .map_response(move |res: HttpResponse| {
                let current = IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
                let max = MAX_SEEN.load(Ordering::SeqCst);
                if current > max {
                    MAX_SEEN.store(current, Ordering::SeqCst);
                }
                res
            })
            .map_request(move |req: HttpRequest| {
                let n = IN_FLIGHT.fetch_add(1, Ordering::SeqCst) + 1;
                let max = MAX_SEEN.load(Ordering::SeqCst);
                if n > max {
                    MAX_SEEN.store(n, Ordering::SeqCst);
                }
                req
            })
            .service(inner)
            .into_box()
    });

    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .http_service_boxed(stack)
        .build()?;

    let client = Arc::new(client);
    let mut handles = Vec::new();
    for _ in 0..4 {
        let c = client.clone();
        handles.push(tokio::spawn(async move { c.get("/slow").send().await }));
    }
    for h in handles {
        h.await.unwrap()?;
    }

    assert!(
        MAX_SEEN.load(Ordering::SeqCst) <= 1,
        "concurrency limit should cap in-flight transport calls"
    );
    Ok(())
}

#[tokio::test]
async fn service_backend_allows_concurrent_transport_calls() -> Result<()> {
    let server = MockServer::start().await;
    static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
    static MAX_SEEN: AtomicUsize = AtomicUsize::new(0);

    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("ok")
                .set_delay(Duration::from_millis(100)),
        )
        .mount(&server)
        .await;

    let stack = stack::build(reqwest::Client::new(), |inner| {
        ServiceBuilder::new()
            .map_response(move |res: HttpResponse| {
                IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
                res
            })
            .map_request(move |req: HttpRequest| {
                let n = IN_FLIGHT.fetch_add(1, Ordering::SeqCst) + 1;
                let max = MAX_SEEN.load(Ordering::SeqCst);
                if n > max {
                    MAX_SEEN.store(n, Ordering::SeqCst);
                }
                req
            })
            .service(inner)
            .into_box()
    });

    let client = Arc::new(
        ClientBuilder::new()
            .base_url(server.uri())?
            .http_service_boxed(stack)
            .build()?,
    );

    let mut handles = Vec::new();
    for _ in 0..4 {
        let c = client.clone();
        handles.push(tokio::spawn(async move { c.get("/slow").send().await }));
    }
    for h in handles {
        h.await.unwrap()?;
    }

    assert!(
        MAX_SEEN.load(Ordering::SeqCst) >= 2,
        "ServiceBackend should allow concurrent transport I/O without a global mutex"
    );
    Ok(())
}

#[tokio::test]
async fn max_in_flight_core_semaphore() -> Result<()> {
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct CountingBackend {
        in_flight: Arc<AtomicUsize>,
        max_seen: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl HttpBackend for CountingBackend {
        async fn execute(&self, _request: HttpRequest) -> Result<HttpResponse> {
            let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            let max = self.max_seen.load(Ordering::SeqCst);
            if current > max {
                self.max_seen.store(current, Ordering::SeqCst);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            Ok(HttpResponse {
                status: StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: Bytes::from_static(b"ok"),
            })
        }

        async fn execute_stream(
            &self,
            _request: HttpRequest,
        ) -> Result<better_fetch::backend::HttpStreamingResponse> {
            Err(Error::Other(
                "streaming not supported in CountingBackend".into(),
            ))
        }
    }

    let in_flight = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));
    let backend = Arc::new(CountingBackend {
        in_flight: in_flight.clone(),
        max_seen: max_seen.clone(),
    });

    let client = Arc::new(
        ClientBuilder::new()
            .base_url("http://localhost")?
            .backend(backend)
            .max_in_flight(1)
            .build()?,
    );

    let mut handles = Vec::new();
    for _ in 0..4 {
        let c = client.clone();
        handles.push(tokio::spawn(async move { c.get("/slow").send().await }));
    }
    for h in handles {
        h.await.unwrap()?;
    }

    assert!(
        max_seen.load(Ordering::SeqCst) <= 1,
        "max_in_flight(1) should cap concurrent backend calls"
    );
    Ok(())
}
