//! Security utilities and validation functions.
//!
//! This module provides security-related utilities including:
//! - Input validation and sanitization
//! - Rate limiting (sync and async variants)
//! - Content security checks

use moka::future::Cache;
use moka::sync::Cache as SyncCache;
use regex::Regex;
use std::sync::LazyLock;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;

/// Maximum URL length to prevent DoS attacks
pub const MAX_URL_LENGTH: usize = 2048;

/// Maximum message length for Telegram
pub const MAX_MESSAGE_LENGTH: usize = 4096;

/// Rate limiting: max requests per minute (async variant)
pub const RATE_LIMIT_REQUESTS: u32 = 10;

/// Rate limiting window in seconds (async variant)
pub const RATE_LIMIT_WINDOW: u64 = 60;

// ── Async rate limiter (for query handlers) ───────────────────────────────

static RATE_LIMITER_CACHE: LazyLock<Cache<i64, Arc<u32>>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(100_000)
        .time_to_live(std::time::Duration::from_secs(RATE_LIMIT_WINDOW))
        .build()
});

pub async fn check_rate_limit(user_id: i64) -> Result<(), SecurityError> {
    let current = RATE_LIMITER_CACHE
        .get(&user_id)
        .await
        .unwrap_or_else(|| Arc::new(0));
    let count = *current + 1;

    if count > RATE_LIMIT_REQUESTS {
        return Err(SecurityError::RateLimitExceeded);
    }

    RATE_LIMITER_CACHE.insert(user_id, Arc::new(count)).await;
    Ok(())
}

// ── Sync rate limiter (for message handlers) ──────────────────────────────

pub struct RateLimiter {
    cache: SyncCache<i64, Arc<()>>,
}

impl RateLimiter {
    pub fn new(min_interval: Duration) -> Self {
        let ttl = min_interval * 3;
        Self {
            cache: SyncCache::builder()
                .max_capacity(100_000)
                .time_to_live(ttl)
                .build(),
        }
    }

    pub fn check(&self, user_id: i64) -> bool {
        match self.cache.get(&user_id) {
            Some(_) => false,
            None => {
                self.cache.insert(user_id, Arc::new(()));
                true
            }
        }
    }
}

pub static RATE_LIMITER: LazyLock<RateLimiter> = LazyLock::new(|| RateLimiter::new(Duration::from_secs(1)));

// ── Input sanitization ─────────────────────────────────────────────────────

static SANITIZE_LIMIT: usize = 4000;

fn sanitize_string(input: &str) -> String {
    let trimmed = input.trim();
    let has_control = trimmed.chars().any(|c| c.is_control());
    let needs_truncation = trimmed.len() > SANITIZE_LIMIT;

    if !has_control && !needs_truncation {
        return trimmed.to_string();
    }

    let mut s: String = trimmed.chars().filter(|c| !c.is_control()).collect();
    if s.len() > SANITIZE_LIMIT {
        s = s.chars().take(SANITIZE_LIMIT).collect();
    }
    s
}

pub fn sanitize_input(input: &str) -> String {
    sanitize_string(input)
}

pub fn sanitize_callback(input: &str) -> String {
    sanitize_string(input)
}

pub fn sanitize_telegram_text(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '&' => "&amp;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#x27;".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

// ── Validation ─────────────────────────────────────────────────────────────
static URL_REGEX: LazyLock<Regex> =

    LazyLock::new(|| Regex::new(r"^https?://[^\s/$.?#]+\.[^\s]*$").unwrap());

static MALICIOUS_PATTERNS: LazyLock<Regex> =

    LazyLock::new(|| Regex::new(r"(?i)(javascript:|data:|vbscript:|file:|ftp:|mailto:)").unwrap());
pub fn validate_url(url: &str) -> Result<String, SecurityError> {
    if url.len() > MAX_URL_LENGTH {
        return Err(SecurityError::UrlTooLong);
    }

    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(SecurityError::EmptyInput);
    }

    if MALICIOUS_PATTERNS.is_match(trimmed) {
        return Err(SecurityError::MaliciousContent);
    }

    if !URL_REGEX.is_match(trimmed) {
        return Err(SecurityError::InvalidUrl);
    }

    match urlencoding::decode(trimmed) {
        Ok(decoded) => {
            if MALICIOUS_PATTERNS.is_match(&decoded) {
                return Err(SecurityError::MaliciousContent);
            }
            Ok(decoded.to_string())
        }
        Err(_) => Err(SecurityError::InvalidUrl),
    }
}

pub fn validate_user_id(user_id: i64) -> Result<i64, SecurityError> {
    if user_id <= 0 {
        return Err(SecurityError::InvalidUserId);
    }
    Ok(user_id)
}

pub fn is_admin(user_id: i64, admin_id: i64) -> bool {
    user_id == admin_id
}

static USER_ID_HASH_SALT: LazyLock<String> = LazyLock::new(|| {
    std::env::var("USER_ID_HASH_SALT").unwrap_or_else(|_| "clearurlsbot-default-salt".to_string())
});

pub fn hash_user_id(user_id: i64) -> String {
    let salted = format!("{}:{}", *USER_ID_HASH_SALT, user_id);
    let mut hasher = Sha256::new();
    hasher.update(salted.as_bytes());
    let result = hasher.finalize();
    // Convert hash result to hex string manually
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

static DOMAIN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,}$").unwrap()
});

pub fn validate_domain(domain: &str) -> Result<String, SecurityError> {
    let trimmed = domain.trim().to_lowercase();

    if trimmed.is_empty() {
        return Err(SecurityError::EmptyInput);
    }

    if trimmed.len() > 253 {
        return Err(SecurityError::ContentTooLong);
    }

    if MALICIOUS_PATTERNS.is_match(&trimmed) {
        return Err(SecurityError::MaliciousContent);
    }

    if !DOMAIN_REGEX.is_match(&trimmed) {
        return Err(SecurityError::InvalidDomain);
    }

    Ok(trimmed)
}

pub fn is_safe_url_scheme(url: &str) -> bool {
    let lower = url.trim().to_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

// ── Error types ────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("URL is too long")]
    UrlTooLong,
    #[error("Input is empty")]
    EmptyInput,
    #[error("Invalid URL format")]
    InvalidUrl,
    #[error("Invalid domain format")]
    InvalidDomain,
    #[error("Malicious content detected")]
    MaliciousContent,
    #[error("Invalid user ID")]
    InvalidUserId,
    #[error("Content is too long")]
    ContentTooLong,
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_url_valid() {
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("http://test.com/path?param=value").is_ok());
    }

    #[test]
    fn test_validate_url_invalid() {
        assert!(matches!(validate_url(""), Err(SecurityError::EmptyInput)));
        assert!(matches!(
            validate_url("not-a-url"),
            Err(SecurityError::InvalidUrl)
        ));
        assert!(matches!(
            validate_url("javascript:alert(1)"),
            Err(SecurityError::MaliciousContent)
        ));
        assert!(matches!(
            validate_url(&"a".repeat(3000)),
            Err(SecurityError::UrlTooLong)
        ));
    }

    #[test]
    fn test_sanitize_telegram_text() {
        assert_eq!(sanitize_telegram_text("<script>"), "&lt;script&gt;");
        assert_eq!(sanitize_telegram_text("normal text"), "normal text");
    }
}
