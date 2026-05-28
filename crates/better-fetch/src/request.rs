//! Per-request fluent builder.
//!
//! Obtain a [`RequestBuilder`] from [`Client::get`](crate::Client::get) (or other verbs), chain
//! path/query/body options, then call [`RequestBuilder::send`] or [`RequestBuilder::send_json`].

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
use crate::streaming::StreamingResponse;
use crate::url_build::QueryValue;
use crate::Result;
use url::Url;

#[cfg(feature = "json")]
use crate::json_parser::JsonParserFn;

/// Parses a header name/value pair for request or client default headers.
pub(crate) fn parse_request_header(
    key: impl AsRef<str>,
    value: impl AsRef<str>,
) -> Result<(http::HeaderName, http::HeaderValue)> {
    let name = http::HeaderName::from_bytes(key.as_ref().as_bytes())
        .map_err(|e| Error::InvalidHeaderName(e.to_string()))?;
    let value = http::HeaderValue::from_str(value.as_ref())
        .map_err(|e| Error::InvalidHeaderValue(e.to_string()))?;
    Ok((name, value))
}

/// Fluent builder for a single HTTP request.
///
/// By default [`send`](Self::send) returns [`Response`] even on non-2xx status. Use
/// [`throw_on_error`](Self::throw_on_error)(`true`) to get `Err` from `send`, or use
/// [`send_json`](Self::send_json) which checks status before deserializing.
#[must_use = "request builders do nothing until you call `.send().await`, `.send_stream().await`, or similar"]
pub struct RequestBuilder<'a> {
    pub(crate) client: &'a Client,
    pub(crate) method: Method,
    pub(crate) path: String,
    pub(crate) base_url: Option<url::Url>,
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
    pub(crate) max_response_bytes: Option<u64>,
    pub(crate) retry_body_peek_bytes: Option<u64>,
    #[cfg(feature = "json")]
    pub(crate) json_parser: Option<JsonParserFn>,
    #[cfg(feature = "validate")]
    pub(crate) validate_response: bool,
}

impl<'a> RequestBuilder<'a> {
    /// Sets a path template parameter (`:key` in the path).
    pub fn param(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.params.insert(key.into(), value.to_string());
        self
    }

    /// Merges path parameters from a map.
    pub fn params(mut self, params: HashMap<String, String>) -> Self {
        self.params.extend(params);
        self
    }

    /// Merges path parameters from an iterator.
    pub fn params_iter(
        mut self,
        params: impl IntoIterator<Item = (impl Into<String>, impl ToString)>,
    ) -> Self {
        for (k, v) in params {
            self.params.insert(k.into(), v.to_string());
        }
        self
    }

    /// Merges path parameters in iterator order (substitution follows `:segment` order in the path).
    ///
    /// Alias for [`params_iter`](Self::params_iter); prefer this name when documenting ordered routes.
    pub fn params_ordered(
        self,
        params: impl IntoIterator<Item = (impl Into<String>, impl ToString)>,
    ) -> Self {
        self.params_iter(params)
    }

