//! Security utilities and validation functions.
//!
//! This module provides security-related utilities including:
//! - Input validation and sanitization
//! - Rate limiting (sync and async variants)
//! - Content security checks
//! - HMAC webhook verification
//! - DNS pinning against SSRF

use moka::future::Cache;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::net::{IpAddr, ToSocketAddrs};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;

const HMAC_BLOCK_SIZE: usize = 64;

/// Compute HMAC-SHA256 manually (avoids digest version conflicts).
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut k = key.to_vec();
    if k.len() > HMAC_BLOCK_SIZE {
        let mut hasher = Sha256::new();
        hasher.update(&k);
        k = hasher.finalize().to_vec();
    }
    k.resize(HMAC_BLOCK_SIZE, 0);

    let mut ipad = vec![0x36u8; HMAC_BLOCK_SIZE];
    let mut opad = vec![0x5cu8; HMAC_BLOCK_SIZE];
    for i in 0..k.len() {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }

    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(inner_hash);
    let result = outer.finalize();
    result.into()
}

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
        },
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

// ── HMAC webhook verification ──────────────────────────────────────────────

/// Verify an HMAC-SHA256 signature for webhook request body.
///
/// Returns `true` if the signature matches.
/// The expected signature is hex-encoded HMAC-SHA256 of the body.
pub fn verify_hmac_signature(body: &[u8], signature: &str, secret: &[u8]) -> bool {
    let expected = hmac_sha256(secret, body);
    let expected_hex: String = expected.iter().map(|b| format!("{:02x}", b)).collect();
    // Constant-time comparison using XOR
    if expected_hex.len() != signature.len() {
        return false;
    }
    let mut result = 0u8;
    for (a, b) in expected_hex.bytes().zip(signature.bytes()) {
        result |= a ^ b;
    }
    result == 0
}

/// Extract the HMAC signature from a `X-Webhook-Signature` header value.
/// Format: `sha256=<hex_signature>` or just the hex string.
pub fn extract_hmac_signature(header: &str) -> Option<&str> {
    if let Some(stripped) = header.strip_prefix("sha256=") {
        Some(stripped.trim())
    } else {
        Some(header.trim())
    }
}

// ── DNS pinning ────────────────────────────────────────────────────────────

/// DNS pinning cache: maps hostname -> resolved IP addresses.
/// Prevents DNS rebinding attacks.
static DNS_PINNING_CACHE: LazyLock<moka::sync::Cache<String, Vec<IpAddr>>> =
    LazyLock::new(|| {
        moka::sync::Cache::builder()
            .max_capacity(10_000)
            .time_to_live(Duration::from_secs(300)) // 5 min TTL
            .build()
    });

/// Resolve a hostname to IP addresses, using DNS pinning cache.
/// Returns cached result if available, otherwise resolves and caches.
pub fn resolve_hostname(host: &str) -> Vec<IpAddr> {
    if let Some(cached) = DNS_PINNING_CACHE.get(host) {
        return cached;
    }

    let addr = format!("{host}:443");
    let ips: Vec<IpAddr> = match addr.to_socket_addrs() {
        Ok(iter) => iter.map(|sa| sa.ip()).collect(),
        Err(_) => return vec![],
    };

    DNS_PINNING_CACHE.insert(host.to_string(), ips.clone());
    ips
}

/// Check if a hostname resolves to a private/reserved IP.
/// Uses DNS pinning cache to prevent rebinding attacks.
pub fn is_private_host(host: &str) -> bool {
    let ips = resolve_hostname(host);
    if ips.is_empty() {
        return true; // Treat unresolvable hosts as private (fail closed)
    }
    ips.iter().any(is_private_ip)
}

/// Check if an IP address is private, reserved, or link-local.
pub fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(addr) => {
            addr.is_private()
                || addr.is_loopback()
                || addr.is_link_local()
                || addr.is_broadcast()
                || addr.is_unspecified()
                || addr.octets()[0] == 100 && (addr.octets()[1] & 0b1100_0000 == 0b0100_0000)
                || addr.octets()[0] == 10
                || addr.octets()[0] == 172 && (addr.octets()[1] & 0b1111_0000 == 0b0001_0000)
                || addr.octets()[0] == 192 && addr.octets()[1] == 168
        },
        IpAddr::V6(addr) => {
            addr.is_loopback()
                || addr.is_unspecified()
                || addr.segments()[0] == 0xfc00
                || addr.segments()[0] == 0xfe80
        },
    }
}

/// Validate that connecting to a host is safe (not SSRF).
/// Checks DNS pinning cache, private IPs, and blocked ranges.
pub fn validate_external_host(host: &str) -> Result<(), SecurityError> {
    if host.is_empty() || host.len() > 253 {
        return Err(SecurityError::InvalidDomain);
    }
    if is_private_host(host) {
        return Err(SecurityError::MaliciousContent);
    }
    Ok(())
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
    fn test_hmac_verification() {
        let body = b"{\"test\": true}";
        let secret = b"my-secret-key";
        let sig_hex: String = hmac_sha256(secret, body)
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        assert!(verify_hmac_signature(body, &sig_hex, secret));
        assert!(!verify_hmac_signature(body, "invalid", secret));
    }

    #[test]
    fn test_hmac_known_vector() {
        // RFC 4231 Test Case 2
        let key = b"Jefe";
        let data = b"what do ya want for nothing?";
        let expected = "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843";
        let result: String = hmac_sha256(key, data)
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_extract_hmac_signature() {
        assert_eq!(extract_hmac_signature("sha256=abc123"), Some("abc123"));
        assert_eq!(extract_hmac_signature("abc123"), Some("abc123"));
    }

    #[test]
    fn test_private_ip_detection() {
        use std::net::IpAddr;
        let loopback: IpAddr = "127.0.0.1".parse().unwrap();
        let private: IpAddr = "10.0.0.1".parse().unwrap();
        let public: IpAddr = "8.8.8.8".parse().unwrap();
        let local_v6: IpAddr = "::1".parse().unwrap();

        assert!(is_private_ip(&loopback));
        assert!(is_private_ip(&private));
        assert!(!is_private_ip(&public));
        assert!(is_private_ip(&local_v6));
    }

    #[test]
    fn test_validate_external_host() {
        assert!(validate_external_host("example.com").is_ok());
        assert!(validate_external_host("google.com").is_ok());
        assert!(validate_external_host("").is_err());
    }

    #[test]
    fn test_sanitize_telegram_text() {
        assert_eq!(sanitize_telegram_text("<script>"), "&lt;script&gt;");
        assert_eq!(sanitize_telegram_text("normal text"), "normal text");
    }
}
