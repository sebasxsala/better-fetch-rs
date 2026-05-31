//! Retry policies for transport and HTTP failures.
//!
//! Configure on [`ClientBuilder::retry`](crate::ClientBuilder::retry) or per-request
//! [`RequestBuilder::retry`](crate::RequestBuilder::retry).
//!
//! On [`RequestBuilder::send_stream`](crate::RequestBuilder::send_stream), HTTP retries use status
//! and headers without reading the body. When a custom [`ShouldRetryFn`](crate::ShouldRetryFn) is
//! set, the client peeks up to [`ClientBuilder::retry_body_peek_bytes`](crate::ClientBuilder::retry_body_peek_bytes)
//! (default 64 KiB, capped by [`ClientBuilder::max_response_bytes`](crate::ClientBuilder::max_response_bytes) when set).
//! The peek is replayed on the stream returned to the caller; only a confirmed retry or
//! [`RequestBuilder::throw_on_error`](crate::RequestBuilder::throw_on_error)(`true`) drains the body.

use std::sync::Arc;
use std::time::Duration;

use http::{HeaderMap, StatusCode};

use crate::response::Response;

/// Predicate for whether a response should be retried.
pub type ShouldRetryFn = Arc<dyn Fn(&Response) -> bool + Send + Sync>;

/// Retry policy configuration.
///
/// The `attempts` value is the maximum number of **retries after the initial request**.
/// For example, `RetryPolicy::count(2)` performs up to three HTTP calls (one initial + two retries).
///
/// # Examples
///
/// ```
/// use better_fetch::RetryPolicy;
///
/// // Up to 3 HTTP calls total (1 initial + 2 retries), 1s between attempts
/// let policy = RetryPolicy::count(2);
/// assert_eq!(policy.max_attempts(), 2);
/// ```
#[derive(Clone)]
#[must_use = "apply with `ClientBuilder::retry` or `RequestBuilder::retry`"]
pub enum RetryPolicy {
    /// Shorthand for linear retry with `attempts` retries and a 1 second delay between attempts.
    Count {
        /// Maximum retries after the first request.
        attempts: u32,
        /// Optional custom retry predicate.
        should_retry: Option<ShouldRetryFn>,
    },
    /// Fixed delay between retries.
    Linear {
        /// Maximum retries after the first request.
        attempts: u32,
        /// Delay between attempts.
        delay: Duration,
        should_retry: Option<ShouldRetryFn>,
        /// When `true`, randomizes delay (see [`Self::with_jitter`]).
        jitter: bool,
    },
    /// Exponential backoff capped at `max_delay`.
    Exponential {
        /// Maximum retries after the first request.
        attempts: u32,
        /// Initial backoff duration.
        base_delay: Duration,
        /// Upper bound on backoff.
        max_delay: Duration,
        should_retry: Option<ShouldRetryFn>,
        jitter: bool,
    },
}

impl RetryPolicy {
    /// Shorthand: `attempts` retries with a **fixed 1 second** delay and default status codes.
    ///
    /// `Count` does **not** apply jitter. For randomized backoff use [`Self::exponential`]
    /// (jitter on by default) or [`Self::linear`] with [`Self::with_jitter`](Self::with_jitter)(`true`).
    pub fn count(attempts: u32) -> Self {
        Self::Count {
            attempts,
            should_retry: None,
        }
    }

    /// Linear backoff with a fixed `delay` between retries.
    pub fn linear(attempts: u32, delay: Duration) -> Self {
        Self::Linear {
            attempts,
            delay,
            should_retry: None,
            jitter: false,
        }
    }

    /// Exponential backoff from `base_delay` up to `max_delay` (jitter enabled by default).
    pub fn exponential(attempts: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self::Exponential {
            attempts,
            base_delay,
            max_delay,
            should_retry: None,
            jitter: true,
        }
    }

    /// Enables randomized backoff jitter on linear or exponential policies.
    ///
    /// No effect on [`RetryPolicy::Count`](Self::Count) — use [`Self::exponential`] for jittered delays.
    #[must_use = "chain with `ClientBuilder::retry` or `RequestBuilder::retry`"]
    pub fn with_jitter(mut self, jitter: bool) -> Self {
        match &mut self {
            Self::Linear { jitter: j, .. } | Self::Exponential { jitter: j, .. } => *j = jitter,
            Self::Count { .. } => {}
        }
        self
    }

