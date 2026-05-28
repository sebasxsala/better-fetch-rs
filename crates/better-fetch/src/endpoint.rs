//! Typed API routes via the [`Endpoint`] trait.
//!
//! Define routes as types, then use [`Client::call`](crate::Client::call) for a typed
//! [`EndpointRequestBuilder`]. Path params use [`.params()`](EndpointRequestBuilder::params)
//! with type-state ([`NeedsParams`]); query uses [`.query()`](EndpointRequestBuilder::query)
//! with structs when you want a query string — **not** enforced by type-state (see [`EndpointQuery`]).
//!
//! For ad-hoc string paths, use [`Client::get`](crate::Client::get) instead (see [`RequestBuilder`](crate::RequestBuilder)).
//!
//! Helpers: [`endpoint!`], [`define_params!`], and (feature `macros`) `EndpointParamsDerive` /
//! `EndpointQueryDerive`.

use std::marker::PhantomData;
use std::ops::Deref;

use http::Method;
use indexmap::IndexMap;

use crate::error::Error;
use crate::request::RequestBuilder;
use crate::url_build::QueryValue;

#[cfg(feature = "json")]
use serde::de::DeserializeOwned;

/// Type-state: path parameters still required before send.
#[derive(Debug, Clone, Copy, Default)]
pub struct NeedsParams;

/// Type-state: request body required before send (POST with typed body via `#[derive(Endpoint)]`).
#[derive(Debug, Clone, Copy, Default)]
pub struct NeedsBody;

/// Type-state: ready to configure query/headers and send.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ready;

/// Describes a typed API route.
///
/// Implement this trait (or use [`endpoint!`]) and call [`Client::call`](crate::Client::call).
///
/// **Compile-time enforcement:** [`EndpointParams`] can require [`.params()`](EndpointRequestBuilder::params)
/// via [`NeedsParams`]. [`EndpointBody`] can require a body via [`NeedsBody`] on POST routes.
/// [`EndpointQuery`] only types query values — [`.query()`](EndpointRequestBuilder::query) is optional
/// even when `Query` is not [`()`]; the `Default` bound is for struct construction, not auto-apply on send.
///
/// # Examples
///
/// ```no_run
/// # use better_fetch::{Client, Endpoint, EndpointParams, Result, define_params};
/// # use http::Method;
/// # use serde::Deserialize;
/// define_params!(GetTodoParams for "/todos/:id" { id: u64 });
///
/// struct GetTodo;
///
/// impl Endpoint for GetTodo {
///     const METHOD: Method = Method::GET;
///     const PATH: &'static str = "/todos/:id";
///     type Response = Todo;
///     type Params = GetTodoParams;
///     type Query = ();
///     type Body = ();
///     type Headers = ();
/// }
///
/// #[derive(Deserialize)]
/// struct Todo { id: u64, title: String }
///
/// # #[tokio::main]
/// # async fn main() -> Result<()> {
/// let client = Client::new("https://jsonplaceholder.typicode.com")?;
/// let todo: Todo = client
///     .call::<GetTodo>()
///     .params(GetTodoParams { id: 1 })
///     .send_json()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub trait Endpoint {
    /// HTTP method for this route.
    const METHOD: Method;
    /// Path template (may include `:param` segments).
    const PATH: &'static str;

    #[cfg(feature = "json")]
    /// JSON response type for [`EndpointRequestBuilder::send_json`].
    type Response: DeserializeOwned;

    #[cfg(not(feature = "json"))]
    /// Response type when the `json` feature is disabled.
    type Response;

    /// Path parameters applied via [`EndpointRequestBuilder::params`].
    type Params: EndpointParams + Default;
    /// Query parameters serialized by [`EndpointRequestBuilder::query`] when you call it.
    ///
    /// Not required at compile time: omitting [`.query()`](EndpointRequestBuilder::query) sends no
    /// typed query string. Use [`()`] when the route has no query struct.
    type Query: EndpointQuery + Default;

    /// Optional typed request body ([`()`] = none).
    type Body: EndpointBody + Default;

    /// Optional typed request headers ([`()`] = none).
    type Headers: EndpointHeaders + Default;
}

