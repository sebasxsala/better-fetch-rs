# better-fetch

Typed HTTP client layer on top of [reqwest](https://docs.rs/reqwest), inspired by
[@better-fetch/fetch](https://better-fetch.vercel.app/docs). Independent Rust implementation.

## Installation

Pick one crate name (same library):

```toml
[dependencies]
better-fetch = "0.1"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Aliases on crates.io: [`typed-fetch`](https://crates.io/crates/typed-fetch), [`api-fetch`](https://crates.io/crates/api-fetch) — `pub use better_fetch::*`.

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

    println!("{todo:#?}");
    Ok(())
}
```

## Highlights

- **Builder API** — `Client::builder()`, per-request `.timeout()`, `.retry()`, `.auth()`, headers, JSON body.
- **Retries** — linear, exponential, or custom; hooks on retry.
- **Hooks & plugins** — compose client and plugin hooks; optional `LoggerPlugin` (requires a `tracing` subscriber in your app).
- **Errors** — `Result` + `?`; `Error::api_json()` to parse JSON error bodies from APIs.
- **Typed endpoints** — `Endpoint` trait + `client.call::<E>()`.
- **Testing** — inject `ClientBuilder::backend(Arc<dyn HttpBackend>)`.

## Features

| Feature | Description |
|---------|-------------|
| `reqwest`, `json`, `rustls-tls` (default) | Async client, JSON, TLS |
| `native-tls` | Platform TLS |
| `blocking`, `multipart`, `cookies` | Passed through to reqwest |
| `schema` / `openapi` | `schemars` registry and minimal OpenAPI document builder |
| `tower` / `tower-http` | Tower `Service` transport stack (`better_fetch::tower`) |
| `validate` | Response validation with `garde` (`send_json_validated`) |
| `macros` | Reserved `better-fetch-macros` crate |

Enable optional stacks in `Cargo.toml`, for example:

```toml
better-fetch = { version = "0.1", features = ["tower", "validate"] }
```

See [CHANGELOG.md](CHANGELOG.md) for the full 0.1.0 scope.

## Examples

```bash
cargo run -p better-fetch --example basic
cargo run -p better-fetch --example retry
cargo test -p better-fetch
cargo test -p better-fetch --features openapi,validate,tower,json
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