    /// Overrides the default retry predicate (408, 429, 502, 503, 504).
    #[must_use = "chain with `ClientBuilder::retry` or `RequestBuilder::retry`"]
    pub fn with_should_retry(self, f: ShouldRetryFn) -> Self {
        match self {
            Self::Count { attempts, .. } => Self::Count {
                attempts,
                should_retry: Some(f),
            },
            Self::Linear {
                attempts,
                delay,
                jitter,
                ..
            } => Self::Linear {
                attempts,
                delay,
                should_retry: Some(f),
                jitter,
            },
            Self::Exponential {
                attempts,
                base_delay,
                max_delay,
                jitter,
                ..
            } => Self::Exponential {
                attempts,
                base_delay,
                max_delay,
                should_retry: Some(f),
                jitter,
            },
        }
    }

    /// Returns the maximum number of retries after the initial request.
    pub fn max_attempts(&self) -> u32 {
        match self {
            Self::Count { attempts, .. }
            | Self::Linear { attempts, .. }
            | Self::Exponential { attempts, .. } => *attempts,
        }
    }

    pub(crate) fn delay_before_attempt(&self, attempt: u32) -> Duration {
        match self {
            Self::Count { .. } => Duration::from_secs(1),
            Self::Linear { delay, .. } => *delay,
            Self::Exponential {
                base_delay,
                max_delay,
                ..
            } => {
                let exp = base_delay.saturating_mul(2u32.saturating_pow(attempt));
                exp.min(*max_delay)
            }
        }
    }

    /// Computes sleep duration using policy backoff, optional [`Retry-After`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Retry-After), and jitter.
    ///
    /// A server-provided `Retry-After` is honored exactly (never reduced by jitter), since it is a
    /// minimum delay the server mandated. Jitter only applies to the policy's own backoff.
    pub(crate) fn delay_after_response(&self, attempt: u32, headers: &HeaderMap) -> Duration {
        if let Some(retry_after) = parse_retry_after(headers) {
            return retry_after;
        }
        let base = self.delay_before_attempt(attempt);
        if self.uses_jitter() {
            apply_jitter(base)
        } else {
            base
        }
    }

    pub(crate) fn uses_jitter(&self) -> bool {
        match self {
            Self::Count { .. } => true,
            Self::Linear { jitter, .. } | Self::Exponential { jitter, .. } => *jitter,
        }
    }

    /// Returns `true` when a custom [`ShouldRetryFn`] predicate is configured.
    pub(crate) fn has_custom_should_retry(&self) -> bool {
        matches!(
            self,
            Self::Count {
                should_retry: Some(_),
                ..
            } | Self::Linear {
                should_retry: Some(_),
                ..
            } | Self::Exponential {
                should_retry: Some(_),
                ..
            }
        )
    }

    pub(crate) fn should_retry_response(
        &self,
        response: &Response,
        transport_failed: bool,
    ) -> bool {
        if transport_failed {
            return true;
        }

        let custom = match self {
            Self::Count { should_retry, .. }
            | Self::Linear { should_retry, .. }
            | Self::Exponential { should_retry, .. } => should_retry.as_ref(),
        };

        if let Some(f) = custom {
            return f(response);
        }

        default_should_retry(response.status())
    }
}

/// Default HTTP status codes that trigger a retry when no custom predicate is set.
pub fn default_should_retry(status: StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 429 | 502 | 503 | 504)
}

/// Parses `Retry-After` as a delay.
///
/// Supports both the integer `delay-seconds` form and the HTTP-date form
/// (RFC 7231); a date in the past yields [`Duration::ZERO`].
pub fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(http::header::RETRY_AFTER)?.to_str().ok()?;
    let value = value.trim();
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let when = httpdate::parse_http_date(value).ok()?;
    Some(
        when.duration_since(std::time::SystemTime::now())
            .unwrap_or(Duration::ZERO),
    )
}

fn apply_jitter(delay: Duration) -> Duration {
    let nanos = delay.as_nanos().min(u128::from(u64::MAX)) as u64;
    if nanos == 0 {
        return delay;
    }
    let half = nanos / 2;
    let span = nanos.saturating_sub(half).max(1);
    Duration::from_nanos(half + fastrand::u64(..span))
}

