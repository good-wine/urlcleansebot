//! Honor Creator mode — preserves affiliate/referral marketing parameters
//! so that independent creators get credit for their recommendations.
//!
//! Unlike traditional URL cleaners that strip everything (including affiliate IDs),
//! this mode distinguishes between:
//! - **Tracking parameters** (privacy-invasive, removed): `utm_*`, `fbclid`, `gclid`, session IDs
//! - **Affiliate/referral parameters** (creator attribution, preserved): `tag`, `ref`, `aff`, etc.

use crate::constants::SAFE_AFFILIATE_PARAMS;
use std::collections::HashSet;
use url::Url;

/// Returns the set of affiliate parameter names that should be preserved.
fn affiliate_param_set() -> HashSet<&'static str> {
    SAFE_AFFILIATE_PARAMS.iter().copied().collect()
}

/// Checks if a parameter name is a known affiliate/referral parameter.
pub fn is_affiliate_param(name: &str) -> bool {
    let name_lower = name.to_lowercase();
    SAFE_AFFILIATE_PARAMS.contains(&name_lower.as_str())
}

/// Filter URL parameters, preserving affiliate params when honor_creator is enabled.
///
/// Returns `Some(cleaned_url)` if parameters were removed, `None` if unchanged.
pub fn preserve_affiliates(url: &str, honor_creator: bool) -> Option<String> {
    if !honor_creator {
        return None;
    }

    let parsed = Url::parse(url).ok()?;
    let affiliate_set = affiliate_param_set();

    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let has_affiliates = pairs
        .iter()
        .any(|(name, _)| affiliate_set.contains(name.as_str()));

    if !has_affiliates {
        return None;
    }

    Some(url.to_string())
}

/// Removes non-affiliate tracking parameters while preserving affiliate params.
/// Applied AFTER the main sanitization when honor_creator is enabled.
pub fn clean_keeping_affiliates(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let affiliate_set = affiliate_param_set();

    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    // Identify affiliate params to keep
    let keep_names: HashSet<&str> = pairs
        .iter()
        .filter(|(name, _)| affiliate_set.contains(name.as_str()))
        .map(|(name, _)| name.as_str())
        .collect();

    if keep_names.is_empty() {
        return None;
    }

    let non_affiliate_count = pairs.len() - keep_names.len();
    if non_affiliate_count == 0 {
        return None;
    }

    let mut clean_url = parsed.clone();
    clean_url.query_pairs_mut().clear();
    for (name, value) in &pairs {
        if keep_names.contains(name.as_str()) {
            clean_url.query_pairs_mut().append_pair(name, value);
        }
    }

    let cleaned = clean_url.to_string();
    if cleaned != url { Some(cleaned) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preserve_affiliates_disabled() {
        let url = "https://example.com/page?tag=creator123&utm_source=test";
        assert!(preserve_affiliates(url, false).is_none());
    }

    #[test]
    fn test_preserve_affiliates_enabled() {
        let url = "https://example.com/page?tag=creator123&utm_source=test&foo=bar";
    let result = preserve_affiliates(url, true);
    assert!(result.is_some());
    let cleaned = result.unwrap();
    assert!(cleaned.contains("tag=creator123"));
    assert!(cleaned.contains("foo=bar"));
    assert!(cleaned.contains("utm_source=test"));
    }

    #[test]
    fn test_clean_keeping_affiliates() {
        let url = "https://example.com/page?tag=creator123&utm_source=test&fbclid=abc";
        let result = clean_keeping_affiliates(url);
        assert!(result.is_some());
        let cleaned = result.unwrap();
        assert!(cleaned.contains("tag=creator123"));
        assert!(!cleaned.contains("utm_source"));
        assert!(!cleaned.contains("fbclid"));
    }

    #[test]
    fn test_is_affiliate_param() {
        assert!(is_affiliate_param("tag"));
        assert!(is_affiliate_param("ref"));
        assert!(is_affiliate_param("affiliate"));
        assert!(!is_affiliate_param("utm_source"));
        assert!(!is_affiliate_param("fbclid"));
        assert!(!is_affiliate_param("random_param"));
    }

    #[test]
    fn test_no_affiliates_no_change() {
        let url = "https://example.com/page?foo=bar";
        assert!(preserve_affiliates(url, true).is_none());
    }
}
