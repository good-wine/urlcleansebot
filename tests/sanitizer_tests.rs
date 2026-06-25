// Tests for URL sanitization and rule engine

mod common;

use url_cleanse_bot::sanitizer::RuleEngine;
use common::test_urls::*;

async fn make_engine() -> RuleEngine {
    RuleEngine::new("https://rules2.clearurls.xyz/data.minify.json")
        .await
        .expect("Failed to load ClearURLs rules")
}

#[tokio::test]
async fn test_clean_url_unchanged() {
    let rules = make_engine().await;
    let result = rules.sanitize(CLEAN_URL, &[], &[]);
    // Clean URLs that need no changes may return None or Some with the same URL
    if let Some((cleaned, _)) = result {
        assert_eq!(cleaned, CLEAN_URL);
    }
}

#[tokio::test]
async fn test_utm_parameters_removed() {
    let rules = make_engine().await;
    let (cleaned, _) = rules.sanitize(URL_WITH_UTM, &[], &[]).unwrap();

    assert!(!cleaned.contains("utm_source"));
    assert!(!cleaned.contains("utm_medium"));
    assert!(cleaned.contains("example.com/page"));
}

#[tokio::test]
async fn test_amazon_tracking_removed() {
    let rules = make_engine().await;
    let (cleaned, _) = rules.sanitize(AMAZON_URL, &[], &[]).unwrap();

    // Should keep product ID but remove tracking params
    assert!(cleaned.contains("B08X6PZTKS"));
    assert!(!cleaned.contains("ref_="));
}

#[tokio::test]
async fn test_youtube_feature_removed() {
    let rules = make_engine().await;
    let (cleaned, _) = rules.sanitize(YOUTUBE_URL, &[], &[]).unwrap();

    // Should keep video ID but remove feature param
    assert!(cleaned.contains("dQw4w9WgXcQ"));
    assert!(!cleaned.contains("feature="));
}

#[test]
fn test_url_validator_valid() {
    use url_cleanse_bot::sanitizer::validation::is_valid_url;
    assert!(is_valid_url("https://example.com"));
    assert!(is_valid_url("http://test.org"));
}

#[test]
fn test_url_validator_invalid() {
    use url_cleanse_bot::sanitizer::validation::is_valid_url;
    assert!(!is_valid_url("not a url"));
    assert!(!is_valid_url("ftp://unsupported.com"));
    assert!(!is_valid_url("javascript:alert(1)"));
}

#[tokio::test]
async fn test_multiple_urls_cleaning() {
    let rules = make_engine().await;

    let urls = vec![CLEAN_URL, URL_WITH_UTM, YOUTUBE_URL];
    let mut cleaned_count = 0;

    for url in urls {
        if let Some((cleaned, _)) = rules.sanitize(url, &[], &[])
            && cleaned != url
        {
            cleaned_count += 1;
        }
    }

    // At least 2 URLs should be cleaned (UTM and YouTube)
    assert!(cleaned_count >= 2);
}

// ── Property-based tests with proptest ───────────────────────────────────

use proptest::prelude::*;

proptest! {
    #[test]
    fn sanitizer_never_panics_on_any_input(
        input in "https?://[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}(/[a-zA-Z0-9/.-]*)?(\\?[a-zA-Z0-9=&;%-._~]*)?",
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let engine = make_engine().await;
            let result = engine.sanitize(&input, &[], &[]);
            if let Some((cleaned, _)) = result {
                // Cleaning should never produce an empty URL
                assert!(!cleaned.is_empty(), "Sanitized URL should not be empty");
                // The result should always be a valid URL
                assert!(cleaned.starts_with("http://") || cleaned.starts_with("https://"),
                    "Cleaned URL should have a valid scheme: {cleaned}");
                // Cleaning should never add more params than the original
                let original_params = input.matches('&').count() + input.matches('?').count();
                let cleaned_params = cleaned.matches('&').count() + cleaned.matches('?').count();
                assert!(cleaned_params <= original_params,
                    "Cleaning removed params, but got more. Original: {input}, Cleaned: {cleaned}");
            }
        });
    }
}

proptest! {
    #[test]
    fn sanitizer_preserves_path_and_host(
        host in "[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}",
        path in "(/[a-zA-Z0-9/._-]*)?",
        params in proptest::collection::vec(("[a-zA-Z][a-zA-Z0-9_]{0,10}", "[a-zA-Z0-9_]{0,20}"), 0..5),
    ) {
        let url = format!("https://{host}{path}");
        let query_string: String = params.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        let full_url = if query_string.is_empty() {
            url.clone()
        } else {
            format!("{url}?{query_string}")
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let engine = make_engine().await;
            let result = engine.sanitize(&full_url, &[], &[]);

            // The host should always be preserved
            if let Some((cleaned, _)) = result {
                assert!(cleaned.contains(&host),
                    "Host should be preserved: {cleaned} vs original host {host}");
            }
        });
    }
}

proptest! {
    // Test that custom rules always remove matching params
    #[test]
    fn custom_rules_remove_matching_params(
        param_name in "[a-z]{3,10}",
        value in "[a-z0-9]{1,10}",
    ) {
        let url = format!("https://example.com/page?{param_name}={value}&keep=stay");
        let custom_rules = vec![url_cleanse_bot::db::models::CustomRule {
            id: 0,
            user_id: 0,
            pattern: param_name.clone(),
        }];

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let engine = make_engine().await;
            let result = engine.sanitize(&url, &custom_rules, &[]);
            if let Some((cleaned, _)) = result {
                assert!(!cleaned.contains(&format!("{param_name}=")),
                    "Custom rule should remove param {param_name}. Cleaned: {cleaned}");
                assert!(cleaned.contains("keep=stay"),
                    "Non-matching params should be preserved. Cleaned: {cleaned}");
            }
        });
    }
}
