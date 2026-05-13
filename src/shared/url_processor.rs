//! URL cleaning and security scanning coordination logic.
//!
//! Handles the orchestration of URL expansion, security checks, and sanitization.

use crate::db::models::CustomRule;
use crate::sanitizer::RuleEngine;
use crate::shared::security::is_safe_url_scheme;
use std::collections::HashSet;
use teloxide::utils::html;

/// Represents a cleaned URL result with metadata.
#[derive(Debug, Clone)]
pub struct CleanedUrlInfo {
    /// Original URL as provided
    pub original_url: String,
    /// Expanded URL (after following redirects)
    pub expanded_url: String,
    /// Final cleaned URL
    pub cleaned_url: String,
    /// Provider that performed the cleaning
    pub provider: String,
    /// Number of parameters removed
    pub params_removed: usize,
}

/// Configuration for URL cleaning process.
pub struct UrlCleaningConfig {
    pub ai_enabled: bool,
    pub ignore_whitelist: bool,
    pub max_urls: usize,
}

impl Default for UrlCleaningConfig {
    fn default() -> Self {
        Self {
            ai_enabled: false,
            ignore_whitelist: false,
            max_urls: 10,
        }
    }
}

/// Process and clean a single URL.
///
/// # Returns
/// `Some(CleanedUrlInfo)` if URL was cleaned, `None` if no security warnings detected or no cleaning needed.
pub async fn process_single_url(
    original_url: &str,
    rules: &RuleEngine,
    custom_rules: &[CustomRule],
    ignored_domains: &[String],
) -> Option<CleanedUrlInfo> {
    // Expand shortened URLs
    let expanded_url = rules.expand_url(original_url).await;

    // Check if URL format is supported by ClearURLs
    if !rules.is_supported_by_clearurls(&expanded_url) {
        return None;
    }

    // Apply sanitization
    let (cleaned_url, provider) = match rules.sanitize(&expanded_url, custom_rules, ignored_domains)
    {
        Some((url, prov)) => (url, prov),
        None => return None,
    };

    let params_removed = count_removed_params(&expanded_url, &cleaned_url);

    Some(CleanedUrlInfo {
        original_url: original_url.to_string(),
        expanded_url,
        cleaned_url,
        provider,
        params_removed,
    })
}

/// Count the number of query parameters removed between original and cleaned URLs.
pub fn count_removed_params(original: &str, cleaned: &str) -> usize {
    let original_params = extract_query_params(original).len();
    let cleaned_params = extract_query_params(cleaned).len();
    (original_params as i32 - cleaned_params as i32).max(0) as usize
}

/// Extract all query parameters from a URL.
fn extract_query_params(url: &str) -> Vec<String> {
    if let Ok(parsed) = url::Url::parse(url) {
        parsed.query_pairs().map(|(k, _)| k.to_string()).collect()
    } else {
        Vec::new()
    }
}

/// Format a cleaned URL for display in Telegram message.
///
/// Uses HTML links if safe, otherwise code blocks.
pub fn format_url_for_display(url: &str) -> String {
    let escaped = html::escape(url);
    if is_safe_url_scheme(url) {
        format!("<a href=\"{escaped}\">{escaped}</a>")
    } else {
        format!("<code>{escaped}</code>")
    }
}

/// Generate HTML formatted response for multiple cleaned URLs.
pub fn build_cleaned_urls_response(
    cleaned_infos: &[CleanedUrlInfo],
    prefix: &str,
    max_length: usize,
) -> String {
    let mut response = prefix.to_string();

    if cleaned_infos.len() == 1 {
        let info = &cleaned_infos[0];
        response.push_str(&format!(
            "• {}\n",
            format_url_for_display(&info.cleaned_url)
        ));
    } else {
        for (idx, info) in cleaned_infos.iter().enumerate() {
            let connector = if idx == cleaned_infos.len() - 1 {
                "└─"
            } else {
                "├─"
            };
            let line = format!(
                "{} {}\n",
                connector,
                format_url_for_display(&info.cleaned_url)
            );

            if response.len() + line.len() > max_length {
                response.push_str("└─ <i>... e altri URL</i>\n");
                response.push_str("<i>Messaggio troncato per lunghezza</i>");
                break;
            }
            response.push_str(&line);
        }
    }

    response
}

/// Deduplicate URLs based on expanded form.
pub fn deduplicate_urls(urls: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    urls.into_iter()
        .filter(|url| seen.insert(url.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_removed_params() {
        let original = "https://example.com?utm_source=test&foo=bar";
        let cleaned = "https://example.com?foo=bar";
        assert_eq!(count_removed_params(original, cleaned), 1);
    }

    #[test]
    fn test_count_removed_params_none() {
        let url = "https://example.com?foo=bar";
        assert_eq!(count_removed_params(url, url), 0);
    }

    #[test]
    fn test_deduplicate_urls() {
        let urls = vec![
            "https://example.com".to_string(),
            "https://example.com".to_string(),
            "https://other.com".to_string(),
        ];
        let deduped = deduplicate_urls(urls);
        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn test_format_url_safe() {
        let url = "https://example.com";
        let formatted = format_url_for_display(url);
        assert!(formatted.contains("<a href="));
    }
}
