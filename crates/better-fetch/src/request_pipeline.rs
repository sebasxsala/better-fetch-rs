//! Shared request preparation, retry logic, and execution loops for buffered and streaming paths.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use tokio::sync::OwnedSemaphorePermit;
use url::Url;

use crate::backend::{HttpBackend, HttpBody, HttpRequest};
use crate::cancel::{execute_or_cancel, CancellationToken};
use crate::client::Client;
use crate::error::Error;
use crate::hooks::{
    ErrorContext, Hooks, RequestContext, ResponseContext, StreamingResponseContext,
    StreamingSuccessContext, SuccessContext,
};
use crate::plugin::PreparedRequest;
use crate::request::RequestBuilder;
use crate::response::Response;
use crate::retry::{sleep_or_cancel, RetryPolicy};
use crate::streaming::{
    body_stream_prepend, drain_body_for_retry, drain_remaining, peek_stream_prefix,
    wrap_cancellation, wrap_max_bytes, BodyStream, StreamingResponse,
};
use crate::url_build::build_url;
use crate::Result;

#[cfg(feature = "json")]
use crate::json_parser::JsonParserFn;

#[cfg(feature = "multipart")]
use crate::multipart::Form as MultipartForm;

fn body_for_context(body: &HttpBody) -> Option<Bytes> {
    match body {
        HttpBody::Empty => None,
        HttpBody::Bytes(b) => Some(b.clone()),
        HttpBody::Stream(_) => None,
    }
}

/// Applies hook-mutated request body over the builder default.
pub(crate) fn http_body_from_context(ctx: &RequestContext, fallback: HttpBody) -> HttpBody {
    match &ctx.body {
        Some(b) if !b.is_empty() => HttpBody::Bytes(b.clone()),
        Some(_) => HttpBody::Empty,
        None => fallback,
    }
}

/// Builds a stub [`Response`] for retry hooks and transport failures.
pub(crate) fn retry_stub_response(
    status: StatusCode,
    headers: HeaderMap,
    url: Option<Url>,
    body: Bytes,
    #[cfg(feature = "json")] json_parser: Option<JsonParserFn>,
) -> Response {
    Response::new(
        status,
        headers,
        body,
        url,
        #[cfg(feature = "json")]
        json_parser,
    )
}

/// Prepared state shared by buffered and streaming execution loops.
pub(crate) struct PreparedExecution {
    pub url: Url,
    pub method: Method,
    pub headers: HeaderMap,
    pub request_body: HttpBody,
    pub req_ctx: RequestContext,
    pub timeout: Option<Duration>,
    pub retry_policy: Option<RetryPolicy>,
    pub throw_on_error: bool,
    pub cancel: Option<CancellationToken>,
    pub max_response_bytes: Option<u64>,
    pub retry_body_peek_bytes: u64,
    #[cfg(feature = "json")]
    pub json_parser: Option<JsonParserFn>,
    pub backend: Arc<dyn HttpBackend>,
    pub merged_hooks: Hooks,
    pub _in_flight_permit: Option<OwnedSemaphorePermit>,
    #[cfg(feature = "multipart")]
    pub multipart: Option<MultipartForm>,
    /// Request used a streaming body (not replayable on retry).
    pub non_replayable_body: bool,
    /// Path template used for schema registry lookups.
    pub route_path: String,
    #[cfg(feature = "schema")]
    pub schema_registry: Option<std::sync::Arc<crate::schema::SchemaRegistry>>,
}

