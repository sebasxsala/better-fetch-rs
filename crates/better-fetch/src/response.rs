//! HTTP response wrapper with a **fully buffered** body.
//!
//! For incremental reads, use [`RequestBuilder::send_stream`](crate::RequestBuilder::send_stream)
//! and the [`streaming`](crate::streaming) module. Prefer [`Response::into_json`] and
//! [`Response::into_text`] on hot paths; async methods are aliases without extra I/O.

use bytes::Bytes;
use http::{HeaderMap, StatusCode};

use crate::error::Error;
use crate::Result;

/// HTTP response wrapper.
///
/// The full body is already buffered in memory as [`Bytes`] when you receive this type.
/// Methods named `into_*` perform parsing synchronously; the `async` counterparts
/// (`text`, `json`, …) delegate to `into_*` without additional I/O and exist for
/// ergonomics in async code (e.g. [`RequestBuilder::send_json`](crate::request::RequestBuilder::send_json)).
/// Prefer `into_json`, `into_text`, and `into_bytes_checked` on hot paths.
///
/// This model suits typical JSON APIs. For large or chunked bodies, use
/// [`StreamingResponse`](crate::StreamingResponse) via [`send_stream`](crate::RequestBuilder::send_stream).
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

    /// HTTP status code.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Response headers.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Returns a reference to the fully buffered response body.
    pub fn bytes(&self) -> &Bytes {
        &self.body
    }

    /// Final request URL when available.
    pub fn url(&self) -> Option<&url::Url> {
        self.url.as_ref()
    }

    /// Returns `true` for 2xx status codes.
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
    ///
    /// Prefer this over [`text`](Self::text) when you do not need an `.await` (no extra I/O).
    pub fn into_text(self) -> Result<String> {
        self.error_for_status()?;
        Ok(String::from_utf8_lossy(&self.body).into_owned())
    }

    /// Async alias for [`into_text`](Self::into_text); does not perform additional I/O.
    pub async fn text(self) -> Result<String> {
        self.into_text()
    }

    /// Returns the body after checking for a success status.
    ///
    /// Prefer this over [`bytes_checked`](Self::bytes_checked) on hot paths.
    pub fn into_bytes_checked(self) -> Result<Bytes> {
        self.error_for_status()?;
        Ok(self.body)
    }

    /// Async alias for [`into_bytes_checked`](Self::into_bytes_checked).
    pub async fn bytes_checked(self) -> Result<Bytes> {
        self.into_bytes_checked()
    }

    /// Deserializes JSON after checking for a success status, using the client or request
    /// [`JsonParserFn`](crate::json_parser::JsonParserFn) when configured.
    ///
    /// Prefer this over [`json`](Self::json) on hot paths. See [`crate::json_parser`] for the
    /// default single-step path vs a custom two-step parser.
    #[cfg(feature = "json")]
    pub fn into_json<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        self.error_for_status()?;
        crate::json_parser::deserialize(&self.body, self.status, self.json_parser.as_ref())
    }

    /// Async alias for [`into_json`](Self::into_json).
    #[cfg(feature = "json")]
    pub async fn json<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        self.into_json()
    }

    /// Deserializes JSON in one step with a custom closure (`Bytes` → `T`).
    ///
    /// Ignores any client- or request-level [`JsonParserFn`](crate::json_parser::JsonParserFn).
    /// Use this for BOM stripping or other transforms without the `Value` intermediate
    /// required by [`ClientBuilder::json_parser`](crate::client::ClientBuilder::json_parser).
    #[cfg(feature = "json")]
    pub fn into_json_with<T, F>(self, parse: F) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        F: FnOnce(&Bytes) -> std::result::Result<T, String>,
    {
        self.error_for_status()?;
        parse(&self.body).map_err(|message| {
            crate::json_parser::deserialize_error(self.status, message, &self.body)
        })
    }

    /// Async alias for [`into_json_with`](Self::into_json_with).
    #[cfg(feature = "json")]
    pub async fn json_with<T, F>(self, parse: F) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        F: FnOnce(&Bytes) -> std::result::Result<T, String>,
    {
        self.into_json_with(parse)
    }

    /// Deserializes JSON without checking HTTP status, using the configured [`JsonParserFn`](crate::json_parser::JsonParserFn) when set.
    #[cfg(feature = "json")]
    pub fn into_json_unchecked<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        crate::json_parser::deserialize(&self.body, self.status, self.json_parser.as_ref())
    }

    /// Async alias for [`into_json_unchecked`](Self::into_json_unchecked).
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

    /// Splits into status, headers, and body.
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

    #[test]
    fn into_json_with_strips_bom_without_client_parser() {
        let response = Response::new(
            StatusCode::OK,
            HeaderMap::new(),
            Bytes::from_static(b"\xef\xbb\xbf{\"id\":3}"),
            None,
            None,
        );
        let parsed: IdOnly = response
            .into_json_with(|body| {
                let slice = body.strip_prefix(b"\xef\xbb\xbf").unwrap_or(body);
                serde_json::from_slice(slice).map_err(|e| e.to_string())
            })
            .unwrap();
        assert_eq!(parsed, IdOnly { id: 3 });
    }
}
