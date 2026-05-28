# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-05-28

### Added

- **Battle testing** — expanded CI (feature matrix, MSRV, docs, `cargo-deny`, `cargo-semver-checks`, weekly fuzz), `tests/url_edge_tests.rs`, proptest coverage for URL/query/path, criterion benches, and fuzz targets under `fuzz/`.

### Changed

- **MSRV** — raised to **1.88** (matches current dependency floor, e.g. `cookie_store`).
- **CI** — dropped Miri job (low signal with workspace `unsafe_code = "forbid"`).

### Fixed

- **`max_response_bytes` on buffered responses** — `ClientBuilder::max_response_bytes` and per-request `.max_response_bytes()` now apply to `send()` and `send_json()`, not only `send_stream()` / `collect()`. Bodies are read via the streaming transport when a cap is set; oversized `Content-Length` headers fail fast with `Error::BodyTooLarge`.
- **Streaming non-2xx bodies** — `send_stream()` preserves the full response body when status is not 2xx and `throw()` is not set.

## [0.4.0] - 2026-05-27

### Added

- **Streaming uploads** — `RequestBuilder::body_stream` and `Error::NonReplayableBody` when retry cannot replay the body.
- **Tower dual stack** — `ClientBuilder::transport_stack` wires separate buffered and streaming transports; use this when middleware must apply to `send_stream()`.
- **Strict JSON Schema** (feature `schema-validate`) — optional validation of request/response JSON, query, and path params when the registry is strict; stream responses validate on `collect()`.
- **Garde validation** (feature `validate`) — `json_validated`, `query_validated`, `with_headers_validated` on builders.
- **Typed endpoint macros** — `#[derive(Endpoint)]` with `#[param]`, `#[query_field]`, `#[query]`, `#[endpoint(register)]`, and `NeedsBody` for POST bodies.
- **SSE** — incremental `SseDecoder` and `StreamingResponse::sse_events()`.
- **API helpers** — `into_api_result` / `send_api`, `ResponseBodyKind`, `RecordingBackend` for tests, `better_fetch::prelude`, `RequestBuilder::base_url`.
- **Features** — `full`, `miette` (`DiagnosticError`), `otel` (OpenTelemetry re-exports for tracing subscribers).
- **Errors** — `QuerySerialize`, `RequestValidation`, `SchemaValidation`, `InvalidHeaderName` / `InvalidHeaderValue`, `MissingPathParam`, `SchemaRoute`, `Transport::source`, and clearer `Io` / `Config`.
- **Docs** — [testing](docs/testing.md), [observability](docs/observability.md); publishing notes in README.

### Changed

- **Breaking:** typed `.query(...)` and `EndpointQuery::apply_query` return `Result`; serialization errors become `Error::QuerySerialize`.
- **Breaking:** `ClientConfig::hooks` is private; use `ClientConfig::effective_hooks()`.
- **Breaking:** `EndpointRequestBuilder` implements `Deref` only (no `DerefMut`); use typed `.query(MyQuery)?` instead of stringly `.query("key", "value")` on ready builders.
- **Breaking:** `transport_stack` takes `(buffered, streaming)` and returns two boxed services.
- **Breaking:** `impl Endpoint` requires `Body` and `Headers` associated types (often `()`).
- **Breaking:** `Error::Transport` carries an optional `source`; `Error::RetryExhausted::last` is `Option<Box<Error>>`.
- **Hooks on streams** — `on_error` runs for failed `send_stream()` responses even without `throw_on_error`, matching buffered behavior.
- **`http_service` / `http_service_boxed`** — documented: Tower layers apply to buffered `send()` only; streaming still uses reqwest unless you use `transport_stack`.
- **`LoggerPlugin`** — richer tracing (`http.request` / `http.response`, retry attempt, error body preview).
- **Retry** — shared buffered/streaming retry loop; aligned backoff, cancellation, and `throw_on_error` body handling.

### Fixed

