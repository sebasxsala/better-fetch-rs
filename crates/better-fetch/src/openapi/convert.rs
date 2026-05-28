//! JSON Schema (schemars) → OpenAPI `components` conversion.

use std::collections::HashSet;

use indexmap::IndexMap;
use schemars::schema::RootSchema;
use serde_json::Value;

use super::document::{OpenApiParameter, OpenApiSchemaRef};

/// Collected component schemas with stable `$ref` paths.
#[derive(Debug, Default)]
pub(crate) struct SchemaCatalog {
    pub schemas: IndexMap<String, Value>,
}

impl SchemaCatalog {
    /// Register a [`RootSchema`] and return a component `$ref`, or `None` for null/empty types.
    pub fn register(&mut self, preferred_name: &str, root: &RootSchema) -> Option<String> {
        if is_null_schema(root) {
            return None;
        }

        let mut value = serde_json::to_value(root).ok()?;
        if let Some(defs) = value
            .as_object_mut()
            .and_then(|obj| obj.remove("definitions"))
        {
            if let Some(def_map) = defs.as_object() {
                for (name, schema) in def_map {
                    self.insert_component(name, schema.clone());
                }
            }
        }

        if let Some(obj) = value.as_object_mut() {
            obj.remove("$schema");
        }

        let name = value
            .get("title")
            .and_then(|t| t.as_str())
            .filter(|title| *title != "Null")
            .map(sanitize_component_name)
            .unwrap_or_else(|| sanitize_component_name(preferred_name));

        self.insert_component(&name, value);
        Some(format!("#/components/schemas/{name}"))
    }

    fn insert_component(&mut self, name: &str, schema: Value) {
        let name = sanitize_component_name(name);
        if self.schemas.contains_key(&name) {
            return;
        }
        let cleaned = rewrite_schema_refs(strip_meta(schema));
        self.schemas.insert(name, cleaned);
    }
}

pub(crate) fn schema_ref(ref_path: String) -> OpenApiSchemaRef {
    OpenApiSchemaRef::Ref { ref_path }
}

pub(crate) fn is_null_schema(root: &RootSchema) -> bool {
    let Ok(value) = serde_json::to_value(root) else {
        return true;
    };
    value.get("type").and_then(|t| t.as_str()) == Some("null")
        || value.get("title").and_then(|t| t.as_str()) == Some("Null")
}

/// Convert better-fetch path templates (`/items/:id`) to OpenAPI (`/items/{id}`).
pub(crate) fn to_openapi_path(path: &str) -> String {
    let mut out = String::new();
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        out.push('/');
        if let Some(name) = segment.strip_prefix(':') {
            out.push('{');
            out.push_str(name);
            out.push('}');
        } else {
            out.push_str(segment);
        }
    }
    if out.is_empty() {
        "/".to_string()
    } else {
        out
    }
}

pub(crate) fn path_param_names(path: &str) -> HashSet<String> {
    crate::path_params::path_param_names(path)
        .into_iter()
        .collect()
}

pub(crate) fn operation_id(method: &http::Method, path: &str) -> String {
    let slug = path_slug(path);
    format!("{}{}", method.as_str().to_ascii_lowercase(), slug)
}

/// Build query/path parameters from a schemars object schema.
pub(crate) fn parameters_from_schema(
    root: &RootSchema,
    location: &str,
    path_names: &HashSet<String>,
    catalog: &mut SchemaCatalog,
    preferred_prefix: &str,
) -> Vec<OpenApiParameter> {
    if is_null_schema(root) {
        return Vec::new();
    }

    let Ok(mut value) = serde_json::to_value(root) else {
        return Vec::new();
    };
    let Some(properties) = value
        .as_object_mut()
        .and_then(|obj| obj.remove("properties"))
        .and_then(|p| p.as_object().cloned())
    else {
        return Vec::new();
    };

    let required: HashSet<String> = value
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut params = Vec::new();
    for (name, prop_schema) in properties {
        if location == "path" && !path_names.contains(&name) {
            continue;
        }
        if location == "query" && path_names.contains(&name) {
            continue;
        }

        let inline = rewrite_schema_refs(prop_schema);
        let schema = if let Some(ref_path) = inline_ref_to_component(
            &inline,
            catalog,
            &format!("{preferred_prefix}{}", capitalize(&name)),
        ) {
            OpenApiSchemaRef::Ref { ref_path }
        } else {
            OpenApiSchemaRef::Inline(inline)
        };

        params.push(OpenApiParameter {
            name: name.clone(),
            location: location.to_string(),
            description: None,
            required: location == "path" || required.contains(&name),
            schema,
        });
    }

    params.sort_by(|a, b| a.name.cmp(&b.name));
    params
}

