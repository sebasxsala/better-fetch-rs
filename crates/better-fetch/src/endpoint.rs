//! Typed API routes via the [`Endpoint`] trait.
//!
//! Define routes as types, then use [`Client::call`](crate::Client::call) for a typed
//! [`EndpointRequestBuilder`]. Path and query use [`.params()`](EndpointRequestBuilder::params)
//! and [`.query()`](EndpointRequestBuilder::query) with structs — not string keys.
//!
//! For ad-hoc string paths, use [`Client::get`](crate::Client::get) instead (see [`RequestBuilder`](crate::RequestBuilder)).
//!
//! Helpers: [`endpoint!`], [`define_params!`], and (feature `macros`) `EndpointParamsDerive` /
//! `EndpointQueryDerive`.

use std::marker::PhantomData;

use http::Method;
use indexmap::IndexMap;

use crate::request::RequestBuilder;
use crate::url_build::QueryValue;

#[cfg(feature = "json")]
use serde::de::DeserializeOwned;

/// Type-state: path parameters still required before send.
#[derive(Debug, Clone, Copy, Default)]
pub struct NeedsParams;

/// Type-state: ready to configure query/headers and send.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ready;

/// Describes a typed API route.
///
/// Implement this trait (or use [`endpoint!`]) and call [`Client::call`](crate::Client::call).
/// Path and query parameters are typed via [`EndpointParams`] and [`EndpointQuery`] structs;
/// use [`.params()`](EndpointRequestBuilder::params) and [`.query()`](EndpointRequestBuilder::query).
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
    /// Query parameters applied via [`EndpointRequestBuilder::query`].
    type Query: EndpointQuery + Default;
}

/// Initial builder state for an endpoint's path parameters.
pub type ParamsBuilderState<P> = <P as EndpointParams>::BuilderState;

/// Creates the initial [`EndpointRequestBuilder`] for `client.call::<E>()`.
pub trait EndpointParamsInitial<E: Endpoint>: EndpointParams {
    fn initial(client: &crate::Client) -> EndpointRequestBuilder<'_, E, Self::BuilderState>;
}

impl<E: Endpoint> EndpointParamsInitial<E> for () {
    fn initial(client: &crate::Client) -> EndpointRequestBuilder<'_, E, Ready> {
        EndpointRequestBuilder::new_ready(client.request(E::METHOD, E::PATH))
    }
}

impl<E: Endpoint, P: EndpointParams<BuilderState = NeedsParams>> EndpointParamsInitial<E> for P {
    fn initial(client: &crate::Client) -> EndpointRequestBuilder<'_, E, NeedsParams> {
        EndpointRequestBuilder::new_needs_params(client.request(E::METHOD, E::PATH))
    }
}

/// Applies path parameters to a [`RequestBuilder`].
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

/// Applies a serde-serializable query struct to a request builder (feature `json`).
#[cfg(feature = "json")]
pub fn apply_serialized_query<T: serde::Serialize>(
    query: T,
    builder: RequestBuilder<'_>,
) -> RequestBuilder<'_> {
    match crate::url_build::serialize_to_query_map(&query) {
        Ok(map) => builder.queries(map),
        Err(_) => builder,
    }
}

/// Fluent builder for a typed [`Endpoint`].
///
/// When `E::Params` is not [`()`], the builder starts in [`NeedsParams`] and requires
/// [`.params()`](Self::params) before [`.send_json()`](Self::send_json).
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

    /// Applies typed path parameters for `E::Params` and transitions to [`Ready`].
    pub fn params(self, params: E::Params) -> EndpointRequestBuilder<'a, E, Ready> {
        EndpointRequestBuilder {
            inner: params.apply_params(self.inner),
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
    pub fn query(self, query: E::Query) -> Self {
        Self {
            inner: query.apply_query(self.inner),
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
            ) -> $crate::RequestBuilder<'_> {
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
