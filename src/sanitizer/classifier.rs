//! Lightweight ML-inspired parameter classifier.
//!
//! Uses multi-feature scoring to distinguish tracking parameters from
//! functional ones, extending the Shannon entropy approach from PURL
//! (USENIX Security 2024).
//!
//! Features considered:
//! - Shannon entropy (bits/char)
//! - Length distribution
//! - Character type ratios (uppercase, lowercase, digits, special)
//! - UUID / base64 / hex patterns
//! - Known functional parameter names
//! - Known tracking parameter name patterns

use crate::sanitizer::entropy::shannon_entropy;

/// Classification result for a parameter.
#[derive(Debug, Clone, PartialEq)]
pub enum Classification {
    /// Definitely tracking — should be removed.
    Tracking,
    /// Probably tracking — safe to remove.
    LikelyTracking,
    /// Uncertain — needs further analysis.
    Uncertain,
    /// Probably functional — keep.
    LikelyFunctional,
    /// Definitely functional — must keep.
    Functional,
}

/// Score between 0.0 (definitely functional) and 1.0 (definitely tracking).
pub type TrackingScore = f64;

/// Classification thresholds.
pub const TRACKING_THRESHOLD: f64 = 0.8;
pub const LIKELY_TRACKING_THRESHOLD: f64 = 0.45;
pub const UNCERTAIN_THRESHOLD: f64 = 0.35;
pub const LIKELY_FUNCTIONAL_THRESHOLD: f64 = 0.2;

/// Feature vector extracted from a parameter name+value pair.
#[derive(Debug, Clone)]
struct Features {
    entropy: f64,
    length: usize,
    _name_length: usize,
    uppercase_ratio: f64,
    digit_ratio: f64,
    special_ratio: f64,
    _has_dashes: bool,
    _has_underscores: bool,
    is_uuid_like: bool,
    is_hex_like: bool,
    is_base64_like: bool,
    is_numeric: bool,
    name_has_tracking_pattern: bool,
    name_is_functional: bool,
}

/// Extract feature vector from a parameter name and value.
fn extract_features(name: &str, value: &str) -> Features {
    let entropy = shannon_entropy(value);
    let len = value.len();
    let _name_len = name.len();

    let upper_count = value.chars().filter(|c| c.is_uppercase()).count();
    let digit_count = value.chars().filter(|c| c.is_ascii_digit()).count();
    let special_count = value
        .chars()
        .filter(|c| !c.is_alphanumeric())
        .count();
    let total = len.max(1) as f64;

    let uppercase_ratio = upper_count as f64 / total;
    let digit_ratio = digit_count as f64 / total;
    let special_ratio = special_count as f64 / total;

    let has_dashes = value.contains('-');
    let has_underscores = value.contains('_');

    let clean = value.replace(['-', '_'], "");
    let is_uuid_like = clean.len() == 32 && clean.chars().all(|c| c.is_ascii_hexdigit());
    let is_hex_like = len >= 16 && value.chars().all(|c| c.is_ascii_hexdigit());
    let is_base64_like = (len >= 12 && value.ends_with('='))
        || (len >= 12 && value.contains('/') && value.contains('+'));
    let is_numeric = !value.is_empty() && value.chars().all(|c| c.is_ascii_digit());

    let name_lower = name.to_lowercase();
    let tracking_patterns = [
        "click", "track", "session", "token", "visitor", "client",
        "analytics", "event", "campaign", "source", "medium",
        "content", "term", "gclid", "fbclid", "msclkid",
    ];
    let name_has_tracking_pattern = tracking_patterns
        .iter()
        .any(|p| name_lower.contains(p));

    let functional_names = [
        "q", "query", "search", "id", "page", "p", "offset", "limit",
        "sort", "order", "filter", "category", "lang", "locale",
        "slug", "v", "t", "next", "prev", "ref", "redirect",
    ];
    let name_is_functional = functional_names.contains(&name_lower.as_str());

    Features {
        entropy,
        length: len,
        _name_length: _name_len,
        uppercase_ratio,
        digit_ratio,
        special_ratio,
        _has_dashes: has_dashes,
        _has_underscores: has_underscores,
        is_uuid_like,
        is_hex_like,
        is_base64_like,
        is_numeric,
        name_has_tracking_pattern,
        name_is_functional,
    }
}

