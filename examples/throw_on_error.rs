//! `throw_on_error` — fail fast on non-2xx from `send()` and `send_stream()`.
//!
//! Without `throw_on_error`, `send()` returns `Ok(Response)` for any HTTP status.
//! With it, you get `Err(Error::Http { .. })` and can read the response body via
//! [`Error::body`](https://docs.rs/better-fetch/latest/better_fetch/enum.Error.html#method.body)
//! or [`Error::api_json`](https://docs.rs/better-fetch/latest/better_fetch/enum.Error.html#method.api_json).
//!
//! ```bash
//! cargo run -p better-fetch --example throw_on_error
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use better_fetch::backend::{HttpBackend, HttpRequest, HttpResponse, HttpStreamingResponse};
use better_fetch::{BodyStream, ClientBuilder, Error, Result};
use bytes::Bytes;
use futures_util::stream;
use http::StatusCode;

struct NotFoundBackend;

#[async_trait]
impl HttpBackend for NotFoundBackend {
    async fn execute(&self, _req: HttpRequest) -> Result<HttpResponse> {
        Ok(HttpResponse {
            status: StatusCode::NOT_FOUND,
            headers: Default::default(),
            body: Bytes::from_static(br#"{"code":"not_found"}"#),
        })
    }

    async fn execute_stream(&self, req: HttpRequest) -> Result<HttpStreamingResponse> {
        let resp = self.execute(req).await?;
        Ok(HttpStreamingResponse {
            status: resp.status,
            headers: resp.headers,
            body: Box::pin(stream::once(async move { Ok(resp.body) })) as BodyStream,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = ClientBuilder::new()
        .base_url("https://example.com")?
        .backend(Arc::new(NotFoundBackend))
        .build()?;

    let err = client
        .get("/missing")
        .throw_on_error(true)
        .send()
        .await
        .unwrap_err();
    match &err {
        Error::Http {
            status,
            body: Some(_),
            ..
        } => assert_eq!(status.as_u16(), 404),
        other => panic!("expected Http with body, got {other:?}"),
    }
    let api: serde_json::Value = err.api_json().expect("JSON error body");
    assert_eq!(api["code"], "not_found");

    let err = client
        .get("/missing")
        .throw_on_error(true)
        .send_stream()
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Http { body: Some(_), .. }));

    println!("throw_on_error works for send and send_stream");
    Ok(())
}
