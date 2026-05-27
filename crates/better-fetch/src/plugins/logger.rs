use async_trait::async_trait;
use tracing::{error, info, warn};

use crate::hooks::{ErrorContext, Hooks};
use crate::plugin::Plugin;

/// Tracing-based logger plugin (request, response, retry, error).
#[derive(Debug, Clone)]
pub struct LoggerPlugin {
    pub enabled: bool,
    pub verbose: bool,
}

impl LoggerPlugin {
    pub fn new() -> Self {
        Self {
            enabled: true,
            verbose: false,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }
}

impl Default for LoggerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for LoggerPlugin {
    fn id(&self) -> &'static str {
        "logger"
    }

    fn hooks(&self) -> Hooks {
        let enabled = self.enabled;
        let verbose = self.verbose;

        Hooks::new()
            .on_request(move |ctx| {
                let enabled = enabled;
                let verbose = verbose;
                async move {
                    if enabled {
                        if verbose {
                            info!(
                                method = %ctx.method,
                                url = %ctx.url,
                                "better-fetch request"
                            );
                        } else {
                            info!(url = %ctx.url, "better-fetch request");
                        }
                    }
                    Ok(ctx)
                }
            })
            .on_response({
                let enabled = self.enabled;
                let verbose = self.verbose;
                move |ctx| {
                    let enabled = enabled;
                    let verbose = verbose;
                    async move {
                        if enabled {
                            let status = ctx.response.status();
                            if verbose {
                                info!(
                                    status = %status,
                                    url = %ctx.request.url,
                                    "better-fetch response"
                                );
                            } else {
                                info!(status = %status, "better-fetch response");
                            }
                        }
                        Ok(ctx.response)
                    }
                }
            })
            .on_retry({
                let enabled = self.enabled;
                move |ctx| {
                    let enabled = enabled;
                    async move {
                        if enabled {
                            warn!(
                                retry_attempt = ctx.request.retry_attempt,
                                next_attempt = ctx.request.retry_attempt + 1,
                                status = %ctx.response.status(),
                                url = %ctx.request.url,
                                "better-fetch retry"
                            );
                        }
                    }
                }
            })
            .on_error({
                let enabled = self.enabled;
                move |ctx: ErrorContext| {
                    let enabled = enabled;
                    async move {
                        if enabled {
                            error!(
                                error = %ctx.error,
                                url = %ctx.request.url,
                                "better-fetch error"
                            );
                        }
                    }
                }
            })
    }
}