/// Compute tracking score from features.
/// Returns 0.0 (functional) to 1.0 (tracking).
fn compute_score(f: &Features) -> TrackingScore {
    let mut score = 0.0;
    let mut weight_sum = 0.0;

    // Entropy: >3.0 bits/char suggests tracking
    let entropy_weight = 3.0;
    weight_sum += entropy_weight;
    if f.entropy > 3.5 {
        score += entropy_weight * 1.0;
    } else if f.entropy > 3.0 {
        score += entropy_weight * 0.7;
    } else if f.entropy > 2.5 {
        score += entropy_weight * 0.3;
    }

    // Length: very long values (>32) are suspicious
    let len_weight = 1.0;
    weight_sum += len_weight;
    if f.length > 64 {
        score += len_weight * 1.0;
    } else if f.length > 32 {
        score += len_weight * 0.6;
    } else if f.length < 4 {
        score -= len_weight * 0.3; // Short values are more likely functional
    }

    // Character distribution: high digit or special ratio suggests encoded ID
    let char_weight = 1.5;
    weight_sum += char_weight;
    if f.digit_ratio > 0.6 && f.length >= 8 {
        score += char_weight * 0.7;
    }
    if f.special_ratio > 0.3 {
        score += char_weight * 0.5;
    }
    if f.uppercase_ratio > 0.4 && f.length >= 6 {
        score += char_weight * 0.4;
    }

    // UUID / hex / base64 detection (high weight for strong signals)
    let pattern_weight = 4.0;
    weight_sum += pattern_weight;
    if f.is_uuid_like || f.is_hex_like || f.is_base64_like {
        score += pattern_weight * 1.0;
    }

    // Name-based signals (high weight: tracking param names are very informative)
    let name_weight = 5.0;
    weight_sum += name_weight;
    if f.name_has_tracking_pattern {
        score += name_weight * 1.0;
    }
    if f.name_is_functional {
        score -= name_weight * 1.0;
    }

    // Short numeric parameters are typically functional (page=2, offset=10)
    let numeric_weight = 1.0;
    weight_sum += numeric_weight;
    if f.is_numeric && f.length < 6 {
        score -= numeric_weight * 0.7;
    }

    if weight_sum > 0.0 {
        f64::clamp(score / weight_sum, 0.0, 1.0)
    } else {
        0.5
    }
}

/// Classify a parameter as tracking or functional.
pub fn classify(name: &str, value: &str) -> Classification {
    let features = extract_features(name, value);
    let score = compute_score(&features);

    if score >= TRACKING_THRESHOLD {
        Classification::Tracking
    } else if score >= LIKELY_TRACKING_THRESHOLD {
        Classification::LikelyTracking
    } else if score >= UNCERTAIN_THRESHOLD {
        Classification::Uncertain
    } else if score >= LIKELY_FUNCTIONAL_THRESHOLD {
        Classification::LikelyFunctional
    } else {
        Classification::Functional
    }
}

/// Get a numeric tracking score for a parameter.
pub fn tracking_score(name: &str, value: &str) -> TrackingScore {
    let features = extract_features(name, value);
    compute_score(&features)
}

/// Filter URL parameters using the classifier.
/// Removes parameters classified as Tracking or LikelyTracking.
pub fn filter_tracking_params(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;

    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let original_count = pairs.len();
    let keep: Vec<_> = pairs
        .into_iter()
        .filter(|(name, value)| {
            let class = classify(name, value);
            !matches!(class, Classification::Tracking | Classification::LikelyTracking)
        })
        .collect();

    if keep.len() == original_count {
        return None;
    }

    let mut clean_url = parsed.clone();
    clean_url.query_pairs_mut().clear();
    for (name, value) in &keep {
        clean_url.query_pairs_mut().append_pair(name, value);
    }

    let cleaned = clean_url.to_string();
    if cleaned != url { Some(cleaned) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_tracking_uuid() {
        let class = classify("sid", "550e8400-e29b-41d4-a716-446655440000");
        assert!(matches!(class, Classification::Tracking | Classification::LikelyTracking));
    }

    #[test]
    fn test_classify_tracking_hex() {
        let class = classify("token", "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6");
        assert!(matches!(class, Classification::Tracking | Classification::LikelyTracking));
    }

    #[test]
    fn test_classify_functional_search() {
        let class = classify("q", "hello world");
        assert!(matches!(class, Classification::Functional | Classification::LikelyFunctional));
    }

    #[test]
    fn test_classify_functional_page() {
        let class = classify("page", "2");
        assert!(matches!(class, Classification::Functional | Classification::LikelyFunctional));
    }

    #[test]
    fn test_classify_uncertain() {
        let class = classify("name", "john");
        assert!(matches!(class, Classification::Functional | Classification::LikelyFunctional));
        // "john" has low entropy and no pattern signals; the classification
        // depends on how the weighted features balance out.
    }

    #[test]
    fn test_filter_tracking_params() {
        let url = "https://example.com/page?sid=a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6&q=hello";
        let result = filter_tracking_params(url);
        assert!(result.is_some());
        let cleaned = result.unwrap();
        assert!(!cleaned.contains("sid"));
        assert!(cleaned.contains("q=hello"));
    }

    #[test]
    fn test_tracking_score_range() {
        let score = tracking_score("sid", "550e8400-e29b-41d4-a716-446655440000");
        assert!((0.0..=1.0).contains(&score));
        let score2 = tracking_score("q", "hello");
        assert!((0.0..=1.0).contains(&score2));
        assert!(score > score2, "UUID should score higher than search query");
    }

    #[test]
    fn test_filter_noop_for_clean() {
        let url = "https://example.com/page?q=hello";
        assert!(filter_tracking_params(url).is_none());
    }

    #[test]
    fn test_base64_tracking() {
        let class = classify("data", "dGhpcyBpcyBhIHRlc3Q=");
        assert!(matches!(class, Classification::Tracking | Classification::LikelyTracking));
    }

    #[test]
    fn test_tracking_name_pattern() {
        let class = classify("click_id", "12345abcde");
        assert!(matches!(class, Classification::LikelyTracking | Classification::Tracking | Classification::Uncertain));
    }
}
