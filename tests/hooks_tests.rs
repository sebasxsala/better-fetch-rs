use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use better_fetch::{Client, Error, Hooks, Result};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn on_request_can_replace_body() -> Result<()> {
    use wiremock::matchers::{body_string, method, path};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/body"))
        .and(body_string("from-hook"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let hooks = Hooks::new().on_request(|mut ctx| async move {
        ctx.body = Some(bytes::Bytes::from_static(b"from-hook"));
        Ok(ctx)
    });

    let client = Client::builder()
        .base_url(server.uri())?
        .hooks(hooks)
        .build()?;

    client
        .post("/body")
        .body(bytes::Bytes::from_static(b"ignored"))
        .send()
        .await?;
    Ok(())
}

#[tokio::test]
async fn on_request_can_modify_headers() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/h"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let hooks = Hooks::new().on_request(|mut ctx| async move {
        ctx.headers
            .insert("x-test", http::HeaderValue::from_static("1"));
        Ok(ctx)
    });

    let client = Client::builder()
        .base_url(server.uri())?
        .hooks(hooks)
        .build()?;

    assert!(client.get("/h").send().await?.is_success());
    Ok(())
}

#[tokio::test]
async fn on_success_only_for_2xx() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ok"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let success = Arc::new(AtomicBool::new(false));
    let success_c = success.clone();
    let hooks = Hooks::new().on_success(move |_| {
        let success_c = success_c.clone();
        async move {
            success_c.store(true, Ordering::SeqCst);
        }
    });

    let client = Client::builder()
        .base_url(server.uri())?
        .hooks(hooks)
        .build()?;

    client.get("/ok").send().await?;
    assert!(success.load(Ordering::SeqCst));
    Ok(())
}

#[tokio::test]
async fn on_error_for_http_500() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fail"))
        .respond_with(ResponseTemplate::new(500).set_body_string("err"))
        .mount(&server)
        .await;

    let fired = Arc::new(AtomicBool::new(false));
    let fired_c = fired.clone();
    let hooks = Hooks::new().on_error(move |ctx| {
        let fired_c = fired_c.clone();
        async move {
            assert!(ctx.response.is_some());
            assert_eq!(
                ctx.error.status(),
                Some(http::StatusCode::INTERNAL_SERVER_ERROR)
            );
            fired_c.store(true, Ordering::SeqCst);
        }
    });

    let client = Client::builder()
        .base_url(server.uri())?
        .hooks(hooks)
        .build()?;

    let _ = client.get("/fail").send().await?;
    assert!(fired.load(Ordering::SeqCst));
    Ok(())
}

#[tokio::test]
async fn on_error_fires_for_send_stream_5xx_without_throw() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fail-stream"))
        .respond_with(ResponseTemplate::new(500).set_body_string("err"))
        .mount(&server)
        .await;

    let fired = Arc::new(AtomicBool::new(false));
    let fired_c = fired.clone();
    let hooks = Hooks::new().on_error(move |ctx| {
        let fired_c = fired_c.clone();
        async move {
            assert!(ctx.response.is_some());
            fired_c.store(true, Ordering::SeqCst);
        }
    });

    let client = Client::builder()
        .base_url(server.uri())?
        .hooks(hooks)
        .build()?;

    let stream = client.get("/fail-stream").send_stream().await?;
    assert_eq!(stream.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    assert!(fired.load(Ordering::SeqCst));
    Ok(())
}

#[tokio::test]
async fn on_error_not_called_for_2xx() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fine"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let fired = Arc::new(AtomicBool::new(false));
    let fired_c = fired.clone();
    let hooks = Hooks::new().on_error(move |_| {
        let fired_c = fired_c.clone();
        async move {
            fired_c.store(true, Ordering::SeqCst);
        }
    });

    let client = Client::builder()
        .base_url(server.uri())?
        .hooks(hooks)
        .build()?;

    client.get("/fine").send().await?;
    assert!(!fired.load(Ordering::SeqCst));
    Ok(())
}

#[tokio::test]
async fn on_retry_fires_during_retry() -> Result<()> {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    let count_c = count.clone();

    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(move |_: &wiremock::Request| {
            let n = count_c.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(503)
            } else {
                ResponseTemplate::new(200).set_body_string("ok")
            }
        })
        .mount(&server)
        .await;

    let retries = Arc::new(AtomicUsize::new(0));
    let retries_c = retries.clone();
    let hooks = Hooks::new().on_retry(move |ctx| {
        let retries_c = retries_c.clone();
        async move {
            retries_c.store(ctx.request.retry_attempt as usize, Ordering::SeqCst);
        }
    });

    let client = Client::builder()
        .base_url(server.uri())?
        .hooks(hooks)
        .retry(better_fetch::RetryPolicy::count(2))
        .build()?;

    assert!(client.get("/flaky").send().await?.is_success());
    assert_eq!(retries.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn retry_attempt_zero_without_retry() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/once"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let seen = Arc::new(AtomicUsize::new(99));
    let seen_c = seen.clone();
    let hooks = Hooks::new().on_response(move |ctx| {
        let seen_c = seen_c.clone();
        async move {
            seen_c.store(ctx.request.retry_attempt as usize, Ordering::SeqCst);
            Ok(ctx.response)
        }
    });

    let client = Client::builder()
        .base_url(server.uri())?
        .hooks(hooks)
        .build()?;

    client.get("/once").send().await?;
    assert_eq!(seen.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn retry_attempt_visible_on_retry() -> Result<()> {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    let count_c = count.clone();

    Mock::given(method("GET"))
        .and(path("/flaky2"))
        .respond_with(move |_: &wiremock::Request| {
            let n = count_c.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(503)
            } else {
                ResponseTemplate::new(200).set_body_string("ok")
            }
        })
        .mount(&server)
        .await;

    let on_retry_seen = Arc::new(AtomicUsize::new(99));
    let on_retry_seen_c = on_retry_seen.clone();
    let on_response_seen = Arc::new(AtomicUsize::new(99));
    let on_response_seen_c = on_response_seen.clone();

    let hooks = Hooks::new()
        .on_retry(move |ctx| {
            let on_retry_seen_c = on_retry_seen_c.clone();
            async move {
                on_retry_seen_c.store(ctx.request.retry_attempt as usize, Ordering::SeqCst);
            }
        })
        .on_response(move |ctx| {
            let on_response_seen_c = on_response_seen_c.clone();
            async move {
                on_response_seen_c.store(ctx.request.retry_attempt as usize, Ordering::SeqCst);
                Ok(ctx.response)
            }
        });

    let client = Client::builder()
        .base_url(server.uri())?
        .hooks(hooks)
        .retry(better_fetch::RetryPolicy::count(2))
        .build()?;

    assert!(client.get("/flaky2").send().await?.is_success());
    assert_eq!(on_retry_seen.load(Ordering::SeqCst), 0);
    assert_eq!(on_response_seen.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn hook_error_propagates() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/x"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let hooks = Hooks::new().on_request(|_| async { Err(Error::hook("blocked")) });

    let client = Client::builder()
        .base_url(server.uri())?
        .hooks(hooks)
        .build()?;

    let err = client.get("/x").send().await.unwrap_err();
    assert!(matches!(err, Error::Hook(_)));
    Ok(())
}
