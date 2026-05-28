//! Runtime JSON Schema validation against a [`SchemaRegistry`](crate::schema::SchemaRegistry).

use std::collections::HashMap;

use http::Method;
use indexmap::IndexMap;
use jsonschema::{Draft, Validator};
use schemars::schema::RootSchema;
use serde_json::Value;

use crate::error::Error;
use crate::response::Response;
use crate::schema::SchemaRegistry;
use crate::url_build::QueryValue;
use crate::Result;

/// Context for validating a streamed response after [`StreamingResponse::collect`](crate::StreamingResponse::collect).
#[cfg(feature = "schema-validate")]
#[derive(Clone)]
pub(crate) struct StreamResponseSchemaCtx {
    pub registry: std::sync::Arc<SchemaRegistry>,
    pub route_path: String,
    pub method: Method,
}

/// Validates a buffered [`Response`] when strict mode and a response schema are registered.
#[cfg(feature = "schema-validate")]
pub(crate) fn validate_response_if_registered(
    registry: &SchemaRegistry,
    path: &str,
    method: &Method,
    response: &Response,
) -> Result<()> {
    if !registry.is_strict() || !response.is_success() {
        return Ok(());
    }
    if registry.response_schema(path, method).is_none() {
        return Ok(());
    }
    let bytes = response.bytes();
    if bytes.is_empty() {
        return Ok(());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|e| Error::SchemaValidation {
        phase: "response",
        message: format!("response body is not JSON: {e}"),
    })?;
    validate_response(registry, path, method, &value)
}

/// Validates a JSON request body against the registered request schema for `path` + `method`.
///
/// No-op when the registry is not [strict](SchemaRegistry::is_strict).
pub fn validate_request(
    registry: &SchemaRegistry,
    path: &str,
    method: &Method,
    body: &Value,
) -> Result<()> {
    if !registry.is_strict() {
        return Ok(());
    }
    let Some(schema) = registry.request_schema(path, method) else {
        return Ok(());
    };
    validate_value(schema, body, "request")
}

/// Validates a JSON response body against the registered response schema for `path` + `method`.
///
/// No-op when the registry is not strict.
pub fn validate_response(
    registry: &SchemaRegistry,
    path: &str,
    method: &Method,
    body: &Value,
) -> Result<()> {
    if !registry.is_strict() {
        return Ok(());
    }
    let Some(schema) = registry.response_schema(path, method) else {
        return Ok(());
    };
    validate_value(schema, body, "response")
}

/// Validates path parameters (as a JSON object) when a params schema is registered.
///
/// Wire values are coerced from strings (numbers, booleans) before validation. No-op when not strict.
pub fn validate_params(
    registry: &SchemaRegistry,
    path: &str,
    method: &Method,
    params: &HashMap<String, String>,
) -> Result<()> {
    if !registry.is_strict() {
        return Ok(());
    }
    let Some(schema) = registry.params_schema(path, method) else {
        return Ok(());
    };
    validate_value(schema, &params_to_json(params), "params")
}

/// Validates query parameters (as a JSON object) when a query schema is registered.
///
/// Wire values are coerced from strings (numbers, booleans) before validation. No-op when not strict.
pub fn validate_query(
    registry: &SchemaRegistry,
    path: &str,
    method: &Method,
    query: &IndexMap<String, QueryValue>,
) -> Result<()> {
    if !registry.is_strict() {
        return Ok(());
    }
    let Some(schema) = registry.query_schema(path, method) else {
        return Ok(());
    };
    validate_value(schema, &query_to_json(query), "query")
}

/// Coerces a single query/path wire string into a JSON value for schema validation.
pub(crate) fn wire_scalar_to_json(s: &str) -> Value {
    match s {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => {
            if let Ok(n) = s.parse::<i64>() {
                return Value::Number(n.into());
            }
            if let Ok(n) = s.parse::<u64>() {
                return Value::Number(n.into());
            }
            if let Ok(n) = s.parse::<f64>() {
                if let Some(num) = serde_json::Number::from_f64(n) {
                    return Value::Number(num);
                }
            }
            Value::String(s.to_owned())
        }
    }
}

fn params_to_json(params: &HashMap<String, String>) -> Value {
    let mut map = serde_json::Map::new();
    for (key, value) in params {
        map.insert(key.clone(), wire_scalar_to_json(value));
    }
    Value::Object(map)
}

fn query_to_json(query: &IndexMap<String, QueryValue>) -> Value {
    let mut map = serde_json::Map::new();
    for (key, value) in query {
        let json_value = match value {
            QueryValue::Scalar(s) => wire_scalar_to_json(s),
            QueryValue::Array(values) => {
                Value::Array(values.iter().map(|v| wire_scalar_to_json(v)).collect())
            }
        };
        map.insert(key.clone(), json_value);
    }
    Value::Object(map)
}

fn validate_value(schema: &RootSchema, value: &Value, phase: &'static str) -> Result<()> {
    let validator = Validator::options()
        .with_draft(Draft::Draft7)
        .build(&serde_json::to_value(schema).map_err(|e| Error::Config(e.to_string()))?)
        .map_err(|e| Error::Config(e.to_string()))?;
    validator
        .validate(value)
        .map_err(|error| Error::SchemaValidation {
            phase,
            message: error.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_scalar_coerces_numbers_and_bools() {
        assert_eq!(wire_scalar_to_json("42"), json!(42));
        assert_eq!(wire_scalar_to_json("true"), json!(true));
        assert_eq!(wire_scalar_to_json("hello"), json!("hello"));
    }

    #[test]
    fn query_json_scalar_and_array() {
        let mut q = IndexMap::new();
        q.insert("tag".into(), QueryValue::Scalar("a".into()));
        q.insert(
            "ids".into(),
            QueryValue::Array(vec!["1".into(), "2".into()]),
        );
        let v = query_to_json(&q);
        assert_eq!(v["tag"], json!("a"));
        assert_eq!(v["ids"], json!([1, 2]));
    }

    #[test]
    fn params_json_coerces_id() {
        let mut p = HashMap::new();
        p.insert("id".into(), "7".into());
        assert_eq!(params_to_json(&p)["id"], json!(7));
    }
}