pub(crate) async fn prepare_execution(
    client: &Client,
    builder: RequestBuilder<'_>,
) -> Result<PreparedExecution> {
    let config = client.config();

    #[cfg(feature = "json")]
    let json_parser = builder
        .json_parser
        .clone()
        .or_else(|| config.json_parser.clone());

    let base = builder.base_url.as_ref().unwrap_or(&config.base_url);
    let built = build_url(base, &builder.path, &builder.params, &builder.query)?;

    let mut method = builder.method;
    if let Some(override_method) = built.method_override {
        method = override_method;
    }

    #[cfg(feature = "schema")]
    if let Some(registry) = &config.schema_registry {
        registry.ensure_route(&builder.path, &method)?;
    }

    let mut url = built.url;
    let mut headers = builder.headers;
    let auth = builder.auth.clone().or_else(|| config.auth.clone());
    if let Some(auth) = auth {
        auth.apply(&mut headers).await?;
    }

    let mut prepared = PreparedRequest {
        url: url.clone(),
        path: builder.path.clone(),
        method: method.clone(),
        headers: headers.clone(),
    };
    config.plugins.run_init_all(&mut prepared).await?;
    url = prepared.url;
    headers = prepared.headers;
    method = prepared.method;

    let fallback_body = builder.body;
    let mut req_ctx = RequestContext {
        url: url.clone(),
        method: method.clone(),
        headers: headers.clone(),
        body: body_for_context(&fallback_body),
        retry_attempt: 0,
    };

    req_ctx = config.merged_hooks.run_on_request(req_ctx).await?;
    url = req_ctx.url.clone();
    headers = req_ctx.headers.clone();
    method = req_ctx.method.clone();
    let request_body = http_body_from_context(&req_ctx, fallback_body);

    let _in_flight_permit = match &config.max_in_flight {
        Some(sem) => Some(
            sem.clone()
                .acquire_owned()
                .await
                .map_err(|_| Error::Config("max_in_flight semaphore closed".into()))?,
        ),
        None => None,
    };

    #[cfg(feature = "multipart")]
    let multipart = builder.multipart;
    let non_replayable_body = crate::backend::body_is_non_replayable(&request_body) || {
        #[cfg(feature = "multipart")]
        {
            multipart.is_some()
        }
        #[cfg(not(feature = "multipart"))]
        {
            false
        }
    };

    #[cfg(feature = "schema-validate")]
    if let Some(registry) = &config.schema_registry {
        if registry.is_strict() && registry.request_schema(&builder.path, &method).is_some() {
            match &request_body {
                HttpBody::Bytes(bytes) if bytes.is_empty() => {
                    crate::schema_validate::validate_request(
                        registry,
                        &builder.path,
                        &method,
                        &serde_json::Value::Null,
                    )?;
                }
                HttpBody::Bytes(bytes) => {
                    let value: serde_json::Value =
                        serde_json::from_slice(bytes).map_err(|e| Error::SchemaValidation {
                            phase: "request",
                            message: format!("request body is not JSON: {e}"),
                        })?;
                    crate::schema_validate::validate_request(
                        registry,
                        &builder.path,
                        &method,
                        &value,
                    )?;
                }
                HttpBody::Empty => {
                    crate::schema_validate::validate_request(
                        registry,
                        &builder.path,
                        &method,
                        &serde_json::Value::Null,
                    )?;
                }
                HttpBody::Stream(_) => {
                    return Err(Error::SchemaValidation {
                        phase: "request",
                        message: "request schema registered but body is a stream".into(),
                    });
                }
            }
        }
        crate::schema_validate::validate_params(registry, &builder.path, &method, &builder.params)?;
        crate::schema_validate::validate_query(registry, &builder.path, &method, &builder.query)?;
    }

    Ok(PreparedExecution {
        url,
        method,
        headers,
        request_body,
        req_ctx,
        timeout: builder.timeout,
        retry_policy: builder.retry.clone().or_else(|| config.retry.clone()),
        throw_on_error: builder.throw_on_error,
        cancel: builder.cancellation,
        max_response_bytes: builder.max_response_bytes.or(config.max_response_bytes),
        retry_body_peek_bytes: builder
            .retry_body_peek_bytes
            .unwrap_or(config.retry_body_peek_bytes),
        #[cfg(feature = "json")]
        json_parser,
        backend: client.backend_arc().clone(),
        merged_hooks: config.merged_hooks.clone(),
        _in_flight_permit,
        #[cfg(feature = "multipart")]
        multipart,
        non_replayable_body,
        route_path: builder.path.clone(),
        #[cfg(feature = "schema")]
        schema_registry: config.schema_registry.clone(),
    })
}

