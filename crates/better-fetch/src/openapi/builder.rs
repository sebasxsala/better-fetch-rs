//! [`OpenApiBuilder`] — builds [`OpenApiDocument`] from a [`SchemaRegistry`](crate::schema::SchemaRegistry).

use indexmap::IndexMap;

use crate::schema::{EndpointSchema, SchemaRegistry};

use super::convert::{
    is_null_schema, operation_id, parameters_from_schema, path_param_names, schema_ref,
    to_openapi_path, SchemaCatalog,
};
use super::document::{
    OpenApiComponents, OpenApiDocument, OpenApiInfo, OpenApiMediaType, OpenApiOperation,
    OpenApiRequestBody, OpenApiResponse, OpenApiResponses, OpenApiServer,
};

const JSON_CONTENT: &str = "application/json";

/// Builds an OpenAPI 3.0 document from a [`SchemaRegistry`].
#[derive(Debug, Default)]
pub struct OpenApiBuilder {
    title: String,
    version: String,
    description: Option<String>,
    servers: Vec<OpenApiServer>,
}

impl OpenApiBuilder {
    /// Creates a builder with default title and version.
    pub fn new() -> Self {
        Self {
            title: "API".into(),
            version: "0.1.0".into(),
            description: None,
            servers: Vec::new(),
        }
    }

    /// Sets the API title in `info.title`.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets `info.version`.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Sets `info.description`.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Adds a server URL.
    pub fn server(mut self, url: impl Into<String>) -> Self {
        self.servers.push(OpenApiServer {
            url: url.into(),
            description: None,
        });
        self
    }

    /// Adds a server URL with description.
    pub fn server_with_description(
        mut self,
        url: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        self.servers.push(OpenApiServer {
            url: url.into(),
            description: Some(description.into()),
        });
        self
    }

    /// Builds an OpenAPI 3.0 document from a [`SchemaRegistry`](crate::schema::SchemaRegistry).
    ///
    /// See the `openapi_export` example in the repository for a full workflow.
    pub fn from_registry(&self, registry: &SchemaRegistry) -> OpenApiDocument {
        let mut catalog = SchemaCatalog::default();
        let mut paths: IndexMap<String, IndexMap<String, OpenApiOperation>> = IndexMap::new();

        for entry in registry.entries() {
            let operation = build_operation(entry, &mut catalog);
            let openapi_path = to_openapi_path(&entry.path);
            let method = entry.method.as_str().to_ascii_lowercase();
            paths
                .entry(openapi_path)
                .or_default()
                .insert(method, operation);
        }

        let components = if catalog.schemas.is_empty() {
            None
        } else {
            Some(OpenApiComponents {
                schemas: catalog.schemas,
            })
        };

        OpenApiDocument {
            openapi: "3.0.3".into(),
            info: OpenApiInfo {
                title: self.title.clone(),
                version: self.version.clone(),
                description: self.description.clone(),
            },
            servers: if self.servers.is_empty() {
                None
            } else {
                Some(self.servers.clone())
            },
            paths,
            components,
        }
    }
}

fn build_operation(entry: &EndpointSchema, catalog: &mut SchemaCatalog) -> OpenApiOperation {
    let path_names = path_param_names(&entry.path);
    let prefix = operation_id(&entry.method, &entry.path);

    let mut parameters = Vec::new();
    if let Some(params_schema) = &entry.params_schema {
        parameters.extend(parameters_from_schema(
            params_schema,
            "path",
            &path_names,
            catalog,
            &format!("{prefix}Path"),
        ));
    }
    if let Some(query_schema) = &entry.query_schema {
        parameters.extend(parameters_from_schema(
            query_schema,
            "query",
            &path_names,
            catalog,
            &format!("{prefix}Query"),
        ));
    }

    let request_body = entry.request_schema.as_ref().and_then(|schema| {
        if is_null_schema(schema) {
            return None;
        }
        let ref_path = catalog.register(&format!("{prefix}Request"), schema)?;
        Some(OpenApiRequestBody {
            description: Some("Request body".into()),
            required: true,
            content: json_content(schema_ref(ref_path)),
        })
    });

    let mut response_map: IndexMap<String, OpenApiResponse> = IndexMap::new();
    if let Some(response_schema) = &entry.response_schema {
        if !is_null_schema(response_schema) {
            if let Some(ref_path) = catalog.register(&format!("{prefix}Response"), response_schema)
            {
                response_map.insert(
                    "200".into(),
                    OpenApiResponse {
                        description: "Success".into(),
                        content: Some(json_content(schema_ref(ref_path))),
                    },
                );
            }
        }
    }
    if response_map.is_empty() {
        response_map.insert(
            "200".into(),
            OpenApiResponse {
                description: "Success".into(),
                content: None,
            },
        );
    }

    let summary = format!("{} {}", entry.method, entry.path);

    OpenApiOperation {
        summary: Some(summary),
        description: None,
        operation_id: Some(prefix),
        parameters,
        request_body,
        responses: OpenApiResponses {
            statuses: response_map,
        },
    }
}

fn json_content(schema: super::document::OpenApiSchemaRef) -> IndexMap<String, OpenApiMediaType> {
    let mut map = IndexMap::new();
    map.insert(JSON_CONTENT.into(), OpenApiMediaType { schema });
    map
}
