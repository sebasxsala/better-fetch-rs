//! Typed API routes via the [`Endpoint`] trait.
//!
//! Define routes as types, then use [`Client::call`](crate::Client::call) for a typed
//! [`EndpointRequestBuilder`]. See also the [`endpoint!`] macro for simple definitions.

use std::marker::PhantomData;

use http::Method;
use indexmap::IndexMap;

use crate::request::RequestBuilder;
use crate::url_build::QueryValue;

#[cfg(feature = "json")]
use serde::de::DeserializeOwned;

/// Describes a typed API route.
///
/// Implement this trait (or use [`endpoint!`]) and call [`Client::call`](crate::Client::call).
///
/// # Examples
///
/// ```no_run
/// # use better_fetch::{Client, Endpoint, Result};
/// # use http::Method;
/// # use serde::Deserialize;
/// struct GetTodo;
///
/// impl Endpoint for GetTodo {
///     const METHOD: Method = Method::GET;
///     const PATH: &'static str = "/todos/:id";
///     type Response = Todo;
///     type Params = ();
///     type Query = ();
/// }
///
/// #[derive(Deserialize)]
/// struct Todo { id: u64, title: String }
///
/// # #[tokio::main]
/// # async fn main() -> Result<()> {
/// let client = Client::new("https://jsonplaceholder.typicode.com")?;
/// let todo: Todo = client.call::<GetTodo>().param("id", 1).send_json().await?;
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
    /// Query parameters applied via [`EndpointRequestBuilder::query`].
    type Query: EndpointQuery + Default;
}

/// Applies path parameters to a [`RequestBuilder`].
pub trait EndpointParams {
    /// Applies this type's parameters to `builder`.
    fn apply_params(self, builder: RequestBuilder<'_>) -> RequestBuilder<'_>;
}

impl EndpointParams for () {
    fn apply_params(self, builder: RequestBuilder<'_>) -> RequestBuilder<'_> {
        builder
    }
}

impl EndpointParams for std::collections::HashMap<String, String> {
    fn apply_params(self, builder: RequestBuilder<'_>) -> RequestBuilder<'_> {
        builder.params(self)
    }
}

impl EndpointParams for Vec<(String, String)> {
    fn apply_params(self, builder: RequestBuilder<'_>) -> RequestBuilder<'_> {
        builder.params_iter(self)
    }
}

/// Applies query parameters to a [`RequestBuilder`].
pub trait EndpointQuery {
    /// Applies this type's query map to `builder`.
    fn apply_query(self, builder: RequestBuilder<'_>) -> RequestBuilder<'_>;
}

impl EndpointQuery for () {
    fn apply_query(self, builder: RequestBuilder<'_>) -> RequestBuilder<'_> {
        builder
    }
}

impl EndpointQuery for IndexMap<String, QueryValue> {
    fn apply_query(self, builder: RequestBuilder<'_>) -> RequestBuilder<'_> {
        builder.queries(self)
    }
}

/// Fluent builder for a typed [`Endpoint`].
pub struct EndpointRequestBuilder<'a, E: Endpoint> {
    pub(crate) inner: RequestBuilder<'a>,
    _marker: PhantomData<E>,
}

impl<'a, E: Endpoint> EndpointRequestBuilder<'a, E> {
    pub(crate) fn new(inner: RequestBuilder<'a>) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }

    /// Applies typed path parameters for `E::Params`.
    pub fn params(self, params: E::Params) -> Self {
        Self {
            inner: params.apply_params(self.inner),
            _marker: PhantomData,
        }
    }

    /// Applies typed query parameters for `E::Query`.
    pub fn query(self, query: E::Query) -> Self {
        Self {
            inner: query.apply_query(self.inner),
            _marker: PhantomData,
        }
    }

    /// Sets a single path parameter.
    pub fn param(self, key: impl Into<String>, value: impl ToString) -> Self {
        Self {
            inner: self.inner.param(key, value),
            _marker: PhantomData,
        }
    }

    /// Adds a query parameter.
    pub fn query_pair(self, key: impl Into<String>, value: impl ToString) -> Self {
        Self {
            inner: self.inner.query(key, value),
            _marker: PhantomData,
        }
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

    /// Executes the request and returns [`Response`](crate::Response).
    pub async fn send(self) -> crate::Result<crate::Response> {
        self.inner.send().await
    }

    /// Executes and deserializes `E::Response` (feature `json`).
    #[cfg(feature = "json")]
    pub async fn send_json(self) -> crate::Result<E::Response> {
        self.inner.send().await?.json::<E::Response>().await
    }

    /// Returns the underlying [`RequestBuilder`] for advanced options.
    pub fn into_inner(self) -> RequestBuilder<'a> {
        self.inner
    }
}

/// Defines a simple [`Endpoint`] with no params or query.
///
/// # Examples
///
/// ```
/// use better_fetch::endpoint;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// pub struct Health {
///     ok: bool,
/// }
///
/// endpoint!(HealthCheck, GET, "/health", Response = Health);
/// ```
#[macro_export]
macro_rules! endpoint {
    (
        $name:ident,
        $method:ident,
        $path:literal,
        Response = $response:ty
    ) => {
        pub struct $name;
        impl $crate::Endpoint for $name {
            const METHOD: http::Method = http::Method::$method;
            const PATH: &'static str = $path;
            type Response = $response;
            type Params = ();
            type Query = ();
        }
    };
}
