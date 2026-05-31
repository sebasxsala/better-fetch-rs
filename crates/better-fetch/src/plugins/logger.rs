use async_trait::async_trait;
use tracing::{error, info, info_span, warn};

use crate::hooks::{ErrorContext, Hooks};
use crate::plugin::Plugin;

/// Tracing-based logger plugin (request, response, retry, error).
#[derive(Debug, Clone)]
pub struct LoggerPlugin {
    /// When `false`, hooks are registered but do not log.
    pub enabled: bool,
    /// When `true`, includes method and URL on each line.
    pub verbose: bool,
}

impl LoggerPlugin {
    /// Creates a plugin with logging enabled.
    pub fn new() -> Self {
        Self {
            enabled: true,
            verbose: false,
        }
    }

    /// Enables or disables log output.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Enables verbose log fields.
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
    /// Plugin id: `"logger"`.
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
                        let span = info_span!(
                            "http.request",
                            method = %ctx.method,
                            url = %ctx.url,
                            retry_attempt = ctx.retry_attempt,
                        );
                        let _guard = span.enter();
                        if verbose {
                            info!(header_count = ctx.headers.len(), "better-fetch request");
                        } else {
                            info!("better-fetch request");
                        }
                    }
                    Ok(ctx)
                }
            })
            .on_response_stream({
                let enabled = self.enabled;
                let verbose = self.verbose;
                move |ctx| {
                    let enabled = enabled;
                    let verbose = verbose;
                    async move {
                        if enabled {
                            let span = info_span!(
                                "http.response",
                                status = %ctx.status,
                                url = %ctx.request.url,
                                streaming = true,
                            );
                            let _guard = span.enter();
                            if verbose {
                                info!(
                                    header_count = ctx.headers.len(),
                                    "better-fetch stream response"
                                );
                            } else {
                                info!("better-fetch stream response");
                            }
                        }
                        Ok(crate::hooks::StreamingResponseMeta {
                            status: ctx.status,
                            headers: ctx.headers,
                        })
                    }
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
                            let span = info_span!(
                                "http.response",
                                status = %status,
                                url = %ctx.request.url,
                                streaming = false,
                            );
                            let _guard = span.enter();
                            if verbose {
                                info!(
                                    header_count = ctx.response.headers().len(),
                                    "better-fetch response"
                                );
                            } else {
                                info!("better-fetch response");
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
                            let status = ctx.response.as_ref().map(|r| r.status().as_u16());
                            let body_preview = ctx.response_body_preview(256);
                            error!(
                                error = %ctx.error,
                                url = %ctx.request.url,
                                ?status,
                                body_preview = body_preview.as_deref(),
                                retry_attempt = ctx.request.retry_attempt,
                                "better-fetch error"
                            );
                        }
                    }
                }
            })
    }
}
