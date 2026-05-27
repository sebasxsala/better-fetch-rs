use bytes::Bytes;
use http::StatusCode;
use thiserror::Error;

/// Error type for better-fetch operations.
#[derive(Debug, Error, Clone)]
#[must_use = "errors must be handled or propagated with `?`"]
pub enum Error {
    #[error("invalid base URL: {0}")]
    InvalidBaseUrl(#[from] url::ParseError),

    #[error("transport error: {0}")]
    Transport(String),

    #[error("HTTP {status} {status_text}: {message}")]
    Http {
        status: StatusCode,
        status_text: String,
        message: String,
        body: Option<Bytes>,
    },

    #[cfg(feature = "json")]
    #[error("failed to deserialize response body: {message}")]
    Deserialize {
        status: StatusCode,
        message: String,
        body: Option<Bytes>,
    },

    #[cfg(feature = "validate")]
    #[error("response validation failed: {message}")]
    Validation {
        status: StatusCode,
        message: String,
        body: Option<Bytes>,
    },

    #[error("request timed out")]
    Timeout,

    #[error("request was cancelled")]
    Cancelled,

    #[error("client base URL is required; call ClientBuilder::base_url")]
    MissingBaseUrl,

    #[error("retries exhausted after {attempts} attempts")]
    RetryExhausted { attempts: u32, last: Option<String> },

    /// Returned from [`on_request`](crate::hooks::Hooks::on_request) or
    /// [`on_response`](crate::hooks::Hooks::on_response) to abort the pipeline.
    /// Prefer constructing this with [`Error::hook`](Self::hook) rather than [`Error::Other`](Self::Other).
    #[error("hook error: {0}")]
    Hook(String),

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn http(status: StatusCode, message: impl Into<String>, body: Option<Bytes>) -> Self {
        Self::http_with_status_text(
            status,
            status.canonical_reason().unwrap_or("").to_string(),
            message,
            body,
        )
    }

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

    pub fn status_text(&self) -> Option<&str> {
        match self {
            Self::Http { status_text, .. } => Some(status_text),
            _ => None,
        }
    }

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

    /// Returns `true` when the request was cancelled via [`CancellationToken`](crate::CancellationToken).
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
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
            last: Some(last.to_string()),
        }
    }

    /// Parse the error response body as JSON (for API error payloads).
    #[cfg(feature = "json")]
    pub fn api_json<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        let body = self.body()?;
        serde_json::from_slice(body).ok()
    }

    /// Parse and validate the error response body (feature `validate`).
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
        Error::Timeout
    } else {
        Error::Transport(err.to_string())
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
    }
}
