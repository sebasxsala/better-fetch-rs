#![cfg_attr(docsrs, feature(doc_cfg))]

//! # better-fetch
//!
//! Typed HTTP client layer on top of [reqwest](https://docs.rs/reqwest), inspired by
//! [@better-fetch/fetch](https://better-fetch.vercel.app/docs). This crate is not affiliated
//! with the upstream TypeScript project.
//!
//! ## Quick flow
//!
//! 1. Create a [`Client`] (or [`ClientBuilder`]) with a base URL.
//! 2. Start a request with [`Client::get`] / [`Client::post`] (flexible [`RequestBuilder`])
//!    or [`Client::call`] (typed [`Endpoint`] routes).
//! 3. Configure path params, query, body, auth, retries on the builder.
//! 4. Execute with [`RequestBuilder::send`] (buffered [`Response`]),
//!    [`RequestBuilder::send_stream`] (incremental [`StreamingResponse`]),
//!    [`send_json`](RequestBuilder::send_json), or [`EndpointRequestBuilder::send_json`](EndpointRequestBuilder::send_json).
//!
//! ## Buffered vs streaming
//!
//! - **`send` / `send_json`** — full body in memory; hooks and retry predicates can read the body.
//! - **`send_stream`** — `bytes_stream()` from reqwest; use [`StreamingResponse::collect`] to buffer when needed.
//!   See the [`streaming`] module for limits (hooks, custom retry predicates, Tower backend).
//!
//! Use [`.get()`](Client::get) when you want string paths and a typed JSON response (`send_json::<T>()`).
//! Use [`Client::call`] when method, path, params, query, and response are bound to an [`Endpoint`] type.
//!
//! ## Cargo features
//!
//! The client always uses [reqwest](https://docs.rs/reqwest) as the default HTTP backend.
//! Enable crate features to turn on reqwest capabilities and optional APIs.
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `json` (default) | JSON bodies, `send_json`, custom [`JsonParserFn`] |
//! | `rustls-tls` (default) | TLS via rustls (enable `native-tls` instead, not both) |
//! | `native-tls` | TLS via the platform stack (do not combine with `rustls-tls`) |
//! | `multipart` | [`RequestBuilder::multipart`] |
//! | `tower` | Tower transport stack via [`ClientBuilder::transport_stack`] (implies `rustls-tls`) |
//! | `schema` | [`SchemaRegistry`] route metadata |
//! | `openapi` | OpenAPI 3.0 export from schema registry |
//! | `validate` | Garde validation on JSON responses |
//! | `blocking`, `cookies` | Passed through to reqwest |
//! | `macros` | `#[derive(EndpointParamsDerive)]`, `#[derive(EndpointQueryDerive)]` proc-macros |
//!
//! See the [repository README](https://github.com/sebasxsala/better-fetch-rs) for full examples.
//!
//! ## Example (`.get()` — flexible path, typed response)
//!
//! ```no_run
//! # use better_fetch::{Client, Result};
//! # use serde::Deserialize;
//! # #[derive(Debug, Deserialize)]
//! # #[serde(rename_all = "camelCase")]
//! # struct Todo { user_id: u64, id: u64, title: String, completed: bool }
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! let client = Client::new("https://jsonplaceholder.typicode.com")?;
//!
//! // send() returns Response for any status; json() fails on non-2xx
//! let todo: Todo = client
//!     .get("/todos/:id")
//!     .param("id", 1)
//!     .send()
//!     .await?
//!     .json()
//!     .await?;
//!
//! // Or in one step:
//! let todo: Todo = client.get("/todos/:id").param("id", 1).send_json().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Example (typed endpoint — method, path, params, response)
//!
//! ```no_run
//! # use better_fetch::{Client, Endpoint, Result, define_params};
//! # use http::Method;
//! # use serde::Deserialize;
//! define_params!(GetTodoParams for "/todos/:id" { id: u64 });
//!
//! struct GetTodo;
//! impl Endpoint for GetTodo {
//!     const METHOD: Method = Method::GET;
//!     const PATH: &'static str = "/todos/:id";
//!     type Response = Todo;
//!     type Params = GetTodoParams;
//!     type Query = ();
//! }
//!
//! # #[derive(Deserialize)]
//! # struct Todo { id: u64, title: String }
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! let client = Client::new("https://jsonplaceholder.typicode.com")?;
//! let todo = client
//!     .call::<GetTodo>()
//!     .params(GetTodoParams { id: 1 })
//!     .send_json()
//!     .await?;
//! # Ok(())
//! # }
//! ```

mod url_build;

pub mod auth;
pub mod backend;
pub mod cancel;
pub mod client;
pub mod endpoint;
pub mod error;
pub mod hooks;
#[cfg(feature = "json")]
mod json_parser;

pub mod plugin;
pub mod plugins;
pub mod request;
pub mod response;
pub mod retry;
pub mod streaming;
#[cfg(feature = "validate")]
mod validate_json;

#[cfg(feature = "schema")]
pub mod schema;

#[cfg(feature = "openapi")]
pub mod openapi;

#[cfg(feature = "tower")]
pub mod tower;

pub use auth::{AsyncTokenProvider, Auth, TokenSource};
pub use backend::{
    HttpBackend, HttpBody, HttpRequest, HttpResponse, HttpStreamingResponse, ReqwestBackend,
};
pub use cancel::CancellationToken;
pub use client::{Client, ClientBuilder, ClientConfig};
pub use endpoint::{
    Endpoint, EndpointParams, EndpointParamsInitial, EndpointQuery, EndpointRequestBuilder,
    NeedsParams, ParamsBuilderState, Ready,
};

#[cfg(feature = "macros")]
pub use better_fetch_macros::{
    EndpointParams as EndpointParamsDerive, EndpointQuery as EndpointQueryDerive,
};
pub use error::{Error, TransportKind};
pub use hooks::{
    ErrorContext, Hooks, RequestContext, ResponseContext, StreamingResponseContext,
    StreamingResponseMeta, StreamingSuccessContext, SuccessContext,
};
#[cfg(feature = "json")]
pub use json_parser::{json_parser, serde_json_parser, JsonParserFn};
pub use plugin::{Plugin, PluginRegistry, PreparedRequest};
pub use plugins::LoggerPlugin;
pub use request::RequestBuilder;
#[cfg(feature = "multipart")]
/// Re-export of [reqwest multipart](https://docs.rs/reqwest/latest/reqwest/multipart/) types (feature `multipart`).
pub use reqwest::multipart;
pub use response::Response;
pub use retry::{default_should_retry, parse_retry_after, RetryPolicy, ShouldRetryFn};
pub use streaming::{BodyStream, StreamingResponse};
pub use url_build::QueryValue;

#[cfg(feature = "schema")]
pub use schema::{EndpointSchema, SchemaRegistry};

#[cfg(feature = "openapi")]
pub use openapi::{
    OpenApiBuilder, OpenApiComponents, OpenApiDocument, OpenApiInfo, OpenApiOperation,
    OpenApiSchemaRef, OpenApiServer,
};

#[cfg(feature = "tower")]
pub use tower::{BoxHttpService, ReqwestHttpService, ServiceBackend};

/// Result alias using [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
