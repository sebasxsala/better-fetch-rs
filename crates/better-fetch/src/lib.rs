//! # better-fetch
//!
//! Typed HTTP client layer on top of [reqwest](https://docs.rs/reqwest), inspired by
//! [@better-fetch/fetch](https://better-fetch.vercel.app/docs). This crate is not affiliated
//! with the upstream TypeScript project.
//!
//! ## Quick flow
//!
//! 1. Create a [`Client`] (or [`ClientBuilder`]) with a base URL.
//! 2. Start a request with [`Client::get`] / [`Client::post`] / [`Client::call`].
//! 3. Configure path params, query, body, auth, retries on [`RequestBuilder`].
//! 4. Execute with [`RequestBuilder::send`] (returns [`Response`]) or [`RequestBuilder::send_json`]
//!    (deserializes JSON and fails on non-2xx).
//!
//! ## Cargo features
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `json` (default) | JSON bodies, `send_json`, custom [`JsonParserFn`] |
//! | `reqwest` / `rustls-tls` (default) | Reqwest backend |
//! | `multipart` | [`RequestBuilder::multipart`] |
//! | `tower` | Tower transport stack via [`ClientBuilder::transport_stack`] |
//! | `schema` | [`SchemaRegistry`] route metadata |
//! | `openapi` | OpenAPI 3.0 export from schema registry |
//! | `validate` | Garde validation on JSON responses |
//! | `macros` | Proc-macro helpers (reserved) |
//!
//! See the [repository README](https://github.com/sebasxsala/better-fetch-rs) for full examples.
//!
//! ## Example
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
#[cfg(feature = "validate")]
mod validate_json;

#[cfg(feature = "schema")]
pub mod schema;

#[cfg(feature = "openapi")]
pub mod openapi;

#[cfg(feature = "tower")]
pub mod tower;

pub use auth::{AsyncTokenProvider, Auth, TokenSource};
pub use backend::{HttpBackend, HttpBody, HttpRequest, HttpResponse, ReqwestBackend};
pub use cancel::CancellationToken;
#[cfg(feature = "multipart")]
/// Re-export of [reqwest multipart](https://docs.rs/reqwest/latest/reqwest/multipart/) types (feature `multipart`).
pub use reqwest::multipart;
pub use client::{Client, ClientBuilder, ClientConfig};
pub use endpoint::{Endpoint, EndpointParams, EndpointQuery, EndpointRequestBuilder};
pub use error::Error;
pub use hooks::{ErrorContext, Hooks, RequestContext, ResponseContext, SuccessContext};
#[cfg(feature = "json")]
pub use json_parser::{json_parser, serde_json_parser, JsonParserFn};
pub use plugin::{Plugin, PluginRegistry, PreparedRequest};
pub use plugins::LoggerPlugin;
pub use request::RequestBuilder;
pub use response::Response;
pub use retry::{default_should_retry, parse_retry_after, RetryPolicy, ShouldRetryFn};
pub use url_build::QueryValue;

#[cfg(feature = "schema")]
pub use schema::{EndpointSchema, SchemaRegistry};

#[cfg(feature = "openapi")]
pub use openapi::{
    OpenApiBuilder, OpenApiComponents, OpenApiDocument, OpenApiInfo, OpenApiOperation, OpenApiSchemaRef,
    OpenApiServer,
};

#[cfg(feature = "tower")]
pub use tower::{BoxHttpService, ReqwestHttpService, ServiceBackend};

/// Result alias using [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
