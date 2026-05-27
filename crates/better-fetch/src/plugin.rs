use async_trait::async_trait;
use url::Url;

use crate::hooks::Hooks;
use crate::Result;

/// Prepared request state passed to plugin `init`.
#[derive(Debug, Clone)]
pub struct PreparedRequest {
    pub url: Url,
    pub path: String,
}

/// Plugin extension point for better-fetch.
#[async_trait]
pub trait Plugin: Send + Sync {
    fn id(&self) -> &'static str;

    async fn init(&self, _prepared: &mut PreparedRequest) -> Result<()> {
        Ok(())
    }

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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<P: Plugin + 'static>(mut self, plugin: P) -> Self {
        self.plugins.push(Box::new(plugin));
        self
    }

    pub fn push(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }

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
