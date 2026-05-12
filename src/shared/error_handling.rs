//! Error Handling Best Practices Guide
//!
//! This module documents and demonstrates improved error handling patterns
//! for the ClearURLs Bot project.

/// Error Handling Strategy Overview
///
/// The ClearURLs Bot uses a hierarchical error handling approach:
///
/// ```text
/// Application Error (AppError)
///     |
/// Result<T, AppError> alias AppResult<T>
///     |
/// Teloxide RequestError (for Telegram operations)
///     |
/// HTTP Errors (reqwest)
///     |
/// Database Errors (sqlx)
///     |
/// Standard Rust Errors (std::error::Error)
/// ```

#[doc = "
# Anti-Patterns to Avoid

## ❌ NEVER DO THIS:

```rust,ignore
// Never use .unwrap() in production code
let value = some_result.unwrap(); // ← WRONG!

// Never use .expect() without context
let value = some_result.expect(\"failed\"); // ← WRONG!

// Never silently ignore errors
let _ = db.save_user_config(&config).await; // ← WRONG! (except in specific cases)

// Never create new errors without context
return Err(AppError::Internal(\"error\".to_string())); // ← WRONG! No context
```

## ✅ CORRECT PATTERNS:

```rust,ignore
// Use ? operator for error propagation
let value = some_result?; // ← RIGHT!

// Use map_err for error transformation
let value = some_result.map_err(|e| {
    AppError::Internal(format!(\"Failed to process: {}\", e))
})?; // ← RIGHT!

// Use ? with context for important operations
let config = db.get_user_config(user_id)
    .await
    .map_err(|e| {
        AppError::Database(format!(\"Failed to load config for user {}: {}\", user_id, e))
    })?;

// Log important errors instead of silently ignoring
if let Err(e) = db.save_user_config(&config).await {
    tracing::warn!(error = %e, user_id = %user_id, \"Failed to save user config\");
    // Optionally notify user
}
```
"]

pub mod error_handling_patterns {
    use crate::shared::error::{AppError, AppResult};
    use std::collections::HashMap;

    /// Pattern 1: Error Propagation with ? Operator
    ///
    /// Use ? for functions that return Result
    pub async fn pattern_error_propagation(input: &str) -> AppResult<String> {
        // Validates input and propagates error if invalid
        let validated = crate::shared::validation::validate_url(input)?;
        Ok(validated)
    }

    /// Pattern 2: Error Transformation with map_err
    ///
    /// Use map_err when you need to convert one error type to another
    pub async fn pattern_error_transformation(
        db: &crate::db::Db,
        user_id: i64,
    ) -> AppResult<HashMap<String, String>> {
        let config = db.get_user_config(user_id)
            .await
            .map_err(|e| {
                AppError::Internal(format!(
                    "Failed to load user config for user {}: {}",
                    user_id, e
                ))
            })?;

        Ok(HashMap::from_iter(vec![
            ("language".to_string(), config.language),
        ]))
    }

    /// Pattern 3: Graceful Degradation with unwrap_or
    ///
    /// Use unwrap_or for non-critical operations
    pub fn pattern_graceful_degradation(
        text: &str,
        fallback: &str,
    ) -> String {
        crate::shared::validation::sanitize_html_content(text)
            .split('#')
            .next()
            .unwrap_or(fallback)
            .to_string()
    }

    /// Pattern 4: Logging Without Propagation
    ///
    /// Log errors that don't affect flow but should be tracked
    pub async fn pattern_log_and_continue(
        db: &crate::db::Db,
        user_id: i64,
        update: &crate::db::models::UserConfig,
    ) {
        if let Err(e) = db.save_user_config(update).await {
            tracing::warn!(
                error = %e,
                user_id = user_id,
                "Failed to save user configuration, continuing anyway"
            );
            // Note: Don't return error here, just log and continue
        }
    }

    /// Pattern 5: Conditional Error Handling
    ///
    /// Different handling based on error type or condition
    pub async fn pattern_conditional_handling(
        text: &str,
    ) -> AppResult<String> {
        match crate::shared::validation::validate_url(text) {
            Ok(url) => Ok(url),
            Err(AppError::Validation(msg)) => {
                // Validation errors get logged but turned into a user-friendly message
                tracing::debug!("URL validation failed: {}", msg);
                Err(AppError::Validation("❌ Invalid URL format".to_string()))
            }
            Err(e) => {
                // Other errors are propagated as-is
                Err(e)
            }
        }
    }