fn check_attempt_replay(prep: &PreparedExecution, attempt: u32) -> Result<()> {
    if attempt > 0 && prep.non_replayable_body {
        return Err(Error::NonReplayableBody);
    }
    Ok(())
}

fn take_body_for_attempt(prep: &mut PreparedExecution, attempt: u32) -> HttpBody {
    match &prep.request_body {
        HttpBody::Stream(_) if attempt == 0 => {
            match std::mem::replace(&mut prep.request_body, HttpBody::Empty) {
                HttpBody::Stream(s) => HttpBody::Stream(s),
                other => other,
            }
        }
        _ => prep.request_body.clone(),
    }
}

fn build_http_request(prep: &mut PreparedExecution, body: HttpBody) -> HttpRequest {
    HttpRequest {
        method: prep.method.clone(),
        url: prep.url.clone(),
        headers: prep.headers.clone(),
        body,
        timeout: prep.timeout,
        cancellation: prep.cancel.clone(),
        #[cfg(feature = "multipart")]
        multipart: prep.multipart.take(),
    }
}

#[cfg(feature = "schema-validate")]
fn maybe_validate_response(prep: &PreparedExecution, response: &Response) -> Result<()> {
    let Some(registry) = prep.schema_registry.as_ref() else {
        return Ok(());
    };
    crate::schema_validate::validate_response_if_registered(
        registry,
        &prep.route_path,
        &prep.method,
        response,
    )
}

#[cfg(feature = "schema-validate")]
fn stream_response_schema_ctx(
    prep: &PreparedExecution,
) -> Option<crate::schema_validate::StreamResponseSchemaCtx> {
    let registry = prep.schema_registry.as_ref()?;
    if !registry.is_strict() {
        return None;
    }
    registry.response_schema(&prep.route_path, &prep.method)?;
    Some(crate::schema_validate::StreamResponseSchemaCtx {
        registry: std::sync::Arc::clone(registry),
        route_path: prep.route_path.clone(),
        method: prep.method.clone(),
    })
}

fn stream_peek_limit(prep: &PreparedExecution) -> u64 {
    prep.max_response_bytes
        .map(|m| m.min(prep.retry_body_peek_bytes))
        .unwrap_or(prep.retry_body_peek_bytes)
}

async fn finish_stream_http_status(
    prep: &PreparedExecution,
    status: StatusCode,
    stream_headers: HeaderMap,
    body: BodyStream,
    request_url: Url,
    peeked_body: Option<Bytes>,
) -> Result<LoopOutput> {
    let peek_limit = stream_peek_limit(prep);

    let (peeked, mut body) = match peeked_body {
        Some(peeked) => (peeked, body),
        None => {
            let (peeked, rest) = peek_stream_prefix(body, peek_limit).await?;
            let body = body_stream_prepend(peeked.clone(), rest);
            (peeked, body)
        }
    };

    let err_body = if peeked.is_empty() {
        None
    } else {
        Some(peeked.clone())
    };
    let http_err = Error::http_error_for_status(status, err_body);
    let stub = retry_stub_response(
        status,
        stream_headers.clone(),
        Some(request_url.clone()),
        peeked,
        #[cfg(feature = "json")]
        prep.json_parser.clone(),
    );
    prep.merged_hooks
        .run_on_error(ErrorContext {
            request: prep.req_ctx.clone(),
            response: Some(stub),
            error: http_err.clone(),
        })
        .await;

    if prep.throw_on_error {
        let _ = drain_body_for_retry(body, peek_limit).await?;
        return Err(http_err);
    }

    if let Some(limit) = prep.max_response_bytes {
        body = wrap_max_bytes(body, limit);
    }
    if let Some(token) = prep.cancel.clone() {
        body = wrap_cancellation(body, token);
    }

    Ok(LoopOutput::Stream(StreamingResponse::new(
        status,
        stream_headers,
        body,
        Some(request_url),
        prep.max_response_bytes,
        #[cfg(feature = "json")]
        prep.json_parser.clone(),
        #[cfg(feature = "schema-validate")]
        stream_response_schema_ctx(prep),
    )))
}

