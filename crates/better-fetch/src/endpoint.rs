use http::Method;

use crate::client::Client;
use crate::request::RequestBuilder;

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

    type Params: Default;
    type Query: Default;
}

impl Client {
    /// Start a request for a typed [`Endpoint`].
    pub fn call<E: Endpoint>(&self) -> RequestBuilder<'_> {
        self.request(E::METHOD, E::PATH)
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
