//! Helpers for typed success vs error response bodies.

use crate::error::Error;
use crate::response::Response;
use crate::Result;

/// Deserializes a buffered response into success `T` or API error `E` by HTTP status.
///
/// Returns `Ok(Ok(T))` for 2xx, `Ok(Err(E))` when the body deserializes as `E` on non-success,
/// or `Err(Error::...)` for transport/deserialize failures.
#[cfg(feature = "json")]
pub fn into_api_result<T, E>(response: Response) -> Result<std::result::Result<T, E>>
where
    T: serde::de::DeserializeOwned,
    E: serde::de::DeserializeOwned,
{
    if response.is_success() {
        return Ok(Ok(response.into_json()?));
    }
    let status = response.status();
    let body = response.bytes().clone();
    match serde_json::from_slice::<E>(&body) {
        Ok(err_body) => Ok(std::result::Result::Err(err_body)),
        Err(_) => Err(Error::http(
            status,
            "failed to deserialize error response body",
            Some(body),
        )),
    }
}

/// Extension trait for [`Response`].
#[cfg(feature = "json")]
pub trait ApiResponseExt {
    /// See [`into_api_result`].
    fn into_api_result<T, E>(self) -> Result<std::result::Result<T, E>>
    where
        T: serde::de::DeserializeOwned,
        E: serde::de::DeserializeOwned;
}

#[cfg(feature = "json")]
impl ApiResponseExt for Response {
    fn into_api_result<T, E>(self) -> Result<std::result::Result<T, E>>
    where
        T: serde::de::DeserializeOwned,
        E: serde::de::DeserializeOwned,
    {
        into_api_result(self)
    }
}
