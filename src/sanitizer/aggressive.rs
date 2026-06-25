use url::Url;

/// Parameters that are essential for page functionality.
/// Everything else is stripped in aggressive mode.
const FUNCTIONAL_PARAMS: &[&str] = &[
    "id", "page", "p", "q", "query", "search", "sort", "order", "filter",
    "category", "lang", "locale", "slug", "v", "t", "next", "prev",
    "offset", "limit", "per_page", "count", "start", "index", "page_size",
    "redirect", "return", "url", "path", "tab", "section",
];

/// Returns `true` if the parameter name is considered functional.
pub fn is_functional_param(name: &str) -> bool {
    let name_lower = name.to_ascii_lowercase();
    FUNCTIONAL_PARAMS
        .iter()
        .any(|&f| name_lower == f || name_lower.starts_with(&format!("{f}_")))
}

/// Aggressively sanitize a URL by removing ALL query parameters
/// except those in the FUNCTIONAL_PARAMS whitelist.
///
/// Returns `Some(cleaned_url)` if params were removed, `None` otherwise.
pub fn sanitize_aggressive(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;

    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    if pairs.is_empty() {
        return None;
    }

    let keep: Vec<&(String, String)> = pairs
        .iter()
        .filter(|(name, _)| is_functional_param(name))
        .collect();

    if keep.len() == pairs.len() {
        return None;
    }

    let mut clean_url = parsed.clone();
    clean_url.query_pairs_mut().clear();
    for (name, value) in &keep {
        clean_url
            .query_pairs_mut()
            .append_pair(name, value);
    }

    let cleaned = clean_url.to_string();
    if cleaned != url { Some(cleaned) } else { None }
}

/// Extract the list of removed parameter names between two URLs.
/// Returns `None` if no params were removed or URLs can't be parsed.
pub fn extract_removed_params(original: &str, cleaned: &str) -> Option<Vec<String>> {
    let orig = Url::parse(original).ok()?;
    let clean = Url::parse(cleaned).ok()?;

    let orig_params: std::collections::HashSet<String> = orig
        .query_pairs()
        .map(|(k, _)| k.to_string())
        .collect();

    let clean_params: std::collections::HashSet<String> = clean
        .query_pairs()
        .map(|(k, _)| k.to_string())
        .collect();

    let removed: Vec<String> = orig_params
        .difference(&clean_params)
        .cloned()
        .collect();

    if removed.is_empty() { None } else { Some(removed) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_functional_params_kept() {
        assert!(is_functional_param("id"));
        assert!(is_functional_param("q"));
        assert!(is_functional_param("page"));
        assert!(is_functional_param("lang"));
        assert!(is_functional_param("page_size"));
        assert!(is_functional_param("id_product"));
    }

    #[test]
    fn test_tracking_params_not_functional() {
        assert!(!is_functional_param("utm_source"));
        assert!(!is_functional_param("fbclid"));
        assert!(!is_functional_param("gclid"));
        assert!(!is_functional_param("ref"));
    }

    #[test]
    fn test_sanitize_aggressive_removes_tracking() {
        let url = "https://example.com/page?id=123&utm_source=news&fbclid=abc";
        let result = sanitize_aggressive(url);
        assert!(result.is_some());
        let cleaned = result.unwrap();
        assert!(cleaned.contains("id=123"));
        assert!(!cleaned.contains("utm_source"));
        assert!(!cleaned.contains("fbclid"));
    }

    #[test]
    fn test_sanitize_aggressive_noop_for_clean() {
        let url = "https://example.com/page";
        assert!(sanitize_aggressive(url).is_none());
    }

    #[test]
    fn test_sanitize_aggressive_noop_for_functional_only() {
        let url = "https://example.com/page?id=123&q=hello";
        assert!(sanitize_aggressive(url).is_none());
    }

    #[test]
    fn test_extract_removed_params() {
        let original = "https://example.com/page?a=1&utm_source=x&fbclid=y";
        let cleaned = "https://example.com/page?a=1";
        let removed = extract_removed_params(original, cleaned);
        assert!(removed.is_some());
        let names = removed.unwrap();
        assert!(names.contains(&"utm_source".to_string()));
        assert!(names.contains(&"fbclid".to_string()));
        assert!(!names.contains(&"a".to_string()));
    }

    #[test]
    fn test_extract_removed_params_none() {
        assert!(extract_removed_params("https://example.com/page", "https://example.com/page").is_none());
    }

    #[test]
    fn test_extract_removed_params_invalid() {
        assert!(extract_removed_params("not a url", "also not").is_none());
    }
}
