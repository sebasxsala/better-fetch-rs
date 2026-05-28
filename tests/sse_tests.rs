use better_fetch::{parse_sse_events, Client, Result, SseDecoder};
use futures_util::StreamExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn parse_sse_events_multiline() {
    let events = parse_sse_events("event: msg\ndata: a\ndata: b\n\n");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event.as_deref(), Some("msg"));
    assert_eq!(events[0].data, "a\nb");
}

#[tokio::test]
async fn read_sse_events_from_stream() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/events"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: one\n\ndata: two\n\n"),
        )
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let events = client
        .get("/events")
        .send_stream()
        .await?
        .read_sse_events(Some(64 * 1024))
        .await?;
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].data, "one");
    assert_eq!(events[1].data, "two");
    Ok(())
}

#[tokio::test]
async fn sse_events_stream_incremental() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/events-stream"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: first\n\ndata: sec"),
        )
        .mount(&server)
        .await;

    let client = Client::new(server.uri())?;
    let mut stream = client
        .get("/events-stream")
        .send_stream()
        .await?
        .sse_events();
    let first = stream.next().await.unwrap()?;
    assert_eq!(first.data, "first");
    Ok(())
}

#[test]
fn sse_decoder_across_chunk_boundary() {
    let mut decoder = SseDecoder::new();
    assert!(decoder.push_chunk(b"data: a\n").unwrap().is_empty());
    let events = decoder.push_chunk(b"\n\n").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data, "a");
}
