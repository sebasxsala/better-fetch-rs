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

/// Returns `:param` segment names in left-to-right path order (ignores an embedded `?query`).
pub fn path_param_names(path: &str) -> Vec<String> {
    crate::path_params::path_param_names(path)
}

/// Fuzzing entry point: builds a URL from `path` against a fixed base (no params/query).
#[doc(hidden)]
pub fn fuzz_build_url(path: &str) -> Result<BuiltUrl> {
    build_url(
        &Url::parse("https://api.example.com").map_err(Error::InvalidBaseUrl)?,
        path,
        &HashMap::new(),
        &IndexMap::new(),
    )
}

/// Fuzzing entry point: merges an embedded `?query` suffix from `path` with an empty builder query.
#[doc(hidden)]
pub fn fuzz_parse_embedded_query(path: &str) -> Result<BuiltUrl> {
    fuzz_build_url(path)
}

/// Build a request URL from base URL, path template, params, and query.
///
/// Query keys are serialized in insertion order ([`IndexMap`]). A `?foo=bar` suffix on `path`
/// is merged first; explicit `query` entries override embedded keys.
pub fn build_url(
    base: &Url,
    path: &str,
    params: &HashMap<String, String>,
    query: &IndexMap<String, QueryValue>,
) -> Result<BuiltUrl> {
    if path.starts_with("http://") || path.starts_with("https://") {
        let (path_only, method_override) = parse_method_modifier(path);
        let (path_only, embedded_query) = split_embedded_query(path_only);
        let resolved_path = substitute_params(path_only, params)?;
        let mut url = Url::parse(&resolved_path).map_err(Error::InvalidBaseUrl)?;
        let merged = merge_queries(embedded_query, query);
        apply_query(&mut url, &merged)?;
        return Ok(BuiltUrl {
            url,
            method_override,
        });
    }

    let (path_only, method_override) = parse_method_modifier(path);
    let (path_only, embedded_query) = split_embedded_query(path_only);
    let resolved_path = substitute_params(path_only, params)?;
    let mut url = join_path(base, &resolved_path)?;
    let merged = merge_queries(embedded_query, query);
    apply_query(&mut url, &merged)?;

    Ok(BuiltUrl {
        url,
        method_override,
    })
}

fn split_embedded_query(path: &str) -> (&str, IndexMap<String, QueryValue>) {
    let Some((path_only, query_str)) = path.split_once('?') else {
        return (path, IndexMap::new());
    };
    (path_only, parse_query_string(query_str))
}

fn parse_query_string(query_str: &str) -> IndexMap<String, QueryValue> {
    let mut map = IndexMap::new();
    for (key, value) in url::form_urlencoded::parse(query_str.as_bytes()) {
        let key = key.into_owned();
        let value = value.into_owned();
        match map.get_mut(&key) {
            None => {
                map.insert(key, QueryValue::Scalar(value));
            }
            Some(QueryValue::Scalar(prev)) => {
                let first = prev.clone();
                map.insert(key, QueryValue::Array(vec![first, value]));
            }
            Some(QueryValue::Array(values)) => {
                values.push(value);
            }
        }
    }
    map
}

