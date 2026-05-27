//! OpenAPI 3.0 document builder (requires `openapi` feature).
//!
//! Builds a spec from [`SchemaRegistry`](crate::schema::SchemaRegistry) including
//! `components.schemas`, request bodies, response content, and parameters.

mod builder;
mod convert;
mod document;

pub use builder::OpenApiBuilder;
pub use document::{
    OpenApiComponents, OpenApiDocument, OpenApiInfo, OpenApiMediaType, OpenApiOperation,
    OpenApiParameter, OpenApiRequestBody, OpenApiResponse, OpenApiResponses, OpenApiSchemaRef,
    OpenApiServer,
};
