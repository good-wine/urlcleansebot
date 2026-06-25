//! Integration tests using wiremock to simulate external HTTP services.
//! Tests the ClearURLs rules download and sanitization with mocked data.

use url_cleanse_bot::sanitizer::RuleEngine;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

const SAMPLE_RULES_JSON: &str = r#"{
  "providers": {
    "TestProvider": {
      "urlPattern": "test\\.com",
      "rules": ["utm_.*"],
      "exceptions": [],
      "rawRules": [],
      "redirections": [],
      "referralMarketing": [],
      "forceRedirection": false
    },
    "Amazon": {
      "urlPattern": "amazon\\.com",
      "rules": ["ref_?", "tag=.*"],
      "exceptions": [],
      "rawRules": [],
      "redirections": [],
      "referralMarketing": [],
      "forceRedirection": false
    }
  }
}"#;

#[tokio::test]
async fn test_rule_engine_downloads_from_wiremock() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/data.minify.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_RULES_JSON))
        .mount(&mock_server)
        .await;

    let url = format!("{}/data.minify.json", mock_server.uri());
    let engine = RuleEngine::new(&url).await.expect("Failed to create RuleEngine from wiremock data");

    // Verify provider was loaded - test.com URL should be recognized
    assert!(engine.is_supported_by_clearurls("https://test.com/page"));

    // Verify rules work
    let result = engine.sanitize("https://test.com/page?utm_source=twitter&foo=bar", &[], &[]);
    assert!(result.is_some(), "URL should be sanitized");
    if let Some((cleaned, provider)) = result {
        assert!(!cleaned.contains("utm_source"), "utm_source should be removed: {cleaned}");
        assert!(cleaned.contains("foo=bar"), "foo=bar should be preserved: {cleaned}");
        assert_eq!(provider, "TestProvider");
    }
}

#[tokio::test]
async fn test_rule_engine_handles_404() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/data.minify.json"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let url = format!("{}/data.minify.json", mock_server.uri());
    let result = RuleEngine::new(&url).await;
    assert!(result.is_err(), "Should fail with 404");
}

#[tokio::test]
async fn test_rule_engine_handles_invalid_json() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/data.minify.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
        .mount(&mock_server)
        .await;

    let url = format!("{}/data.minify.json", mock_server.uri());
    let result = RuleEngine::new(&url).await;
    assert!(result.is_err(), "Should fail with invalid JSON");
}

#[tokio::test]
async fn test_rule_engine_amazon_rules() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/data.minify.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_RULES_JSON))
        .mount(&mock_server)
        .await;

    let url = format!("{}/data.minify.json", mock_server.uri());
    let engine = RuleEngine::new(&url).await.unwrap();

    // Amazon URL with tracking
    let result = engine.sanitize(
        "https://www.amazon.com/product/dp/B08X6PZTKS?ref_=ast_sto_dp&th=1&psc=1",
        &[],
        &[],
    );
    assert!(result.is_some(), "Amazon URL should be sanitized");
    if let Some((cleaned, provider)) = result {
        assert_eq!(provider, "Amazon");
        assert!(cleaned.contains("B08X6PZTKS"), "Product ID should be preserved");
        assert!(!cleaned.contains("ref_"), "ref_ should be removed");
    }
}

#[tokio::test]
async fn test_rule_engine_with_custom_rules() {
    use url_cleanse_bot::db::models::CustomRule;

    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/data.minify.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_RULES_JSON))
        .mount(&mock_server)
        .await;

    let url = format!("{}/data.minify.json", mock_server.uri());
    let engine = RuleEngine::new(&url).await.unwrap();

    let custom_rules = vec![CustomRule {
        id: 0,
        user_id: 1,
        pattern: "fbclid".to_string(),
    }];

    // URL with both utm (from rules) and fbclid (custom rule)
    let result = engine.sanitize(
        "https://test.com/page?utm_source=news&fbclid=abc123&keep=stay",
        &custom_rules,
        &[],
    );
    assert!(result.is_some(), "URL should be sanitized");
    if let Some((cleaned, _)) = result {
        assert!(!cleaned.contains("utm_source"), "utm_source should be removed");
        assert!(!cleaned.contains("fbclid"), "fbclid should be removed by custom rule");
        assert!(cleaned.contains("keep=stay"), "keep should be preserved");
    }
}