/// Applies a typed request body before send.
pub trait EndpointBody: Default + Sized {
    /// Builder state after [`.params()`](EndpointRequestBuilder::params) when path params were required.
    type ParamsNext: Default;
    /// Whether [`Client::call`](crate::Client::call) starts in [`NeedsBody`] (POST + required body).
    type CallInitial: Default;

    /// Applies this body to the builder.
    fn apply_body(self, builder: RequestBuilder<'_>) -> crate::Result<RequestBuilder<'_>>;
}

impl EndpointBody for () {
    type ParamsNext = Ready;
    type CallInitial = Ready;

    fn apply_body(self, builder: RequestBuilder<'_>) -> crate::Result<RequestBuilder<'_>> {
        Ok(builder)
    }
}

/// Default `()` params initial state when `E::Params` is [`()`].
pub trait DefaultParamsInitial<E: Endpoint> {
    fn initial(
        client: &crate::Client,
    ) -> EndpointRequestBuilder<'_, E, <E::Body as EndpointBody>::CallInitial>;
}

impl<E: Endpoint> DefaultParamsInitial<E> for ()
where
    E::Params: EndpointParams<BuilderState = Ready>,
    E::Body: EndpointBody<CallInitial = Ready>,
{
    fn initial(client: &crate::Client) -> EndpointRequestBuilder<'_, E, Ready> {
        EndpointRequestBuilder::new_ready(client.request(E::METHOD, E::PATH))
    }
}

/// Applies typed default headers before send.
pub trait EndpointHeaders: Default + Sized {
    /// Applies headers to the builder.
    fn apply_headers(self, builder: RequestBuilder<'_>) -> crate::Result<RequestBuilder<'_>>;
}

impl EndpointHeaders for () {
    fn apply_headers(self, builder: RequestBuilder<'_>) -> crate::Result<RequestBuilder<'_>> {
        Ok(builder)
    }
}

/// Initial builder state for an endpoint's path parameters.
pub type ParamsBuilderState<P> = <P as EndpointParams>::BuilderState;

/// Creates the initial [`EndpointRequestBuilder`] for `client.call::<E>()`.
pub trait EndpointParamsInitial<E: Endpoint>: EndpointParams {
    /// Type-state after [`Client::call`](crate::Client::call).
    type State;
    fn initial(client: &crate::Client) -> EndpointRequestBuilder<'_, E, Self::State>;
}

impl<E: Endpoint> EndpointParamsInitial<E> for ()
where
    (): DefaultParamsInitial<E>,
{
    type State = <E::Body as EndpointBody>::CallInitial;

    fn initial(client: &crate::Client) -> EndpointRequestBuilder<'_, E, Self::State> {
        <() as DefaultParamsInitial<E>>::initial(client)
    }
}

impl<E: Endpoint, P: EndpointParams<BuilderState = NeedsParams>> EndpointParamsInitial<E> for P {
    type State = NeedsParams;

    fn initial(client: &crate::Client) -> EndpointRequestBuilder<'_, E, NeedsParams> {
        EndpointRequestBuilder::new_needs_params(client.request(E::METHOD, E::PATH))
    }
}

/// Applies path parameters to a [`RequestBuilder`].
///
/// Unlike [`EndpointQuery`], this trait participates in type-state: non-unit params use
/// [`NeedsParams`] so [`.params()`](EndpointRequestBuilder::params) is required before send.
pub trait EndpointParams: Default + Sized {
    /// When [`NeedsParams`], [`.params()`](EndpointRequestBuilder::params) is required before send.
    type BuilderState;
    /// Applies this type's parameters to `builder`.
    fn apply_params(self, builder: RequestBuilder<'_>) -> RequestBuilder<'_>;
}

impl EndpointParams for () {
    type BuilderState = Ready;

    fn apply_params(self, builder: RequestBuilder<'_>) -> RequestBuilder<'_> {
        builder
    }
}

impl EndpointParams for std::collections::HashMap<String, String> {
    type BuilderState = NeedsParams;

    fn apply_params(self, builder: RequestBuilder<'_>) -> RequestBuilder<'_> {
        builder.params(self)
    }
}

impl EndpointParams for Vec<(String, String)> {
    type BuilderState = NeedsParams;

