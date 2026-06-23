//! URL normalization using url-normalize.
//!
//! Canonicalizes URLs before sanitization:
//! - Lowercases hostname
//! - Removes default ports, www, trailing slashes
//! - Sorts query parameters
//! - Removes UTM parameters
//! - Decodes unnecessary percent-encoding

use tracing::warn;
use url_normalize::{normalize_url, Options, QueryFilter, RemoveQueryParameters};

/// Default normalization options.
/// - Removes UTM tracking parameters
/// - Sorts remaining query parameters
/// - Removes www prefix
/// - Removes default ports
/// - Lowercases hostname
pub fn default_options() -> Options {
    Options {
        remove_query_parameters: RemoveQueryParameters::List(vec![
            QueryFilter::Exact("utm_source".into()),
            QueryFilter::Exact("utm_medium".into()),
            QueryFilter::Exact("utm_campaign".into()),
            QueryFilter::Exact("utm_term".into()),
            QueryFilter::Exact("utm_content".into()),
            QueryFilter::Exact("utm_id".into()),
        ]),
        sort_query_parameters: true,
        strip_www: true,
        strip_authentication: true,
        remove_trailing_slash: true,
        ..Options::default()
    }
}

/// Aggressive normalization - removes ALL query parameters.
/// Useful as a maximum-privacy mode.
pub fn aggressive_options() -> Options {
    Options {
        remove_query_parameters: RemoveQueryParameters::All,
        sort_query_parameters: true,
        strip_www: true,
        strip_authentication: true,
        remove_trailing_slash: true,
        ..Options::default()
    }
}

/// Normalize a URL with default options.
///
/// Returns the normalized URL on success, or the original on failure.
pub fn normalize(url: &str) -> String {
    normalize_with_options(url, &default_options())
}

/// Normalize a URL with custom options.
///
/// Returns the normalized URL on success, or the original on failure.
pub fn normalize_with_options(url: &str, options: &Options) -> String {
    match normalize_url(url, options) {
        Ok(normalized) => normalized,
        Err(e) => {
            warn!(error = %e, url = %url, "URL normalization failed, using original");
            url.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_lowercase_host() {
        let result = normalize("https://Example.COM/Page");
        assert_eq!(result, "https://example.com/Page");
    }

    #[test]
    fn test_normalize_remove_www() {
        let result = normalize("https://www.example.com/page");
        assert_eq!(result, "https://example.com/page");
    }

    #[test]
    fn test_normalize_remove_default_port() {
        let result = normalize("https://example.com:443/page");
        assert_eq!(result, "https://example.com/page");
    }

    #[test]
    fn test_normalize_remove_utm() {
        let result = normalize("https://example.com/page?utm_source=test&foo=bar");
        assert!(result.contains("foo=bar"));
        assert!(!result.contains("utm_source"));
    }

    #[test]
    fn test_normalize_sort_params() {
        let result = normalize("https://example.com/page?z=last&a=first");
        let pos_a = result.find("a=first").unwrap_or(0);
        let pos_z = result.find("z=last").unwrap_or(0);
        assert!(pos_a < pos_z, "params should be sorted: {}", result);
    }

    #[test]
    fn test_normalize_remove_trailing_slash() {
        let result = normalize("https://example.com/page/");
        assert_eq!(result, "https://example.com/page");
    }

    #[test]
    fn test_normalize_relative_url() {
        // Without a scheme, url-normalize may prepend http://
        // so we accept either the original or the http-prefixed version
        let result = normalize("not a url");
        assert!(result == "not a url" || result.starts_with("http://"));
    }

    #[test]
    fn test_aggressive_removes_all_params() {
        let result = normalize_with_options(
            "https://example.com/page?foo=bar&baz=qux",
            &aggressive_options(),
        );
        assert_eq!(result, "https://example.com/page");
    }
}