pub(crate) use crate::cancel::sleep_or_cancel;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response::Response;
    use http::StatusCode;

    fn response_with_status(status: u16) -> Response {
        Response::new(
            StatusCode::from_u16(status).unwrap(),
            http::HeaderMap::new(),
            bytes::Bytes::new(),
            None,
            #[cfg(feature = "json")]
            None,
        )
    }

    #[test]
    fn default_should_retry_codes() {
        assert!(default_should_retry(StatusCode::REQUEST_TIMEOUT));
        assert!(default_should_retry(StatusCode::TOO_MANY_REQUESTS));
        assert!(default_should_retry(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!default_should_retry(StatusCode::NOT_FOUND));
    }

    #[test]
    fn count_policy_max_attempts() {
        assert_eq!(RetryPolicy::count(3).max_attempts(), 3);
    }

    #[test]
    fn count_with_should_retry_stays_count() {
        let policy = RetryPolicy::count(2)
            .with_should_retry(Arc::new(|r| r.status() == StatusCode::NOT_FOUND));
        assert!(matches!(policy, RetryPolicy::Count { .. }));
        assert!(policy.should_retry_response(&response_with_status(404), false));
        assert!(!policy.should_retry_response(&response_with_status(503), false));
    }

    #[test]
    fn linear_delay_is_constant() {
        let policy = RetryPolicy::linear(3, Duration::from_millis(500));
        assert_eq!(policy.delay_before_attempt(0), Duration::from_millis(500));
        assert_eq!(policy.delay_before_attempt(2), Duration::from_millis(500));
    }

    #[test]
    fn exponential_delay_caps_at_max() {
        let policy = RetryPolicy::exponential(5, Duration::from_secs(1), Duration::from_secs(5));
        assert_eq!(policy.delay_before_attempt(0), Duration::from_secs(1));
        assert_eq!(policy.delay_before_attempt(10), Duration::from_secs(5));
    }

    #[test]
    fn custom_should_retry_overrides_default() {
        let policy = RetryPolicy::linear(2, Duration::from_millis(1))
            .with_should_retry(Arc::new(|r| r.status() == StatusCode::NOT_FOUND));
        assert!(policy.should_retry_response(&response_with_status(404), false));
        assert!(!policy.should_retry_response(&response_with_status(503), false));
    }

    #[test]
    fn parse_retry_after_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::RETRY_AFTER, "3".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(3)));
    }

    #[test]
    fn delay_after_response_uses_retry_after() {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::RETRY_AFTER, "2".parse().unwrap());
        let policy = RetryPolicy::linear(1, Duration::from_millis(100)).with_jitter(false);
        assert_eq!(
            policy.delay_after_response(0, &headers),
            Duration::from_secs(2)
        );
    }

    #[test]
    fn retry_after_is_not_reduced_by_jitter() {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::RETRY_AFTER, "5".parse().unwrap());
        let policy = RetryPolicy::exponential(3, Duration::from_secs(1), Duration::from_secs(30));
        assert!(policy.uses_jitter());
        for _ in 0..20 {
            assert_eq!(
                policy.delay_after_response(0, &headers),
                Duration::from_secs(5)
            );
        }
    }

    #[test]
    fn parse_retry_after_future_http_date() {
        let future = std::time::SystemTime::now() + Duration::from_secs(3600);
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::RETRY_AFTER,
            httpdate::fmt_http_date(future).parse().unwrap(),
        );
        let delay = parse_retry_after(&headers).expect("date delay");
        assert!(delay > Duration::from_secs(3000) && delay <= Duration::from_secs(3600));
    }

    #[test]
    fn jitter_stays_within_bounds() {
        let base = Duration::from_secs(4);
        for _ in 0..20 {
            let jittered = apply_jitter(base);
            assert!(jittered >= Duration::from_secs(2));
            assert!(jittered <= base);
        }
    }

    #[test]
    fn parse_retry_after_invalid_is_none() {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::RETRY_AFTER, "not-a-number".parse().unwrap());
        assert!(parse_retry_after(&headers).is_none());
    }

    #[test]
    fn exponential_uses_jitter_by_default() {
        let policy = RetryPolicy::exponential(3, Duration::from_secs(1), Duration::from_secs(8));
        assert!(policy.uses_jitter());
    }

    #[test]
    fn linear_jitter_disabled_by_default() {
        assert!(!RetryPolicy::linear(1, Duration::from_secs(1)).uses_jitter());
    }
}
