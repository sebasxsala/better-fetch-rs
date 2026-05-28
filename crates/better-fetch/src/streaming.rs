//! Streaming HTTP responses (`send_stream`).
//!
//! Use [`RequestBuilder::send_stream`](crate::RequestBuilder::send_stream) for large or chunked
//! bodies. The buffered [`Response`](crate::Response) from [`RequestBuilder::send`](crate::RequestBuilder::send)
//! remains the default for JSON APIs.

use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Bytes, BytesMut};
use futures_util::{Future, Stream};
use http::{HeaderMap, StatusCode};

use crate::cancel::CancellationToken;
use crate::error::Error;
use crate::response::Response;
use crate::Result;
use tokio_util::sync::WaitForCancellationFutureOwned;

/// Byte stream yielding `Result<Bytes>` chunks from the transport.
pub type BodyStream = Pin<Box<dyn Stream<Item = Result<Bytes>> + Send + Sync>>;

/// HTTP response with a streaming body.
///
/// Status and headers are available immediately. Consume the body via [`Self::bytes_stream`]
/// or buffer it with [`Self::collect`].
///
/// # Examples
///
/// ```no_run
/// # use better_fetch::{Client, Result};
/// # use futures_util::StreamExt;
/// # #[tokio::main]
/// # async fn main() -> Result<()> {
/// let client = Client::new("https://httpbin.org")?;
/// let mut stream = client.get("/stream/5").send_stream().await?;
/// while let Some(chunk) = stream.bytes_stream().next().await {
///     let chunk = chunk?;
///     println!("got {} bytes", chunk.len());
/// }
/// # Ok(())
/// # }
/// ```
pub struct StreamingResponse {
    status: StatusCode,
    headers: HeaderMap,
    url: Option<url::Url>,
    body: BodyStream,
    max_response_bytes: Option<u64>,
    #[cfg(feature = "json")]
    json_parser: Option<crate::json_parser::JsonParserFn>,
    #[cfg(feature = "schema-validate")]
    response_schema: Option<crate::schema_validate::StreamResponseSchemaCtx>,
}

impl StreamingResponse {
    pub(crate) fn new(
        status: StatusCode,
        headers: HeaderMap,
        body: BodyStream,
        url: Option<url::Url>,
        max_response_bytes: Option<u64>,
        #[cfg(feature = "json")] json_parser: Option<crate::json_parser::JsonParserFn>,
        #[cfg(feature = "schema-validate")] response_schema: Option<
            crate::schema_validate::StreamResponseSchemaCtx,
        >,
    ) -> Self {
        Self {
            status,
            headers,
            url,
            body,
            max_response_bytes,
            #[cfg(feature = "json")]
            json_parser,
            #[cfg(feature = "schema-validate")]
            response_schema,
        }
    }

    /// HTTP status code.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Response headers.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Final request URL when available.
    pub fn url(&self) -> Option<&url::Url> {
        self.url.as_ref()
    }

    /// Returns `true` for 2xx status codes.
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// Returns an error if the status is not success (does not read the body).
    #[must_use = "call `?` or handle the error explicitly"]
    pub fn error_for_status(&self) -> Result<()> {
        if self.status.is_success() {
            return Ok(());
        }
        Err(Error::http_error_for_status(self.status, None))
    }

    /// Mutable reference to the response body stream.
    pub fn bytes_stream(&mut self) -> &mut BodyStream {
        &mut self.body
    }

