//! Server-Sent Events (`text/event-stream`) helpers for [`StreamingResponse`](crate::StreamingResponse).
//!
//! Use [`SseDecoder`] for incremental parsing, [`parse_sse_events`] for complete buffers, or
//! [`StreamingResponse::read_sse_events`](crate::StreamingResponse::read_sse_events) to buffer first.

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::Stream;
use pin_project_lite::pin_project;

use crate::Result;

/// One SSE event (may aggregate multiple `data:` lines).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// Optional `event:` field.
    pub event: Option<String>,
    /// Concatenated `data:` lines joined with `\n`.
    pub data: String,
    /// Optional `id:` field.
    pub id: Option<String>,
}

/// Incrementally parses SSE events from UTF-8 chunks (blocks delimited by `\n\n`).
#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: String,
}

impl SseDecoder {
    /// Creates an empty decoder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a chunk and returns any complete events parsed from the buffer.
    pub fn push_chunk(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>> {
        let text = std::str::from_utf8(chunk)
            .map_err(|e| crate::Error::Config(format!("SSE chunk is not valid UTF-8: {e}")))?;
        self.buffer.push_str(text);
        Ok(self.drain_complete_events())
    }

    /// Parses any trailing bytes as a final event block (if non-empty).
    pub fn finish(mut self) -> Vec<SseEvent> {
        let tail = std::mem::take(&mut self.buffer);
        if tail.trim().is_empty() {
            return Vec::new();
        }
        parse_sse_block(&tail).into_iter().collect()
    }

    fn drain_complete_events(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();
        while let Some(pos) = self.buffer.find("\n\n") {
            let block: String = self.buffer.drain(..pos + 2).collect();
            let block = block.trim();
            if block.is_empty() {
                continue;
            }
            if let Some(event) = parse_sse_block(block) {
                events.push(event);
            }
        }
        events
    }
}

pin_project! {
    /// Stream of [`SseEvent`] parsed incrementally from a response body.
    pub struct SseEventStream {
        #[pin]
        body: crate::BodyStream,
        decoder: SseDecoder,
        pending: std::collections::VecDeque<SseEvent>,
        max_bytes: Option<u64>,
        accumulated: u64,
        finished: bool,
    }
}

impl SseEventStream {
    pub(crate) fn new(body: crate::BodyStream, max_bytes: Option<u64>) -> Self {
        Self {
            body,
            decoder: SseDecoder::new(),
            pending: std::collections::VecDeque::new(),
            max_bytes,
            accumulated: 0,
            finished: false,
        }
    }
}

impl Stream for SseEventStream {
    type Item = Result<SseEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if let Some(event) = this.pending.pop_front() {
            return Poll::Ready(Some(Ok(event)));
        }

        if *this.finished {
            return Poll::Ready(None);
        }

        loop {
            match this.body.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    if let Some(limit) = *this.max_bytes {
                        *this.accumulated += chunk.len() as u64;
                        if *this.accumulated > limit {
                            return Poll::Ready(Some(Err(crate::Error::BodyTooLarge { limit })));
                        }
                    }
                    match this.decoder.push_chunk(&chunk) {
                        Ok(events) => {
                            for event in events {
                                this.pending.push_back(event);
                            }
                            if let Some(event) = this.pending.pop_front() {
                                return Poll::Ready(Some(Ok(event)));
                            }
                        }
                        Err(e) => return Poll::Ready(Some(Err(e))),
                    }
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => {
                    *this.finished = true;
                    let decoder = std::mem::take(this.decoder);
                    for event in decoder.finish() {
                        this.pending.push_back(event);
                    }
                    if let Some(event) = this.pending.pop_front() {
                        return Poll::Ready(Some(Ok(event)));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Parses SSE events from a buffer (blocks separated by blank lines).
pub fn parse_sse_events(buffer: &str) -> Vec<SseEvent> {
    let mut events = Vec::new();
    for block in buffer.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        if let Some(event) = parse_sse_block(block) {
            events.push(event);
        }
    }
    events
}

fn parse_sse_block(block: &str) -> Option<SseEvent> {
    let mut event_name = None;
    let mut id = None;
    let mut data_lines = Vec::new();

    for line in block.lines() {
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            event_name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        } else if let Some(rest) = line.strip_prefix("id:") {
            id = Some(rest.trim().to_string());
        }
    }

    if data_lines.is_empty() && event_name.is_none() && id.is_none() {
        return None;
    }

    Some(SseEvent {
        event: event_name,
        data: data_lines.join("\n"),
        id,
    })
}

/// Reads a streaming body to completion and parses SSE events.
pub async fn read_sse_from_bytes(
    body: crate::BodyStream,
    max_bytes: Option<u64>,
) -> Result<Vec<SseEvent>> {
    let bytes = crate::streaming::accumulate_stream(body, max_bytes).await?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(parse_sse_events(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_event() {
        let events = parse_sse_events("data: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn parses_event_name_and_multiline_data() {
        let raw = "event: ping\ndata: line1\ndata: line2\n\n";
        let events = parse_sse_events(raw);
        assert_eq!(events[0].event.as_deref(), Some("ping"));
        assert_eq!(events[0].data, "line1\nline2");
    }

    #[test]
    fn decoder_splits_across_chunks() {
        let mut decoder = SseDecoder::new();
        let first = decoder.push_chunk(b"data: hel").unwrap();
        assert!(first.is_empty());
        let second = decoder.push_chunk(b"lo\n\ndata: world\n\n").unwrap();
        assert_eq!(second.len(), 2);
        assert_eq!(second[0].data, "hello");
        assert_eq!(second[1].data, "world");
    }
}
