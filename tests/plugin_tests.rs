use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use better_fetch::{Client, Hooks, LoggerPlugin, Plugin, PreparedRequest, Result};
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct RewritePlugin {
    target: String,
}

#[async_trait]
impl Plugin for RewritePlugin {
    fn id(&self) -> &'static str {
        "rewrite"
    }

    async fn init(&self, prepared: &mut PreparedRequest) -> Result<()> {
        prepared.url = Url::parse(&self.target)?;
        Ok(())
    }
}

struct OrderPlugin {
    id: &'static str,
    log: Arc<AtomicUsize>,
}

#[async_trait]
impl Plugin for OrderPlugin {
    fn id(&self) -> &'static str {
        self.id
    }

    fn hooks(&self) -> Hooks {
        let log = self.log.clone();
        let id = self.id;
        Hooks::new().on_request(move |ctx| {
            let log = log.clone();
            async move {
                log.fetch_add(1, Ordering::SeqCst);
                let _ = id;
                Ok(ctx)
            }
        })
    }
}

#[tokio::test]
async fn plugin_init_rewrites_url() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rewritten"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url("http://unused.local")?
        .plugin(RewritePlugin {
            target: format!("{}/rewritten", server.uri()),
        })
        .build()?;

    assert!(client.get("/ignored").send().await?.is_success());
    Ok(())
}

#[tokio::test]
async fn hooks_run_in_order() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hooks"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let order = Arc::new(AtomicUsize::new(0));
    let order_a = order.clone();
    let order_b = order.clone();

    let hooks = Hooks::new()
        .on_request(move |ctx| {
            let order_a = order_a.clone();
            async move {
                order_a.store(1, Ordering::SeqCst);
                Ok(ctx)
            }
        })
        .on_response(move |ctx| {
            let order_b = order_b.clone();
            async move {
                assert_eq!(order_b.load(Ordering::SeqCst), 1);
                order_b.store(2, Ordering::SeqCst);
                Ok(ctx.response)
            }
        });

    let client = Client::builder()
        .base_url(server.uri())?
        .hooks(hooks)
        .build()?;

    let _ = client.get("/hooks").send().await?;
    assert_eq!(order.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn logger_plugin_does_not_break_pipeline() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/log"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let client = Client::builder()
        .base_url(server.uri())?
        .plugin(LoggerPlugin::new())
        .build()?;

    assert!(client.get("/log").send().await?.is_success());
    Ok(())
}

#[tokio::test]
async fn multiple_plugins_hooks_merge_in_order() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/multi"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let log = Arc::new(AtomicUsize::new(0));
    let client = Client::builder()
        .base_url(server.uri())?
        .plugin(OrderPlugin {
            id: "p1",
            log: log.clone(),
        })
        .plugin(OrderPlugin {
            id: "p2",
            log: log.clone(),
        })
        .build()?;

    client.get("/multi").send().await?;
    assert_eq!(log.load(Ordering::SeqCst), 2);
    Ok(())
}
