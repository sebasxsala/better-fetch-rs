# better-fetch

Typed HTTP client layer on top of [reqwest](https://docs.rs/reqwest), inspired by
[@better-fetch/fetch](https://better-fetch.vercel.app/docs). Independent Rust implementation.

## Installation

Pick one crate name (same library):

```toml
[dependencies]
better-fetch = "0.4"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
tokio-util = "0.7"
```

Aliases on crates.io: [`typed-fetch`](https://crates.io/crates/typed-fetch), [`api-fetch`](https://crates.io/crates/api-fetch) — `pub use better_fetch::*`.

Optional features (defaults: `json`, `rustls-tls`):

```toml
better-fetch = { version = "0.4", features = ["tower", "validate", "multipart"] }
```

Minimal build (pick **one** TLS feature — do not enable `rustls-tls` and `native-tls` together):

```toml
better-fetch = { version = "0.4", default-features = false, features = ["json", "rustls-tls"] }
```

## Quick start

Flexible requests with [`.get()`](https://docs.rs/better-fetch/latest/better_fetch/struct.Client.html#method.get) — string paths, typed JSON response:

```rust
use better_fetch::{Client, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Todo {
    user_id: u64,
    id: u64,
    title: String,
    completed: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::new("https://jsonplaceholder.typicode.com")?;

    // send() returns Response (any status); json() fails on non-2xx
    let todo: Todo = client
        .get("/todos/:id")
        .param("id", 1)
        .send()
        .await?
        .json()
        .await?;

    // Or in one step (response type from the variable or turbofish):
    let todo: Todo = client.get("/todos/:id").param("id", 1).send_json().await?;

    println!("{todo:#?}");
    Ok(())
}
```

For compile-time route definitions (method, path, params, query, response), see [Typed endpoint](#typed-endpoint) below.

## Migrating from 0.3.x to 0.4.0

- **Typed `.query(...)`** — returns `Result`; add `?` before `.send_json()`: `.query(MyQuery { ... })?`.
- **`ClientConfig::hooks`** — field is no longer public; use [`effective_hooks()`](https://docs.rs/better-fetch/latest/better_fetch/struct.ClientConfig.html#method.effective_hooks) for the merged hook chain used at runtime.
- **`on_request` body** — setting `RequestContext::body` in a hook now updates the outgoing request (previously ignored).
- **`throw_on_error` + `send_stream`** — non-2xx errors include a peeked body in `Error::Http`, matching buffered `send()`.
- **`EndpointRequestBuilder`** — no `DerefMut`; use typed `.query(MyQuery)?` instead of stringly `.query("key", "value")` on `Ready`.
- **`http_service` / `http_service_boxed`** — Tower layers apply to buffered `send()` only; use `transport_stack` for `send_stream()`.
- **`schema-validate` strict** — query/params wire values are coerced to JSON numbers/bools when possible before JSON Schema checks.

See [CHANGELOG.md](CHANGELOG.md#040---2026-05-27) for the full list.

## When to use `better-fetch` vs reqwest alone

| Use **better-fetch** | Use **reqwest** directly |
|----------------------|---------------------------|
| Typed routes (`Endpoint`), plugins, hooks, retries, `throw_on_error` | One-off raw HTTP with minimal deps |
| Shared client config (base URL, auth, default headers, retry policy) | You already have a custom stack and only need transport |
| Testing via [`HttpBackend`](https://docs.rs/better-fetch/latest/better_fetch/trait.HttpBackend.html) / [`RecordingBackend`](https://docs.rs/better-fetch/latest/better_fetch/struct.RecordingBackend.html) | Full control of every reqwest option with no abstraction |

better-fetch **is** reqwest under the hood for the default backend; you can pass a custom [`reqwest::Client`](https://docs.rs/reqwest) with [`Client::from_reqwest`](https://docs.rs/better-fetch/latest/better_fetch/struct.Client.html#method.from_reqwest).

**Concurrency:** [`ClientBuilder::max_in_flight`](https://docs.rs/better-fetch/latest/better_fetch/struct.ClientBuilder.html#method.max_in_flight) limits concurrent requests (including retries) inside the client. With `feature = "tower"`, [`ConcurrencyLimitLayer`](https://docs.rs/better-fetch/latest/better_fetch/tower/stack/struct.ConcurrencyLimitLayer.html) caps the transport separately — avoid stacking both at the same numeric limit unless intentional. See [`examples/tower_vs_streaming.rs`](examples/tower_vs_streaming.rs).

See [docs/testing.md](docs/testing.md) (testing) and [docs/observability.md](docs/observability.md) (tracing / OTel / miette).

## Highlights

- **Builder API** — `Client::builder()`, per-request `.timeout()`, `.retry()`, `.auth()`, headers, JSON body.
- **Retries** — linear, exponential, or `count`; `Retry-After`, jitter, custom `should_retry`; default retry on 408/429/502/503/504.
- **Hooks & plugins** — compose client and plugin hooks; optional `LoggerPlugin` (requires a `tracing` subscriber in your app).
- **Errors** — `Result` + `?`; [`TransportKind`](https://docs.rs/better-fetch/latest/better_fetch/enum.TransportKind.html) on transport failures; `Error::api_json()` to parse JSON error bodies from APIs; `Error::hook()` from `on_request` / `on_response` hooks.
- **Typed endpoints** — `Endpoint` trait + `client.call::<E>()` with typed `params`/`query` structs and `send_json()` (path/body enforced by type-state when applicable; query typed but optional).
- **Testing** — inject `ClientBuilder::backend(Arc<dyn HttpBackend>)`.
- **Cancellation** — `CancellationToken` per request; cooperative abort during requests and retry backoff.
- **Throw mode** — `throw_on_error(true)` makes `send()` return `Err` on non-2xx (like upstream `throw: true`).
- **Form & multipart** — `.form([...])` for url-encoded bodies; `.multipart(form)` with feature `multipart`.

### Request options

**On [`RequestBuilder`](https://docs.rs/better-fetch/latest/better_fetch/struct.RequestBuilder.html)** (`client.get(...)` / `.post(...)`):

| Method | Description |
|--------|-------------|
| `.param` / `.params` / `.params_iter` | Path template `:id` substitution |
| `.query` / `.queries` | Query string (stable insertion order via `IndexMap`) |
| `.query_json` | Serialize a value into a query param (feature `json`) |
| `.json` / `.body` | Request body |
| `.form` | `application/x-www-form-urlencoded` body |
| `.multipart` | Multipart form (feature `multipart`) |
| `.timeout` / `.retry` | Per-request overrides |
| `.auth` / `.bearer_token` | Per-request auth |
| `.cancellation_token` | Cancel in-flight request + retry sleeps |
| `.throw_on_error` | `send()` returns `Err` on non-2xx when `true` |
| `.send` / `.send_json` | Execute request (`send_json::<T>()` for typed JSON) |
| `.json_parser` | Custom `Bytes` → `Value` parser (feature `json`; see below) |

**On [`EndpointRequestBuilder`](https://docs.rs/better-fetch/latest/better_fetch/struct.EndpointRequestBuilder.html)** (`client.call::<E>()`):

| Method | Description |
|--------|-------------|
| `.params(E::Params)` | Typed path parameters (required when `E::Params` is not `()`) |
| `.query(E::Query)` | Typed query struct or `IndexMap<String, QueryValue>` (optional — call to attach query string) |
| `.header` / `.bearer_token` / `.cancellation_token` / `.throw_on_error` | Same as `RequestBuilder` |
| `.send` / `.send_json` | Execute; `send_json()` returns `E::Response` |

### Custom JSON parsing

By default, JSON responses deserialize in one step (`Bytes` → `T` via `serde_json::from_slice`).

`ClientBuilder::json_parser` (or per-request `.json_parser`) uses two steps: your function returns `serde_json::Value`, then the library maps to `T`. Use this for BOM stripping or payload normalization:

```rust
use better_fetch::{ClientBuilder, Result};
use bytes::Bytes;

let client = ClientBuilder::new()
    .base_url("https://api.example.com")?
    .json_parser(|body: &Bytes| {
        let slice = body.strip_prefix(b"\xef\xbb\xbf").unwrap_or(body);
        serde_json::from_slice(slice).map_err(|e| e.to_string())
    })
    .build()?;
```

For maximum performance on a single response, skip a global parser and use `Response::into_json_with` for a direct `Bytes` → `T` closure.

### Cancellation

```rust
use better_fetch::{CancellationToken, Client, Result};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    let token = CancellationToken::new();
    let client = Client::new("https://httpbin.org")?;

    let handle = tokio::spawn({
        let token = token.clone();
        let client = client.clone();
        async move {
            client
                .get("/delay/10")
                .cancellation_token(token)
                .send()
                .await
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    token.cancel();

    assert!(handle.await.unwrap().unwrap_err().is_cancelled());
    Ok(())
}
```

### Throw on HTTP error

```rust
// Default: Ok(Response) even for 404
let response = client.get("/missing").send().await?;

// Like upstream throw: true
let err = client
    .get("/missing")
    .throw_on_error(true)
    .send()
    .await
    .unwrap_err();
```

### Typed endpoint

Typed endpoints bind HTTP method, path template, and response type at compile time.
Path parameters use `E::Params` with [`.params()`](https://docs.rs/better-fetch/latest/better_fetch/struct.EndpointRequestBuilder.html#method.params);
query parameters use `E::Query` with [`.query()`](https://docs.rs/better-fetch/latest/better_fetch/struct.EndpointRequestBuilder.html#method.query)
when you want them on the wire.

**What compile-time guarantees apply**

| Part | Enforced before send? |
|------|----------------------|
| Path (`E::Params`) | Yes, when not `()` — type-state [`NeedsParams`](https://docs.rs/better-fetch/latest/better_fetch/struct.NeedsParams.html) |
| Body (`E::Body` on POST, etc.) | Yes, when the `macros` endpoint marks a required body — [`NeedsBody`](https://docs.rs/better-fetch/latest/better_fetch/struct.NeedsBody.html) |
| Query (`E::Query`) | **No** — typed, but [`.query()`](https://docs.rs/better-fetch/latest/better_fetch/struct.EndpointRequestBuilder.html#method.query) is optional; omit it to send no query string |
| Response (`E::Response`) | Typed deserialization via `.send_json()` |

`E::Query: Default` does **not** mean the library applies a default query on send — you must call `.query(...)?` to serialize `E::Query`. For required query params at runtime, use `schema-validate` strict or tests.

```rust
use better_fetch::{Client, Endpoint, Result, define_params};
use http::Method;
use serde::Deserialize;

define_params!(GetTodoParams for "/todos/:id" { id: u64 });

struct GetTodo;
impl Endpoint for GetTodo {
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/todos/:id";
    type Response = Todo;
    type Params = GetTodoParams;
    type Query = ();
}

#[derive(Deserialize)]
struct Todo { id: u64, title: String }

async fn example(client: &Client) -> Result<()> {
    let todo = client
        .call::<GetTodo>()
        .params(GetTodoParams { id: 1 })
        .send_json()
        .await?;
    Ok(())
}
```

With the `macros` feature, use `#[derive(EndpointParamsDerive)]` and `#[derive(EndpointQueryDerive)]`
instead of `define_params!` / `impl_serde_endpoint_query!`.

**POST + typed body (`NeedsBody`):** when using `#[derive(Endpoint)]` with `POST` and a non-unit `#[body]`,
you must call `.json()`, `.with_body()`, or `.body()` before `.send()` / `.send_json()` (see `examples/needs_body.rs`).

**Schema registry:** `#[endpoint(register)]` on `#[derive(Endpoint)]` generates `YourEndpoint::register(&mut SchemaRegistry)`.

With the `schema-validate` feature and a **strict** registry, runtime JSON Schema checks run when schemas are registered for that route: request body, response body (after `send()` or `StreamingResponse::collect()`), query string, and path params. Failures surface as [`Error::SchemaValidation`](https://docs.rs/better-fetch/latest/better_fetch/enum.Error.html#variant.SchemaValidation).

**Macros:** `#[query] MyQuery` on `#[derive(Endpoint)]` implements [`EndpointQuery`](https://docs.rs/better-fetch/latest/better_fetch/trait.EndpointQuery.html) for `MyQuery` (requires `Serialize`). Use `#[query_field]` for inline query fields on the endpoint struct instead.

Typed query example:

```rust
use better_fetch::{impl_serde_endpoint_query, Endpoint, Client};
use serde::Serialize;

#[derive(Default, Serialize)]
struct ListQuery { user_id: Option<u64> }
impl_serde_endpoint_query!(ListQuery);

// impl Endpoint { type Query = ListQuery; ... }
// client.call::<ListTodos>().query(ListQuery { user_id: Some(1) })?.send_json().await?;
```

`.get()` / `.post()` remain available for ad-hoc requests; only `client.call::<E>()` uses the typed builder.

### Form and multipart

```rust
// URL-encoded form
client
    .post("/login")
    .form([("user", "alice"), ("pass", "secret")])
    .send()
    .await?;

// Multipart (feature "multipart")
let form = better_fetch::multipart::Form::new().text("file", "hello");
client.post("/upload").multipart(form).send().await?;
```

Note: **automatic retry is not supported** with `.multipart()` (the body cannot be replayed). Use `.form`, JSON, or raw bytes if you need retries.

### Client builder

`ClientBuilder::build()` requires `.base_url(...)` — otherwise `Error::MissingBaseUrl`.

```rust
use better_fetch::{ClientBuilder, RetryPolicy};
use std::time::Duration;

let client = ClientBuilder::new()
    .base_url("https://api.example.com")?
    .retry(RetryPolicy::exponential(3, Duration::from_secs(1), Duration::from_secs(30)))
    .build()?;
```

### Concurrency limits

`ClientBuilder::max_in_flight` uses a tokio semaphore in the core client (counts retries as in-flight work). The `tower` feature’s `ConcurrencyLimitLayer` is a separate transport-level cap. Use **one** of these at a given limit unless you intentionally want two stacked caps (e.g. app-wide budget + per-host transport limit).

### Tower transport (`feature = "tower"`)

Wire a custom transport with `ClientBuilder::http_service`, `http_service_boxed`, or `transport_stack`. Stack helpers live in `better_fetch::tower::stack` (`build`, `ConcurrencyLimitLayer`, `with_buffer`, etc.).

[`ServiceBackend`](https://docs.rs/better-fetch/latest/better_fetch/tower/struct.ServiceBackend.html) clones the boxed Tower stack per request, so concurrent transport calls can run in parallel. Wrap the inner service with [`tower::buffer::Buffer`](https://docs.rs/tower/latest/tower/buffer/struct.Buffer.html) only when the inner service is not [`Clone`] or is expensive to clone (see `examples/tower_stack` and `stack::with_buffer`). When you do not need Tower middleware, use the default reqwest backend. Do not stack `max_in_flight` and `ConcurrencyLimitLayer` at the same numeric limit without intent.

### Response bodies: buffered vs streaming

| API | Use when |
|-----|----------|
| [`send()`](https://docs.rs/better-fetch/latest/better_fetch/struct.RequestBuilder.html#method.send) → [`Response`](https://docs.rs/better-fetch/latest/better_fetch/struct.Response.html) | Typical JSON APIs; full body in memory; hooks and retry predicates can inspect the body |
| [`send_stream()`](https://docs.rs/better-fetch/latest/better_fetch/struct.RequestBuilder.html#method.send_stream) → [`StreamingResponse`](https://docs.rs/better-fetch/latest/better_fetch/struct.StreamingResponse.html) | Large downloads, chunked bodies, incremental processing |
| [`collect()`](https://docs.rs/better-fetch/latest/better_fetch/struct.StreamingResponse.html#method.collect) on a stream | Opt back into the buffered `Response` API after streaming |

`send()` always buffers via `response.bytes().await` in the reqwest backend. `send_stream()` uses `bytes_stream()` and does not buffer until you call `collect()`.

**Streaming hooks:** [`on_response_stream`](https://docs.rs/better-fetch/latest/better_fetch/struct.Hooks.html#method.on_response_stream) and [`on_success_stream`](https://docs.rs/better-fetch/latest/better_fetch/struct.Hooks.html#method.on_success_stream) run on the streaming path (status + headers only). Buffered [`on_response`](https://docs.rs/better-fetch/latest/better_fetch/struct.Hooks.html#method.on_response) / [`on_success`](https://docs.rs/better-fetch/latest/better_fetch/struct.Hooks.html#method.on_success) are not invoked for `send_stream`.

**Retry on streams:** Status/header retries work as with `send()`. Custom [`RetryPolicy::with_should_retry`](https://docs.rs/better-fetch/latest/better_fetch/struct.RetryPolicy.html) predicates can peek at up to [`retry_body_peek_bytes`](https://docs.rs/better-fetch/latest/better_fetch/struct.ClientBuilder.html#method.retry_body_peek_bytes) (default 64 KiB, bounded by `max_response_bytes` when set). Without a custom predicate, the body is not read before retrying.

**Tower + streaming:** [`transport_stack`](https://docs.rs/better-fetch/latest/better_fetch/struct.ClientBuilder.html#method.transport_stack) wires separate buffered and streaming Tower stacks. Request middleware (e.g. `map_request`) runs on **both** `send()` and `send_stream()` when you configure both inner services in the closure.

**Other notes:**

- [`send_json`](https://docs.rs/better-fetch/latest/better_fetch/struct.RequestBuilder.html#method.send_json) is not available on streams — use `collect()` then `into_json()`, or deserialize from chunks yourself.
- Cancellation is cooperative: the stream wakes on cancel when the inner read is pending, not inside a blocking OS read.
- `collect()` without `max_response_bytes` can use unbounded memory; set a cap for untrusted payloads.

Optional caps: [`ClientBuilder::max_response_bytes`](https://docs.rs/better-fetch/latest/better_fetch/struct.ClientBuilder.html#method.max_response_bytes) and per-request [`.max_response_bytes()`](https://docs.rs/better-fetch/latest/better_fetch/struct.RequestBuilder.html#method.max_response_bytes) yield [`Error::BodyTooLarge`](https://docs.rs/better-fetch/latest/better_fetch/enum.Error.html#variant.BodyTooLarge) when exceeded.

See [`examples/streaming.rs`](examples/streaming.rs), [`examples/buffered_vs_streaming.rs`](examples/buffered_vs_streaming.rs), [`examples/throw_on_error.rs`](examples/throw_on_error.rs), and [`examples/cancel.rs`](examples/cancel.rs).

### Plugins

`Plugin::init` receives `PreparedRequest` with `url`, `path`, `method`, and `headers` (after auth, before lifecycle hooks). Use it to rewrite URLs or inspect auth headers.

### Logging

[`LoggerPlugin`](https://docs.rs/better-fetch/latest/better_fetch/struct.LoggerPlugin.html) emits `tracing` spans (`http.request`, `http.response`). Install a subscriber in your app, for example:

```rust
tracing_subscriber::fmt::init();
```

For OpenTelemetry export via `tracing-opentelemetry`, see [docs/observability.md](docs/observability.md).

## Features

| Feature | Description |
|---------|-------------|
| `json`, `rustls-tls` (default) | JSON API + TLS via rustls (reqwest is always the default backend) |
| `native-tls` | Platform TLS instead of rustls (do not combine with `rustls-tls`) |
| `blocking`, `cookies` | Passed through to reqwest |
| `multipart` | `RequestBuilder::multipart` + `better_fetch::multipart` re-export |
| `schema` / `openapi` | `schemars` registry, strict routes, and OpenAPI 3.0 export |
| `tower` / `tower-http` | Tower `Service` transport stack (`better_fetch::tower`) |
| `validate` | Request/response validation with `garde` (`json_validated`, `send_json_validated`) |
| `schema-validate` | Runtime JSON Schema validation (`jsonschema`) when registry is strict (request/response body, query, params) |
| `macros` | `#[derive(Endpoint)]`, `define_params!`, `EndpointParamsDerive`, `EndpointQueryDerive` |
| `miette` | [`DiagnosticError`](https://docs.rs/better-fetch/latest/better_fetch/struct.DiagnosticError.html) — **Cargo feature only** (wrap errors in your app; not a `Plugin`) |
| `otel` | Re-exports `opentelemetry`, `opentelemetry_sdk`, `tracing_opentelemetry`; see [docs/observability.md](docs/observability.md) |
| `full` | Enables `json`, `rustls-tls`, `macros`, `schema`, `validate`, `schema-validate`, `miette`, `otel`, `tower`, `multipart`, `openapi` |

Use `better_fetch::prelude::*` for common imports in application code.

See [CHANGELOG.md](https://github.com/sebasxsala/better-fetch-rs/blob/main/CHANGELOG.md) for release notes.

## Examples

```bash
cargo run -p better-fetch --example streaming
cargo run -p better-fetch --example buffered_vs_streaming --features json
cargo run -p better-fetch --example throw_on_error --features json
cargo run -p better-fetch --example cancel
cargo run -p better-fetch --example basic
cargo run -p better-fetch --example typed_endpoint --features json
cargo run -p better-fetch --example tower_stack --features tower,json
cargo run -p better-fetch --example tower_vs_streaming --features tower,json
cargo run -p better-fetch --example multipart --features multipart
cargo run -p better-fetch --example retry
cargo run -p better-fetch --example openapi_export --features openapi
cargo run -p better-fetch --example upload_stream --features json
cargo run -p better-fetch --example validated_request --features validate
cargo test -p better-fetch
cargo test -p better-fetch --features default,validate,tower,multipart,macros
```

## Crates in this repository

| crates.io | Role |
|-----------|------|
| [better-fetch](https://crates.io/crates/better-fetch) | Main library |
| [typed-fetch](https://crates.io/crates/typed-fetch) | Re-export alias |
| [api-fetch](https://crates.io/crates/api-fetch) | Re-export alias |
| [better-fetch-macros](https://crates.io/crates/better-fetch-macros) | Proc-macros for typed endpoint params/query |

## Publishing

Push a semver tag matching `version` in the workspace `Cargo.toml`:

```bash
git tag v0.4.0
git push origin v0.4.0
```

That runs [`.github/workflows/release.yml`](.github/workflows/release.yml): CI, crates.io trusted publishing (crates `better-fetch-macros`, `better-fetch`, `typed-fetch`, `api-fetch`), then a GitHub Release with the matching section from [CHANGELOG.md](CHANGELOG.md).

**Trusted publishing (one-time per crate):** on [crates.io](https://crates.io) → crate → **Settings** → **Trusted Publishing** — GitHub owner `sebasxsala`, repository `better-fetch-rs`, workflow `release.yml`, environment `release`. Create the `release` environment on GitHub if you use approval gates. Each crate must have been published manually at least once before trusted publishing works.

**Manual order** (if needed): `cargo publish -p better-fetch-macros`, then `better-fetch`, then `typed-fetch`, then `api-fetch`.

## License

MIT — see [LICENSE](LICENSE). Upstream inspiration: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
