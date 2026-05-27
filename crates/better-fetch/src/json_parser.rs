//! JSON response parsing and optional custom parsers.
//!
//! # Default (fast path)
//!
//! When no [`JsonParserFn`] is configured on the client or request, deserialization is a
//! single step: **`Bytes` → `T`** via `serde_json::from_slice`.
//!
//! # Custom parser (two steps)
//!
//! [`ClientBuilder::json_parser`](crate::client::ClientBuilder::json_parser) and
//! [`RequestBuilder::json_parser`](crate::request::RequestBuilder::json_parser) install a
//! function that returns [`serde_json::Value`]. The library then deserializes that value to
//! `T` with `serde_json::from_value`. Use this for BOM stripping, normalizing payloads, or
//! other transforms before serde maps JSON to your types.
//!
//! That path costs an extra allocation and parse step compared to the default.
//!
//! # Direct `Bytes` → `T` (advanced)
//!
//! For maximum performance on a single response (e.g. BOM strip without a global two-step
//! parser), use [`Response::into_json_with`](crate::response::Response::into_json_with),
//! which runs your closure once and ignores any client-level [`JsonParserFn`].

use std::sync::Arc;

use bytes::Bytes;
use http::StatusCode;
use serde::de::DeserializeOwned;

use crate::error::Error;
use crate::Result;

/// Parses response bytes into [`serde_json::Value`] before deserializing to `T`.
///
/// Prefer leaving the client without a custom parser when you do not need transforms;
/// see the [module-level documentation](self) for the fast path vs two-step behavior.
pub type JsonParserFn =
    Arc<dyn Fn(&Bytes) -> std::result::Result<serde_json::Value, String> + Send + Sync>;

/// Wraps a custom JSON parse function for use with [`ClientBuilder::json_parser`](crate::client::ClientBuilder::json_parser).
pub fn json_parser<F>(f: F) -> JsonParserFn
where
    F: Fn(&Bytes) -> std::result::Result<serde_json::Value, String> + Send + Sync + 'static,
{
    Arc::new(f)
}

/// Default parser using `serde_json::from_slice` (same semantics as the fast path, as a [`JsonParserFn`]).
pub fn serde_json_parser() -> JsonParserFn {
    json_parser(|body| serde_json::from_slice(body).map_err(|e| e.to_string()))
}

pub(crate) fn deserialize_error(status: StatusCode, message: String, body: &Bytes) -> Error {
    Error::Deserialize {
        status,
        message,
        body: Some(body.clone()),
    }
}

pub(crate) fn deserialize<T: DeserializeOwned>(
    body: &Bytes,
    status: StatusCode,
    parser: Option<&JsonParserFn>,
) -> Result<T> {
    match parser {
        None => serde_json::from_slice(body)
            .map_err(|source| deserialize_error(status, source.to_string(), body)),
        Some(parse) => {
            let value = parse(body).map_err(|message| deserialize_error(status, message, body))?;
            serde_json::from_value(value)
                .map_err(|source| deserialize_error(status, source.to_string(), body))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct IdOnly {
        id: u64,
    }

    fn strip_bom(body: &Bytes) -> std::result::Result<serde_json::Value, String> {
        let slice = body.strip_prefix(b"\xef\xbb\xbf").unwrap_or(body);
        serde_json::from_slice(slice).map_err(|e| e.to_string())
    }

    #[test]
    fn fast_path_without_parser() {
        let body = Bytes::from_static(br#"{"id":1}"#);
        let parsed: IdOnly =
            deserialize(&body, StatusCode::OK, None).expect("serde_json fast path");
        assert_eq!(parsed, IdOnly { id: 1 });
    }

    #[test]
    fn custom_parser_strips_bom() {
        let body = Bytes::from_static(b"\xef\xbb\xbf{\"id\":2}");
        let parser = json_parser(strip_bom);
        let parsed: IdOnly =
            deserialize(&body, StatusCode::OK, Some(&parser)).expect("custom parser");
        assert_eq!(parsed, IdOnly { id: 2 });
    }

    #[test]
    fn custom_parser_error_maps_to_deserialize() {
        let body = Bytes::from_static(b"not-json");
        let parser = json_parser(|_| Err("bad json".into()));
        let err = deserialize::<IdOnly>(&body, StatusCode::OK, Some(&parser)).unwrap_err();
        assert!(matches!(err, Error::Deserialize { message, .. } if message == "bad json"));
    }
}
