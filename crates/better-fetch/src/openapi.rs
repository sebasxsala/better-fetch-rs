//! Minimal OpenAPI document builder (requires `openapi` feature).

use indexmap::IndexMap;

use crate::schema::SchemaRegistry;

/// OpenAPI 3.0 document (minimal subset for v1).
#[derive(Debug, Clone, serde::Serialize)]
pub struct OpenApiDocument {
    pub openapi: String,
    pub info: OpenApiInfo,
    pub paths: IndexMap<String, IndexMap<String, OpenApiOperation>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OpenApiInfo {
    pub title: String,
    pub version: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OpenApiOperation {
    pub summary: Option<String>,
    pub responses: IndexMap<String, OpenApiResponse>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OpenApiResponse {
    pub description: String,
}

/// Builds an OpenAPI document from a [`SchemaRegistry`].
#[derive(Debug, Default)]
pub struct OpenApiBuilder {
    title: String,
    version: String,
}

impl OpenApiBuilder {
    pub fn new() -> Self {
        Self {
            title: "API".into(),
            version: "0.1.0".into(),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn from_registry(&self, registry: &SchemaRegistry) -> OpenApiDocument {
        let mut paths: IndexMap<String, IndexMap<String, OpenApiOperation>> = IndexMap::new();

        for entry in registry.entries() {
            let method = entry.method.as_str().to_lowercase();
            let path_entry = paths.entry(entry.path.clone()).or_default();
            path_entry.insert(
                method,
                OpenApiOperation {
                    summary: Some(format!("{:?} {}", entry.method, entry.path)),
                    responses: IndexMap::from([(
                        "200".into(),
                        OpenApiResponse {
                            description: "Success".into(),
                        },
                    )]),
                },
            );
        }

        OpenApiDocument {
            openapi: "3.0.3".into(),
            info: OpenApiInfo {
                title: self.title.clone(),
                version: self.version.clone(),
            },
            paths,
        }
    }
}