fn merge_queries(
    embedded: IndexMap<String, QueryValue>,
    builder: &IndexMap<String, QueryValue>,
) -> IndexMap<String, QueryValue> {
    let mut merged = embedded;
    for (key, value) in builder {
        merged.insert(key.clone(), value.clone());
    }
    merged
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
    for key in path_param_names(path) {
        let placeholder = format!(":{key}");
        let Some(value) = params.get(&key) else {
            return Err(Error::MissingPathParam(key));
        };
        let encoded: Cow<'_, str> = utf8_percent_encode(value, PATH_PARAM_ENCODE).into();
        result = result.replace(&placeholder, encoded.as_ref());
    }

    if result.contains(':') {
        for segment in result.split('/') {
            if segment.starts_with(':') {
                let name = segment.trim_start_matches(':');
                return Err(Error::MissingPathParam(name.to_string()));
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
        assert!(matches!(err, Error::MissingPathParam(_)));
    }

    #[test]
    fn embedded_query_in_path_is_merged() {
        let built = build_url(
            &base(),
            "/search?tag=rust",
            &HashMap::new(),
            &IndexMap::new(),
        )
        .unwrap();
        assert_eq!(built.url.query(), Some("tag=rust"));
    }

    #[test]
    fn builder_query_overrides_embedded_query() {
        let mut query = IndexMap::new();
        query.insert("tag".into(), QueryValue::Scalar("override".into()));
        let built = build_url(&base(), "/search?tag=rust", &HashMap::new(), &query).unwrap();
        assert_eq!(built.url.query(), Some("tag=override"));
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

    #[cfg(feature = "json")]
    mod serialize_tests {
        use super::*;
        use serde::Serialize;

        #[derive(Serialize)]
        struct SearchQuery {
            q: String,
            page: u32,
            active: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            tag: Option<String>,
        }

        #[test]
        fn serialize_to_query_map_skips_null_and_serializes_fields() {
            let value = SearchQuery {
                q: "rust".into(),
                page: 2,
                active: true,
                tag: None,
            };
            let map = serialize_to_query_map(&value).unwrap();
            assert_eq!(map.len(), 3);
            assert!(matches!(map.get("q"), Some(QueryValue::Scalar(s)) if s == "rust"));
            assert!(matches!(map.get("page"), Some(QueryValue::Scalar(s)) if s == "2"));
            assert!(matches!(map.get("active"), Some(QueryValue::Scalar(s)) if s == "true"));
            assert!(!map.contains_key("tag"));
        }

        #[test]
        fn serialize_to_query_map_array_field() {
            #[derive(Serialize)]
            struct Tags {
                tags: Vec<String>,
            }
            let value = Tags {
                tags: vec!["a".into(), "b".into()],
            };
            let map = serialize_to_query_map(&value).unwrap();
            assert!(matches!(map.get("tags"), Some(QueryValue::Array(v)) if v == &["a", "b"]));
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn base() -> Url {
        Url::parse("https://api.example.com").unwrap()
    }

    proptest! {
        #[test]
        fn substitute_preserves_literal_segments(path in r"([a-z]+/)*[a-z]+") {
            let mut params = HashMap::new();
            params.insert("id".into(), "42".into());
            let template = format!("/{path}/:id");
            let built = build_url(&base(), &template, &params, &IndexMap::new()).unwrap();
            prop_assert!(built.url.as_str().ends_with("/42"));
        }

        #[test]
        fn scalar_query_round_trips_key_value(
            key in r"[a-zA-Z][a-zA-Z0-9_-]{0,15}",
            value in r"[a-zA-Z0-9._-]{0,32}",
        ) {
            let mut query = IndexMap::new();
            query.insert(key.clone(), QueryValue::Scalar(value.clone()));
            let built = build_url(&base(), "/search", &HashMap::new(), &query).unwrap();
            let q = built.url.query().unwrap();
            let needle = format!("{key}={value}");
            prop_assert!(q.contains(&needle));
        }

        #[test]
        fn builder_query_overrides_embedded_key(
            key in r"[a-z][a-z0-9]{0,8}",
            embedded in r"[a-z0-9]{1,12}",
            override_val in r"[a-z0-9]{1,12}",
        ) {
            let _ = embedded;
            let mut query = IndexMap::new();
            query.insert(key.clone(), QueryValue::Scalar(override_val.clone()));
            let path = format!("/search?{key}={embedded}");
            let built = build_url(&base(), &path, &HashMap::new(), &query).unwrap();
            let q = built.url.query().unwrap();
            let parsed: std::collections::HashMap<String, String> =
                url::form_urlencoded::parse(q.as_bytes())
                    .map(|(k, v)| (k.into_owned(), v.into_owned()))
                    .collect();
            prop_assert_eq!(parsed.get(&key).map(String::as_str), Some(override_val.as_str()));
        }

        #[test]
        fn missing_path_param_always_errors(
            name in r"[a-z][a-z0-9]{0,12}",
        ) {
            let template = format!("/items/:{name}");
            let err = build_url(&base(), &template, &HashMap::new(), &IndexMap::new()).unwrap_err();
            prop_assert!(matches!(err, Error::MissingPathParam(_)));
        }

        #[test]
        fn join_path_preserves_base_for_empty_path(
            trailing in prop::bool::ANY,
        ) {
            let base_str = if trailing {
                "https://api.example.com/"
            } else {
                "https://api.example.com"
            };
            let base_url = Url::parse(base_str).unwrap();
            let built = build_url(&base_url, "", &HashMap::new(), &IndexMap::new()).unwrap();
            prop_assert_eq!(built.url.as_str(), "https://api.example.com/");
        }

        #[test]
        fn array_query_repeats_key(
            key in r"[a-z][a-z0-9]{0,6}",
            a in r"[a-z0-9]{1,8}",
            b in r"[a-z0-9]{1,8}",
        ) {
            let mut query = IndexMap::new();
            query.insert(key.clone(), QueryValue::Array(vec![a.clone(), b.clone()]));
            let built = build_url(&base(), "/search", &HashMap::new(), &query).unwrap();
            let q = built.url.query().unwrap();
            let a_needle = format!("{key}={a}");
            let b_needle = format!("{key}={b}");
            prop_assert!(q.contains(&a_needle));
            prop_assert!(q.contains(&b_needle));
        }
    }
}
