//! Entropy-based detection of unknown tracking parameters.
//!
//! Uses Shannon entropy to identify high-randomness parameter values
//! that are likely tracking identifiers, even if not in any known rule list.
//!
//! Based on research from PURL (USENIX Security 2024) and LinkGuard:
//! - ATS link decorations have Shannon entropy > 3.0 bits/char
//! - Functional parameters (ref, page, etc.) have entropy < 3.0 bits/char

use std::collections::HashMap;

/// Threshold above which a parameter value is considered tracking (bits/char).
/// Derived from PURL paper: >70% of non-tracking params have entropy < 3.0,
/// while >92% of tracking params have entropy >= 3.0.
pub const ENTROPY_THRESHOLD: f64 = 3.0;

/// Parameter names that are always kept regardless of entropy (functional).
pub const FUNCTIONAL_PARAMS: &[&str] = &[
    "q", "query", "search", "id", "page", "p", "offset", "limit", "sort", "order", "filter", "tag",
    "category", "lang", "locale", "ref", "redirect", "next", "prev", "slug", "v", "t",
];

/// Parameter names that are always removed (known trackers not in rules).
pub const KNOWN_TRACKERS: &[&str] = &[
    "_ga",
    "_gl",
    "_hsenc",
    "_hsmi",
    "hsCtaTracking",
    "openstapled",
    "affiliate",
    "aff",
    "ref_src",
    "ref_url",
];

/// Calculate Shannon entropy of a string (bits per character).
///
/// Shannon entropy measures the randomness/information content.
/// Higher values indicate more random strings (typical of tracking IDs).
pub fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let len = s.len() as f64;
    let mut freq = HashMap::new();
    for c in s.chars() {
        *freq.entry(c).or_insert(0usize) += 1;
    }
    -freq.values().fold(0.0, |sum, &count| {
        let p = count as f64 / len;
        sum + p * p.log2()
    })
}

/// Check if a parameter value looks like a tracking token based on entropy.
///
/// Returns `true` if the value has high entropy and doesn't match known
/// functional patterns (short values, common words, numbers, etc.).
pub fn is_likely_tracking(param_name: &str, param_value: &str) -> bool {
    if param_value.is_empty() || param_value.len() < 6 {
        return false;
    }

    let name_lower = param_name.to_lowercase();
    if FUNCTIONAL_PARAMS.iter().any(|&f| f == name_lower) {
        return false;
    }

    let entropy = shannon_entropy(param_value);

    // Check for UUID patterns
    if is_uuid_like(param_value) {
        return true;
    }

    // Check for base64-like padding
    if param_value.ends_with("==") || (param_value.ends_with('=') && param_value.len() > 10) {
        let entropy_no_padding = shannon_entropy(param_value.trim_end_matches('='));
        if entropy_no_padding > ENTROPY_THRESHOLD {
            return true;
        }
    }

    // Check for hex hash patterns (32+ hex chars)
    if param_value.len() >= 32 && param_value.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }

    entropy > ENTROPY_THRESHOLD
}