    /// Buffers the full body into a [`Response`].
    ///
    /// Respects [`ClientBuilder::max_response_bytes`](crate::ClientBuilder::max_response_bytes) when
    /// configured on the request or client (the limit is enforced on the underlying stream).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use better_fetch::{Client, Result};
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// let client = Client::new("https://api.example.com")?;
    /// let buffered = client.get("/data").send_stream().await?.collect().await?;
    /// let text = buffered.into_text()?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn collect(self) -> Result<Response> {
        self.error_for_status()?;
        let bytes = accumulate_stream(self.body, self.max_response_bytes).await?;
        let response = Response::new(
            self.status,
            self.headers,
            bytes,
            self.url,
            #[cfg(feature = "json")]
            self.json_parser,
        );
        #[cfg(feature = "schema-validate")]
        if let Some(ctx) = self.response_schema {
            crate::schema_validate::validate_response_if_registered(
                &ctx.registry,
                &ctx.route_path,
                &ctx.method,
                &response,
            )?;
        }
        Ok(response)
    }

    /// Splits into status, headers, and the body stream.
    pub fn into_parts(self) -> (StatusCode, HeaderMap, BodyStream) {
        (self.status, self.headers, self.body)
    }

    /// Writes the response body to `path`, returning the number of bytes written.
    ///
    /// Enforces `max_bytes` when set (same semantics as [`accumulate_stream`](crate::streaming::accumulate_stream)).
    /// Checks for success status before writing.
    pub async fn stream_to_file(
        mut self,
        path: impl AsRef<Path>,
        max_bytes: Option<u64>,
    ) -> Result<u64> {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        self.error_for_status()?;
        let limit = max_bytes.or(self.max_response_bytes);
        let mut file = tokio::fs::File::create(path.as_ref())
            .await
            .map_err(|e| Error::Io(format!("create file: {e}")))?;
        let mut written: u64 = 0;

        while let Some(chunk) = self.body.next().await {
            let chunk = chunk?;
            let chunk_len = u64::try_from(chunk.len())
                .map_err(|_| Error::Config("chunk size overflow".into()))?;
            let new_written = written
                .checked_add(chunk_len)
                .ok_or_else(|| Error::Config("response body length overflow".into()))?;
            if let Some(limit) = limit {
                if new_written > limit {
                    return Err(Error::BodyTooLarge { limit });
                }
            }
            file.write_all(&chunk)
                .await
                .map_err(|e| Error::Io(format!("write file: {e}")))?;
            written = new_written;
        }

        file.flush()
            .await
            .map_err(|e| Error::Io(format!("flush file: {e}")))?;
        Ok(written)
    }

    /// Buffers the stream (up to `max_bytes`) and parses `text/event-stream` events.
    pub async fn read_sse_events(
        self,
        max_bytes: Option<u64>,
    ) -> Result<Vec<crate::sse::SseEvent>> {
        crate::sse::read_sse_from_bytes(self.body, max_bytes.or(self.max_response_bytes)).await
    }

    /// Incrementally parses SSE events from the response body as a [`Stream`](futures_util::Stream).
    ///
    /// Respects `max_bytes` when set on the request (same as [`Self::collect`]).
    pub fn sse_events(self) -> crate::sse::SseEventStream {
        crate::sse::SseEventStream::new(self.body, self.max_response_bytes)
    }
}

impl std::fmt::Debug for StreamingResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamingResponse")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("url", &self.url)
            .field("body", &"<stream>")
            .finish()
    }
}

pub(crate) fn wrap_max_bytes(stream: BodyStream, limit: u64) -> BodyStream {
    Box::pin(MaxBytesStream {
        inner: stream,
        limit,
        read: 0,
        limit_hit: false,
    })
}

pub(crate) fn wrap_cancellation(stream: BodyStream, token: CancellationToken) -> BodyStream {
    Box::pin(CancelBodyStream {
        inner: stream,
        cancelled: token.cancelled_owned(),
    })
}

/// Default maximum bytes read from a streaming body when evaluating a custom retry predicate.
pub(crate) const RETRY_BODY_PEEK_DEFAULT: u64 = 64 * 1024;

/// Reads up to `limit` bytes from `body` for retry predicate evaluation.
pub(crate) async fn drain_body_for_retry(body: BodyStream, limit: u64) -> Result<Bytes> {
    accumulate_stream(body, Some(limit)).await
}

/// Accumulates a body stream into a single buffer, optionally enforcing `limit`.
pub(crate) async fn accumulate_stream(mut body: BodyStream, limit: Option<u64>) -> Result<Bytes> {
    use futures_util::StreamExt;

    let mut buf = BytesMut::new();
    while let Some(chunk) = body.next().await {
        let chunk = chunk?;
        let new_len = buf
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| Error::Config("response body length overflow".into()))?;
        if let Some(limit) = limit {
            if new_len as u64 > limit {
                return Err(Error::BodyTooLarge { limit });
            }
        }
        buf.reserve(chunk.len());
        buf.extend_from_slice(&chunk);
        debug_assert_eq!(buf.len(), new_len);
    }
    Ok(buf.freeze())
}

/// Creates a single-chunk body stream from bytes.
pub fn body_stream_from_bytes(bytes: Bytes) -> BodyStream {
    Box::pin(futures_util::stream::once(async move { Ok(bytes) }))
}

