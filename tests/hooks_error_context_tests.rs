use better_fetch::{Client, Error, Hooks, Result};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn error_context_response_body_preview() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fail"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal detail"))
        .mount(&server)
        .await;

    let seen = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let seen_hook = seen.clone();
    let hooks = Hooks::new().on_error(move |ctx| {
        let seen_hook = seen_hook.clone();
        async move {
            if let Some(preview) = ctx.response_body_preview(32) {
                *seen_hook.lock().unwrap() = Some(preview);
            }
        }
    });

    let client = Client::builder()
        .base_url(server.uri())?
        .hooks(hooks)
        .build()?;

    let _ = client.get("/fail").throw_on_error(true).send().await;

    let preview = seen.lock().unwrap().clone();
    assert_eq!(preview.as_deref(), Some("internal detail"));
    Ok(())
}

#[test]
fn error_context_preview_without_response() {
    let ctx = better_fetch::ErrorContext {
        request: better_fetch::RequestContext {
            url: url::Url::parse("https://example.com").unwrap(),
            method: http::Method::GET,
            headers: Default::default(),
            body: None,
            retry_attempt: 0,
        },
        response: None,
        error: Error::Timeout,
    };
    assert!(ctx.response_body_preview(64).is_none());
}