    /// Overrides the client base URL for this request only.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{Client, Result};
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// let client = Client::new("https://api.example.com")?;
    /// let _ = client
    ///     .get("/health")
    ///     .base_url("https://status.example.com")?
    ///     .send()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn base_url(mut self, base_url: impl AsRef<str>) -> Result<Self> {
        self.base_url = Some(Url::parse(base_url.as_ref()).map_err(Error::InvalidBaseUrl)?);
        Ok(self)
    }

    /// Adds a query string parameter.
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

    /// Serializes `value` as JSON and uses it as a query parameter (feature `json`).
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

    /// Adds a request header.
    pub fn header(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Result<Self> {
        let (name, value) = parse_request_header(key, value)?;
        self.headers.insert(name, value);
        Ok(self)
    }

    /// Sets a JSON request body (feature `json`).
    #[cfg(feature = "json")]
    pub fn json<T: serde::Serialize>(mut self, body: &T) -> Result<Self> {
        let bytes = serde_json::to_vec(body).map_err(|e| Error::Config(e.to_string()))?;
        self.body = HttpBody::Bytes(Bytes::from(bytes));
        if !self.headers.contains_key(http::header::CONTENT_TYPE) {
            self.headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            );
        }
        Ok(self)
    }

    /// Sets a raw request body.
    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = HttpBody::Bytes(body.into());
        self
    }

    /// Sets `Content-Type` when not already present.
    pub fn content_type(mut self, value: impl AsRef<str>) -> Result<Self> {
        self.headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_str(value.as_ref())
                .map_err(|e| Error::InvalidHeaderValue(e.to_string()))?,
        );
        Ok(self)
    }

    /// Sets a streaming request body (not replayable with automatic retry).
    ///
    /// Sets `Content-Type` to `application/octet-stream` when not already set.
    pub fn body_stream(mut self, stream: crate::BodyStream) -> Self {
        self.body = HttpBody::Stream(stream);
        if !self.headers.contains_key(http::header::CONTENT_TYPE) {
            self.headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/octet-stream"),
            );
        }
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
    ///
    /// Automatic retry is not supported when multipart bodies are used.
    #[cfg(feature = "multipart")]
    pub fn multipart(mut self, form: crate::multipart::Form) -> Self {
        self.multipart = Some(form);
        self.body = HttpBody::Empty;
        self
    }

    /// Overrides the client default timeout for this request.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Overrides the client default retry policy for this request.
    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = Some(policy);
        self
    }

    /// Overrides authentication for this request.
    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Sets bearer authentication for this request.
    pub fn bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(Auth::bearer(token));
        self
    }

    /// Cancels the in-flight request and retry sleeps when this token is triggered.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{CancellationToken, Client, Result};
    /// # use std::time::Duration;
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// let client = Client::new("https://api.example.com")?;
    /// let token = CancellationToken::new();
    /// let token_clone = token.clone();
    /// tokio::spawn(async move {
    ///     tokio::time::sleep(Duration::from_millis(10)).await;
    ///     token_clone.cancel();
    /// });
    /// let err = client
    ///     .get("/slow")
    ///     .cancellation_token(token)
    ///     .send()
    ///     .await
    ///     .unwrap_err();
    /// assert!(err.is_cancelled());
    /// # Ok(())
    /// # }
    /// ```
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

    /// Executes the request and returns the [`Response`].
    ///
    /// Non-2xx responses are returned as `Ok(Response)` unless [`throw_on_error`](Self::throw_on_error)
    /// is `true`. Deserialize JSON with [`Response::json`](crate::Response::json) or use
    /// [`send_json`](Self::send_json) for a one-step typed result.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{Client, Result};
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// let client = Client::new("https://api.example.com")?;
    /// let response = client.get("/users/1").send().await?;
    /// if response.is_success() {
    ///     println!("status {}", response.status());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send(self) -> Result<Response> {
        self.client.execute(self).await
    }

    /// Maximum response body size in bytes for this request.
    ///
    /// Applies to [`send`](Self::send), [`send_json`](Self::send_json), [`send_stream`](Self::send_stream),
    /// and [`StreamingResponse::collect`](crate::StreamingResponse::collect). When the body would exceed
    /// the limit, returns [`Error::BodyTooLarge`](crate::Error::BodyTooLarge). On the streaming path,
    /// the limit is also enforced incrementally on each chunk.
    pub fn max_response_bytes(mut self, limit: u64) -> Self {
        self.max_response_bytes = Some(limit);
        self
    }

    /// Overrides the client default for how many bytes may be read when a custom retry predicate runs on a stream.
    pub fn retry_body_peek_bytes(mut self, limit: u64) -> Self {
        self.retry_body_peek_bytes = Some(limit);
        self
    }

    /// Executes the request and returns a [`StreamingResponse`] without buffering the full body.
    ///
    /// Uses [`Hooks::on_request`](crate::Hooks::on_request), [`Hooks::on_response_stream`](crate::Hooks::on_response_stream),
    /// and [`Hooks::on_success_stream`](crate::Hooks::on_success_stream) (2xx). Buffered
    /// [`Hooks::on_response`](crate::Hooks::on_response) / [`on_success`](crate::Hooks::on_success) are not called.
    /// Custom retry predicates may peek up to [`ClientBuilder::retry_body_peek_bytes`](crate::ClientBuilder::retry_body_peek_bytes).
    /// Cancellation wakes pending body reads via the cancellation token (checked on each stream poll).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{Client, Result};
    /// # use futures_util::StreamExt;
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// let client = Client::new("https://api.example.com")?;
    /// let mut response = client.get("/export").send_stream().await?;
    /// while let Some(chunk) = response.bytes_stream().next().await {
    ///     let _chunk = chunk?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_stream(self) -> Result<StreamingResponse> {
        self.client.execute_stream(self).await
    }

    /// Executes the request and deserializes JSON on success (feature `json`).
    ///
    /// Fails with [`Error::Http`](crate::Error::Http) or [`Error::Deserialize`](crate::Error::Deserialize)
    /// on non-2xx or invalid JSON.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{Client, Result};
    /// # use serde::Deserialize;
    /// # #[derive(Deserialize)]
    /// # struct User { id: u64 }
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// let client = Client::new("https://api.example.com")?;
    /// let user: User = client.get("/users/:id").param("id", 1).send_json().await?;
    /// # Ok(())
    /// # }
    /// ```
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

    /// `send` + [`Response::json_validated`](crate::Response::json_validated) (feature `validate`).
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

    /// Serializes and validates `body` with [`garde::Validate`] before sending (feature `validate`).
    #[cfg(feature = "validate")]
    pub fn json_validated<T>(mut self, body: &T) -> Result<Self>
    where
        T: serde::Serialize + garde::Validate,
        T::Context: Default,
    {
        body.validate().map_err(|report| Error::RequestValidation {
            message: report.to_string(),
        })?;
        let bytes = serde_json::to_vec(body).map_err(|e| Error::Config(e.to_string()))?;
        self.body = HttpBody::Bytes(Bytes::from(bytes));
        if !self.headers.contains_key(http::header::CONTENT_TYPE) {
            self.headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            );
        }
        Ok(self)
    }
}