/// Detect UUID-like patterns (hex with dashes, or compact hex ≥32 chars).
fn is_uuid_like(s: &str) -> bool {
    let clean = s.replace('-', "");
    if clean.len() == 32 && clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    if s.len() == 36 {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() == 5
            && parts[0].len() == 8
            && parts[1].len() == 4
            && parts[2].len() == 4
            && parts[3].len() == 4
            && parts[4].len() == 12
        {
            return parts
                .iter()
                .all(|p| p.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }
    false
}

/// Extract parameters from a URL and return those likely to be tracking.
///
/// Returns a list of `(param_name, param_value, entropy)` tuples.
pub fn find_tracking_params(url: &str) -> Vec<(String, String, f64)> {
    let parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let mut tracking = Vec::new();
    for (name, value) in parsed.query_pairs() {
        let name = name.to_string();
        let value = value.to_string();

        if KNOWN_TRACKERS.iter().any(|&k| k == name) {
            tracking.push((name, value.clone(), shannon_entropy(&value)));
            continue;
        }

        if is_likely_tracking(&name, &value) {
            let entropy = shannon_entropy(&value);
            tracking.push((name, value, entropy));
        }
    }
    tracking
}

/// Remove tracking parameters from a URL based on entropy analysis.
///
/// Returns `Some(cleaned_url)` if parameters were removed, `None` otherwise.
pub fn remove_high_entropy_params(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let tracking = find_tracking_params(url);
    if tracking.is_empty() {
        return None;
    }

    let tracking_names: std::collections::HashSet<&str> =
        tracking.iter().map(|(n, _, _)| n.as_str()).collect();

    let mut clean_url = parsed.clone();
    let mut pairs: Vec<(String, String)> = clean_url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    pairs.retain(|(name, _)| !tracking_names.contains(name.as_str()));

    clean_url.query_pairs_mut().clear();
    for (name, value) in &pairs {
        clean_url.query_pairs_mut().append_pair(name, value);
    }

    let cleaned = clean_url.to_string();
    if cleaned != url { Some(cleaned) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn remove_high_entropy_does_not_break_url(domain in "[a-z]{3,10}", path in "[a-z0-9/]{0,30}") {
            let url = format!("https://{domain}.com/{path}");
            let result = super::remove_high_entropy_params(&url);
            // Some URLs may be rejected (None) if malformed
            if let Some(ref cleaned) = result {
                prop_assert!(cleaned.contains(&format!("{domain}.com")),
                    "Result should preserve the domain: got '{}'", cleaned);
            }
        }

        #[test]
        fn clean_urls_unchanged(domain in "[a-z]{3,10}\\.com", path in "(/[a-zA-Z0-9/_-]{0,20})?") {
            let url = format!("https://{domain}{path}");
            let result = super::remove_high_entropy_params(&url);
            // URLs without query params should remain unchanged (Some equal to input)
            if let Some(ref cleaned) = result {
                prop_assert_eq!(cleaned, &url, "URLs without query params should be unchanged");
            }
        }
    }

    #[test]
    fn test_shannon_entropy_empty() {
        assert_eq!(shannon_entropy(""), 0.0);
    }

    #[test]
    fn test_shannon_entropy_low() {
        let e = shannon_entropy("homepage");
        assert!(e < ENTROPY_THRESHOLD, "entropy={} >= threshold", e);
    }

    #[test]
    fn test_shannon_entropy_high() {
        let e = shannon_entropy("a1b2c3d4e5f6g7h8");
        assert!(e > ENTROPY_THRESHOLD, "entropy={} <= threshold", e);
    }

    #[test]
    fn test_uuid_detection() {
        assert!(is_uuid_like("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!is_uuid_like("not-a-uuid"));
    }

    #[test]
    fn test_hex_hash_detection() {
        assert!(is_likely_tracking(
            "sid",
            "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6"
        ));
        assert!(!is_likely_tracking("id", "12345"));
    }

    #[test]
    fn test_functional_params_not_tracked() {
        assert!(!is_likely_tracking("q", "some search query"));
        assert!(!is_likely_tracking("page", "2"));
        assert!(!is_likely_tracking("v", "dQw4w9WgXcQ"));
    }

    #[test]
    fn test_base64_detection() {
        assert!(is_likely_tracking("token", "dGhpcyBpcyBhIHRlc3Q=="));
    }

    #[test]
    fn test_find_tracking_params_in_url() {
        let url =
            "https://example.com/page?q=hello&sid=a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6&ref=home";
        let tracking = find_tracking_params(url);
        assert!(!tracking.is_empty());
        assert!(tracking.iter().any(|(n, _, _)| n == "sid"));
        assert!(!tracking.iter().any(|(n, _, _)| n == "q"));
        assert!(!tracking.iter().any(|(n, _, _)| n == "ref"));
    }

    #[test]
    fn test_remove_high_entropy_params() {
        let url = "https://example.com/page?sid=a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6&q=hello";
        let result = remove_high_entropy_params(url);
        assert!(result.is_some());
        let cleaned = result.unwrap();
        assert!(!cleaned.contains("sid"));
        assert!(cleaned.contains("q=hello"));
    }

    #[test]
    fn test_remove_noop_for_clean_url() {
        let url = "https://example.com/page?q=hello";
        assert!(remove_high_entropy_params(url).is_none());
    }

    #[test]
    fn test_known_trackers_detected() {
        assert!(is_likely_tracking("_ga", "GA1.2.123456789.1234567890"));
        assert!(is_likely_tracking("_gl", "1*abc123*_ga*xyz789"));
    }
}
