//! Error types and helpers for HTTP, transport, hooks, and retries.
//!
//! Most operations return [`crate::Result`]. Use [`Error::status`] and [`Error::body`] on HTTP
//! failures, [`Error::transport_kind`] on transport failures, and [`Error::api_json`] to parse
//! structured API error payloads.

use std::fmt;

use bytes::Bytes;
use http::StatusCode;
use thiserror::Error;

/// Classification of underlying transport failures (connection, body, decode, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportKind {
    /// Connection failed (TCP/TLS/DNS and similar).
    Connect,
    /// Request or response body error.
    Body,
    /// Response body decoding error (e.g. decompression).
    Decode,
    /// Redirect policy violation.
    Redirect,
    /// Error building or sending the request.
    Request,
    /// Invalid request configuration.
    Builder,
    /// Protocol upgrade failure.
    Upgrade,
    /// Unclassified transport failure.
    Other,
}

impl fmt::Display for TransportKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect => write!(f, "connect"),
            Self::Body => write!(f, "body"),
            Self::Decode => write!(f, "decode"),
            Self::Redirect => write!(f, "redirect"),
            Self::Request => write!(f, "request"),
            Self::Builder => write!(f, "builder"),
            Self::Upgrade => write!(f, "upgrade"),
            Self::Other => write!(f, "other"),
        }
    }
}

