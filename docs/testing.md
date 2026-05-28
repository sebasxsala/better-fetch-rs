# Testing with better-fetch

## Recommended pattern: `HttpBackend`

Inject a custom backend with [`ClientBuilder::backend`](https://docs.rs/better-fetch/latest/better_fetch/struct.ClientBuilder.html#method.backend) so tests never hit the network:

```rust
use std::sync::Arc;
use async_trait::async_trait;
use better_fetch::backend::{HttpBackend, HttpRequest, HttpResponse};
use better_fetch::{ClientBuilder, Result};
use bytes::Bytes;
use http::StatusCode;

struct MockBackend;

#[async_trait]
impl HttpBackend for MockBackend {
    async fn execute(&self, _req: HttpRequest) -> Result<HttpResponse> {
        Ok(HttpResponse {
            status: StatusCode::OK,
            headers: Default::default(),
            body: Bytes::from_static(b"{}"),
        })
    }

    async fn execute_stream(&self, req: HttpRequest) -> Result<better_fetch::HttpStreamingResponse> {
        Err(better_fetch::Error::Other("not used".into()))
    }
}

let client = ClientBuilder::new()
    .base_url("https://example.com")?
    .backend(Arc::new(MockBackend))
    .build()?;
```

## `RecordingBackend`

[`RecordingBackend`](https://docs.rs/better-fetch/latest/better_fetch/struct.RecordingBackend.html) wraps any backend and records the last [`HttpRequest`](https://docs.rs/better-fetch/latest/better_fetch/struct.HttpRequest.html) plus call counts:

```rust
use std::sync::Arc;
use better_fetch::backend::RecordingBackend;
use better_fetch::{ClientBuilder, ReqwestBackend};

let inner = Arc::new(ReqwestBackend::new(reqwest::Client::new()));
let recording = Arc::new(RecordingBackend::new(inner));
let client = ClientBuilder::new()
    .base_url("https://api.example.com")?
    .backend(recording.clone())
    .build()?;
```

Integration tests can share helpers from `tests/support/mod.rs` (`recording_client`, `slow_backend`, `flaky_503_backend`, `slow_flaky_503_backend`, `StalledStreamBackend`).

Use [`last_recorded`](https://docs.rs/better-fetch/latest/better_fetch/struct.RecordingBackend.html#method.last_recorded) and [`RecordedBodyKind::Stream`](https://docs.rs/better-fetch/latest/better_fetch/enum.RecordedBodyKind.html) to assert upload streams without buffering the full body. [`last_request`](https://docs.rs/better-fetch/latest/better_fetch/struct.RecordingBackend.html#method.last_request) clones streaming bodies as empty.

## Non-replayable request bodies

Automatic retry cannot resend:

- [`RequestBuilder::body_stream`](https://docs.rs/better-fetch/latest/better_fetch/struct.RequestBuilder.html#method.body_stream) (`HttpBody::Stream`)
- `multipart` bodies (feature `multipart`)

The client returns [`Error::NonReplayableBody`](https://docs.rs/better-fetch/latest/better_fetch/enum.Error.html#variant.NonReplayableBody) on the second attempt.

## Edge-case matrix

| Scenario | Test file | Representative test |
|----------|-----------|---------------------|
| Retry + non-replayable upload stream | `tests/upload_stream_tests.rs`, `tests/retry_edge_tests.rs` | `stream_body_with_retry_returns_non_replayable`, `stream_upload_retry_only_one_wiremock_hit` |
| `throw_on_error` + large response (peek / drain) | `tests/http_edge_tests.rs` | `throw_on_error_stream_large_body_errors_body_too_large`, `throw_on_error_stream_http_error_body_is_peek_prefix` |
| Plugin `init` vs client vs plugin `on_request` | `tests/plugin_tests.rs` | `plugin_init_before_client_hook_before_plugin_hook` |
| Embedded query in path + builder `.query()` | `tests/url_edge_tests.rs` | `embedded_query_merged_with_builder_query`, `path_param_with_embedded_query_and_extra_builder_param` |
| Tower buffered vs streaming transport | `tests/streaming_tower_tests.rs` (feature `tower`) | `transport_stack_buffered_only_send_stream_uses_reqwest` |
| Cancel mid-collect / mid-retry request | `tests/cancel_tests.rs`, `tests/retry_cancel_tests.rs` | `cancellation_mid_collect_returns_cancelled`, `cancellation_during_second_retry_request_returns_cancelled` |

Run the full integration suite:

```bash
cargo test -p better-fetch
cargo test -p better-fetch --features tower
```

## wiremock

Most tests in this repository use [wiremock](https://docs.rs/wiremock) with `Client::new(mock_server.uri())?` for realistic HTTP without a live API.

## Compile-fail tests

Typed endpoint invariants are checked with `trybuild` under `tests/endpoint_compile_fail/` and `tests/endpoint_macros_compile_fail/`.

## Property tests, fuzz, and benchmarks

- **Property tests** — `proptest` in `crates/better-fetch/src/url_build.rs` and integration edge cases in `tests/url_edge_tests.rs`.
- **Fuzz** — `cargo +nightly fuzz run build_url` / `parse_query` from the repo-root `fuzz/` crate (weekly CI smoke).
- **Benchmarks** — `cargo bench -p better-fetch --bench url_build` (criterion; not run on every PR).
