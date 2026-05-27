//! # better-fetch
//!
//! Typed HTTP client layer on top of [reqwest](https://docs.rs/reqwest), inspired by
//! [@better-fetch/fetch](https://better-fetch.vercel.app/docs). This crate is not affiliated
//! with the upstream TypeScript project.
//!
//! ## Quick start
//!
//! ```no_run
//! use better_fetch::{Client, Result};
//! use serde::Deserialize;
//!
//! #[derive(Debug, Deserialize)]
//! struct Todo {
//!     user_id: u64,
//!     id: u64,
//!     title: String,
//!     completed: bool,
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = Client::new("https://jsonplaceholder.typicode.com")?;
//!     let todo: Todo = client
//!         .get("/todos/:id")
//!         .param("id", 1)
//!         .send()
//!         .await?
//!         .json()
//!         .await?;
//!     println!("{todo:#?}");
//!     Ok(())
//! }
//! ```

mod url_build;

pub mod auth;
pub mod backend;
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
pub use backend::{HttpBackend, HttpRequest, HttpResponse, ReqwestBackend};
pub use client::{Client, ClientBuilder, ClientConfig};
pub use endpoint::Endpoint;
pub use error::Error;
pub use hooks::{ErrorContext, Hooks, RequestContext, ResponseContext, SuccessContext};
#[cfg(feature = "json")]
pub use json_parser::{json_parser, serde_json_parser, JsonParserFn};
pub use plugin::{Plugin, PluginRegistry, PreparedRequest};
pub use plugins::LoggerPlugin;
pub use request::RequestBuilder;
pub use response::Response;
pub use retry::{default_should_retry, RetryPolicy, ShouldRetryFn};

#[cfg(feature = "schema")]
pub use schema::{EndpointSchema, SchemaRegistry};

#[cfg(feature = "openapi")]
pub use openapi::{OpenApiBuilder, OpenApiDocument};

#[cfg(feature = "tower")]
pub use tower::{BoxHttpService, ReqwestHttpService, ServiceBackend};

/// Result alias using [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
