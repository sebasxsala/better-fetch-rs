# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- Optional features: `schema` / `openapi` (registry + minimal OpenAPI builder), `tower` / `tower-http` (transport `Service` stack), `validate` (garde response validation), `macros` (reserved proc-macro crate).
- **`typed-fetch`** and **`api-fetch`** — crates.io aliases that re-export `better-fetch`.
- **`better-fetch-tower`** — optional companion crate for Tower transport integration.
- **`better-fetch-macros`** — placeholder proc-macro crate for future derives.
- Workspace examples (`basic`, `typed_endpoint`, `hooks`, `logger_plugin`, `retry`, `auth`, `validated_response`, `tower_stack`).
- Integration and unit tests (60+ cases) with wiremock.

### Notes

- Inspired by [@better-fetch/fetch](https://better-fetch.vercel.app/docs); independent Rust implementation, not affiliated with the upstream TypeScript project.

[0.1.2]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.1.2
[0.1.1]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.1.1
[0.1.0]: https://github.com/sebasxsala/better-fetch-rs/releases/tag/v0.1.0