- **`on_request` hooks** — changes to `RequestContext::body` are applied to the outgoing request.
- **`throw_on_error` + `send_stream`** — HTTP errors include a peeked body when possible.
- **Query in path templates** — `?foo=bar` in the path merges with builder query (builder wins).
- **Stream limits** — `max_response_bytes` applies to `collect()`, `stream_to_file`, and SSE helpers when set on the client or request.

### Migration (0.3.x → 0.4.0)

- Add `?` after typed `.query(...)`.
- Replace `config().hooks` with `config().effective_hooks()`.
- Replace `into_inner()` with `Deref` or delegated builder methods.
- Do not use stringly `.query("k", "v")` on `client.call::<E>()` after params are set.
- For Tower on both `send()` and `send_stream()`, use `transport_stack`, not `http_service` alone.
- Add `type Body = (); type Headers = ();` to manual `Endpoint` impls if missing.
- Multipart or streaming upload retry may return `Error::NonReplayableBody` instead of a generic error.

## [0.3.0] - 2026-05-27

### Changed

- **Typed endpoints (breaking)** — `EndpointRequestBuilder` from `client.call::<E>()` no longer has `.param()` / `.query_pair()`. Use `.params(E::Params)` and `.query(E::Query)` with typed structs (`define_params!`, `impl_serde_endpoint_query!`, or proc-macros). When `E::Params` is not `()`, `.params()` is required before `.send_json()`. Untyped `client.get()` / `.post()` are unchanged.
- **`Error::Transport`** — now `{ kind: TransportKind, message: String }` instead of a plain `String`. Use [`Error::transport`](https://docs.rs/better-fetch/latest/better_fetch/enum.Error.html#method.transport) or [`Error::transport_message`](https://docs.rs/better-fetch/latest/better_fetch/enum.Error.html#method.transport_message) to construct transport errors.
- **`Error::RetryExhausted::last`** — now `Option<Box<Error>>` instead of `Option<String>` (preserves structured last failure).
- **Transport mapping** — `map_transport_error` classifies reqwest failures into [`TransportKind`](https://docs.rs/better-fetch/latest/better_fetch/enum.TransportKind.html) (`Connect`, `Body`, `Decode`, `Redirect`, `Request`, `Builder`, `Upgrade`, `Other`) in addition to the existing `Timeout` variant.

### Added

- **`define_params!`**, extended **`endpoint!`**, **`impl_serde_endpoint_query!`** for typed path/query without proc-macros.
- **Type-state builder** — `NeedsParams` vs `Ready` on `EndpointRequestBuilder`.
- **Proc-macros** (feature `macros`) — `EndpointParamsDerive`, `EndpointQueryDerive` with path validation.
- **`serialize_to_query_map`** — serde structs to query params for OpenAPI/runtime parity.
- **Streaming API** — `RequestBuilder::send_stream`, `StreamingResponse`, `HttpBackend::execute_stream`, and `BodyStream` for incremental response bodies. `max_response_bytes` ends the stream after `Error::BodyTooLarge` (no infinite error loop).
- **Streaming hooks** — `on_response_stream`, `on_success_stream`, `StreamingResponseContext`, `StreamingResponseMeta`, `StreamingSuccessContext` (metadata only; buffered `on_response` / `on_success` are not called on the streaming path).
- **Streaming retry peek** — custom `RetryPolicy::with_should_retry` on `send_stream` can inspect up to `retry_body_peek_bytes` (default 64 KiB) of the body; status-only retries do not read the body.
- **Streaming cancellation** — `CancelBodyStream` registers the cancellation waker while the inner body read is pending.
- **Tower streaming** — `ServiceBackend` delegates `execute_stream` to a `ReqwestBackend` sharing the same `reqwest::Client` as the Tower stack (`transport_stack` wires this automatically).
- **`Error::BodyTooLarge`** — when a streaming response exceeds `max_response_bytes`.
- **`ClientBuilder::max_response_bytes`** / **`RequestBuilder::max_response_bytes`** — optional size cap on the streaming path.
- **`ClientBuilder::retry_body_peek_bytes`** / **`RequestBuilder::retry_body_peek_bytes`** — cap for retry predicate body peek on streams.
- **Example** — `examples/streaming.rs`; tests `streaming_tests` and `streaming_tower_tests` (feature `tower`).
- **docs.rs** — `package.metadata.docs.rs` with `all-features`, `doc_cfg`, and rustdoc example scraping.
- **`TransportKind`** — public enum for transport failure categories.
- **`Error::transport_kind`**, **`Error::transport_detail`**, **`Error::is_transport`**, **`Error::is_timeout`**, **`Error::retry_exhausted_last`**.
- **CI** — minimal feature-set checks (`json` + `rustls-tls` / `native-tls`, `multipart`).
- **Release automation** — `.github/workflows/release.yml` (tag `v*` → crates.io trusted publishing + GitHub Release from `CHANGELOG.md`).

### Migration (0.2.x → 0.3.0)

- **`client.call::<E>()`** — replace `.param("id", n)` with `.params(E::Params { ... })` (or `define_params!` / proc-macros). See README typed endpoint section.
- **`send()` / `send_json()`** — unchanged; still fully buffered.
- **Custom `HttpBackend`** — implement `execute_stream` (return an error if unsupported).
- **Tower `ServiceBackend`** — implement `execute_stream` on custom backends; with `transport_stack`, streaming uses the shared reqwest client (Tower middleware does not apply to `send_stream`).

## [0.2.3] - 2026-05-27

### Added

- **Rustdoc** — module-level docs for `client`, `request`, `response`, `endpoint`, `retry`, `auth`, `backend`, `error`, and `plugins`; expanded docs on public API types, methods, and fields.
- **Rustdoc examples** — 14 doctests on the crate root, `Client` / `ClientBuilder`, `RequestBuilder::send` / `send_json`, typed `Endpoint`, `RetryPolicy`, hooks, `Error::api_json`, auth, cancellation, mock `HttpBackend`, custom `json_parser`, and `transport_stack` (feature `tower`).
- **Crate root** — feature table, request flow overview, and quick-start example on [docs.rs](https://docs.rs/better-fetch).

### Changed

- **`RetryPolicy::max_attempts`** — now public (documents retry semantics).
- **docs.rs coverage** — documented items raised from ~30% to ~95% (default features).

## [0.2.2] - 2026-05-27

### Fixed

- **README on crates.io** — changelog link now points to `main` on GitHub (relative `CHANGELOG.md` 404’d in the published crate readme).

## [0.2.1] - 2026-05-27

### Added

- `Response::into_json_with` / `json_with` — single-step `Bytes` → `T` deserialization (ignores client `json_parser`).
- `Error::hook` / `Error::is_hook` — canonical constructor and matcher for hook failures (`Error::Hook` already existed in 0.2.0).

### Changed

- **Path parameter encoding** — use the `percent-encoding` crate instead of an inline encoder (same RFC 3986 unreserved set).
- **Documentation** — custom JSON fast path vs two-step parser, Tower `Buffer` production pattern (`tower_stack`), `ServiceBackend` mutex behavior, full-body buffering limits, `into_*` vs async response methods.

## [0.2.0] - 2026-05-27

### Added

- **Cancellation** — `CancellationToken` (from `tokio-util`), `RequestBuilder::cancellation_token()`, and `Error::Cancelled` with cooperative abort during requests and retry backoff.
- **Throw mode** — `RequestBuilder::throw_on_error(true)` returns `Err` on non-2xx from `send()` (like upstream `throw: true`).
- **Form bodies** — `RequestBuilder::form([...])` for `application/x-www-form-urlencoded`.
- **Multipart** — `RequestBuilder::multipart(form)` behind the `multipart` feature; re-export `better_fetch::multipart` for `reqwest::multipart::Form`.
- **Typed endpoints** — `EndpointRequestBuilder` via `client.call::<E>()`, with `EndpointParams` / `EndpointQuery` and typed `send_json()`.
- **Retry** — `Retry-After` header support, jitter on backoff, **408** in default retry codes; `RetryPolicy::Count` keeps `with_should_retry` without converting to linear.
- **Plugins** — `PreparedRequest` now includes `method` and `headers` in `init` (after auth).
- **Dependencies** — `indexmap` (stable query order), `tokio-util`, `fastrand` (lightweight).
- Example `multipart` and integration tests for cancel, throw, form, multipart, query order, and retry edge cases.

### Changed

- **`ClientBuilder::build()`** — requires `.base_url(...)`; returns `Error::MissingBaseUrl` instead of defaulting to `http://localhost` (**breaking**).
- **Query parameters** — stored in `IndexMap` so URL query strings follow insertion order.
- **`HttpBackend::execute`** — takes `HttpRequest` by value; client reuses one built request per attempt (no full clone per retry for byte bodies).
- **`ClientConfig`** — pre-merges plugin hooks at build time (`merged_hooks`).
- **Multipart + retry** — automatic retry is rejected with a clear error if a multipart body was used (multipart forms are not cloneable).

## [0.1.2] - 2026-05-27

### Changed

- **`openapi` feature** — `OpenApiBuilder` emits full OpenAPI 3.0 JSON: `components.schemas` (from schemars `RootSchema`), `requestBody` and response `content` with `$ref`, path/query `parameters`, `:param` → `{param}` paths, optional `servers`, and `OpenApiDocument::to_json` / `to_json_pretty`.
- **`register_typed`** — also registers `Endpoint::Query` and `Endpoint::Params` when they implement `JsonSchema`.

### Added

- Example `openapi_export` (`cargo run -p better-fetch --example openapi_export --features openapi`).

## [0.1.1] - 2026-05-27

### Fixed

- `tower` feature: import `HttpResponse` in `ClientBuilder::http_service` (CI compile error).

### Removed

- **`better-fetch-tower`** workspace crate — use `better-fetch` with features `tower` / `tower-http` instead. The `0.1.0` release on crates.io is yanked.

## [0.1.0] - 2026-05-27

### Added

- **`better-fetch`** — typed HTTP client on top of reqwest with `Client`, `ClientBuilder`, and fluent `RequestBuilder`.
- JSON request/response via serde (`json`, `send_json`, `json_unchecked`, optional custom `json_parser`).
- Dynamic path parameters (`:id`), query strings (including repeated keys and `query_json`), and `@put/...` method path modifiers.
- Absolute URL paths that bypass `base_url` when the path is a full `http(s)://` URL.
- Authentication: Bearer, Basic, and custom prefix; static, sync, and async token sources.
- Retry policies: `count`, linear, and exponential backoff with custom `should_retry` and default retry on 429/502/503/504.
- Lifecycle hooks: `on_request`, `on_response`, `on_success`, `on_error`, `on_retry`; `retry_attempt` on request context.
- Plugin system with `init` and merged hooks; built-in `LoggerPlugin` using `tracing`.
- `Endpoint` trait and `client.call::<E>()` for typed routes.
- `HttpBackend` abstraction with reqwest implementation and `ClientBuilder::backend` for mocks.
- Error type with HTTP status, `status_text`, response body bytes, and `api_json()` for API error payloads.
- Optional features: `schema` / `openapi` (registry + OpenAPI builder), `tower` / `tower-http` (transport `Service` stack), `validate` (garde response validation), `macros` (reserved proc-macro crate).
- **`typed-fetch`** and **`api-fetch`** — crates.io aliases that re-export `better-fetch`.
- Workspace examples (`basic`, `typed_endpoint`, `hooks`, `logger_plugin`, `retry`, `auth`, `validated_response`, `tower_stack`).
- Integration and unit tests (60+ cases) with wiremock.

### Notes

- Inspired by [@better-fetch/fetch](https://better-fetch.vercel.app/docs); independent Rust implementation, not affiliated with the upstream TypeScript project.

[0.4.0]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.4.0
[0.3.0]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.3.0
[0.2.3]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.2.3
[0.2.2]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.2.2
[0.2.1]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.2.1
[0.2.0]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.2.0
[0.1.2]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.1.2
[0.1.1]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.1.1
[0.1.0]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.1.0
