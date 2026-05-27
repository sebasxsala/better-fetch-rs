use bytes::Bytes;
use http::{HeaderMap, StatusCode};

use crate::error::Error;
use crate::Result;

/// HTTP response wrapper.
#[derive(Clone)]
pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
    url: Option<url::Url>,
    #[cfg(feature = "json")]
    json_parser: Option<crate::json_parser::JsonParserFn>,
}

impl Response {
    pub(crate) fn new(
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
        url: Option<url::Url>,
        #[cfg(feature = "json")] json_parser: Option<crate::json_parser::JsonParserFn>,
    ) -> Self {
        Self {
            status,
            headers,
            body,
            url,
            #[cfg(feature = "json")]
            json_parser,
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn bytes(&self) -> &Bytes {
        &self.body
    }

    pub fn url(&self) -> Option<&url::Url> {
        self.url.as_ref()
    }

    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// Returns an error if the status is not success.
    #[must_use = "call `?` or handle the error explicitly"]
    pub fn error_for_status(&self) -> Result<()> {
        if self.status.is_success() {
            return Ok(());
        }
        Err(Error::http_with_status_text(
            self.status,
            self.status.canonical_reason().unwrap_or("request failed"),
            self.status.canonical_reason().unwrap_or("request failed"),
            Some(self.body.clone()),
        ))
    }

    /// Reads the body as UTF-8 after checking for a success status.
    pub fn into_text(self) -> Result<String> {
        self.error_for_status()?;
        Ok(String::from_utf8_lossy(&self.body).into_owned())
    }

    /// Reads the body as UTF-8 after checking for a success status.
    pub async fn text(self) -> Result<String> {
        self.into_text()
    }

    /// Returns the body after checking for a success status.
    pub fn into_bytes_checked(self) -> Result<Bytes> {
        self.error_for_status()?;
        Ok(self.body)
    }

    /// Returns the body after checking for a success status.
    pub async fn bytes_checked(self) -> Result<Bytes> {
        self.into_bytes_checked()
    }

    #[cfg(feature = "json")]
    pub fn into_json<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        self.error_for_status()?;
        crate::json_parser::deserialize(&self.body, self.status, self.json_parser.as_ref())
    }

    #[cfg(feature = "json")]
    pub async fn json<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        self.into_json()
    }

    #[cfg(feature = "json")]
    pub fn into_json_unchecked<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        crate::json_parser::deserialize(&self.body, self.status, self.json_parser.as_ref())
    }

    #[cfg(feature = "json")]
    pub async fn json_unchecked<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        self.into_json_unchecked()
    }

    /// Deserialize JSON and run [`garde::Validate`] rules (feature `validate`).
    #[cfg(feature = "validate")]
    pub fn into_json_validated<T>(self) -> Result<T>
    where
        T: serde::de::DeserializeOwned + garde::Validate,
        T::Context: Default,
    {
        self.error_for_status()?;
        crate::validate_json::deserialize_validated(
            &self.body,
            self.status,
            self.json_parser.as_ref(),
        )
    }

    /// Deserialize JSON and run [`garde::Validate`] rules (feature `validate`).
    #[cfg(feature = "validate")]
    pub async fn json_validated<T>(self) -> Result<T>
    where
        T: serde::de::DeserializeOwned + garde::Validate,
        T::Context: Default,
    {
        self.into_json_validated()
    }

    /// Like [`into_json_validated`](Self::into_json_validated) without checking HTTP status.
    #[cfg(feature = "validate")]
    pub fn into_json_validated_unchecked<T>(self) -> Result<T>
    where
        T: serde::de::DeserializeOwned + garde::Validate,
        T::Context: Default,
    {
        crate::validate_json::deserialize_validated(
            &self.body,
            self.status,
            self.json_parser.as_ref(),
        )
    }

    /// Like [`into_json_validated`](Self::into_json_validated) without checking HTTP status.
    #[cfg(feature = "validate")]
    pub async fn json_validated_unchecked<T>(self) -> Result<T>
    where
        T: serde::de::DeserializeOwned + garde::Validate,
        T::Context: Default,
    {
        self.into_json_validated_unchecked()
    }

    pub fn into_parts(self) -> (StatusCode, HeaderMap, Bytes) {
        (self.status, self.headers, self.body)
    }
}

impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("Response");
        debug
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("body", &self.body)
            .field("url", &self.url);
        #[cfg(feature = "json")]
        if self.json_parser.is_some() {
            debug.field("json_parser", &"<custom>");
        }
        debug.finish()
    }
}

#[cfg(all(test, feature = "json"))]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct IdOnly {
        id: u64,
    }

    #[test]
    fn into_text_returns_body_on_success() {
        let response = Response::new(
            StatusCode::OK,
            HeaderMap::new(),
            Bytes::from_static(b"hello"),
            None,
            None,
        );
        assert_eq!(response.into_text().unwrap(), "hello");
    }

    #[test]
    fn into_json_deserializes_without_async() {
        let response = Response::new(
            StatusCode::OK,
            HeaderMap::new(),
            Bytes::from_static(br#"{"id":7}"#),
            None,
            None,
        );
        assert_eq!(response.into_json::<IdOnly>().unwrap(), IdOnly { id: 7 });
    }
}
