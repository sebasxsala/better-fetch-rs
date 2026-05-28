//! Rich diagnostics for [`Error`](crate::Error) when the `miette` feature is enabled.

use http::Method;
use miette::Diagnostic;
use thiserror::Error;
use url::Url;

use crate::error::Error;

/// Wraps a fetch [`Error`] with request context for [`miette`](https://docs.rs/miette) reporting.
#[derive(Debug, Error, Diagnostic)]
#[error("{inner}")]
pub struct DiagnosticError {
    #[source]
    inner: Error,
    #[help]
    method: Option<String>,
    #[help]
    url: Option<String>,
}

impl DiagnosticError {
    /// Attaches optional HTTP method and URL to `error` for prettier reports.
    pub fn new(error: Error, method: Option<&Method>, url: Option<&Url>) -> Self {
        Self {
            inner: error,
            method: method.map(|m| m.to_string()),
            url: url.map(|u| u.to_string()),
        }
    }
}

impl From<Error> for DiagnosticError {
    fn from(inner: Error) -> Self {
        Self {
            inner,
            method: None,
            url: None,
        }
    }
}