fn inline_ref_to_component(
    schema: &Value,
    catalog: &mut SchemaCatalog,
    preferred_name: &str,
) -> Option<String> {
    if let Some(obj) = schema.as_object() {
        if obj.len() == 1 {
            if let Some(r) = obj.get("$ref").and_then(|v| v.as_str()) {
                if let Some(name) = r.strip_prefix("#/definitions/") {
                    let full = format!("#/components/schemas/{}", sanitize_component_name(name));
                    if !catalog.schemas.contains_key(&sanitize_component_name(name)) {
                        return None;
                    }
                    return Some(full);
                }
            }
        }
    }
    if schema.is_object() && schema.as_object().is_some_and(|o| !o.is_empty()) {
        let name = sanitize_component_name(preferred_name);
        if !catalog.schemas.contains_key(&name) {
            catalog
                .schemas
                .insert(name.clone(), rewrite_schema_refs(schema.clone()));
        }
        return Some(format!("#/components/schemas/{name}"));
    }
    None
}

fn strip_meta(mut value: Value) -> Value {
    if let Some(obj) = value.as_object_mut() {
        obj.remove("$schema");
        obj.remove("definitions");
    }
    value
}

fn rewrite_schema_refs(value: Value) -> Value {
    match value {
        Value::Object(mut map) => {
            if let Some(Value::String(reference)) = map.get("$ref") {
                if let Some(rest) = reference.strip_prefix("#/definitions/") {
                    map.insert(
                        "$ref".into(),
                        Value::String(format!(
                            "#/components/schemas/{}",
                            sanitize_component_name(rest)
                        )),
                    );
                }
            }
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(v) = map.remove(&key) {
                    map.insert(key, rewrite_schema_refs(v));
                }
            }
            Value::Object(map)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(rewrite_schema_refs).collect()),
        other => other,
    }
}

pub(crate) fn sanitize_component_name(name: &str) -> String {
    let mut out = String::new();
    let mut upper_next = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if upper_next {
                out.extend(ch.to_uppercase());
                upper_next = false;
            } else {
                out.push(ch);
            }
        } else {
            upper_next = true;
        }
    }
    if out.is_empty() {
        "Schema".into()
    } else {
        out
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn path_slug(path: &str) -> String {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(|segment| {
            if let Some(name) = segment.strip_prefix(':') {
                capitalize(name)
            } else {
                capitalize(segment)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;

    #[test]
    fn converts_colon_path_params() {
        assert_eq!(to_openapi_path("/todos/:id"), "/todos/{id}");
        assert_eq!(path_param_names("/todos/:id"), HashSet::from(["id".into()]));
    }

    #[test]
    fn null_unit_schema_skipped() {
        assert!(is_null_schema(&schemars::schema_for!(())));
    }

    #[derive(JsonSchema)]
    #[expect(dead_code)]
    struct Sample {
        name: String,
    }

    #[test]
    fn registers_schema_with_title() {
        let mut catalog = SchemaCatalog::default();
        let ref_path = catalog
            .register("Fallback", &schemars::schema_for!(Sample))
            .unwrap();
        assert_eq!(ref_path, "#/components/schemas/Sample");
        assert!(catalog.schemas.contains_key("Sample"));
    }
}
