# better-fetch

Typed HTTP client layer on top of [reqwest](https://docs.rs/reqwest), inspired by
[@better-fetch/fetch](https://better-fetch.vercel.app/docs). Independent Rust implementation.

## Installation

Pick one crate name (same library):

```toml
[dependencies]
better-fetch = "0.2"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
tokio-util = "0.7"
```

Aliases on crates.io: [`typed-fetch`](https://crates.io/crates/typed-fetch), [`api-fetch`](https://crates.io/crates/api-fetch) — `pub use better_fetch::*`.

Optional features:

```toml
better-fetch = { version = "0.2", features = ["tower", "validate", "multipart"] }
```

## Quick start

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

    // Or in one step:
    let todo: Todo = client.get("/todos/:id").param("id", 1).send_json().await?;

    // Hot path (no extra await): client.get(...).send().await?.into_json()
    println!("{todo:#?}");
    Ok(())
}
```

## Highlights

- **Builder API** — `Client::builder()`, per-request `.timeout()`, `.retry()`, `.auth()`, headers, JSON body.
- **Retries** — linear, exponential, or `count`; `Retry-After`, jitter, custom `should_retry`; default retry on 408/429/502/503/504.
- **Hooks & plugins** — compose client and plugin hooks; optional `LoggerPlugin` (requires a `tracing` subscriber in your app).
- **Errors** — `Result` + `?`; `Error::api_json()` to parse JSON error bodies from APIs; `Error::hook()` from `on_request` / `on_response` hooks.
- **Typed endpoints** — `Endpoint` trait + `client.call::<E>()` → `EndpointRequestBuilder` with typed `send_json()`.
- **Testing** — inject `ClientBuilder::backend(Arc<dyn HttpBackend>)`.
- **Cancellation** — `CancellationToken` per request; cooperative abort during requests and retry backoff.
- **Throw mode** — `throw_on_error(true)` makes `send()` return `Err` on non-2xx (like upstream `throw: true`).
- **Form & multipart** — `.form([...])` for url-encoded bodies; `.multipart(form)` with feature `multipart`.

### Request options

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
| `.send` / `.send_json` | Execute request |
| `.json_parser` | Custom `Bytes` → `Value` parser (feature `json`; see below) |

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

```rust
use better_fetch::{Client, Endpoint, Result};
use http::Method;
use serde::Deserialize;

struct GetTodo;
impl Endpoint for GetTodo {
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/todos/:id";
    type Response = Todo;
    type Params = ();
    type Query = ();
}

#[derive(Deserialize)]
struct Todo { id: u64, title: String }

async fn example(client: &Client) -> Result<()> {
    let todo = client
        .call::<GetTodo>()
        .param("id", 1)
        .send_json()
        .await?;
    Ok(())
}
```

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

Wire a custom transport with `ClientBuilder::http_service`, `http_service_boxed`, or `transport_stack`. Stack helpers live in `better_fetch::tower::stack` (`build`, `ConcurrencyLimitLayer`, etc.).

**Production pattern:** wrap the inner service with [`tower::buffer::Buffer`](https://docs.rs/tower/latest/tower/buffer/struct.Buffer.html) (its worker is spawned on the Tokio runtime), then pass the buffered service to the client. See `examples/tower_stack`.

[`ServiceBackend`](https://docs.rs/better-fetch/latest/better_fetch/tower/struct.ServiceBackend.html) holds a `Mutex` around the boxed service, so concurrent requests still take turns at the transport lock. When you do not need Tower middleware, use the default reqwest backend (no transport mutex). Do not stack `max_in_flight` and `ConcurrencyLimitLayer` at the same numeric limit without intent.

### Response bodies and size limits

Every response is read fully into memory (`Bytes`) before you get a `Response`. This fits typical JSON APIs. It is not a streaming client: large downloads, chunked bodies, or custom size limits should use reqwest (or another backend) directly. Error types may clone the body for debugging (`Error::Http`, `Error::Deserialize`).

### Plugins

`Plugin::init` receives `PreparedRequest` with `url`, `path`, `method`, and `headers` (after auth, before lifecycle hooks). Use it to rewrite URLs or inspect auth headers.

## Features

| Feature | Description |
|---------|-------------|
| `reqwest`, `json`, `rustls-tls` (default) | Async client, JSON, TLS |
| `native-tls` | Platform TLS |
| `blocking`, `cookies` | Passed through to reqwest |
| `multipart` | `RequestBuilder::multipart` + `better_fetch::multipart` re-export |
| `schema` / `openapi` | `schemars` registry, strict routes, and OpenAPI 3.0 export |
| `tower` / `tower-http` | Tower `Service` transport stack (`better_fetch::tower`) |
| `validate` | Response validation with `garde` (`send_json_validated`) |
| `macros` | Reserved `better-fetch-macros` crate |

See [CHANGELOG.md](CHANGELOG.md) for release notes.

## Examples

```bash
cargo run -p better-fetch --example basic
cargo run -p better-fetch --example tower_stack --features tower,json
cargo run -p better-fetch --example multipart --features multipart
cargo run -p better-fetch --example retry
cargo test -p better-fetch
cargo test -p better-fetch --features default,validate,tower,multipart
```

## Crates in this repository

| crates.io | Role |
|-----------|------|
| [better-fetch](https://crates.io/crates/better-fetch) | Main library |
| [typed-fetch](https://crates.io/crates/typed-fetch) | Re-export alias |
| [api-fetch](https://crates.io/crates/api-fetch) | Re-export alias |
| [better-fetch-macros](https://crates.io/crates/better-fetch-macros) | Proc macros (reserved) |

## License

MIT — see [LICENSE](LICENSE). Upstream inspiration: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