    fn apply_params(self, builder: RequestBuilder<'_>) -> RequestBuilder<'_> {
        builder.params_iter(self)
    }
}

/// Applies query parameters to a [`RequestBuilder`].
///
/// This trait does **not** use type-state: [`Client::call`](crate::Client::call) does not require
/// [`.query()`](EndpointRequestBuilder::query) before [`.send_json()`](EndpointRequestBuilder::send_json),
/// even when `E::Query` is a custom struct. Call [`.query()`](EndpointRequestBuilder::query) explicitly
/// to serialize `self` onto the request.
pub trait EndpointQuery {
    /// Applies this type's query map to `builder`.
    fn apply_query(self, builder: RequestBuilder<'_>) -> crate::Result<RequestBuilder<'_>>;
}

impl EndpointQuery for () {
    fn apply_query(self, builder: RequestBuilder<'_>) -> crate::Result<RequestBuilder<'_>> {
        Ok(builder)
    }
}

impl EndpointQuery for IndexMap<String, QueryValue> {
    fn apply_query(self, builder: RequestBuilder<'_>) -> crate::Result<RequestBuilder<'_>> {
        Ok(builder.queries(self))
    }
}

/// Applies a serde-serializable query struct to a request builder (feature `json`).
#[cfg(feature = "json")]
pub fn apply_serialized_query<T: serde::Serialize>(
    query: T,
    builder: RequestBuilder<'_>,
) -> crate::Result<RequestBuilder<'_>> {
    let map = crate::url_build::serialize_to_query_map(&query).map_err(|e| match e {
        Error::Other(msg) => Error::query_serialize(msg),
        other => other,
    })?;
    Ok(builder.queries(map))
}

/// Serializes and validates a query struct before applying it (feature `validate`).
#[cfg(all(feature = "json", feature = "validate"))]
pub fn apply_serialized_query_validated<T>(
    query: T,
    builder: RequestBuilder<'_>,
) -> crate::Result<RequestBuilder<'_>>
where
    T: serde::Serialize + garde::Validate,
    T::Context: Default,
{
    garde::Validate::validate(&query).map_err(|report: garde::Report| {
        Error::RequestValidation {
            message: report.to_string(),
        }
    })?;
    apply_serialized_query(query, builder)
}

/// Fluent builder for a typed [`Endpoint`].
///
/// When `E::Params` is not [`()`], the builder starts in [`NeedsParams`] and requires
/// [`.params()`](Self::params) before [`.send_json()`](Self::send_json).
/// When `E::Query` is not [`()`], [`.query()`](Self::query) is still **optional** — it only runs when called.
///
/// In [`Ready`] state, use [`.query(E::Query)`](Self::query) for typed query structs, or the forwarded
/// methods on this type (`.header`, `.json`, etc.). Prefer typed `.query()` over string keys on [`Deref`].
#[must_use = "endpoint builders do nothing until you call `.send().await`, `.send_json().await`, or similar"]
pub struct EndpointRequestBuilder<'a, E: Endpoint, S> {
    pub(crate) inner: RequestBuilder<'a>,
    _marker: PhantomData<(E, S)>,
}

impl<'a, E: Endpoint> EndpointRequestBuilder<'a, E, NeedsParams> {
    pub(crate) fn new_needs_params(inner: RequestBuilder<'a>) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }

    /// Applies typed path parameters and transitions to the next builder state.
    pub fn params(
        self,
        params: E::Params,
    ) -> EndpointRequestBuilder<'a, E, ParamsBuilderStateAfter<E>>
    where
        E::Body: EndpointBody,
    {
        EndpointRequestBuilder {
            inner: params.apply_params(self.inner),
            _marker: PhantomData,
        }
    }
}

/// Builder state after path params when `E::Body` may require a body.
pub type ParamsBuilderStateAfter<E> = <<E as Endpoint>::Body as EndpointBody>::ParamsNext;

impl<'a, E: Endpoint> EndpointRequestBuilder<'a, E, NeedsBody> {
    pub fn new_needs_body(inner: RequestBuilder<'a>) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }

    /// JSON request body (transitions to [`Ready`]).
    #[cfg(feature = "json")]
    pub fn json<T: serde::Serialize>(
        self,
        body: &T,
    ) -> crate::Result<EndpointRequestBuilder<'a, E, Ready>> {
        Ok(EndpointRequestBuilder {
            inner: self.inner.json(body)?,
            _marker: PhantomData,
        })
    }

