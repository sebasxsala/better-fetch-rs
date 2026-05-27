//! Request cancellation ([`CancellationToken`]) compatible with cooperative async abort.

pub use tokio_util::sync::CancellationToken;

use crate::error::Error;
use crate::Result;

/// Runs `sleep` unless `token` is cancelled first.
pub(crate) async fn sleep_or_cancel(
    delay: std::time::Duration,
    token: Option<&CancellationToken>,
) -> Result<()> {
    match token {
        None => {
            tokio::time::sleep(delay).await;
            Ok(())
        }
        Some(token) => {
            tokio::select! {
                () = tokio::time::sleep(delay) => Ok(()),
                () = token.cancelled() => Err(Error::Cancelled),
            }
        }
    }
}

/// Executes `fut` unless `token` is cancelled first.
pub(crate) async fn execute_or_cancel<F, T>(token: Option<&CancellationToken>, fut: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    match token {
        None => fut.await,
        Some(token) => {
            tokio::select! {
                res = fut => res,
                () = token.cancelled() => Err(Error::Cancelled),
            }
        }
    }
}
