// Tests for database operations

mod common;

use common::setup_test_db;

#[tokio::test]
async fn test_user_config_creation() {
    let db = setup_test_db().await;
    let user_id = 12345;

    // Get or create user config
    let config = db.get_user_config(user_id).await.unwrap();
    assert_eq!(config.user_id, user_id);
    assert_eq!(config.language, "en"); // Default language
}

#[tokio::test]
async fn test_user_config_update() {
    let db = setup_test_db().await;
    let user_id = 12345;

    // Update language
    let mut config = db.get_user_config(user_id).await.unwrap();
    config.language = "it".to_string();
    db.save_user_config(&config).await.unwrap();

    let updated_config = db.get_user_config(user_id).await.unwrap();
    assert_eq!(updated_config.language, "it");
}

#[tokio::test]
async fn test_statistics_tracking() {
    let db = setup_test_db().await;
    let user_id = 12345;

    // Initial state
    let config = db.get_user_config(user_id).await.unwrap();
    assert_eq!(config.cleaned_count, 0);

    // Increment stats
    db.increment_cleaned_count(user_id, 1).await.unwrap();
    db.increment_cleaned_count(user_id, 1).await.unwrap();

    let updated_config = db.get_user_config(user_id).await.unwrap();
    assert_eq!(updated_config.cleaned_count, 2);
}

#[tokio::test]
async fn test_history_tracking() {
    let db = setup_test_db().await;
    let user_id = 12345;

    let original = "https://example.com?utm_source=test";
    let cleaned = "https://example.com";
    let provider_name = "RegexRules"; // Renamed to provider_name

    // Add to history
    db.log_cleaned_link(user_id, original, cleaned, provider_name)
        .await
        .unwrap();

    // Retrieve history
    let history = db.get_history(user_id, 10).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].original_url, original);
    assert_eq!(history[0].cleaned_url, cleaned);
    assert_eq!(history[0].provider_name, Some(provider_name.to_string()));
}

#[tokio::test]
async fn test_whitelist_operations() {
    let db = setup_test_db().await;
    let user_id = 12345;
    let domain = "trusted.example.com";

    // Add to whitelist
    db.add_to_whitelist(user_id, domain).await.unwrap();

    // Check if whitelisted
    let is_whitelisted = db.is_whitelisted(user_id, domain).await.unwrap();
    assert!(is_whitelisted);

    // Get whitelist
    let whitelist = db.get_whitelist(user_id).await.unwrap();
    assert_eq!(whitelist.len(), 1);
    assert_eq!(whitelist[0], domain);

    // Remove from whitelist
    db.remove_from_whitelist(user_id, domain).await.unwrap();
    let is_still_whitelisted = db.is_whitelisted(user_id, domain).await.unwrap();
    assert!(!is_still_whitelisted);
}

#[tokio::test]
async fn test_top_users_leaderboard() {
    let db = setup_test_db().await;

    // Create multiple users with different link counts
    for user_id in 1..=5 {
        for _ in 0..user_id {
            db.increment_cleaned_count(user_id as i64, 1).await.unwrap();
        }
    }

    // Get top users
    let top_users = db.get_top_users(3).await.unwrap();
    assert_eq!(top_users.len(), 3);

    // Should be sorted by link count descending
    assert!(top_users[0].1 >= top_users[1].1);
    assert!(top_users[1].1 >= top_users[2].1);
}

#[tokio::test]
async fn test_global_stats() {
    let db = setup_test_db().await;

    // Create some activity
    for user_id in 1..=3 {
        // Ensure user config exists before incrementing count
        db.get_user_config(user_id).await.unwrap();
        db.increment_cleaned_count(user_id, 1).await.unwrap();
    }

    let global_stats = db.get_global_stats().await.unwrap();
    assert_eq!(global_stats.1, 3); // total_users
    assert_eq!(global_stats.0, 3); // total_cleaned_links (formerly total_links)
}

#[tokio::test]
async fn test_feature_flags() {
    let db = setup_test_db().await;
    let user_id = 12345;

    // Enable a feature
    db.set_feature_flag(user_id, "ai_engine", true)
        .await
        .unwrap();

    let is_enabled = db.is_feature_enabled(user_id, "ai_engine").await.unwrap();
    assert!(is_enabled);

    // Disable feature
    db.set_feature_flag(user_id, "ai_engine", false)
        .await
        .unwrap();
    let is_still_enabled = db.is_feature_enabled(user_id, "ai_engine").await.unwrap();
    assert!(!is_still_enabled);
}