    /// Validated JSON request body (feature `validate`).
    #[cfg(feature = "validate")]
    pub fn json_validated<T>(self, body: &T) -> crate::Result<EndpointRequestBuilder<'a, E, Ready>>
    where
        T: serde::Serialize + garde::Validate,
        T::Context: Default,
    {
        Ok(EndpointRequestBuilder {
            inner: self.inner.json_validated(body)?,
            _marker: PhantomData,
        })
    }

    /// Applies typed request body for `E::Body` (transitions to [`Ready`]).
    pub fn with_body(self, body: E::Body) -> crate::Result<EndpointRequestBuilder<'a, E, Ready>> {
        Ok(EndpointRequestBuilder {
            inner: body.apply_body(self.inner)?,
            _marker: PhantomData,
        })
    }

    /// Raw request body (transitions to [`Ready`]).
    pub fn body(self, body: impl Into<bytes::Bytes>) -> EndpointRequestBuilder<'a, E, Ready> {
        EndpointRequestBuilder {
            inner: self.inner.body(body),
            _marker: PhantomData,
        }
    }
}

impl<'a, E: Endpoint> EndpointRequestBuilder<'a, E, Ready> {
    pub(crate) fn new_ready(inner: RequestBuilder<'a>) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }

    /// Applies typed query parameters for `E::Query`.
    ///
    /// Optional at compile time: you can call [`.send_json()`](Self::send_json) without this method;
    /// no query string from `E::Query` is sent unless you call `.query(...)`.
    ///
    /// Returns [`Error::QuerySerialize`](crate::Error::QuerySerialize) when serde serialization fails
    /// (since 0.4.0 — failures are no longer ignored).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{Client, Endpoint, Result, define_params};
    /// # use http::Method;
    /// # use serde::{Deserialize, Serialize};
    /// define_params!(ItemParams for "/items/:id" { id: u64 });
    ///
    /// #[derive(Default, Serialize)]
    /// struct ItemQuery { tag: Option<String> }
    /// better_fetch::impl_serde_endpoint_query!(ItemQuery);
    ///
    /// struct GetItem;
    /// impl Endpoint for GetItem {
    ///     const METHOD: Method = Method::GET;
    ///     const PATH: &'static str = "/items/:id";
    ///     type Response = serde_json::Value;
    ///     type Params = ItemParams;
    ///     type Query = ItemQuery;
    ///     type Body = ();
    ///     type Headers = ();
    /// }
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// let client = Client::new("https://api.example.com")?;
    /// let _ = client
    ///     .call::<GetItem>()
    ///     .params(ItemParams { id: 1 })
    ///     .query(ItemQuery { tag: Some("news".into()) })?
    ///     .send_json()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn query(self, query: E::Query) -> crate::Result<Self> {
        Ok(Self {
            inner: query.apply_query(self.inner)?,
            _marker: PhantomData,
        })
    }

    /// Like [`Self::query`] but runs [`garde::Validate`] on the query value first (feature `validate`).
    ///
    /// Intended for serde query structs (including those using [`EndpointQueryDerive`](crate::EndpointQueryDerive)).
    #[cfg(all(feature = "json", feature = "validate"))]
    pub fn query_validated(self, query: E::Query) -> crate::Result<Self>
    where
        E::Query: serde::Serialize + garde::Validate,
        <E::Query as garde::Validate>::Context: Default,
    {
        Ok(Self {
            inner: apply_serialized_query_validated(query, self.inner)?,
            _marker: PhantomData,
        })
    }

    /// Adds a request header.
    pub fn header(self, key: impl AsRef<str>, value: impl AsRef<str>) -> crate::Result<Self> {
        Ok(Self {
            inner: self.inner.header(key, value)?,
            _marker: PhantomData,
        })
    }

    /// Sets bearer authentication.
    pub fn bearer_token(self, token: impl Into<String>) -> Self {
        Self {
            inner: self.inner.bearer_token(token),
            _marker: PhantomData,
        }
    }

    /// Attaches a cancellation token.
    pub fn cancellation_token(self, token: crate::CancellationToken) -> Self {
        Self {
            inner: self.inner.cancellation_token(token),
            _marker: PhantomData,
        }
    }

