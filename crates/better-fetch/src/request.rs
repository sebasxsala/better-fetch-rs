use std::collections::HashMap;
use std::time::Duration;

use bytes::Bytes;
use http::{HeaderMap, Method};
use indexmap::IndexMap;

use crate::auth::Auth;
use crate::backend::HttpBody;
use crate::cancel::CancellationToken;
use crate::client::Client;
use crate::error::Error;
use crate::response::Response;
use crate::retry::RetryPolicy;
use crate::url_build::QueryValue;
use crate::Result;

#[cfg(feature = "json")]
use crate::json_parser::JsonParserFn;

/// Fluent builder for a single HTTP request.
pub struct RequestBuilder<'a> {
    pub(crate) client: &'a Client,
    pub(crate) method: Method,
    pub(crate) path: String,
    pub(crate) params: HashMap<String, String>,
    pub(crate) query: IndexMap<String, QueryValue>,
    pub(crate) headers: HeaderMap,
    pub(crate) body: HttpBody,
    #[cfg(feature = "multipart")]
    pub(crate) multipart: Option<crate::multipart::Form>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) retry: Option<RetryPolicy>,
    pub(crate) auth: Option<Auth>,
    pub(crate) cancellation: Option<CancellationToken>,
    pub(crate) throw_on_error: bool,
    #[cfg(feature = "json")]
    pub(crate) json_parser: Option<JsonParserFn>,
    #[cfg(feature = "validate")]
    pub(crate) validate_response: bool,
}

impl<'a> RequestBuilder<'a> {
    pub fn param(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.params.insert(key.into(), value.to_string());
        self
    }

    pub fn params(mut self, params: HashMap<String, String>) -> Self {
        self.params.extend(params);
        self
    }

    pub fn params_iter(
        mut self,
        params: impl IntoIterator<Item = (impl Into<String>, impl ToString)>,
    ) -> Self {
        for (k, v) in params {
            self.params.insert(k.into(), v.to_string());
        }
        self
    }

    pub fn query(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.query
            .insert(key.into(), QueryValue::Scalar(value.to_string()));
        self
    }

    /// Sets multiple query parameters preserving insertion order.
    pub fn queries(mut self, query: IndexMap<String, QueryValue>) -> Self {
        for (k, v) in query {
            self.query.insert(k, v);
        }
        self
    }

    #[cfg(feature = "json")]
    pub fn query_json<T: serde::Serialize>(
        mut self,
        key: impl Into<String>,
        value: &T,
    ) -> Result<Self> {
        self.query
            .insert(key.into(), QueryValue::from_serializable(value)?);
        Ok(self)
    }

    pub fn header(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Result<Self> {
        let name = http::HeaderName::from_bytes(key.as_ref().as_bytes())
            .map_err(|e| Error::Other(format!("invalid header name: {e}")))?;
        let value = http::HeaderValue::from_str(value.as_ref())
            .map_err(|e| Error::Other(format!("invalid header value: {e}")))?;
        self.headers.insert(name, value);
        Ok(self)
    }

    #[cfg(feature = "json")]
    pub fn json<T: serde::Serialize>(mut self, body: &T) -> Result<Self> {
        let bytes = serde_json::to_vec(body).map_err(|e| Error::Other(e.to_string()))?;
        self.body = HttpBody::Bytes(Bytes::from(bytes));
        if !self.headers.contains_key(http::header::CONTENT_TYPE) {
            self.headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            );
        }
        Ok(self)
    }

    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = HttpBody::Bytes(body.into());
        self
    }

    /// URL-encoded form body (`application/x-www-form-urlencoded`).
    pub fn form<I, K, V>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in fields {
            serializer.append_pair(k.as_ref(), v.as_ref());
        }
        self.body = HttpBody::Bytes(Bytes::from(serializer.finish()));
        if !self.headers.contains_key(http::header::CONTENT_TYPE) {
            self.headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/x-www-form-urlencoded"),
            );
        }
        self
    }

    /// Multipart form body (requires the `multipart` feature).
    #[cfg(feature = "multipart")]
    pub fn multipart(mut self, form: crate::multipart::Form) -> Self {
        self.multipart = Some(form);
        self.body = HttpBody::Empty;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = Some(policy);
        self
    }

    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(Auth::bearer(token));
        self
    }

    /// Cancels the in-flight request and retry sleeps when this token is triggered.
    pub fn cancellation_token(mut self, token: CancellationToken) -> Self {
        self.cancellation = Some(token);
        self
    }

    /// When `true`, [`send`](Self::send) returns `Err` on non-2xx HTTP status (like upstream `throw: true`).
    pub fn throw_on_error(mut self, throw: bool) -> Self {
        self.throw_on_error = throw;
        self
    }

    /// Overrides the client's JSON parser for this request only.
    ///
    /// See [`crate::json_parser`] for fast path vs two-step parsing.
    #[cfg(feature = "json")]
    pub fn json_parser<F>(mut self, f: F) -> Self
    where
        F: Fn(&Bytes) -> std::result::Result<serde_json::Value, String> + Send + Sync + 'static,
    {
        self.json_parser = Some(crate::json_parser::json_parser(f));
        self
    }

    /// Overrides the client's JSON parser for this request only.
    #[cfg(feature = "json")]
    pub fn json_parser_fn(mut self, parser: JsonParserFn) -> Self {
        self.json_parser = Some(parser);
        self
    }

    pub async fn send(self) -> Result<Response> {
        self.client.execute(self).await
    }

    #[cfg(feature = "json")]
    #[must_use = "send the request with `.await` and handle the result"]
    pub async fn send_json<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        self.send().await?.json::<T>().await
    }

    /// When `false`, [`send_json_validated`](Self::send_json_validated) only deserializes (no garde).
    #[cfg(feature = "validate")]
    pub fn validate_response(mut self, validate: bool) -> Self {
        self.validate_response = validate;
        self
    }

    /// `send` + [`Response::json_validated`](crate::Response::json_validated).
    #[cfg(feature = "validate")]
    pub async fn send_json_validated<T>(self) -> Result<T>
    where
        T: serde::de::DeserializeOwned + garde::Validate,
        T::Context: Default,
    {
        if !self.validate_response {
            return self.send_json().await;
        }
        self.send().await?.json_validated().await
    }
}
