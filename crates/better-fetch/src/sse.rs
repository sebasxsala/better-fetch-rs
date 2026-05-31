//! Server-Sent Events (`text/event-stream`) helpers for [`StreamingResponse`](crate::StreamingResponse).
//!
//! Enable with Cargo feature `sse` on the `better-fetch` crate.
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

/// Incrementally parses SSE events from UTF-8 chunks (blocks delimited by a blank line).
///
/// Line terminators may be LF, CR, or CRLF (normalized to LF), including a CRLF split across chunks.
#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: String,
    /// Previous chunk ended with a lone `\r` that may pair with a leading `\n` of the next chunk.
    pending_cr: bool,
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
        self.push_normalized(text);
        Ok(self.drain_complete_events())
    }

    /// Appends `text` to the buffer, normalizing CR / CRLF line endings to LF.
    fn push_normalized(&mut self, text: &str) {
        let mut chars = text.chars().peekable();
        if self.pending_cr {
            self.pending_cr = false;
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            self.buffer.push('\n');
        }
        while let Some(c) = chars.next() {
            if c == '\r' {
                match chars.peek() {
                    Some('\n') => {
                        chars.next();
                        self.buffer.push('\n');
                    }
                    Some(_) => self.buffer.push('\n'),
                    None => self.pending_cr = true,
                }
            } else {
                self.buffer.push(c);
            }
        }
    }

    /// Parses any trailing bytes as a final event block (if non-empty).
    pub fn finish(mut self) -> Vec<SseEvent> {
        if self.pending_cr {
            self.buffer.push('\n');
        }
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

/// Parses SSE events from a buffer (blocks separated by blank lines; LF, CR, or CRLF).
pub fn parse_sse_events(buffer: &str) -> Vec<SseEvent> {
    let normalized = buffer.replace("\r\n", "\n").replace('\r', "\n");
    let mut events = Vec::new();
    for block in normalized.split("\n\n") {
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

/// Removes a single optional leading space after the field colon (per the SSE spec).
fn strip_one_space(value: &str) -> &str {
    value.strip_prefix(' ').unwrap_or(value)
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
            event_name = Some(strip_one_space(rest).to_string());
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(strip_one_space(rest).to_string());
        } else if let Some(rest) = line.strip_prefix("id:") {
            id = Some(strip_one_space(rest).to_string());
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

    #[test]
    fn parses_crlf_delimited_events() {
        let events = parse_sse_events("data: a\r\n\r\ndata: b\r\n\r\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "a");
        assert_eq!(events[1].data, "b");
    }

    #[test]
    fn decoder_handles_crlf_split_across_chunks() {
        let mut decoder = SseDecoder::new();
        // Chunk ends mid-CRLF (lone `\r`); the next chunk supplies the `\n`.
        assert!(decoder.push_chunk(b"data: hello\r").unwrap().is_empty());
        let events = decoder.push_chunk(b"\n\r\n").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn keeps_significant_leading_space_after_single_strip() {
        let events = parse_sse_events("data:  two\n\n");
        assert_eq!(events[0].data, " two");
    }
}
