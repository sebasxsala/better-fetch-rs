//! Deserialize JSON then run [`garde::Validate`] (feature `validate`).

use bytes::Bytes;
use garde::Validate;
use http::StatusCode;
use serde::de::DeserializeOwned;

use crate::error::Error;
use crate::json_parser::deserialize;
use crate::json_parser::JsonParserFn;
use crate::Result;

pub(crate) fn deserialize_validated<T>(
    body: &Bytes,
    status: StatusCode,
    parser: Option<&JsonParserFn>,
) -> Result<T>
where
    T: DeserializeOwned + Validate,
    T::Context: Default,
{
    let value: T = deserialize(body, status, parser)?;
    value.validate().map_err(|report| Error::Validation {
        status,
        message: report.to_string(),
        body: Some(body.clone()),
    })?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use garde::Validate;
    use http::StatusCode;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, Validate, PartialEq)]
    struct Todo {
        #[garde(length(min = 1))]
        title: String,
    }

    #[test]
    fn accepts_valid_payload() {
        let body = Bytes::from_static(br#"{"title":"hello"}"#);
        let todo: Todo = deserialize_validated(&body, StatusCode::OK, None).expect("valid todo");
        assert_eq!(todo.title, "hello");
    }

    #[test]
    fn rejects_empty_title() {
        let body = Bytes::from_static(br#"{"title":""}"#);
        let err = deserialize_validated::<Todo>(&body, StatusCode::OK, None).unwrap_err();
        assert!(matches!(err, Error::Validation { .. }));
    }

    #[test]
    fn maps_json_errors_to_deserialize() {
        let body = Bytes::from_static(b"not-json");
        let err = deserialize_validated::<Todo>(&body, StatusCode::OK, None).unwrap_err();
        assert!(matches!(err, Error::Deserialize { .. }));
    }
}
