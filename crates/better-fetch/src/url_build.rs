use std::borrow::Cow;
use std::collections::HashMap;

use http::Method;
use indexmap::IndexMap;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use url::Url;

use crate::error::Error;
use crate::Result;

/// RFC 3986 unreserved — same set as the former inline path param encoder.
const PATH_PARAM_ENCODE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// Result of building a request URL.
#[derive(Debug, Clone)]
pub struct BuiltUrl {
    pub url: Url,
    pub method_override: Option<Method>,
}

/// Build a request URL from base URL, path template, params, and query.
///
/// Query keys are serialized in insertion order ([`IndexMap`]).
pub fn build_url(
    base: &Url,
    path: &str,
    params: &HashMap<String, String>,
    query: &IndexMap<String, QueryValue>,
) -> Result<BuiltUrl> {
    if path.starts_with("http://") || path.starts_with("https://") {
        let (path_only, method_override) = parse_method_modifier(path);
        let resolved_path = substitute_params(path_only, params)?;
        let mut url = Url::parse(&resolved_path).map_err(Error::InvalidBaseUrl)?;
        apply_query(&mut url, query)?;
        return Ok(BuiltUrl {
            url,
            method_override,
        });
    }

    let (path_only, method_override) = parse_method_modifier(path);
    let resolved_path = substitute_params(path_only, params)?;
    let mut url = join_path(base, &resolved_path)?;
    apply_query(&mut url, query)?;

    Ok(BuiltUrl {
        url,
        method_override,
    })
}

fn apply_query(url: &mut Url, query: &IndexMap<String, QueryValue>) -> Result<()> {
    if query.is_empty() {
        return Ok(());
    }
    let mut pairs = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in query {
        match value {
            QueryValue::Scalar(v) => {
                pairs.append_pair(key, v);
            }
            QueryValue::Array(values) => {
                for v in values {
                    pairs.append_pair(key, v);
                }
            }
        }
    }
    let query_string = pairs.finish();
    url.set_query(Some(&query_string));
    Ok(())
}

/// Parse `@put/foo` style path modifiers; returns stripped path and optional HTTP method.
pub fn parse_method_modifier(path: &str) -> (&str, Option<Method>) {
    if let Some(rest) = path.strip_prefix('@') {
        if let Some((method, remainder)) = rest.split_once('/') {
            let m = match method.to_ascii_lowercase().as_str() {
                "get" => Some(Method::GET),
                "post" => Some(Method::POST),
                "put" => Some(Method::PUT),
                "patch" => Some(Method::PATCH),
                "delete" => Some(Method::DELETE),
                "head" => Some(Method::HEAD),
                _ => None,
            };
            if m.is_some() {
                return (remainder.trim_start_matches('/'), m);
            }
        }
    }
    (path, None)
}

fn substitute_params(path: &str, params: &HashMap<String, String>) -> Result<String> {
    let mut result = path.to_string();
    for (key, value) in params {
        let placeholder = format!(":{key}");
        if !result.contains(&placeholder) {
            continue;
        }
        let encoded: Cow<'_, str> = utf8_percent_encode(value, PATH_PARAM_ENCODE).into();
        result = result.replace(&placeholder, encoded.as_ref());
    }

    if result.contains(':') {
        for segment in result.split('/') {
            if segment.starts_with(':') {
                return Err(Error::Other(format!(
                    "missing path parameter for `{}`",
                    segment
                )));
            }
        }
    }

    Ok(result)
}

fn join_path(base: &Url, path: &str) -> Result<Url> {
    let path = path.trim_start_matches('/');
    let base_str = base.as_str().trim_end_matches('/');
    let joined = if path.is_empty() {
        base_str.to_string()
    } else {
        format!("{base_str}/{path}")
    };
    Url::parse(&joined).map_err(Error::InvalidBaseUrl)
}

/// Query parameter value (scalar or repeated).
#[derive(Debug, Clone)]
pub enum QueryValue {
    /// Single query value.
    Scalar(String),
    /// Repeated query key (`key=a&key=b`).
    Array(Vec<String>),
}

/// Converts a serializable struct into query parameters keyed by serde field names.
///
/// Skips `null` values (e.g. `None` fields without `skip_serializing_if`).
#[cfg(feature = "json")]
pub fn serialize_to_query_map<T: serde::Serialize>(
    value: &T,
) -> Result<IndexMap<String, QueryValue>> {
    let json = serde_json::to_value(value).map_err(|e| Error::Other(e.to_string()))?;
    let mut map = IndexMap::new();
    if let serde_json::Value::Object(obj) = json {
        for (key, val) in obj {
            if val.is_null() {
                continue;
            }
            map.insert(key, QueryValue::from_serializable(&val)?);
        }
    }
    Ok(map)
}

