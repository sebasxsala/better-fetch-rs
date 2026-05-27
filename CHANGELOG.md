# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[0.2.2]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.2.2
[0.2.1]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.2.1
[0.2.0]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.2.0
[0.1.2]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.1.2
[0.1.1]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.1.1
[0.1.0]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.1.0