    /// When `true`, [`send`](Self::send) returns `Err` on non-2xx.
    pub fn throw_on_error(self, throw: bool) -> Self {
        Self {
            inner: self.inner.throw_on_error(throw),
            _marker: PhantomData,
        }
    }

    /// Overrides the client base URL for this request ([`RequestBuilder::base_url`](crate::RequestBuilder::base_url)).
    pub fn base_url(self, base_url: impl AsRef<str>) -> crate::Result<Self> {
        Ok(Self {
            inner: self.inner.base_url(base_url)?,
            _marker: PhantomData,
        })
    }

    /// Overrides retry policy ([`RequestBuilder::retry`](crate::RequestBuilder::retry)).
    pub fn retry(self, policy: crate::RetryPolicy) -> Self {
        Self {
            inner: self.inner.retry(policy),
            _marker: PhantomData,
        }
    }

    /// Overrides timeout ([`RequestBuilder::timeout`](crate::RequestBuilder::timeout)).
    pub fn timeout(self, timeout: std::time::Duration) -> Self {
        Self {
            inner: self.inner.timeout(timeout),
            _marker: PhantomData,
        }
    }

    /// Streaming execution ([`RequestBuilder::send_stream`](crate::RequestBuilder::send_stream)).
    pub async fn send_stream(self) -> crate::Result<crate::StreamingResponse> {
        self.inner.send_stream().await
    }

    /// Caps response body size ([`RequestBuilder::max_response_bytes`](crate::RequestBuilder::max_response_bytes)).
    pub fn max_response_bytes(self, limit: u64) -> Self {
        Self {
            inner: self.inner.max_response_bytes(limit),
            _marker: PhantomData,
        }
    }

    /// JSON request body ([`RequestBuilder::json`](crate::RequestBuilder::json)).
    #[cfg(feature = "json")]
    pub fn json<T: serde::Serialize>(self, body: &T) -> crate::Result<Self> {
        Ok(Self {
            inner: self.inner.json(body)?,
            _marker: PhantomData,
        })
    }

    /// Validated JSON request body (feature `validate`).
    #[cfg(feature = "validate")]
    pub fn json_validated<T>(self, body: &T) -> crate::Result<Self>
    where
        T: serde::Serialize + garde::Validate,
        T::Context: Default,
    {
        Ok(Self {
            inner: self.inner.json_validated(body)?,
            _marker: PhantomData,
        })
    }

    /// Raw request body ([`RequestBuilder::body`](crate::RequestBuilder::body)).
    pub fn body(self, body: impl Into<bytes::Bytes>) -> Self {
        Self {
            inner: self.inner.body(body),
            _marker: PhantomData,
        }
    }

    /// Executes the request and returns [`Response`](crate::Response).
    pub async fn send(self) -> crate::Result<crate::Response> {
        self.inner.send().await
    }

    /// Applies typed request body for `E::Body`.
    pub fn with_body(self, body: E::Body) -> crate::Result<Self> {
        Ok(Self {
            inner: body.apply_body(self.inner)?,
            _marker: PhantomData,
        })
    }

    /// Applies typed headers for `E::Headers`.
    pub fn with_headers(self, headers: E::Headers) -> crate::Result<Self> {
        Ok(Self {
            inner: headers.apply_headers(self.inner)?,
            _marker: PhantomData,
        })
    }

    /// Like [`Self::with_headers`] but runs [`garde::Validate`] on the headers value first (feature `validate`).
    #[cfg(feature = "validate")]
    pub fn with_headers_validated(self, headers: E::Headers) -> crate::Result<Self>
    where
        E::Headers: garde::Validate,
        <E::Headers as garde::Validate>::Context: Default,
    {
        garde::Validate::validate(&headers).map_err(|report: garde::Report| {
            Error::RequestValidation {
                message: report.to_string(),
            }
        })?;
        self.with_headers(headers)
    }

    /// Executes and deserializes `E::Response` (feature `json`).
    #[cfg(feature = "json")]
    pub async fn send_json(self) -> crate::Result<E::Response> {
        self.inner.send().await?.json::<E::Response>().await
    }

