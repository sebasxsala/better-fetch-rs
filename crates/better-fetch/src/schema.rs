//! Schema registry for endpoint metadata (requires `schema` feature).

use http::Method;
use schemars::schema::RootSchema;
use schemars::JsonSchema;
use std::collections::HashSet;

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
    routes: HashSet<String>,
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
            routes: HashSet::new(),
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
        let path = path.into();
        self.routes.insert(route_key(&path, &method));
        self.entries.push(EndpointSchema {
            path,
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
        let path = path.into();
        self.routes.insert(route_key(&path, &method));
        self.entries.push(EndpointSchema {
            path,
            method,
            request_schema,
            response_schema,
            query_schema,
            params_schema,
        });
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
        let key = route_key(path, method);
        if self.routes.contains(&key) {
            Ok(())
        } else {
            Err(Error::Other(format!(
                "route not in schema registry: {method} {path}"
            )))
        }
    }

    /// Returns all registered endpoint metadata.
    pub fn entries(&self) -> &[EndpointSchema] {
        &self.entries
    }
}

fn route_key(path: &str, method: &Method) -> String {
    format!("{method}:{path}")
}
