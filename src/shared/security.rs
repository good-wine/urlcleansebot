//! Security utilities and validation functions.
//!
//! This module provides security-related utilities including:
//! - Input validation and sanitization
//! - Rate limiting
//! - Content security checks

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Maximum URL length to prevent DoS attacks
pub const MAX_URL_LENGTH: usize = 2048;

/// Maximum message length for Telegram
pub const MAX_MESSAGE_LENGTH: usize = 4096;

/// Rate limiting: max requests per minute
pub const RATE_LIMIT_REQUESTS: u32 = 10;

/// Rate limiting window in seconds
pub const RATE_LIMIT_WINDOW: u64 = 60;

/// Global rate limiter storage
static RATE_LIMITER: Lazy<Mutex<HashMap<i64, Vec<Instant>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Check if user is within rate limits
pub fn check_rate_limit(user_id: i64) -> Result<(), SecurityError> {
    let mut limiter = RATE_LIMITER.lock().unwrap();
    let now = Instant::now();
    let window_start = now - Duration::from_secs(RATE_LIMIT_WINDOW);

    // Get or create user entry
    let timestamps = limiter.entry(user_id).or_default();

    // Remove old timestamps outside the window
    timestamps.retain(|&time| time > window_start);

    // Check if under limit
    if timestamps.len() >= RATE_LIMIT_REQUESTS as usize {
        return Err(SecurityError::RateLimitExceeded);
    }

    // Add current timestamp
    timestamps.push(now);
    Ok(())
}

/// Regex for URL validation
static URL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^https?://[^\s/$.?#].[^\s]*$").unwrap());

/// Regex for detecting potentially malicious patterns
static MALICIOUS_PATTERNS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(javascript:|data:|vbscript:|file:|ftp:|mailto:)").unwrap());

/// Validate and sanitize URL input
pub fn validate_url(url: &str) -> Result<String, SecurityError> {
    // Check length
    if url.len() > MAX_URL_LENGTH {
        return Err(SecurityError::UrlTooLong);
    }

    // Check for empty or whitespace-only
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(SecurityError::EmptyInput);
    }

    // Check for malicious patterns
    if MALICIOUS_PATTERNS.is_match(trimmed) {
        return Err(SecurityError::MaliciousContent);
    }

    // Basic URL validation
    if !URL_REGEX.is_match(trimmed) {
        return Err(SecurityError::InvalidUrl);
    }

    // URL decode to check for encoded malicious content
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

/// Sanitize text for Telegram messages to prevent injection
pub fn sanitize_telegram_text(text: &str) -> String {
    // Remove or escape potentially dangerous characters
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

/// Validate user ID (Telegram user IDs are positive i64)
pub fn validate_user_id(user_id: i64) -> Result<i64, SecurityError> {
    if user_id <= 0 {
        return Err(SecurityError::InvalidUserId);
    }
    Ok(user_id)
}

/// Regex for domain validation
static DOMAIN_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,}$").unwrap()
});

/// Validate domain name
pub fn validate_domain(domain: &str) -> Result<String, SecurityError> {
    let trimmed = domain.trim().to_lowercase();

    if trimmed.is_empty() {
        return Err(SecurityError::EmptyInput);
    }

    if trimmed.len() > 253 {
        // Max domain length per RFC
        return Err(SecurityError::ContentTooLong);
    }

    // Check for malicious patterns
    if MALICIOUS_PATTERNS.is_match(&trimmed) {
        return Err(SecurityError::MaliciousContent);
    }

    // Basic domain validation
    if !DOMAIN_REGEX.is_match(&trimmed) {
        return Err(SecurityError::InvalidDomain);
    }

    Ok(trimmed)
}

/// Security error types
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
