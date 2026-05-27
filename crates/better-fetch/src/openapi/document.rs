//! Serializable OpenAPI 3.0 document types.

use indexmap::IndexMap;
use serde::Serialize;

/// OpenAPI 3.0 document.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiDocument {
    pub openapi: String,
    pub info: OpenApiInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<OpenApiServer>>,
    pub paths: IndexMap<String, IndexMap<String, OpenApiOperation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<OpenApiComponents>,
}

/// API metadata (`info`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiInfo {
    pub title: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Server URL (`servers[]`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiServer {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Reusable schemas (`components.schemas`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiComponents {
    pub schemas: IndexMap<String, serde_json::Value>,
}

/// JSON Schema reference or inline schema.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum OpenApiSchemaRef {
    Ref {
        #[serde(rename = "$ref")]
        ref_path: String,
    },
    Inline(serde_json::Value),
}

/// Path or query parameter.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiParameter {
    pub name: String,
    #[serde(rename = "in")]
    pub location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub required: bool,
    pub schema: OpenApiSchemaRef,
}

/// Request body with JSON content.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub required: bool,
    pub content: IndexMap<String, OpenApiMediaType>,
}

/// Media type entry (`content.application/json`).
#[derive(Debug, Clone, Serialize)]
pub struct OpenApiMediaType {
    pub schema: OpenApiSchemaRef,
}

/// Operation responses map.
#[derive(Debug, Clone, Serialize)]
pub struct OpenApiResponses {
    #[serde(flatten)]
    pub statuses: IndexMap<String, OpenApiResponse>,
}

/// Single HTTP response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiResponse {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<IndexMap<String, OpenApiMediaType>>,
}

/// Path operation (`get`, `post`, …).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiOperation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<OpenApiParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<OpenApiRequestBody>,
    pub responses: OpenApiResponses,
}

impl OpenApiDocument {
    /// Serialize the document to pretty-printed JSON.
    pub fn to_json_pretty(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Serialize the document to JSON.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}
