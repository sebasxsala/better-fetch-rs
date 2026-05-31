//! Schema registry for endpoint metadata (requires `schema` feature).

use http::Method;
use schemars::schema::RootSchema;
use schemars::JsonSchema;
use std::collections::HashMap;

use crate::error::Error;
use crate::Result;

/// Metadata describing a single endpoint for documentation and codegen.
#[derive(Debug, Clone)]
pub struct EndpointSchema {
    /// Route path template.
    pub path: String,
    /// HTTP method.
    pub method: Method,
    /// JSON request body schema.
    pub request_schema: Option<RootSchema>,
    /// JSON response schema.
    pub response_schema: Option<RootSchema>,
    /// Query string schema.
    pub query_schema: Option<RootSchema>,
    /// Path parameter schema.
    pub params_schema: Option<RootSchema>,
}

/// Registry of endpoint schemas.
#[derive(Debug, Clone)]
pub struct SchemaRegistry {
    entries: Vec<EndpointSchema>,
    strict: bool,
    /// `route_key` → index of the first registered entry (O(1) lookups).
    index: HashMap<String, usize>,
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemaRegistry {
    /// Creates an empty registry (non-strict by default).
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            strict: false,
            index: HashMap::new(),
        }
    }

    /// When `true`, [`Self::ensure_route`] rejects unregistered paths at request time.
    pub fn strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Returns whether strict route validation is enabled.
    pub fn is_strict(&self) -> bool {
        self.strict
    }

    /// Registers path, method, and optional request/response JSON schemas.
    pub fn register_endpoint(
        &mut self,
        path: impl Into<String>,
        method: Method,
        request_schema: Option<RootSchema>,
        response_schema: Option<RootSchema>,
    ) {
        self.push_entry(EndpointSchema {
            path: path.into(),
            method,
            request_schema,
            response_schema,
            query_schema: None,
            params_schema: None,
        });
    }

    /// Registers a route with request, response, query, and params schemas.
    pub fn register_full(
        &mut self,
        path: impl Into<String>,
        method: Method,
        request_schema: Option<RootSchema>,
        response_schema: Option<RootSchema>,
        query_schema: Option<RootSchema>,
        params_schema: Option<RootSchema>,
    ) {
        self.push_entry(EndpointSchema {
            path: path.into(),
            method,
            request_schema,
            response_schema,
            query_schema,
            params_schema,
        });
    }

    /// Appends an entry and indexes its route (keeping the first entry per route).
    fn push_entry(&mut self, entry: EndpointSchema) {
        let key = route_key(&entry.path, &entry.method);
        if self.index.contains_key(&key) {
            tracing::warn!(
                path = %entry.path,
                method = %entry.method,
                "duplicate schema registration for route; lookups use the first matching entry"
            );
        }
        let idx = self.entries.len();
        self.index.entry(key).or_insert(idx);
        self.entries.push(entry);
    }

    /// Registers schemas derived from [`Endpoint`](crate::Endpoint) and `JsonSchema` types.
    pub fn register_typed<E, Req, Res>(&mut self)
    where
        E: crate::Endpoint,
        Req: JsonSchema + 'static,
        Res: JsonSchema + 'static,
        E::Params: JsonSchema,
        E::Query: JsonSchema,
    {
        self.register_full(
            E::PATH,
            E::METHOD,
            Some(schemars::schema_for!(Req)),
            Some(schemars::schema_for!(Res)),
            Some(schemars::schema_for!(E::Query)),
            Some(schemars::schema_for!(E::Params)),
        );
    }

    /// Returns an error if strict mode is enabled and the route is not registered.
    pub fn ensure_route(&self, path: &str, method: &Method) -> Result<()> {
        if !self.strict {
            return Ok(());
        }
        if self.index.contains_key(&route_key(path, method)) {
            Ok(())
        } else {
            Err(Error::SchemaRoute {
                method: method.to_string(),
                path: path.to_string(),
            })
        }
    }

    /// Returns all registered endpoint metadata.
    pub fn entries(&self) -> &[EndpointSchema] {
        &self.entries
    }

    /// Returns the first registered entry for a route, if any.
    fn find(&self, path: &str, method: &Method) -> Option<&EndpointSchema> {
        self.index
            .get(&route_key(path, method))
            .map(|&i| &self.entries[i])
    }

    /// Returns the request body schema for a route, if registered.
    pub fn request_schema(&self, path: &str, method: &Method) -> Option<&RootSchema> {
        self.find(path, method)
            .and_then(|e| e.request_schema.as_ref())
    }

    /// Returns the response body schema for a route, if registered.
    pub fn response_schema(&self, path: &str, method: &Method) -> Option<&RootSchema> {
        self.find(path, method)
            .and_then(|e| e.response_schema.as_ref())
    }

    /// Returns the query string schema for a route, if registered.
    pub fn query_schema(&self, path: &str, method: &Method) -> Option<&RootSchema> {
        self.find(path, method).and_then(|e| e.query_schema.as_ref())
    }

    /// Returns the path-parameter schema for a route, if registered.
    pub fn params_schema(&self, path: &str, method: &Method) -> Option<&RootSchema> {
        self.find(path, method)
            .and_then(|e| e.params_schema.as_ref())
    }
}

fn route_key(path: &str, method: &Method) -> String {
    format!("{method}:{path}")
}
