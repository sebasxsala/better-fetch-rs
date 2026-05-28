use ::reqwest::Client;
use futures_util::StreamExt;

use super::{HttpBody, HttpRequest, HttpResponse, HttpStreamingResponse};
use crate::error::map_transport_error;
use crate::streaming::BodyStream;
use crate::Result;

/// Builds a reqwest request. `HttpRequest::cancellation` is not wired into reqwest; cancellation
/// is cooperative via [`crate::cancel::execute_or_cancel`] around backend calls and
/// [`crate::streaming::wrap_cancellation`] on response body streams.
fn configure_reqwest_builder(client: &Client, request: HttpRequest) -> reqwest::RequestBuilder {
    let HttpRequest {
        method,
        url,
        headers,
        body,
        timeout,
        cancellation: _,
        #[cfg(feature = "multipart")]
        multipart,
    } = request;

    let mut builder = client.request(method, url).headers(headers);

    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }

    #[cfg(feature = "multipart")]
    if let Some(form) = multipart {
        return builder.multipart(form);
    }

    match body {
        HttpBody::Empty => {}
        HttpBody::Bytes(body) => {
            builder = builder.body(body);
        }
        HttpBody::Stream(stream) => {
            let stream = stream.map(|result| result.map_err(std::io::Error::other));
            builder = builder.body(reqwest::Body::wrap_stream(stream));
        }
    }

    builder
}

/// Shared reqwest execution path used by [`super::ReqwestBackend`] and [`crate::tower::ReqwestHttpService`].
///
/// Always buffers the full response body into memory via `response.bytes().await`.
pub(crate) async fn send_reqwest(client: &Client, request: HttpRequest) -> Result<HttpResponse> {
    let response = configure_reqwest_builder(client, request)
        .send()
        .await
        .map_err(map_transport_error)?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.bytes().await.map_err(map_transport_error)?;

    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

/// Shared reqwest streaming path: status and headers are immediate; the body is a byte stream.
pub(crate) async fn send_reqwest_stream(
    client: &Client,
    request: HttpRequest,
) -> Result<HttpStreamingResponse> {
    let response = configure_reqwest_builder(client, request)
        .send()
        .await
        .map_err(map_transport_error)?;
    let status = response.status();
    let headers = response.headers().clone();
    let body: BodyStream = Box::pin(
        response
            .bytes_stream()
            .map(|chunk| chunk.map_err(map_transport_error)),
    );

    Ok(HttpStreamingResponse {
        status,
        headers,
        body,
    })
}
