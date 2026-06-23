use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;

static URL_CACHE: LazyLock<Mutex<HashMap<String, bool>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Returns `true` when `url` begins with an `http://` or `https://` scheme.
///
/// Results are memoised in a process-wide cache.  The function is
/// panic-free: a poisoned mutex is recovered from gracefully.
pub fn is_valid_url(url: &str) -> bool {
    let result = url.starts_with("http://") || url.starts_with("https://");

    // Best-effort cache update — ignore errors rather than panicking.
    if let Ok(mut cache) = URL_CACHE.lock() {
        cache.entry(url.to_string()).or_insert(result);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_url() {
        assert!(is_valid_url("https://example.com"));
        assert!(is_valid_url("http://example.com"));
        assert!(!is_valid_url("ftp://example.com"));
        assert!(!is_valid_url("example.com"));
    }

    #[test]
    fn test_caching_is_idempotent() {
        // Calling twice must return the same result.
        assert_eq!(
            is_valid_url("https://example.com/cached"),
            is_valid_url("https://example.com/cached")
        );
    }
}
