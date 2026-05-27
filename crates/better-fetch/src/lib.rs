//! # better-fetch
//!
//! Typed HTTP client layer on top of [reqwest](https://docs.rs/reqwest), inspired by
//! [@better-fetch/fetch](https://better-fetch.vercel.app/docs). This crate is not affiliated
//! with the upstream TypeScript project.

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
