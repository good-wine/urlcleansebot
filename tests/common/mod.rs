// Common test utilities and fixtures

use std::sync::atomic::{AtomicU64, Ordering};

use url_cleanse_bot::config::Config;
use url_cleanse_bot::db::Db;

static DB_COUNTER: AtomicU64 = AtomicU64::new(0);

fn init_drivers() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        sqlx::any::install_default_drivers();
    });
}

/// Setup test database with unique in-memory SQLite per test
pub async fn setup_test_db() -> Db {
    init_drivers();
    let id = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let url = format!("sqlite:file:testdb{id}?mode=memory&cache=shared");
    Db::new(&url).await.unwrap()
}

/// Create test configuration
pub fn test_config() -> Config {
    Config {
        bot_token: "test_token".to_string(),
        bot_username: "@test_bot".to_string(),
        database_url: "sqlite::memory:".to_string(),
        server_addr: "0.0.0.0:8443".to_string(),
        admin_id: 12345,
        clearurls_source: "https://rules2.clearurls.xyz/data.minify.json".to_string(),
        libredirect_url: "https://libredirect.example.com".to_string(),
        farside_url: "https://farside.example.com".to_string(),
        webhook_url: None,
        port: 8443,
        // webhook_secret: Some("some_secret".to_string()), // This field is optional and can be None
        webhook_secret: None,
        ai_api_key: None,
        ai_api_base: "https://api.openai.com/v1".to_string(),
        ai_model: "gpt-4".to_string(),
        inline_max_results: 5,
        webhook_hmac_secret: None,
    }
}

/// Sample URLs for testing
#[allow(dead_code)]
pub mod test_urls {
    pub const CLEAN_URL: &str = "https://example.com/page";
    pub const URL_WITH_UTM: &str = "https://example.com/page?utm_source=test&utm_medium=email";
    pub const AMAZON_URL: &str =
        "https://www.amazon.com/product/dp/B08X6PZTKS?ref_=ast_sto_dp&th=1&psc=1";
    pub const YOUTUBE_URL: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ&feature=share";
    pub const MALICIOUS_URL: &str = "http://malware-test.example.com";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_db_setup() {
        let db = setup_test_db().await;
        // Verify database is initialized
        assert!(db.get_user_config(12345).await.is_ok());
    }

    #[test]
    fn test_config_creation() {
        let config = test_config();
        assert_eq!(config.bot_username, "@test_bot");
        assert_eq!(config.admin_id, 12345);
    }
}