/// Error type for better-fetch operations.
#[derive(Debug, Error, Clone)]
#[must_use = "errors must be handled or propagated with `?`"]
pub enum Error {
    /// Base URL parsing failed ([`ClientBuilder::base_url`](crate::ClientBuilder::base_url)).
    #[error("invalid base URL: {0}")]
    InvalidBaseUrl(#[from] url::ParseError),

    /// Underlying transport failure (connection, DNS, body read, etc.).
    #[error("transport error ({kind}): {message}")]
    Transport {
        /// Coarse category aligned with reqwest's `is_*` helpers.
        kind: TransportKind,
        /// Human-readable detail (typically from the underlying error's `Display`).
        message: String,
    },

    /// Non-success HTTP response (when using throw mode or `send_json`).
    #[error("HTTP {status} {status_text}: {message}")]
    Http {
        /// HTTP status code.
        status: StatusCode,
        /// Canonical reason phrase.
        status_text: String,
        /// Human-readable message.
        message: String,
        /// Response body when buffered.
        body: Option<Bytes>,
    },

    /// JSON response could not be deserialized (feature `json`).
    #[cfg(feature = "json")]
    #[error("failed to deserialize response body: {message}")]
    Deserialize {
        status: StatusCode,
        message: String,
        body: Option<Bytes>,
    },

    /// Response failed garde validation (feature `validate`).
    #[cfg(feature = "validate")]
    #[error("response validation failed: {message}")]
    Validation {
        status: StatusCode,
        message: String,
        body: Option<Bytes>,
    },

    /// Request exceeded the configured timeout.
    #[error("request timed out")]
    Timeout,

    /// Request was cancelled via [`CancellationToken`](crate::CancellationToken).
    #[error("request was cancelled")]
    Cancelled,

    /// Response body exceeded [`ClientBuilder::max_response_bytes`](crate::ClientBuilder::max_response_bytes)
    /// or a per-request [`RequestBuilder::max_response_bytes`](crate::RequestBuilder::max_response_bytes) limit.
    #[error("response body exceeded limit of {limit} bytes")]
    BodyTooLarge {
        /// Configured maximum response size in bytes.
        limit: u64,
    },

    /// [`ClientBuilder::build`](crate::ClientBuilder::build) without [`ClientBuilder::base_url`](crate::ClientBuilder::base_url).
    #[error("client base URL is required; call ClientBuilder::base_url")]
    MissingBaseUrl,

    /// Transport retries were exhausted.
    #[error("retries exhausted after {attempts} attempts")]
    RetryExhausted {
        /// Total attempts made (initial + retries).
        attempts: u32,
        /// Last error before retries were exhausted, when available.
        last: Option<Box<Error>>,
    },

    /// Returned from [`on_request`](crate::hooks::Hooks::on_request) or
    /// [`on_response`](crate::hooks::Hooks::on_response) to abort the pipeline.
    /// Prefer constructing this with [`Error::hook`](Self::hook) rather than [`Error::Other`](Self::Other).
    #[error("hook error: {0}")]
    Hook(String),

    /// Catch-all for configuration or plugin errors.
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Builds a transport error with an explicit [`TransportKind`].
    pub fn transport(kind: TransportKind, message: impl Into<String>) -> Self {
        Self::Transport {
            kind,
            message: message.into(),
        }
    }

    /// Builds a transport error with [`TransportKind::Other`].
    pub fn transport_message(message: impl Into<String>) -> Self {
        Self::transport(TransportKind::Other, message)
    }

    /// Returns the transport failure category when this error is [`Error::Transport`].
    pub fn transport_kind(&self) -> Option<TransportKind> {
        match self {
            Self::Transport { kind, .. } => Some(*kind),
            _ => None,
        }
    }

    /// Returns the transport error detail string when this error is [`Error::Transport`].
    pub fn transport_detail(&self) -> Option<&str> {
        match self {
            Self::Transport { message, .. } => Some(message),
            _ => None,
        }
    }

    /// Returns `true` when this error is [`Error::Transport`].
    pub fn is_transport(&self) -> bool {
        matches!(self, Self::Transport { .. })
    }

    /// Returns `true` when this error is [`Error::Timeout`].
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout)
    }

    /// Builds an HTTP error with canonical status text.
    pub fn http(status: StatusCode, message: impl Into<String>, body: Option<Bytes>) -> Self {
        Self::http_with_status_text(
            status,
            status.canonical_reason().unwrap_or("").to_string(),
            message,
            body,
        )
    }

    /// Builds an HTTP error with explicit status text.
    pub fn http_with_status_text(
        status: StatusCode,
        status_text: impl Into<String>,
        message: impl Into<String>,
        body: Option<Bytes>,
    ) -> Self {
        Self::Http {
            status,
            status_text: status_text.into(),
            message: message.into(),
            body,
        }
    }

    /// Returns the HTTP status when this error is response-related.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Http { status, .. } => Some(*status),
            #[cfg(feature = "json")]
            Self::Deserialize { status, .. } => Some(*status),
            #[cfg(feature = "validate")]
            Self::Validation { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Returns the canonical status text for [`Error::Http`].
    pub fn status_text(&self) -> Option<&str> {
        match self {
            Self::Http { status_text, .. } => Some(status_text),
            _ => None,
        }
    }

    /// Returns the response body when present on HTTP, deserialize, or validation errors.
    pub fn body(&self) -> Option<&Bytes> {
        match self {
            Self::Http { body, .. } => body.as_ref(),
            #[cfg(feature = "json")]
            Self::Deserialize { body, .. } => body.as_ref(),
            #[cfg(feature = "validate")]
            Self::Validation { body, .. } => body.as_ref(),
            _ => None,
        }
    }

    /// Returns `true` when transport retries were configured but all attempts failed.
    pub fn is_retry_exhausted(&self) -> bool {
        matches!(self, Self::RetryExhausted { .. })
    }

    /// Returns the last error from [`Error::RetryExhausted`] when present.
    pub fn retry_exhausted_last(&self) -> Option<&Error> {
        match self {
            Self::RetryExhausted { last, .. } => last.as_deref(),
            _ => None,
        }
    }

    /// Returns `true` when the request was cancelled via [`CancellationToken`](crate::CancellationToken).
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    /// Returns `true` when the response body exceeded a configured size limit.
    pub fn is_body_too_large(&self) -> bool {
        matches!(self, Self::BodyTooLarge { .. })
    }

    /// Returns the configured byte limit when this error is [`Error::BodyTooLarge`].
    pub fn body_too_large_limit(&self) -> Option<u64> {
        match self {
            Self::BodyTooLarge { limit } => Some(*limit),
            _ => None,
        }
    }

    /// Builds a hook failure for [`Hooks::on_request`](crate::hooks::Hooks::on_request) /
    /// [`Hooks::on_response`](crate::hooks::Hooks::on_response).
    pub fn hook(msg: impl Into<String>) -> Self {
        Self::Hook(msg.into())
    }

    /// Returns `true` when the error is [`Error::Hook`](Self::Hook).
    pub fn is_hook(&self) -> bool {
        matches!(self, Self::Hook(_))
    }

    pub(crate) fn retry_exhausted(attempts: u32, last: Error) -> Self {
        Self::RetryExhausted {
            attempts,
            last: Some(Box::new(last)),
        }
    }

    /// Parses the error response body as JSON (for API error payloads).
    ///
    /// # Examples
    ///
    /// ```
    /// use better_fetch::Error;
    /// use http::StatusCode;
    /// use serde::Deserialize;
    ///
    /// #[derive(Debug, Deserialize, PartialEq)]
    /// struct ApiError {
    ///     message: String,
    /// }
    ///
    /// let err = Error::http_with_status_text(
    ///     StatusCode::BAD_REQUEST,
    ///     "Bad Request",
    ///     "bad request",
    ///     Some(bytes::Bytes::from_static(br#"{"message":"invalid"}"#)),
    /// );
    /// let api: ApiError = err.api_json().unwrap();
    /// assert_eq!(api.message, "invalid");
    /// ```
    #[cfg(feature = "json")]
    pub fn api_json<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        let body = self.body()?;
        serde_json::from_slice(body).ok()
    }

    /// Parses and validates the error response body (feature `validate`).
    #[cfg(feature = "validate")]
    pub fn api_json_validated<T>(&self) -> Option<T>
    where
        T: serde::de::DeserializeOwned + garde::Validate,
        T::Context: Default,
    {
        let body = self.body()?;
        let value: T = serde_json::from_slice(body).ok()?;
        value.validate().ok()?;
        Some(value)
    }
}