    /// Pattern 6: Batch Operation Error Handling
    ///
    /// Collect errors from multiple operations
    pub async fn pattern_batch_operations(
        urls: &[String],
        user_id: i64,
    ) -> (Vec<String>, Vec<AppError>) {
        let mut successes = Vec::new();
        let mut errors = Vec::new();

        for url in urls {
            match crate::shared::validation::validate_url(url) {
                Ok(validated) => successes.push(validated),
                Err(e) => errors.push(e),
            }
        }

        if !errors.is_empty() {
            tracing::warn!(
                error_count = errors.len(),
                success_count = successes.len(),
                user_id = user_id,
                "Some URLs failed validation"
            );
        }

        (successes, errors)
    }

    /// Pattern 7: Async Error Handling in Spawned Tasks
    ///
    /// Handle errors in tokio::spawn tasks properly
    pub fn pattern_async_task_error_handling() {
        let example_task = async {
            // Never use .unwrap() in spawned tasks
            // Instead, log and handle gracefully
            match crate::shared::validation::validate_domain("example.com") {
                Ok(domain) => {
                    tracing::info!("Domain validated: {}", domain);
                }
                Err(e) => {
                    tracing::error!("Domain validation failed: {}", e);
                    // Handle error appropriately for the task context
                }
            }
        };

        tokio::spawn(example_task);
    }
}

pub mod error_recovery_strategies {
    use crate::shared::error::{AppError, AppResult};

    /// Strategy 1: Retry with Exponential Backoff
    ///
    /// For transient errors, retry with increasing delays
    pub async fn strategy_retry_with_backoff<F, Fut, T>(
        mut f: F,
        max_retries: u32,
    ) -> AppResult<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = AppResult<T>>,
    {
        let mut delay = std::time::Duration::from_millis(100);

        for attempt in 0..max_retries {
            match f().await {
                Ok(value) => return Ok(value),
                Err(e) if attempt < max_retries - 1 => {
                    tracing::warn!(
                        attempt = attempt,
                        delay_ms = delay.as_millis(),
                        error = %e,
                        "Retrying after transient error"
                    );
                    tokio::time::sleep(delay).await;
                    delay = delay.saturating_mul(2);
                }
                Err(e) => return Err(e),
            }
        }

        Err(AppError::Internal("Max retries exceeded".to_string()))
    }

    /// Strategy 2: Circuit Breaker Pattern
    ///
    /// Stop attempting operations if failure rate is too high
    pub struct CircuitBreaker {
        failure_threshold: usize,
        success_threshold: usize,
        failures: std::sync::atomic::AtomicUsize,
        successes: std::sync::atomic::AtomicUsize,
        is_open: std::sync::atomic::AtomicBool,
    }

    impl CircuitBreaker {
        pub fn new(failure_threshold: usize, success_threshold: usize) -> Self {
            Self {
                failure_threshold,
                success_threshold,
                failures: std::sync::atomic::AtomicUsize::new(0),
                successes: std::sync::atomic::AtomicUsize::new(0),
                is_open: std::sync::atomic::AtomicBool::new(false),
            }
        }

        pub fn is_circuit_open(&self) -> bool {
            self.is_open.load(std::sync::atomic::Ordering::SeqCst)
        }

        pub fn record_success(&self) {
            let successes =
                self.successes.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if successes >= self.success_threshold {
                // Circuit is now closed again
                self.failures.store(0, std::sync::atomic::Ordering::SeqCst);
                self.successes.store(0, std::sync::atomic::Ordering::SeqCst);
                self.is_open.store(false, std::sync::atomic::Ordering::SeqCst);
                tracing::info!("Circuit breaker closed - service recovered");
            }
        }

        pub fn record_failure(&self) {
            let failures = self.failures.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if failures >= self.failure_threshold {
                // Circuit is now open
                self.is_open.store(true, std::sync::atomic::Ordering::SeqCst);
                tracing::error!("Circuit breaker opened - service is down");
            }
        }
    }

    /// Strategy 3: Fallback Values
    ///
    /// Provide sensible defaults when errors occur
    pub async fn strategy_fallback_values(
        primary_result: AppResult<String>,
        fallback: &str,
    ) -> String {
        match primary_result {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!("Used fallback value due to error: {}", e);
                fallback.to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_error_propagation() {
        let result =
            error_handling_patterns::pattern_error_propagation("not a url").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_graceful_degradation() {
        let result = error_handling_patterns::pattern_graceful_degradation(
            "#anchor",
            "fallback",
        );
        assert_eq!(result, "".to_string());
    }

    #[tokio::test]
    async fn test_batch_operations() {
        // Test batch operations without database dependency
        let (_successes, _errors) = error_handling_patterns::pattern_batch_operations(
            &["https://example.com".to_string()],
            123,
        )
        .await;
    }
}
