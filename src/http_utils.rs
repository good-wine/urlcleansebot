//! Utility functions for HTTP requests with retry logic.

use std::time::Duration;
use tokio_retry::{strategy::ExponentialBackoff, Retry};
use tracing::warn;

/// Default retry strategy for HTTP requests.
/// Uses exponential backoff with jitter: starts at 1s, max 30s, max attempts 3.
pub fn default_retry_strategy() -> impl Iterator<Item = Duration> {
    ExponentialBackoff::from_millis(1000)
        .max_delay(Duration::from_secs(30))
        .take(3)
}

/// Execute an async operation with retry logic using exponential backoff.
///
/// # Arguments
/// * `operation` - The async operation to retry
/// * `operation_name` - Name of the operation for logging
///
/// # Returns
/// The result of the operation if successful, or the last error if all retries failed.
pub async fn retry_with_backoff<T, E, Fut, F>(
    operation: F,
    operation_name: &str,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Debug,
{
    let strategy = default_retry_strategy();

    Retry::spawn(strategy, || {
        let op = operation();
        async move {
            match op.await {
                Ok(result) => Ok(result),
                Err(e) => {
                    warn!(
                        operation = operation_name,
                        error = ?e,
                        "Operation failed, will retry"
                    );
                    Err(e)
                }
            }
        }
    })
    .await
}

/// Execute an HTTP request with retry logic.
/// This is a convenience wrapper around `retry_with_backoff` for HTTP requests.
///
/// # Arguments
/// * `request_builder` - A closure that returns a `reqwest::RequestBuilder`
/// * `operation_name` - Name of the operation for logging
///
/// # Returns
/// The HTTP response if successful, or the last error if all retries failed.
pub async fn retry_http_request<F>(
    request_builder: F,
    operation_name: &str,
) -> Result<reqwest::Response, reqwest::Error>
where
    F: Fn() -> reqwest::RequestBuilder,
{
    retry_with_backoff(
        || async {
            let req = request_builder();
            req.send().await
        },
        operation_name,
    )
    .await
}