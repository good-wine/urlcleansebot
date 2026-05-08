//! Input validation and sanitization utilities for enhanced security.
//!
//! Provides comprehensive validation for URLs, command parameters, and callback data.

use regex::Regex;
use url::Url;
use crate::shared::error::{AppError, AppResult};

/// Maximum allowed URL length (typically 2048 bytes for URLs)
const MAX_URL_LENGTH: usize = 2048;
/// Maximum allowed domain length
const MAX_DOMAIN_LENGTH: usize = 255;
/// Maximum allowed command parameter length
const MAX_PARAM_LENGTH: usize = 500;

/// Validates and sanitizes a URL input.
///
/// # Returns
/// - `Ok(String)` if URL is valid
/// - `Err(AppError)` if validation fails
pub fn validate_url(url: &str) -> AppResult<String> {
    // Check length
    if url.is_empty() {
        return Err(AppError::Validation("URL cannot be empty".to_string()));
    }

    if url.len() > MAX_URL_LENGTH {
        return Err(AppError::Validation(format!(
            "URL too long (max {} bytes)",
            MAX_URL_LENGTH
        )));
    }

    // Attempt to parse URL
    let parsed = Url::parse(url)
        .or_else(|_| Url::parse(&format!("https://{}", url)))
        .map_err(|e| AppError::Validation(format!("Invalid URL: {}", e)))?;

    // Check scheme
    let scheme = parsed.scheme();
    if !["http", "https", "ftp", "ftps"].contains(&scheme) {
        return Err(AppError::Validation(format!(
            "Unsupported URL scheme: {}",
            scheme
        )));
    }

    Ok(url.to_string())
}

/// Validates a domain name.
///
/// # Returns
/// - `Ok(String)` if domain is valid
/// - `Err(AppError)` if validation fails
pub fn validate_domain(domain: &str) -> AppResult<String> {
    if domain.is_empty() {
        return Err(AppError::Validation("Domain cannot be empty".to_string()));
    }

    if domain.len() > MAX_DOMAIN_LENGTH {
        return Err(AppError::Validation(format!(
            "Domain too long (max {} characters)",
            MAX_DOMAIN_LENGTH
        )));
    }

    // Basic domain regex: allows alphanumeric, hyphens, dots
    let domain_regex = Regex::new(r"^([a-zA-Z0-9]([a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,}$")
        .expect("Failed to compile domain regex");

    if !domain_regex.is_match(domain) {
        return Err(AppError::Validation(format!(
            "Invalid domain format: {}",
            domain
        )));
    }

    Ok(domain.to_lowercase())
}

/// Validates a command parameter (e.g., language code, setting value).
///
/// # Arguments
/// * `param` - The parameter to validate
/// * `allowed_chars` - Optional regex pattern for allowed characters
///
/// # Returns
/// - `Ok(String)` if parameter is valid
/// - `Err(AppError)` if validation fails
pub fn validate_parameter(param: &str, allowed_pattern: Option<&str>) -> AppResult<String> {
    if param.is_empty() {
        return Err(AppError::Validation("Parameter cannot be empty".to_string()));
    }

    if param.len() > MAX_PARAM_LENGTH {
        return Err(AppError::Validation(format!(
            "Parameter too long (max {} characters)",
            MAX_PARAM_LENGTH
        )));
    }

    if let Some(pattern) = allowed_pattern {
        let regex = Regex::new(pattern)
            .map_err(|e| AppError::Validation(format!("Failed to compile pattern: {}", e)))?;
        
        if !regex.is_match(param) {
            return Err(AppError::Validation(format!(
                "Parameter does not match required pattern"
            )));
        }
    }

    Ok(param.to_string())
}

/// Validates a language code (ISO 639-1 format).
pub fn validate_language_code(code: &str) -> AppResult<String> {
    validate_parameter(code, Some(r"^[a-z]{2}$")).map_err(|_| {
        AppError::Validation("Language code must be 2 lowercase letters (e.g., 'it', 'en')".to_string())
    })
}

/// Sanitizes HTML content for safe display in HTML messages.
///
/// Removes potentially dangerous content while preserving safe HTML.
pub fn sanitize_html_content(content: &str) -> String {
    // Remove script tags and their content
    let no_script = Regex::new(r"(?i)<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>").unwrap();
    let no_script = no_script.replace_all(content, "");

    // Remove event handlers
    let no_events = Regex::new(r#"(?i)\s*on\w+\s*=\s*"[^"]*""#).unwrap();
    let no_events = no_events.replace_all(&no_script, "");

    no_events.to_string()
}

/// Checks if content appears to be a phishing/malicious attempt.
pub fn detect_suspicious_content(content: &str) -> bool {
    let suspicious_patterns = [
        "confirm password",
        "verify account",
        "update payment",
        "urgent action required",
        "click here immediately",
    ];

    let content_lower = content.to_lowercase();
    suspicious_patterns
        .iter()
        .any(|pattern| content_lower.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_url_valid() {
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("http://example.com/path").is_ok());
    }

    #[test]
    fn test_validate_url_invalid_scheme() {
        assert!(validate_url("javascript:alert('xss')").is_err());
    }

    #[test]
    fn test_validate_url_too_long() {
        let long_url = format!("https://example.com/{}", "a".repeat(3000));
        assert!(validate_url(&long_url).is_err());
    }

    #[test]
    fn test_validate_domain_valid() {
        assert!(validate_domain("example.com").is_ok());
        assert!(validate_domain("sub.example.co.uk").is_ok());
    }

    #[test]
    fn test_validate_domain_invalid() {
        assert!(validate_domain("invalid domain").is_err());
        assert!(validate_domain("...com").is_err());
    }

    #[test]
    fn test_validate_language_code() {
        assert!(validate_language_code("it").is_ok());
        assert!(validate_language_code("en").is_ok());
        assert!(validate_language_code("ITA").is_err());
        assert!(validate_language_code("invalid").is_err());
    }

    #[test]
    fn test_sanitize_html_content() {
        let unsafe_html = "<script>alert('xss')</script><p>Safe</p>";
        let sanitized = sanitize_html_content(unsafe_html);
        assert!(!sanitized.contains("<script>"));
    }

    #[test]
    fn test_detect_suspicious_content() {
        assert!(detect_suspicious_content("Please confirm your password"));
        assert!(detect_suspicious_content("Urgent action required"));
        assert!(!detect_suspicious_content("Hello world"));
    }
}
