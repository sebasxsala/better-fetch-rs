use ::reqwest::Client;

use super::{HttpBody, HttpRequest, HttpResponse};
use crate::error::map_transport_error;
use crate::Result;

/// Shared reqwest execution path used by [`super::ReqwestBackend`] and [`crate::tower::ReqwestHttpService`].
///
/// Always buffers the full response body into memory via `response.bytes().await`.
pub(crate) async fn send_reqwest(client: &Client, request: HttpRequest) -> Result<HttpResponse> {
    let mut builder = client
        .request(request.method.clone(), request.url.clone())
        .headers(request.headers.clone());

    if let Some(timeout) = request.timeout {
        builder = builder.timeout(timeout);
    }

    #[cfg(feature = "multipart")]
    if let Some(form) = request.multipart {
        builder = builder.multipart(form);
    } else {
        match request.body {
            HttpBody::Empty => {}
            HttpBody::Bytes(body) => {
                builder = builder.body(body);
            }
        }
    }

    #[cfg(not(feature = "multipart"))]
    match request.body {
        HttpBody::Empty => {}
        HttpBody::Bytes(body) => {
            builder = builder.body(body);
        }
    }

    let response = builder.send().await.map_err(map_transport_error)?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.bytes().await.map_err(map_transport_error)?;

    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}
