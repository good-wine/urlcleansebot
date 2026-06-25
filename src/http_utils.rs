//! Utility functions for HTTP requests with retry logic and DNS pinning.

use std::time::Duration;
use tokio_retry::{Retry, strategy::ExponentialBackoff};
use tracing::warn;
use url::Url;

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
pub async fn retry_with_backoff<T, E, Fut, F>(operation: F, operation_name: &str) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Debug,
{
    let strategy = default_retry_strategy();

    Retry::start(strategy, || {
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
                },
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

/// Check if a hostname resolves to a private/reserved IP using DNS pinning.
/// Uses the shared DNS pinning cache from `crate::shared::security`.
pub fn is_dns_pinned_host_safe(host: &str) -> bool {
    crate::shared::security::validate_external_host(host).is_ok()
}

/// Build a reqwest client that verifies destinations against DNS pinning.
/// Returns `None` if the URL's host resolves to a private/internal IP.
pub fn verify_url_dns_pinned(url_str: &str) -> Result<(), String> {
    let parsed = Url::parse(url_str).map_err(|e| format!("Invalid URL: {e}"))?;
    let host = parsed.host_str().ok_or("URL has no host")?;
    if is_dns_pinned_host_safe(host) {
        Ok(())
    } else {
        Err(format!("Host {host} resolves to a private/reserved IP"))
    }
}

/// Create a reqwest client with sensible defaults and optional DNS pinning.
/// The `verify_dns` parameter controls whether destinations are checked against SSRF protection.
pub fn build_safe_client(verify_dns: bool) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("URLCleanseBot/1.0");

    if verify_dns {
        builder = builder.redirect(reqwest::redirect::Policy::custom(move |attempt| {
            let host = attempt.url().host_str().map(|h| h.to_string());
            if let Some(host) = host
                && !is_dns_pinned_host_safe(&host)
            {
                tracing::warn!(host = %host, "SSRF blocked: redirect blocked by DNS pinning");
                return attempt.error(format!("SSRF blocked: {host} resolves to a private IP"));
            }
            attempt.follow()
        }));
    }

    builder.build().expect("Failed to build reqwest client")
}