struct MaxBytesStream {
    inner: BodyStream,
    limit: u64,
    read: u64,
    /// Set after the first [`Error::BodyTooLarge`]; further polls end the stream.
    limit_hit: bool,
}

impl Stream for MaxBytesStream {
    type Item = Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.limit_hit {
            return Poll::Ready(None);
        }

        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                let chunk_len = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
                let new_read = self.read.saturating_add(chunk_len);
                if new_read > self.limit {
                    self.limit_hit = true;
                    // Drop `chunk` without yielding it; caller must stop after the error.
                    return Poll::Ready(Some(Err(Error::BodyTooLarge { limit: self.limit })));
                }
                self.read = new_read;
                Poll::Ready(Some(Ok(chunk)))
            }
            other => other,
        }
    }
}

pin_project_lite::pin_project! {
    struct CancelBodyStream {
        #[pin]
        inner: BodyStream,
        #[pin]
        cancelled: WaitForCancellationFutureOwned,
    }
}

impl Stream for CancelBodyStream {
    type Item = Result<Bytes>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        if this.cancelled.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Some(Err(Error::Cancelled)));
        }
        match this.inner.poll_next(cx) {
            Poll::Ready(item) => Poll::Ready(item),
            Poll::Pending => {
                let _ = this.cancelled.as_mut().poll(cx);
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{stream, StreamExt};

    fn stream_from_chunks(chunks: Vec<Result<Bytes>>) -> BodyStream {
        Box::pin(stream::iter(chunks))
    }

    #[tokio::test]
    async fn max_bytes_ends_stream_after_limit_error() {
        let inner = stream_from_chunks(vec![
            Ok(Bytes::from_static(b"1234")),
            Ok(Bytes::from_static(b"5678")),
        ]);
        let mut limited = wrap_max_bytes(inner, 5);

        let first = limited.next().await.unwrap().unwrap();
        assert_eq!(first.as_ref(), b"1234");

        let err = limited.next().await.unwrap().unwrap_err();
        assert!(err.is_body_too_large());
        assert_eq!(err.body_too_large_limit(), Some(5));

        // Must not replay the oversized chunk or spin forever.
        assert!(limited.next().await.is_none());
        assert!(limited.next().await.is_none());
    }

    #[tokio::test]
    async fn max_bytes_allows_exact_limit() {
        let inner = stream_from_chunks(vec![
            Ok(Bytes::from_static(b"abc")),
            Ok(Bytes::from_static(b"de")),
        ]);
        let mut limited = wrap_max_bytes(inner, 5);
        assert_eq!(limited.next().await.unwrap().unwrap().as_ref(), b"abc");
        assert_eq!(limited.next().await.unwrap().unwrap().as_ref(), b"de");
        assert!(limited.next().await.is_none());
    }

    #[tokio::test]
    async fn cancel_wakes_pending_inner_read() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let released = Arc::new(AtomicBool::new(false));
        let released_cb = released.clone();
        let inner: BodyStream = Box::pin(futures_util::stream::poll_fn(move |cx| {
            if released_cb.load(Ordering::SeqCst) {
                return Poll::Ready(None);
            }
            cx.waker().wake_by_ref();
            Poll::Pending
        }));

        let token = CancellationToken::new();
        let cancel = token.clone();
        let mut wrapped = wrap_cancellation(inner, token);

        let read = tokio::spawn(async move {
            use futures_util::StreamExt;
            wrapped.next().await
        });

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        cancel.cancel();
        released.store(true, Ordering::SeqCst);

        let item = read.await.unwrap();
        assert!(matches!(item, Some(Err(e)) if e.is_cancelled()));
    }

    #[tokio::test]
    async fn cancel_checked_between_chunks() {
        let inner = stream_from_chunks(vec![
            Ok(Bytes::from_static(b"a")),
            Ok(Bytes::from_static(b"b")),
        ]);
        let token = CancellationToken::new();
        let cancel = token.clone();
        let mut wrapped = wrap_cancellation(inner, token);

        assert_eq!(wrapped.next().await.unwrap().unwrap().as_ref(), b"a");
        cancel.cancel();
        let err = wrapped.next().await.unwrap().unwrap_err();
        assert!(err.is_cancelled());
    }
}
