use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use better_fetch::{Client, Plugin, PreparedRequest, Result};
use http::{HeaderValue, Method};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct HeaderInjectPlugin;

#[async_trait]
impl Plugin for HeaderInjectPlugin {
    fn id(&self) -> &'static str {
        "header-inject"
    }

    async fn init(&self, prepared: &mut PreparedRequest) -> Result<()> {
        assert_eq!(prepared.method, Method::GET);
        prepared
            .headers
            .insert("x-plugin", HeaderValue::from_static("1"));
        Ok(())
    }
}

#[tokio::test]
async fn plugin_init_sees_method_and_can_mutate_headers() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/p"))
        .and(header("x-plugin", "1"))
        .and(header("x-client", "yes"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .plugin(HeaderInjectPlugin)
        .build()?;

    assert!(client
        .get("/p")
        .header("x-client", "yes")?
        .send()
        .await?
        .is_success());
    Ok(())
}

#[tokio::test]
async fn plugin_init_runs_after_auth_headers_applied() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth"))
        .and(header("authorization", "Bearer tok"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let seen = Arc::new(AtomicBool::new(false));
    let seen_c = seen.clone();

    struct AssertPlugin {
        seen: Arc<AtomicBool>,
    }

    #[async_trait]
    impl Plugin for AssertPlugin {
        fn id(&self) -> &'static str {
            "assert"
        }

        async fn init(&self, prepared: &mut PreparedRequest) -> Result<()> {
            assert_eq!(prepared.method, Method::POST);
            let auth = prepared
                .headers
                .get("authorization")
                .and_then(|v| v.to_str().ok());
            assert_eq!(auth, Some("Bearer tok"));
            self.seen.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    let client = Client::builder()
        .base_url(server.uri())?
        .plugin(AssertPlugin { seen: seen_c })
        .build()?;

    assert!(client
        .post("/auth")
        .bearer_token("tok")
        .send()
        .await?
        .is_success());
    assert!(seen.load(Ordering::SeqCst));
    Ok(())
}
