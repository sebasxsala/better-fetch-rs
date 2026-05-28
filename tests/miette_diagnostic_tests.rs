#![cfg(feature = "miette")]

use better_fetch::{DiagnosticError, Error};
use http::Method;
use url::Url;

#[test]
fn diagnostic_error_wraps_fetch_error_with_url() {
    let err = Error::Http {
        status: http::StatusCode::NOT_FOUND,
        status_text: "Not Found".into(),
        message: "missing".into(),
        body: None,
    };
    let url: Url = "https://api.example.com/missing".parse().unwrap();
    let report = DiagnosticError::new(err, Some(&Method::GET), Some(&url));
    let message = format!("{report:?}");
    assert!(message.contains("api.example.com"));
}
