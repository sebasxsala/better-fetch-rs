//! [`RecordingBackend`] — records the last [`HttpRequest`] and call counts for tests.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use http::Method;

use super::{HttpBackend, HttpBody, HttpRequest, HttpResponse, HttpStreamingResponse};
use crate::Result;

/// Kind of request body last observed (streaming bodies are not replayed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedBodyKind {
    /// No body.
    Empty,
    /// Buffered byte body.
    Bytes(Bytes),
    /// Streaming body (not stored; only metadata).
    Stream,
}

/// Snapshot of a recorded HTTP request for assertions in tests.
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    /// HTTP method.
    pub method: Method,
    /// Resolved URL.
    pub url: url::Url,
    /// Body kind (and bytes when buffered).
    pub body: RecordedBodyKind,
}

/// Wraps an [`HttpBackend`] and records each executed request.
#[derive(Clone)]
pub struct RecordingBackend {
    inner: Arc<dyn HttpBackend>,
    state: Arc<RecordingState>,
}

#[derive(Default)]
struct RecordingState {
    last_recorded: Mutex<Option<RecordedRequest>>,
    execute_count: AtomicU32,
    execute_stream_count: AtomicU32,
}

fn snapshot_request(request: &HttpRequest) -> RecordedRequest {
    let body = match &request.body {
        HttpBody::Empty => RecordedBodyKind::Empty,
        HttpBody::Bytes(bytes) => RecordedBodyKind::Bytes(bytes.clone()),
        HttpBody::Stream(_) => RecordedBodyKind::Stream,
    };
    RecordedRequest {
        method: request.method.clone(),
        url: request.url.clone(),
        body,
    }
}

impl RecordingBackend {
    /// Wraps `inner` and starts with empty recording state.
    pub fn new(inner: Arc<dyn HttpBackend>) -> Self {
        Self {
            inner,
            state: Arc::new(RecordingState::default()),
        }
    }

    /// Returns the most recent recorded request snapshot.
    pub fn last_recorded(&self) -> Option<RecordedRequest> {
        self.state.last_recorded.lock().ok()?.clone()
    }

    /// Returns a clone of the most recent [`HttpRequest`] passed to the backend.
    ///
    /// **Note:** [`HttpBody::Stream`] is cloned as [`HttpBody::Empty`] — use [`Self::last_recorded`]
    /// to assert streaming uploads.
    pub fn last_request(&self) -> Option<HttpRequest> {
        self.last_recorded().map(|recorded| HttpRequest {
            method: recorded.method,
            url: recorded.url,
            body: match recorded.body {
                RecordedBodyKind::Empty => HttpBody::Empty,
                RecordedBodyKind::Bytes(bytes) => HttpBody::Bytes(bytes),
                RecordedBodyKind::Stream => HttpBody::Empty,
            },
            headers: Default::default(),
            timeout: None,
            cancellation: None,
            #[cfg(feature = "multipart")]
            multipart: None,
        })
    }

    /// Removes and returns the last recorded snapshot.
    pub fn take_last_recorded(&self) -> Option<RecordedRequest> {
        self.state.last_recorded.lock().ok()?.take()
    }

    /// Number of [`HttpBackend::execute`] calls.
    pub fn execute_count(&self) -> u32 {
        self.state.execute_count.load(Ordering::SeqCst)
    }

    /// Number of [`HttpBackend::execute_stream`] calls.
    pub fn execute_stream_count(&self) -> u32 {
        self.state.execute_stream_count.load(Ordering::SeqCst)
    }

    /// Total transport calls (`execute` + `execute_stream`).
    pub fn total_calls(&self) -> u32 {
        self.execute_count() + self.execute_stream_count()
    }

    fn record(&self, request: &HttpRequest) {
        if let Ok(mut slot) = self.state.last_recorded.lock() {
            *slot = Some(snapshot_request(request));
        }
    }
}

#[async_trait]
impl HttpBackend for RecordingBackend {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse> {
        self.state.execute_count.fetch_add(1, Ordering::SeqCst);
        self.record(&request);
        self.inner.execute(request).await
    }

    async fn execute_stream(&self, request: HttpRequest) -> Result<HttpStreamingResponse> {
        self.state
            .execute_stream_count
            .fetch_add(1, Ordering::SeqCst);
        self.record(&request);
        self.inner.execute_stream(request).await
    }
}
