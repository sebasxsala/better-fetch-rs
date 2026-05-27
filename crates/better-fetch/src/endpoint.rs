use std::marker::PhantomData;

use http::Method;
use indexmap::IndexMap;

use crate::request::RequestBuilder;
use crate::url_build::QueryValue;

#[cfg(feature = "json")]
use serde::de::DeserializeOwned;

/// Describes a typed API route.
pub trait Endpoint {
    const METHOD: Method;
    const PATH: &'static str;

    #[cfg(feature = "json")]
    type Response: DeserializeOwned;

    #[cfg(not(feature = "json"))]
    type Response;

    type Params: EndpointParams + Default;
    type Query: EndpointQuery + Default;
}

/// Applies path parameters to a [`RequestBuilder`].
pub trait EndpointParams {
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

    pub fn params(self, params: E::Params) -> Self {
        Self {
            inner: params.apply_params(self.inner),
            _marker: PhantomData,
        }
    }

    pub fn query(self, query: E::Query) -> Self {
        Self {
            inner: query.apply_query(self.inner),
            _marker: PhantomData,
        }
    }

    pub fn param(self, key: impl Into<String>, value: impl ToString) -> Self {
        Self {
            inner: self.inner.param(key, value),
            _marker: PhantomData,
        }
    }

    pub fn query_pair(self, key: impl Into<String>, value: impl ToString) -> Self {
        Self {
            inner: self.inner.query(key, value),
            _marker: PhantomData,
        }
    }

    pub fn header(self, key: impl AsRef<str>, value: impl AsRef<str>) -> crate::Result<Self> {
        Ok(Self {
            inner: self.inner.header(key, value)?,
            _marker: PhantomData,
        })
    }

    pub fn bearer_token(self, token: impl Into<String>) -> Self {
        Self {
            inner: self.inner.bearer_token(token),
            _marker: PhantomData,
        }
    }

    pub fn cancellation_token(self, token: crate::CancellationToken) -> Self {
        Self {
            inner: self.inner.cancellation_token(token),
            _marker: PhantomData,
        }
    }

    pub fn throw_on_error(self, throw: bool) -> Self {
        Self {
            inner: self.inner.throw_on_error(throw),
            _marker: PhantomData,
        }
    }

    pub async fn send(self) -> crate::Result<crate::Response> {
        self.inner.send().await
    }

    #[cfg(feature = "json")]
    pub async fn send_json(self) -> crate::Result<E::Response> {
        self.inner.send().await?.json::<E::Response>().await
    }

    pub fn into_inner(self) -> RequestBuilder<'a> {
        self.inner
    }
}

/// Helper macro for simple endpoint definitions.
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
