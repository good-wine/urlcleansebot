// Tests for URL sanitization and rule engine

mod common;

use clear_urls_bot::sanitizer::RuleEngine;
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
    use clear_urls_bot::sanitizer::validation::is_valid_url;
    assert!(is_valid_url("https://example.com"));
    assert!(is_valid_url("http://test.org"));
}

#[test]
fn test_url_validator_invalid() {
    use clear_urls_bot::sanitizer::validation::is_valid_url;
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
        if let Some((cleaned, _)) = rules.sanitize(url, &[], &[]) {
            if cleaned != url {
                cleaned_count += 1;
            }
        }
    }

    // At least 2 URLs should be cleaned (UTM and YouTube)
    assert!(cleaned_count >= 2);
}