impl QueryValue {
    /// Encodes a serializable value as a scalar or array query param (feature `json`).
    #[cfg(feature = "json")]
    pub fn from_serializable<T: serde::Serialize>(value: &T) -> Result<Self> {
        match serde_json::to_value(value).map_err(|e| Error::Other(e.to_string()))? {
            serde_json::Value::String(s) => Ok(Self::Scalar(s)),
            serde_json::Value::Number(n) => Ok(Self::Scalar(n.to_string())),
            serde_json::Value::Bool(b) => Ok(Self::Scalar(b.to_string())),
            serde_json::Value::Array(arr) => {
                let values: Vec<String> = arr
                    .into_iter()
                    .map(|v| match v {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    })
                    .collect();
                Ok(Self::Array(values))
            }
            other => Ok(Self::Scalar(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://api.example.com").unwrap()
    }

    #[test]
    fn substitutes_colon_params() {
        let mut params = HashMap::new();
        params.insert("id".into(), "42".into());
        let built = build_url(&base(), "/todos/:id", &params, &IndexMap::new()).unwrap();
        assert_eq!(built.url.as_str(), "https://api.example.com/todos/42");
    }

    #[test]
    fn multiple_params() {
        let mut params = HashMap::new();
        params.insert("id".into(), "1".into());
        params.insert("title".into(), "hello".into());
        let built = build_url(&base(), "/post/:id/:title", &params, &IndexMap::new()).unwrap();
        assert_eq!(built.url.as_str(), "https://api.example.com/post/1/hello");
    }

    #[test]
    fn encodes_special_characters_in_params() {
        let mut params = HashMap::new();
        params.insert("id".into(), "a/b".into());
        let built = build_url(&base(), "/items/:id", &params, &IndexMap::new()).unwrap();
        assert!(built.url.path().contains("a%2Fb"));
    }

    #[test]
    fn missing_param_errors() {
        let err = build_url(&base(), "/todos/:id", &HashMap::new(), &IndexMap::new()).unwrap_err();
        assert!(matches!(err, Error::Other(_)));
    }

    #[test]
    fn query_scalar() {
        let mut query = IndexMap::new();
        query.insert("q".into(), QueryValue::Scalar("rust".into()));
        let built = build_url(&base(), "/search", &HashMap::new(), &query).unwrap();
        assert_eq!(built.url.query(), Some("q=rust"));
    }

    #[test]
    fn query_array() {
        let mut query = IndexMap::new();
        query.insert(
            "tag".into(),
            QueryValue::Array(vec!["a".into(), "b".into()]),
        );
        let built = build_url(&base(), "/search", &HashMap::new(), &query).unwrap();
        let q = built.url.query().unwrap();
        assert!(q.contains("tag=a"));
        assert!(q.contains("tag=b"));
    }

    #[test]
    fn query_preserves_insertion_order() {
        let mut query = IndexMap::new();
        query.insert("z".into(), QueryValue::Scalar("1".into()));
        query.insert("a".into(), QueryValue::Scalar("2".into()));
        query.insert("m".into(), QueryValue::Scalar("3".into()));
        let built = build_url(&base(), "/search", &HashMap::new(), &query).unwrap();
        assert_eq!(built.url.query(), Some("z=1&a=2&m=3"));
    }

    #[test]
    fn method_modifier_put() {
        let (path, method) = parse_method_modifier("@put/user");
        assert_eq!(path, "user");
        assert_eq!(method, Some(Method::PUT));
    }

    #[test]
    fn method_modifier_in_build_url() {
        let built = build_url(&base(), "@patch/items", &HashMap::new(), &IndexMap::new()).unwrap();
        assert_eq!(built.url.path(), "/items");
        assert_eq!(built.method_override, Some(Method::PATCH));
    }

    #[test]
    fn absolute_url_ignores_base() {
        let mut params = HashMap::new();
        params.insert("id".into(), "5".into());
        let built = build_url(
            &base(),
            "https://other.example.com/users/:id",
            &params,
            &IndexMap::new(),
        )
        .unwrap();
        assert_eq!(built.url.as_str(), "https://other.example.com/users/5");
    }

    #[test]
    fn empty_path_uses_base() {
        let built = build_url(&base(), "", &HashMap::new(), &IndexMap::new()).unwrap();
        assert_eq!(built.url.as_str(), "https://api.example.com/");
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn substitute_preserves_literal_segments(path in r"([a-z]+/)*[a-z]+") {
            let mut params = HashMap::new();
            params.insert("id".into(), "42".into());
            let template = format!("/{path}/:id");
            let built = build_url(
                &Url::parse("https://api.example.com").unwrap(),
                &template,
                &params,
                &IndexMap::new(),
            )
            .unwrap();
            prop_assert!(built.url.as_str().ends_with("/42"));
        }
    }
}
