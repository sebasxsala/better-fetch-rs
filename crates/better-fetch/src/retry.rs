use std::sync::Arc;
use std::time::Duration;

use http::StatusCode;

use crate::response::Response;

/// Predicate for whether a response should be retried.
pub type ShouldRetryFn = Arc<dyn Fn(&Response) -> bool + Send + Sync>;

/// Retry policy configuration.
///
/// The `attempts` value is the maximum number of **retries after the initial request**.
/// For example, `RetryPolicy::count(2)` performs up to three HTTP calls (one initial + two retries).
#[derive(Clone)]
pub enum RetryPolicy {
    /// Shorthand for linear retry with `attempts` retries and a 1 second delay between attempts.
    Count(u32),
    Linear {
        attempts: u32,
        delay: Duration,
        should_retry: Option<ShouldRetryFn>,
    },
    Exponential {
        attempts: u32,
        base_delay: Duration,
        max_delay: Duration,
        should_retry: Option<ShouldRetryFn>,
    },
}

impl RetryPolicy {
    pub fn count(attempts: u32) -> Self {
        Self::Count(attempts)
    }

    pub fn linear(attempts: u32, delay: Duration) -> Self {
        Self::Linear {
            attempts,
            delay,
            should_retry: None,
        }
    }

    pub fn exponential(attempts: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self::Exponential {
            attempts,
            base_delay,
            max_delay,
            should_retry: None,
        }
    }

    pub fn with_should_retry(self, f: ShouldRetryFn) -> Self {
        match self {
            Self::Count(attempts) => Self::Linear {
                attempts,
                delay: Duration::from_secs(1),
                should_retry: Some(f),
            },
            Self::Linear {
                attempts,
                delay,
                should_retry: _,
            } => Self::Linear {
                attempts,
                delay,
                should_retry: Some(f),
            },
            Self::Exponential {
                attempts,
                base_delay,
                max_delay,
                should_retry: _,
            } => Self::Exponential {
                attempts,
                base_delay,
                max_delay,
                should_retry: Some(f),
            },
        }
    }

    pub(crate) fn max_attempts(&self) -> u32 {
        match self {
            Self::Count(n)
            | Self::Linear { attempts: n, .. }
            | Self::Exponential { attempts: n, .. } => *n,
        }
    }

    pub(crate) fn delay_before_attempt(&self, attempt: u32) -> Duration {
        match self {
            Self::Count(_) => Duration::from_secs(1),
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

    pub(crate) fn should_retry_response(
        &self,
        response: &Response,
        transport_failed: bool,
    ) -> bool {
        if transport_failed {
            return true;
        }

        let custom = match self {
            Self::Linear { should_retry, .. } | Self::Exponential { should_retry, .. } => {
                should_retry.as_ref()
            }
            Self::Count(_) => None,
        };

        if let Some(f) = custom {
            return f(response);
        }

        default_should_retry(response.status())
    }
}

pub fn default_should_retry(status: StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 502 | 503 | 504)
}

pub(crate) async fn sleep_before_retry(delay: Duration) {
    tokio::time::sleep(delay).await;
}

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
        assert!(default_should_retry(StatusCode::TOO_MANY_REQUESTS));
        assert!(default_should_retry(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!default_should_retry(StatusCode::NOT_FOUND));
    }

    #[test]
    fn count_policy_max_attempts() {
        assert_eq!(RetryPolicy::count(3).max_attempts(), 3);
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
}
