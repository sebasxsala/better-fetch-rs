use reqwest::Client;

use super::{HttpRequest, HttpResponse};
use crate::error::map_transport_error;
use crate::Result;

/// Shared reqwest execution path used by [`super::ReqwestBackend`] and [`crate::tower::ReqwestHttpService`].
pub(crate) async fn send_reqwest(client: &Client, request: HttpRequest) -> Result<HttpResponse> {
    let mut builder = client
        .request(request.method, request.url)
        .headers(request.headers);

    if let Some(timeout) = request.timeout {
        builder = builder.timeout(timeout);
    }

    if let Some(body) = request.body {
        builder = builder.body(body);
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
