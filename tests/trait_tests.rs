//! Tests using mockall-generated mocks from the port traits.
//! These verify that the trait interfaces are correct and that mock
//! objects can be used in place of real implementations.
//!
//! Run with: cargo test --features test-utils --test trait_tests

use url_cleanse_bot::db::models::{UserConfig, CustomRule};
use url_cleanse_bot::shared::ports::{
    MockDatabasePort, DatabasePort,
    MockSanitizerService, SanitizerService,
    MockAiProvider, AiProvider,
    MockRedirectProvider, RedirectProvider,
};

// ── DatabasePort mock tests ────────────────────────────────

#[tokio::test]
async fn test_mock_database_get_user_config() {
    let mut mock = MockDatabasePort::new();
    let user_id = 42i64;

    mock.expect_get_user_config()
        .withf(move |id| *id == user_id)
        .returning(move |_| {
            Ok(UserConfig {
                user_id,
                cleaned_count: 10,
                ..Default::default()
            })
        });

    let config = mock.get_user_config(user_id).await.unwrap();
    assert_eq!(config.user_id, user_id);
    assert_eq!(config.cleaned_count, 10);
}

#[tokio::test]
async fn test_mock_database_increment_cleaned_count() {
    let mut mock = MockDatabasePort::new();

    mock.expect_increment_cleaned_count()
        .withf(|id| *id == 42)
        .returning(|_| Ok(()));

    let result = mock.increment_cleaned_count(42).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_mock_database_custom_rules() {
    let mut mock = MockDatabasePort::new();

    mock.expect_get_custom_rules()
        .withf(|id| *id == 1)
        .returning(|_| {
            Ok(vec![CustomRule {
                id: 0,
                user_id: 1,
                pattern: "utm_".to_string(),
            }])
        });

    let rules = mock.get_custom_rules(1).await.unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].pattern, "utm_");
}

// ── SanitizerService mock tests ─────────────────────────────

#[tokio::test]
async fn test_mock_sanitizer_service() {
    let mut mock = MockSanitizerService::new();

    mock.expect_sanitize()
        .withf(|url, _| url == "https://example.com?utm_source=test")
        .returning(|_, _| {
            Some(url_cleanse_bot::sanitizer::pipeline::SanitizedUrl {
                original_url: "https://example.com?utm_source=test".into(),
                cleaned_url: "https://example.com".into(),
                provider: "Mock".into(),
                removed_params: 1,
                param_names: vec!["utm_source".into()],
            })
        });

    mock.expect_expand_url()
        .withf(|url| url == "https://short.url/abc")
        .returning(|_| "https://example.com/long".to_string());

    mock.expect_is_supported_by_clearurls()
        .withf(|url| url == "https://example.com")
        .returning(|_| true);

    let sanitized = mock.sanitize(
        "https://example.com?utm_source=test",
        &url_cleanse_bot::shared::ports::sanitizer::SanitizeConfig::default(),
    ).await;
    assert!(sanitized.is_some());
    assert_eq!(sanitized.unwrap().cleaned_url, "https://example.com");

    let expanded = mock.expand_url("https://short.url/abc").await;
    assert_eq!(expanded, "https://example.com/long");

    let supported = mock.is_supported_by_clearurls("https://example.com");
    assert!(supported);
}

// ── AiProvider mock tests ───────────────────────────────────

#[tokio::test]
async fn test_mock_ai_provider() {
    let mut mock = MockAiProvider::new();

    mock.expect_sanitize()
        .withf(|url| url == "https://example.com?ref=tracker")
        .returning(|_| Ok(Some("https://example.com".to_string())));

    mock.expect_is_enabled()
        .returning(|| true);

    let result = mock.sanitize("https://example.com?ref=tracker").await.unwrap();
    assert_eq!(result, Some("https://example.com".to_string()));
    assert!(mock.is_enabled());
}

// ── RedirectProvider mock tests ─────────────────────────────

#[tokio::test]
async fn test_mock_redirect_provider() {
    use url_cleanse_bot::redirects::{Frontend, FrontendSource, LookupHit};
    let mut mock = MockRedirectProvider::new();

    mock.expect_lookup()
        .withf(|url| url.contains("youtube.com"))
        .returning(|_| {
            vec![LookupHit {
                service: "youtube".into(),
                frontends: vec![Frontend {
                    service: "youtube".into(),
                    kind: "invidious".into(),
                    url: "https://invidious.example.com".into(),
                    source: FrontendSource::LibRedirect,
                }],
            }]
        });

    mock.expect_is_supported()
        .withf(|domain| domain == "youtube.com")
        .returning(|_| true);

    let hits = mock.lookup("https://www.youtube.com/watch?v=abc").await;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].service, "youtube");

    assert!(mock.is_supported("youtube.com"));
}

// ── Pipeline response builder tests ─────────────────────────

#[test]
fn test_build_response_text_single_url() {
    use url_cleanse_bot::sanitizer::pipeline::{SanitizedUrl, build_response_text};
    use url_cleanse_bot::i18n::get_translations;

    let tr = get_translations("en");
    let sanitized = vec![SanitizedUrl {
        original_url: "https://example.com?utm_source=test".into(),
        cleaned_url: "https://example.com".into(),
        provider: "ClearURLs".into(),
        removed_params: 1,
        param_names: vec!["utm_source".into()],
    }];

    let response = build_response_text(&sanitized, false, "User", &tr);
    assert!(response.contains("utm_source"));
    assert!(response.contains("https://example.com"));
}

#[test]
fn test_build_response_text_group_context() {
    use url_cleanse_bot::sanitizer::pipeline::{SanitizedUrl, build_response_text};
    use url_cleanse_bot::i18n::get_translations;

    let tr = get_translations("en");
    let sanitized = vec![SanitizedUrl {
        original_url: "https://example.com?tracker=1".into(),
        cleaned_url: "https://example.com".into(),
        provider: "ClearURLs".into(),
        removed_params: 1,
        param_names: vec!["tracker".into()],
    }];

    let response = build_response_text(&sanitized, true, "TestUser", &tr);
    assert!(response.contains("TestUser"));
}

#[test]
fn test_build_response_text_no_changes() {
    use url_cleanse_bot::sanitizer::pipeline::{SanitizedUrl, build_response_text};
    use url_cleanse_bot::i18n::get_translations;

    let tr = get_translations("en");
    let sanitized = vec![SanitizedUrl {
        original_url: "https://example.com".into(),
        cleaned_url: "https://example.com".into(),
        provider: "ClearURLs".into(),
        removed_params: 0,
        param_names: vec![],
    }];

    let response = build_response_text(&sanitized, false, "User", &tr);
    assert!(response.contains("cleaned"));
}
