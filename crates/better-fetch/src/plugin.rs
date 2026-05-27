//! Plugin hooks run after URL construction and auth, before request lifecycle hooks.
//!
//! Use [`Plugin::init`] to rewrite the target URL (e.g. add a query param) or inspect
//! [`PreparedRequest::method`] / [`PreparedRequest::headers`] before the request is sent.

use async_trait::async_trait;
use http::{HeaderMap, Method};
use url::Url;

use crate::hooks::Hooks;
use crate::Result;

/// Prepared request state passed to plugin [`Plugin::init`].
#[derive(Debug, Clone)]
pub struct PreparedRequest {
    /// URL after path/query resolution (may be mutated by plugins).
    pub url: Url,
    /// Original path template from the builder.
    pub path: String,
    /// HTTP method.
    pub method: Method,
    /// Headers before transport.
    pub headers: HeaderMap,
}

/// Plugin extension point for better-fetch.
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Unique plugin identifier.
    fn id(&self) -> &'static str;

    /// Called before lifecycle hooks; may mutate `prepared`.
    async fn init(&self, _prepared: &mut PreparedRequest) -> Result<()> {
        Ok(())
    }

    /// Hooks merged into the client hook chain at build time.
    fn hooks(&self) -> Hooks {
        Hooks::default()
    }
}

/// Ordered plugin list.
#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a plugin and returns `self` for chaining.
    pub fn register<P: Plugin + 'static>(mut self, plugin: P) -> Self {
        self.plugins.push(Box::new(plugin));
        self
    }

    /// Appends a plugin to the registry.
    pub fn push(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    /// Returns registered plugins.
    pub fn plugins(&self) -> &[Box<dyn Plugin>] {
        &self.plugins
    }

    pub(crate) async fn run_init_all(&self, prepared: &mut PreparedRequest) -> Result<()> {
        for plugin in &self.plugins {
            plugin.init(prepared).await?;
        }
        Ok(())
    }

    pub(crate) fn merged_hooks(&self) -> Hooks {
        let mut hooks = Hooks::default();
        for plugin in &self.plugins {
            hooks = hooks.merge(plugin.hooks());
        }
        hooks
    }
}