pub(crate) fn map_transport_error(err: reqwest::Error) -> Error {
    if err.is_timeout() {
        return Error::Timeout;
    }

    let kind = transport_kind_from_reqwest(&err);
    Error::Transport {
        kind,
        message: err.to_string(),
    }
}

fn transport_kind_from_reqwest(err: &reqwest::Error) -> TransportKind {
    #[cfg(not(target_arch = "wasm32"))]
    if err.is_connect() {
        return TransportKind::Connect;
    }

    if err.is_body() {
        TransportKind::Body
    } else if err.is_decode() {
        TransportKind::Decode
    } else if err.is_redirect() {
        TransportKind::Redirect
    } else if err.is_request() {
        TransportKind::Request
    } else if err.is_builder() {
        TransportKind::Builder
    } else if err.is_upgrade() {
        TransportKind::Upgrade
    } else {
        TransportKind::Other
    }
}

#[cfg(all(test, feature = "json"))]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct ApiError {
        message: String,
    }

    #[test]
    fn api_json_parses_http_body() {
        let err = Error::http_with_status_text(
            StatusCode::BAD_REQUEST,
            "Bad Request",
            "bad request",
            Some(bytes::Bytes::from_static(br#"{"message":"invalid"}"#)),
        );
        let api: ApiError = err.api_json().unwrap();
        assert_eq!(api.message, "invalid");
    }

    #[test]
    fn status_and_status_text_accessors() {
        let err = Error::http(StatusCode::NOT_FOUND, "not found", None);
        assert_eq!(err.status(), Some(StatusCode::NOT_FOUND));
        assert_eq!(err.status_text(), Some("Not Found"));
    }

    #[test]
    fn api_json_returns_none_without_body() {
        let err = Error::http(StatusCode::INTERNAL_SERVER_ERROR, "err", None);
        assert!(err.api_json::<ApiError>().is_none());
    }

    #[test]
    fn hook_constructor_and_is_hook() {
        let err = Error::hook("blocked");
        assert!(err.is_hook());
        assert!(matches!(err, Error::Hook(msg) if msg == "blocked"));
    }

    #[test]
    fn retry_exhausted_helper_sets_flag() {
        let err = Error::retry_exhausted(3, Error::Timeout);
        assert!(err.is_retry_exhausted());
        assert!(matches!(
            err,
            Error::RetryExhausted {
                attempts: 3,
                last: Some(_)
            }
        ));
        assert!(matches!(err.retry_exhausted_last(), Some(Error::Timeout)));
    }

    #[test]
    fn transport_helpers() {
        let err = Error::transport(TransportKind::Connect, "connection refused");
        assert!(err.is_transport());
        assert_eq!(err.transport_kind(), Some(TransportKind::Connect));
        assert_eq!(err.transport_detail(), Some("connection refused"));
    }

    #[test]
    fn transport_message_defaults_to_other() {
        let err = Error::transport_message("tower layer failed");
        assert_eq!(err.transport_kind(), Some(TransportKind::Other));
    }
}
