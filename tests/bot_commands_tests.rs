// Integration tests for bot command handlers

mod common;

#[cfg(test)]
mod command_tests {
    use super::common::*;

    #[tokio::test]
    async fn test_start_command() {
        // Test that start command initializes user config
        let db = setup_test_db().await;
        let user_id = 99999;

        // Simulate /start command processing
        let config = db.get_user_config(user_id).await.unwrap();
        assert_eq!(config.user_id, user_id);
    }

    #[tokio::test]
    async fn test_stats_command() {
        let db = setup_test_db().await;
        let user_id = 12345;

        // Ensure user config exists
        db.get_user_config(user_id).await.unwrap();
        // Simulate some activity
        db.increment_cleaned_count(user_id, 1).await.unwrap();
        db.increment_cleaned_count(user_id, 1).await.unwrap();

        let config = db.get_user_config(user_id).await.unwrap();
        assert_eq!(config.cleaned_count, 2);
    }

    #[tokio::test]
    async fn test_whitelist_command_flow() {
        let db = setup_test_db().await;
        let user_id = 12345;
        let domain = "example.com";

        // Add domain
        db.add_to_whitelist(user_id, domain).await.unwrap();
        assert!(db.is_whitelisted(user_id, domain).await.unwrap());

        // Show whitelist
        let list = db.get_whitelist(user_id).await.unwrap();
        assert!(list.contains(&domain.to_string()));

        // Remove domain
        db.remove_from_whitelist(user_id, domain).await.unwrap();
        assert!(!db.is_whitelisted(user_id, domain).await.unwrap());
    }

    #[tokio::test]
    async fn test_history_command() {
        let db = setup_test_db().await;
        let user_id = 12345;

        // Add some history entries
        for i in 1..=5 {
            let original = format!("https://example.com?utm_source={}", i);
            let cleaned = "https://example.com";
            let provider_name = "RegexRules";
            db.log_cleaned_link(user_id, &original, cleaned, provider_name)
                .await
                .unwrap();
        }

        // Get history
        let history = db.get_history(user_id, 10).await.unwrap();
        assert_eq!(history.len(), 5);
    }

    #[tokio::test]
    async fn test_export_command() {
        let db = setup_test_db().await;
        let user_id = 12345;

        // Add history for export
        db.log_cleaned_link(
            user_id,
            "https://example.com?track=1",
            "https://example.com",
            "RegexRules",
        )
        .await
        .unwrap();

        let history = db.get_history(user_id, 50).await.unwrap();
        assert!(!history.is_empty());

        // Simulate JSON export
        let json = serde_json::to_string_pretty(&history).unwrap();
        assert!(json.contains("example.com"));
    }

    #[tokio::test]
    async fn test_leaderboard_command() {
        let db = setup_test_db().await;

        // Create test users
        for user_id in 1..=10 {
            // Ensure user config exists
            db.get_user_config(user_id).await.unwrap();
            for _ in 0..user_id {
                db.increment_cleaned_count(user_id, 1).await.unwrap();
            }
        }

        // Get top 5
        let top_users = db.get_top_users(5).await.unwrap();
        assert_eq!(top_users.len(), 5);
        assert!(top_users[0].1 >= top_users[4].1); // Access cleaned_count via tuple index
    }
}
