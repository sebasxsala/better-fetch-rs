//! Convenient re-exports for application code.
//!
//! ```
//! use better_fetch::prelude::*;
//! ```

#[cfg(feature = "json")]
pub use crate::api_response::{into_api_result, ApiResponseExt};
pub use crate::auth::{Auth, TokenSource};
pub use crate::backend::RecordedBodyKind;
pub use crate::backend::{HttpBackend, RecordingBackend, ReqwestBackend};
pub use crate::cancel::CancellationToken;
pub use crate::client::{Client, ClientBuilder, ClientConfig};
pub use crate::endpoint::{
    DefaultParamsInitial, Endpoint, EndpointBody, EndpointHeaders, EndpointParams,
    EndpointParamsInitial, EndpointQuery, EndpointRequestBuilder, NeedsBody, NeedsParams, Ready,
};
pub use crate::error::{Error, TransportKind};
pub use crate::hooks::Hooks;
pub use crate::request::RequestBuilder;
pub use crate::response::{Response, ResponseBodyKind};
pub use crate::retry::RetryPolicy;
pub use crate::streaming::StreamingResponse;
pub use crate::url_build::QueryValue;
pub use crate::Result;

#[cfg(feature = "json")]
pub use crate::endpoint::apply_serialized_query;
#[cfg(feature = "json")]
pub use crate::{define_params, endpoint, impl_serde_endpoint_query};

#[cfg(feature = "macros")]
pub use crate::{EndpointDerive, EndpointParamsDerive, EndpointQueryDerive};

#[cfg(feature = "schema")]
pub use crate::schema::SchemaRegistry;