enum LoopMode {
    Buffered,
    Streaming,
}

enum LoopOutput {
    Buffered(Response),
    Stream(StreamingResponse),
}

fn transport_retryable(err: &Error) -> bool {
    matches!(err, Error::Transport { .. } | Error::Timeout)
}

async fn run_retry_backoff(
    hooks: &Hooks,
    req_ctx: &RequestContext,
    stub: Response,
    headers: &HeaderMap,
    attempt: u32,
    cancel: Option<&CancellationToken>,
    policy: &RetryPolicy,
) -> Result<u32> {
    hooks
        .run_on_retry(ResponseContext {
            request: req_ctx.clone(),
            response: stub,
        })
        .await;
    let delay = policy.delay_after_response(attempt, headers);
    sleep_or_cancel(delay, cancel).await?;
    Ok(attempt + 1)
}

#[allow(clippy::too_many_arguments)]
async fn handle_transport_error(
    hooks: &Hooks,
    req_ctx: &RequestContext,
    request_url: &Url,
    err: Error,
    attempt: u32,
    max_attempts: u32,
    retry_policy: Option<&RetryPolicy>,
    cancel: Option<&CancellationToken>,
    #[cfg(feature = "json")] json_parser: Option<JsonParserFn>,
) -> Result<TransportErrorAction> {
    if err.is_cancelled() {
        hooks
            .run_on_error(ErrorContext {
                request: req_ctx.clone(),
                response: None,
                error: err.clone(),
            })
            .await;
        return Ok(TransportErrorAction::Return(err));
    }

    let retry_transport = transport_retryable(&err);
    if retry_transport && attempt < max_attempts {
        if let Some(policy) = retry_policy {
            let stub = retry_stub_response(
                StatusCode::SERVICE_UNAVAILABLE,
                HeaderMap::new(),
                Some(request_url.clone()),
                Bytes::new(),
                #[cfg(feature = "json")]
                json_parser,
            );
            let _ = run_retry_backoff(
                hooks,
                req_ctx,
                stub,
                &HeaderMap::new(),
                attempt,
                cancel,
                policy,
            )
            .await?;
            return Ok(TransportErrorAction::Retry);
        }
    }

    hooks
        .run_on_error(ErrorContext {
            request: req_ctx.clone(),
            response: None,
            error: err.clone(),
        })
        .await;

    if retry_transport && retry_policy.is_some() {
        return Ok(TransportErrorAction::Return(Error::retry_exhausted(
            attempt + 1,
            err,
        )));
    }

    Ok(TransportErrorAction::Return(err))
}

enum TransportErrorAction {
    Retry,
    Return(Error),
}

enum StreamRetryAction {
    Retry,
    Continue {
        body: BodyStream,
        /// Body prefix already peeked for a custom retry predicate.
        peeked_body: Option<Bytes>,
    },
}

async fn evaluate_stream_retry(
    prep: &PreparedExecution,
    status: StatusCode,
    headers: &HeaderMap,
    body: BodyStream,
    attempt: u32,
    max_attempts: u32,
    request_url: &Url,
) -> Result<StreamRetryAction> {
    let Some(policy) = prep.retry_policy.as_ref() else {
        return Ok(StreamRetryAction::Continue {
            body,
            peeked_body: None,
        });
    };

    let peek_limit = prep
        .max_response_bytes
        .map(|m| m.min(prep.retry_body_peek_bytes))
        .unwrap_or(prep.retry_body_peek_bytes);

    if policy.has_custom_should_retry() {
        let (peeked, rest) = peek_stream_prefix(body, peek_limit).await?;
        let stub = retry_stub_response(
            status,
            headers.clone(),
            Some(request_url.clone()),
            peeked.clone(),
            #[cfg(feature = "json")]
            prep.json_parser.clone(),
        );
        if policy.should_retry_response(&stub, false) && attempt < max_attempts {
            drain_remaining(rest).await?;
            return Ok(StreamRetryAction::Retry);
        }
        return Ok(StreamRetryAction::Continue {
            body: body_stream_prepend(peeked.clone(), rest),
            peeked_body: Some(peeked),
        });
    }

    let stub = retry_stub_response(
        status,
        headers.clone(),
        Some(request_url.clone()),
        Bytes::new(),
        #[cfg(feature = "json")]
        prep.json_parser.clone(),
    );
    if policy.should_retry_response(&stub, false) && attempt < max_attempts {
        let _ = drain_body_for_retry(body, peek_limit).await?;
        Ok(StreamRetryAction::Retry)
    } else {
        Ok(StreamRetryAction::Continue {
            body,
            peeked_body: None,
        })
    }
}

