use std::sync::Arc;

use bytes::Bytes;
use http::StatusCode;
use serde::de::DeserializeOwned;

use crate::error::Error;
use crate::Result;

/// Parses response bytes into JSON before deserializing to `T`.
pub type JsonParserFn =
    Arc<dyn Fn(&Bytes) -> std::result::Result<serde_json::Value, String> + Send + Sync>;

/// Wraps a custom JSON parse function for use with [`ClientBuilder::json_parser`](crate::client::ClientBuilder::json_parser).
pub fn json_parser<F>(f: F) -> JsonParserFn
where
    F: Fn(&Bytes) -> std::result::Result<serde_json::Value, String> + Send + Sync + 'static,
{
    Arc::new(f)
}

/// Default parser using `serde_json::from_slice`.
pub fn serde_json_parser() -> JsonParserFn {
    json_parser(|body| serde_json::from_slice(body).map_err(|e| e.to_string()))
}

pub(crate) fn deserialize<T: DeserializeOwned>(
    body: &Bytes,
    status: StatusCode,
    parser: Option<&JsonParserFn>,
) -> Result<T> {
    match parser {
        None => serde_json::from_slice(body).map_err(|source| Error::Deserialize {
            status,
            message: source.to_string(),
            body: Some(body.clone()),
        }),
        Some(parse) => {
            let value = parse(body).map_err(|message| Error::Deserialize {
                status,
                message,
                body: Some(body.clone()),
            })?;
            serde_json::from_value(value).map_err(|source| Error::Deserialize {
                status,
                message: source.to_string(),
                body: Some(body.clone()),
            })
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
