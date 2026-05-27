use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use base64::Engine;
use http::header::{HeaderValue, AUTHORIZATION};
use http::HeaderMap;

/// Authentication configuration for a client or request.
#[derive(Clone)]
pub enum Auth {
    Bearer {
        token: TokenSource,
    },
    Basic {
        username: TokenSource,
        password: TokenSource,
    },
    Custom {
        prefix: String,
        value: TokenSource,
    },
}

/// Source for credential values (static, sync, or async).
#[derive(Clone)]
pub enum TokenSource {
    Static(String),
    Fn(Arc<dyn Fn() -> Option<String> + Send + Sync>),
    AsyncFn(Arc<dyn AsyncTokenProvider>),
}

/// Async token resolver.
pub trait AsyncTokenProvider: Send + Sync {
    fn resolve(&self) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>>;
}

impl<F, Fut> AsyncTokenProvider for F
where
    F: Send + Sync,
    F: Fn() -> Fut,
    Fut: Future<Output = Option<String>> + Send + 'static,
{
    fn resolve(&self) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>> {
        Box::pin((self)())
    }
}

impl Auth {
    pub fn bearer(token: impl Into<String>) -> Self {
        Self::Bearer {
            token: TokenSource::Static(token.into()),
        }
    }

    pub fn bearer_fn(f: impl Fn() -> Option<String> + Send + Sync + 'static) -> Self {
        Self::Bearer {
            token: TokenSource::Fn(Arc::new(f)),
        }
    }

    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Basic {
            username: TokenSource::Static(username.into()),
            password: TokenSource::Static(password.into()),
        }
    }

    pub async fn apply(&self, headers: &mut HeaderMap) -> crate::Result<()> {
        match self {
            Self::Bearer { token } => {
                if let Some(value) = resolve_token(token).await? {
                    set_authorization(headers, format!("Bearer {value}"))?;
                }
            }
            Self::Basic { username, password } => {
                let user = resolve_token(username).await?;
                let pass = resolve_token(password).await?;
                if let (Some(u), Some(p)) = (user, pass) {
                    let encoded =
                        base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}"));
                    set_authorization(headers, format!("Basic {encoded}"))?;
                }
            }
            Self::Custom { prefix, value } => {
                if let Some(v) = resolve_token(value).await? {
                    set_authorization(headers, format!("{prefix} {v}"))?;
                }
            }
        }
        Ok(())
    }
}

async fn resolve_token(source: &TokenSource) -> crate::Result<Option<String>> {
    match source {
        TokenSource::Static(s) => Ok(Some(s.clone())),
        TokenSource::Fn(f) => Ok(f()),
        TokenSource::AsyncFn(f) => Ok(f.resolve().await),
    }
}

fn set_authorization(headers: &mut HeaderMap, value: String) -> crate::Result<()> {
    let header_value = HeaderValue::from_str(&value)
        .map_err(|e| crate::error::Error::Other(format!("invalid authorization header: {e}")))?;
    headers.insert(AUTHORIZATION, header_value);
    Ok(())
}