async fn run_http_loop(mut prep: PreparedExecution, mode: LoopMode) -> Result<LoopOutput> {
    let mut attempt = 0u32;
    let max_attempts = prep
        .retry_policy
        .as_ref()
        .map(|p| p.max_attempts())
        .unwrap_or(0);
    let cancel = prep.cancel.clone();

    loop {
        let cancel_ref = cancel.as_ref();
        prep.req_ctx.retry_attempt = attempt;
        check_attempt_replay(&prep, attempt)?;

        let body = take_body_for_attempt(&mut prep, attempt);
        let http_req = build_http_request(&mut prep, body);
        let request_url = http_req.url.clone();

        match mode {
            LoopMode::Buffered => {
                let result = execute_or_cancel(cancel_ref, prep.backend.execute(http_req)).await;
                match result {
                    Ok(http_res) => {
                        let response = retry_stub_response(
                            http_res.status,
                            http_res.headers.clone(),
                            Some(request_url.clone()),
                            http_res.body,
                            #[cfg(feature = "json")]
                            prep.json_parser.clone(),
                        );

                        let response = prep
                            .merged_hooks
                            .run_on_response(ResponseContext {
                                request: prep.req_ctx.clone(),
                                response,
                            })
                            .await?;

                        let should_retry = prep
                            .retry_policy
                            .as_ref()
                            .map(|p| p.should_retry_response(&response, false))
                            .unwrap_or(false);

                        if should_retry && attempt < max_attempts {
                            if let Some(policy) = prep.retry_policy.as_ref() {
                                attempt = run_retry_backoff(
                                    &prep.merged_hooks,
                                    &prep.req_ctx,
                                    response.clone(),
                                    response.headers(),
                                    attempt,
                                    cancel_ref,
                                    policy,
                                )
                                .await?;
                                continue;
                            }
                        }

                        if response.is_success() {
                            #[cfg(feature = "schema-validate")]
                            maybe_validate_response(&prep, &response)?;
                            prep.merged_hooks
                                .run_on_success(SuccessContext {
                                    request: prep.req_ctx.clone(),
                                    response: response.clone(),
                                })
                                .await;
                            return Ok(LoopOutput::Buffered(response));
                        }

                        let status = response.status();
                        let http_err =
                            Error::http_error_for_status(status, Some(response.bytes().clone()));
                        prep.merged_hooks
                            .run_on_error(ErrorContext {
                                request: prep.req_ctx.clone(),
                                response: Some(response.clone()),
                                error: http_err.clone(),
                            })
                            .await;

                        if prep.throw_on_error {
                            return Err(http_err);
                        }
                        return Ok(LoopOutput::Buffered(response));
                    }
                    Err(err) => match handle_transport_error(
                        &prep.merged_hooks,
                        &prep.req_ctx,
                        &request_url,
                        err,
                        attempt,
                        max_attempts,
                        prep.retry_policy.as_ref(),
                        cancel_ref,
                        #[cfg(feature = "json")]
                        prep.json_parser.clone(),
                    )
                    .await?
                    {
                        TransportErrorAction::Retry => {
                            attempt += 1;
                            continue;
                        }
                        TransportErrorAction::Return(e) => return Err(e),
                    },
                }
            }
            LoopMode::Streaming => {
                let result =
                    execute_or_cancel(cancel_ref, prep.backend.execute_stream(http_req)).await;
                match result {
                    Ok(http_res) => {
                        let status = http_res.status;
                        let headers = http_res.headers.clone();
                        let body = http_res.body;

                        match evaluate_stream_retry(
                            &prep,
                            status,
                            &headers,
                            body,
                            attempt,
                            max_attempts,
                            &request_url,
                        )
                        .await?
                        {
                            StreamRetryAction::Retry => {
                                let Some(policy) = prep.retry_policy.as_ref() else {
                                    continue;
                                };
                                let stub = retry_stub_response(
                                    status,
                                    headers.clone(),
                                    Some(request_url.clone()),
                                    Bytes::new(),
                                    #[cfg(feature = "json")]
                                    prep.json_parser.clone(),
                                );
                                attempt = run_retry_backoff(
                                    &prep.merged_hooks,
                                    &prep.req_ctx,
                                    stub,
                                    &headers,
                                    attempt,
                                    cancel_ref,
                                    policy,
                                )
                                .await?;
                                continue;
                            }
                            StreamRetryAction::Continue {
                                mut body,
                                peeked_body,
                            } => {
                                let meta = prep
                                    .merged_hooks
                                    .run_on_response_stream(StreamingResponseContext {
                                        request: prep.req_ctx.clone(),
                                        status,
                                        headers: headers.clone(),
                                    })
                                    .await?;
                                let status = meta.status;
                                let stream_headers = meta.headers;

                                if !status.is_success() {
                                    return finish_stream_http_status(
                                        &prep,
                                        status,
                                        stream_headers,
                                        body,
                                        request_url,
                                        peeked_body,
                                    )
                                    .await;
                                }

                                if let Some(limit) = prep.max_response_bytes {
                                    body = wrap_max_bytes(body, limit);
                                }
                                if let Some(token) = prep.cancel.clone() {
                                    body = wrap_cancellation(body, token);
                                }

                                if status.is_success() {
                                    prep.merged_hooks
                                        .run_on_success_stream(StreamingSuccessContext {
                                            request: prep.req_ctx.clone(),
                                            status,
                                            headers: stream_headers.clone(),
                                        })
                                        .await;
                                }

                                return Ok(LoopOutput::Stream(StreamingResponse::new(
                                    status,
                                    stream_headers,
                                    body,
                                    Some(request_url),
                                    prep.max_response_bytes,
                                    #[cfg(feature = "json")]
                                    prep.json_parser.clone(),
                                    #[cfg(feature = "schema-validate")]
                                    stream_response_schema_ctx(&prep),
                                )));
                            }
                        }
                    }
                    Err(err) => match handle_transport_error(
                        &prep.merged_hooks,
                        &prep.req_ctx,
                        &request_url,
                        err,
                        attempt,
                        max_attempts,
                        prep.retry_policy.as_ref(),
                        cancel_ref,
                        #[cfg(feature = "json")]
                        prep.json_parser.clone(),
                    )
                    .await?
                    {
                        TransportErrorAction::Retry => {
                            attempt += 1;
                            continue;
                        }
                        TransportErrorAction::Return(e) => return Err(e),
                    },
                }
            }
        }
    }
}

pub(crate) async fn run_buffered_loop(prep: PreparedExecution) -> Result<Response> {
    match run_http_loop(prep, LoopMode::Buffered).await? {
        LoopOutput::Buffered(response) => Ok(response),
        LoopOutput::Stream(_) => Err(Error::Config(
            "internal error: buffered loop returned stream output".into(),
        )),
    }
}

pub(crate) async fn run_stream_loop(prep: PreparedExecution) -> Result<StreamingResponse> {
    match run_http_loop(prep, LoopMode::Streaming).await? {
        LoopOutput::Stream(response) => Ok(response),
        LoopOutput::Buffered(_) => Err(Error::Config(
            "internal error: streaming loop returned buffered output".into(),
        )),
    }
}
