//! Shared helpers for integration tests.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use better_fetch::backend::RecordingBackend;
use better_fetch::backend::{HttpBackend, HttpRequest, HttpResponse, HttpStreamingResponse};
use better_fetch::{Client, ClientBuilder, Error, Result};
use bytes::Bytes;
use futures_util::stream;
use http::StatusCode;
use wiremock::MockServer;

/// Backend that always returns 503 (for retry tests).
pub struct Flaky503Backend {
    pub hits: Arc<AtomicU32>,
}

#[async_trait]
impl HttpBackend for Flaky503Backend {
    async fn execute(&self, _request: HttpRequest) -> Result<HttpResponse> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        Ok(HttpResponse {
            status: StatusCode::SERVICE_UNAVAILABLE,
            headers: Default::default(),
            body: Bytes::new(),
        })
    }

    async fn execute_stream(&self, request: HttpRequest) -> Result<HttpStreamingResponse> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        let _ = request;
        Ok(HttpStreamingResponse {
            status: StatusCode::SERVICE_UNAVAILABLE,
            headers: Default::default(),
            body: Box::pin(stream::empty()),
        })
    }
}

/// Buffered backend that sleeps before responding.
pub struct SlowBackend {
    pub hits: Arc<AtomicU32>,
    pub delay: Duration,
}

#[async_trait]
impl HttpBackend for SlowBackend {
    async fn execute(&self, _request: HttpRequest) -> Result<HttpResponse> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
        Ok(HttpResponse {
            status: StatusCode::OK,
            headers: Default::default(),
            body: Bytes::from_static(b"ok"),
        })
    }

    async fn execute_stream(&self, _request: HttpRequest) -> Result<HttpStreamingResponse> {
        Err(Error::Config("streaming not supported".into()))
    }
}

/// Builds a shared flaky backend for retry/cancel tests.
pub fn flaky_503_backend() -> (Arc<Flaky503Backend>, Arc<AtomicU32>) {
    let hits = Arc::new(AtomicU32::new(0));
    (
        Arc::new(Flaky503Backend {
            hits: Arc::clone(&hits),
        }),
        hits,
    )
}

/// Builds a slow buffered backend for cancellation tests.
pub fn slow_backend(delay: Duration) -> (Arc<SlowBackend>, Arc<AtomicU32>) {
    let hits = Arc::new(AtomicU32::new(0));
    (
        Arc::new(SlowBackend {
            hits: Arc::clone(&hits),
            delay,
        }),
        hits,
    )
}

/// Builds a client with a [`RecordingBackend`] over wiremock.
pub async fn recording_client() -> Result<(MockServer, Client, Arc<RecordingBackend>)> {
    let server = MockServer::start().await;
    let inner = Arc::new(better_fetch::ReqwestBackend::new(reqwest::Client::new()));
    let recording = Arc::new(RecordingBackend::new(inner));
    let client = ClientBuilder::new()
        .base_url(server.uri())?
        .backend(recording.clone())
        .build()?;
    Ok((server, client, recording))
}