    /// Success/error deserialization by status (feature `json`).
    #[cfg(feature = "json")]
    pub async fn send_api<T, ErrBody>(self) -> crate::Result<std::result::Result<T, ErrBody>>
    where
        T: serde::de::DeserializeOwned,
        ErrBody: serde::de::DeserializeOwned,
    {
        crate::api_response::into_api_result(self.inner.send().await?)
    }
}

impl<'a, E: Endpoint> Deref for EndpointRequestBuilder<'a, E, Ready> {
    type Target = RequestBuilder<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Defines path parameters for a route and implements [`EndpointParams`].
///
/// Each struct field maps to a `:field` segment in `path` (by field name).
/// For compile-time path validation, use `#[derive(EndpointParamsDerive)]` (feature `macros`).
///
/// # Examples
///
/// ```
/// use better_fetch::{define_params, EndpointParams, NeedsParams};
///
/// define_params!(GetTodoParams for "/todos/:id" { id: u64 });
///
/// fn assert_needs_params<T: EndpointParams<BuilderState = NeedsParams>>() {}
/// assert_needs_params::<GetTodoParams>();
/// ```
#[macro_export]
macro_rules! define_params {
    (
        $name:ident for $path:literal {
            $( $field:ident : $ty:ty ),* $(,)?
        }
    ) => {
        #[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
        pub struct $name {
            $( pub $field: $ty, )*
        }

        impl $crate::EndpointParams for $name {
            type BuilderState = $crate::NeedsParams;

            fn apply_params(self, builder: $crate::RequestBuilder<'_>) -> $crate::RequestBuilder<'_> {
                let builder = builder;
                $(
                    let builder = builder.param(stringify!($field), self.$field);
                )*
                builder
            }
        }
    };
}

/// Implements [`EndpointQuery`] for a serde-serializable query struct (feature `json`).
#[cfg(feature = "json")]
#[macro_export]
macro_rules! impl_serde_endpoint_query {
    ($ty:ty) => {
        impl $crate::EndpointQuery for $ty {
            fn apply_query(
                self,
                builder: $crate::RequestBuilder<'_>,
            ) -> $crate::Result<$crate::RequestBuilder<'_>> {
                $crate::endpoint::apply_serialized_query(self, builder)
            }
        }
    };
}

/// Defines a simple [`Endpoint`] with optional typed params and query.
///
/// # Examples
///
/// ```
/// use better_fetch::{endpoint, define_params};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// pub struct Health {
///     ok: bool,
/// }
///
/// endpoint!(HealthCheck, GET, "/health", Response = Health);
///
/// define_params!(GetTodoParams for "/todos/:id" { id: u64 });
/// endpoint!(GetTodo, GET, "/todos/:id", Response = Health, Params = GetTodoParams);
/// ```
#[macro_export]
macro_rules! endpoint {
    (
        $name:ident,
        $method:ident,
        $path:literal,
        Response = $response:ty
    ) => {
        $crate::endpoint!(
            $name,
            $method,
            $path,
            Response = $response,
            Params = (),
            Query = ()
        );
    };
    (
        $name:ident,
        $method:ident,
        $path:literal,
        Response = $response:ty,
        Params = $params:ty
    ) => {
        $crate::endpoint!(
            $name,
            $method,
            $path,
            Response = $response,
            Params = $params,
            Query = ()
        );
    };
    (
        $name:ident,
        $method:ident,
        $path:literal,
        Response = $response:ty,
        Query = $query:ty
    ) => {
        $crate::endpoint!(
            $name,
            $method,
            $path,
            Response = $response,
            Params = (),
            Query = $query
        );
    };
    (
        $name:ident,
        $method:ident,
        $path:literal,
        Response = $response:ty,
        Params = $params:ty,
        Query = $query:ty
    ) => {
        pub struct $name;
        impl $crate::Endpoint for $name {
            const METHOD: http::Method = http::Method::$method;
            const PATH: &'static str = $path;
            type Response = $response;
            type Params = $params;
            type Query = $query;
            type Body = ();
            type Headers = ();
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    define_params!(TestParams for "/items/:id" { id: u64 });

    #[test]
    fn params_builder_state_is_needs_params() {
        fn assert_needs<T: EndpointParams<BuilderState = NeedsParams>>() {}
        assert_needs::<TestParams>();
    }

    #[test]
    fn unit_params_builder_state_is_ready() {
        fn assert_ready<T: EndpointParams<BuilderState = Ready>>() {}
        assert_ready::<()>();
    }
}
