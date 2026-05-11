use super::models::{ChatConfig, CleanedLink, CustomRule, UserConfig};
use anyhow::Result;
use sqlx::{any::AnyPoolOptions, Any, Pool};

#[derive(Clone)]
pub struct Db {
    pub pool: Pool<Any>,
    database_url: String,
}

impl Db {
    // init_tables function removed as it's not used and Db::init handles schema creation.

    pub async fn new(database_url: &str) -> Result<Self> {
        sqlx::any::install_default_drivers();

        let pool = AnyPoolOptions::new()
            .max_connections(5)
            .min_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .idle_timeout(std::time::Duration::from_secs(600))
            .connect_lazy(database_url)?;

        let db = Self {
            pool,
            database_url: database_url.to_string(),
        };
        db.init().await?;
        Ok(db)
    }

    fn is_sqlite(&self) -> bool {
        self.database_url.starts_with("sqlite")
    }

    pub async fn init(&self) -> Result<()> {
        // SQLite schema creation (uses IF NOT EXISTS to preserve data on restart)
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS user_configs (
                user_id INTEGER PRIMARY KEY,
                enabled INTEGER NOT NULL DEFAULT 1,
                ai_enabled INTEGER NOT NULL DEFAULT 0,
                mode TEXT NOT NULL DEFAULT 'reply',
                ignored_domains TEXT NOT NULL DEFAULT '',
                cleaned_count INTEGER NOT NULL DEFAULT 0,
                language TEXT NOT NULL DEFAULT 'en',
                privacy_mode INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS chat_configs (
                chat_id INTEGER PRIMARY KEY,
                title TEXT,
                enabled INTEGER NOT NULL DEFAULT 1,
                added_by INTEGER NOT NULL,
                mode TEXT NOT NULL DEFAULT 'default'
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS custom_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                pattern TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS cleaned_links (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                original_url TEXT NOT NULL,
                cleaned_url TEXT NOT NULL,
                provider_name TEXT,
                timestamp INTEGER NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS whitelist_urls (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                domain TEXT NOT NULL,
                added_at INTEGER NOT NULL,
                UNIQUE(user_id, domain)
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS feature_flags (
                user_id INTEGER NOT NULL,
                feature_name TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (user_id, feature_name)
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS rate_limits (
                user_id INTEGER PRIMARY KEY,
                action_count INTEGER NOT NULL DEFAULT 0,
                window_start INTEGER NOT NULL,
                last_action INTEGER NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        // Create indexes for performance optimization
        // These indexes speed up common queries significantly
        
        // Index for leaderboard queries (get_top_users)
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_user_configs_cleaned_count ON user_configs(cleaned_count DESC)")
            .execute(&self.pool)
            .await?;

        // Index for history retrieval (get_history)
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_cleaned_links_user_timestamp ON cleaned_links(user_id, timestamp DESC)")
            .execute(&self.pool)
            .await?;

        // Index for original_url lookups (get_top_links)
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_cleaned_links_original_url ON cleaned_links(original_url)")
            .execute(&self.pool)
            .await?;

        // Index for whitelist lookups (get_whitelist)
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_whitelist_urls_user_added_at ON whitelist_urls(user_id, added_at DESC)")
            .execute(&self.pool)
            .await?;

        // Index for custom_rules lookups (get_custom_rules)
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_custom_rules_user_id ON custom_rules(user_id)")
            .execute(&self.pool)
            .await?;

        // Index for feature_flags lookups
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_feature_flags_user_id ON feature_flags(user_id)")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn log_cleaned_link(
        &self,
        user_id: i64,
        original: &str,
        cleaned: &str,
        provider: &str,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO cleaned_links (user_id, original_url, cleaned_url, provider_name, timestamp) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(user_id)
        .bind(original)
        .bind(cleaned)
        .bind(provider)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_history(&self, user_id: i64, limit: i64) -> Result<Vec<CleanedLink>> {
        let history = sqlx::query_as::<_, CleanedLink>(
            "SELECT * FROM cleaned_links WHERE user_id = ? ORDER BY timestamp DESC LIMIT ?",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(history)
    }

    pub async fn get_global_stats(&self) -> Result<(i64, i64)> {
        let total_cleaned: (Option<i64>,) =
            sqlx::query_as("SELECT SUM(cleaned_count) FROM user_configs")
                .fetch_one(&self.pool)
                .await?;
        let total_users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_configs")
            .fetch_one(&self.pool)
            .await?;
        Ok((total_cleaned.0.unwrap_or(0), total_users.0))
    }

    pub async fn get_user_config(&self, user_id: i64) -> Result<UserConfig> {
        let config =
            sqlx::query_as::<_, UserConfig>("SELECT * FROM user_configs WHERE user_id = ?")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;

        Ok(config.unwrap_or(UserConfig {
            user_id,
            enabled: 1,
            ai_enabled: 0,
            mode: "reply".to_string(),
            ignored_domains: String::new(),
            cleaned_count: 0,
            language: "en".to_string(),
            privacy_mode: 0,
        }))
    }

    pub async fn save_user_config(&self, config: &UserConfig) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_configs (user_id, enabled, ai_enabled, mode, ignored_domains, cleaned_count, language, privacy_mode) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_id) DO UPDATE SET enabled = ?, ai_enabled = ?, mode = ?, ignored_domains = ?, cleaned_count = ?, language = ?, privacy_mode = ?"
        )
        .bind(config.user_id)
        .bind(config.enabled)
        .bind(config.ai_enabled)
        .bind(&config.mode)
        .bind(&config.ignored_domains)
        .bind(config.cleaned_count)
        .bind(&config.language)
        .bind(config.privacy_mode)
        .bind(config.enabled)
        .bind(config.ai_enabled)
        .bind(&config.mode)
        .bind(&config.ignored_domains)
        .bind(config.cleaned_count)
        .bind(&config.language)
        .bind(config.privacy_mode)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn increment_cleaned_count(&self, user_id: i64, amount: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_configs (user_id, cleaned_count) VALUES (?, ?)
             ON CONFLICT(user_id) DO UPDATE SET cleaned_count = cleaned_count + ?",
        )
        .bind(user_id)
        .bind(amount)
        .bind(amount)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_custom_rules(&self, user_id: i64) -> Result<Vec<CustomRule>> {
        let rules = sqlx::query_as::<_, CustomRule>("SELECT * FROM custom_rules WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rules)
    }

    pub async fn clear_history(&self, user_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM cleaned_links WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_custom_rule(&self, user_id: i64, pattern: &str) -> Result<()> {
        sqlx::query("INSERT INTO custom_rules (user_id, pattern) VALUES (?, ?)")
            .bind(user_id)
            .bind(pattern)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_stats_by_day(&self, user_id: i64) -> Result<Vec<(String, i64)>> {
        let query = if self.is_sqlite() {
            "SELECT date(timestamp, 'unixepoch') as day, COUNT(*) 
             FROM cleaned_links 
             WHERE user_id = ? 
             GROUP BY day ORDER BY day DESC LIMIT 7"
        } else {
            "SELECT to_char(to_timestamp(timestamp), 'YYYY-MM-DD') as day, COUNT(*) 
             FROM cleaned_links 
             WHERE user_id = ? 
             GROUP BY day ORDER BY day DESC LIMIT 7"
        };

        let stats = sqlx::query_as::<_, (String, i64)>(query)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(stats)
    }

    pub async fn get_chat_config(&self, chat_id: i64) -> Result<Option<ChatConfig>> {
        let config =
            sqlx::query_as::<_, ChatConfig>("SELECT * FROM chat_configs WHERE chat_id = ?")
                .bind(chat_id)
                .fetch_optional(&self.pool)
                .await?;

        Ok(config)
    }

    pub async fn get_chat_config_or_default(&self, chat_id: i64) -> Result<ChatConfig> {
        let config = self.get_chat_config(chat_id).await?;

        Ok(config.unwrap_or(ChatConfig {
            chat_id,
            title: None,
            enabled: 1,
            added_by: 0,
            mode: "default".to_string(),
        }))
    }

    pub async fn save_chat_config(&self, config: &ChatConfig) -> Result<()> {
        sqlx::query(
            "INSERT INTO chat_configs (chat_id, title, enabled, added_by, mode) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(chat_id) DO UPDATE SET title = ?, enabled = ?, mode = ?"
        )
        .bind(config.chat_id)
        .bind(&config.title)
        .bind(config.enabled)
        .bind(config.added_by)
        .bind(&config.mode)
        .bind(&config.title)
        .bind(config.enabled)
        .bind(&config.mode)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_chats_for_user(&self, user_id: i64) -> Result<Vec<ChatConfig>> {
        let chats =
            sqlx::query_as::<_, ChatConfig>("SELECT * FROM chat_configs WHERE added_by = ?")
                .bind(user_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(chats)
    }

    pub async fn get_top_users(&self, limit: usize) -> Result<Vec<(i64, i64)>> {
        let rows = sqlx::query_as::<_, (i64, i64)>(
            "SELECT user_id, cleaned_count FROM user_configs ORDER BY cleaned_count DESC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_top_links(&self, limit: usize) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT original_url, COUNT(*) as cleaned_count FROM cleaned_links GROUP BY original_url ORDER BY cleaned_count DESC LIMIT ?"
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn add_to_whitelist(&self, user_id: i64, domain: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        sqlx::query("INSERT INTO whitelist_urls (user_id, domain, added_at) VALUES (?, ?, ?)")
            .bind(user_id)
            .bind(domain)
            .bind(now)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn remove_from_whitelist(&self, user_id: i64, domain: &str) -> Result<()> {
        sqlx::query("DELETE FROM whitelist_urls WHERE user_id = ? AND domain = ?")
            .bind(user_id)
            .bind(domain)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn get_whitelist(&self, user_id: i64) -> Result<Vec<String>> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT domain FROM whitelist_urls WHERE user_id = ? ORDER BY added_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|(domain,)| domain).collect())
    }

    pub async fn is_whitelisted(&self, user_id: i64, domain: &str) -> Result<bool> {
        let result = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM whitelist_urls WHERE user_id = ? AND domain = ?",
        )
        .bind(user_id)
        .bind(domain)
        .fetch_one(&self.pool)
        .await?;

        Ok(result > 0)
    }

    pub async fn get_domain_cleanup_stats(&self, user_id: i64) -> Result<Vec<(String, i64)>> {
        let query = if self.is_sqlite() {
            "SELECT 
                SUBSTR(original_url, INSTR(original_url, '://') + 3, 
                       INSTR(SUBSTR(original_url, INSTR(original_url, '://') + 3), '/') - 1) as domain,
                COUNT(*) as clean_count
             FROM cleaned_links 
             WHERE user_id = ? 
             GROUP BY domain 
             ORDER BY clean_count DESC 
             LIMIT 10"
        } else {
            "SELECT 
                SPLIT_PART(SPLIT_PART(original_url, '://', 2), '/', 1) as domain,
                COUNT(*) as clean_count
             FROM cleaned_links 
             WHERE user_id = $1
             GROUP BY domain 
             ORDER BY clean_count DESC 
             LIMIT 10"
        };

        let rows = sqlx::query_as::<_, (String, i64)>(query)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows)
    }

    // Feature flags implementation
    /// Set a feature flag for a user
    pub async fn set_feature_flag(
        &self,
        user_id: i64,
        feature_name: &str,
        enabled: bool,
    ) -> Result<()> {
        let enabled_val = if enabled { 1 } else { 0 };

        sqlx::query(
            "INSERT INTO feature_flags (user_id, feature_name, enabled) VALUES (?, ?, ?)
             ON CONFLICT(user_id, feature_name) DO UPDATE SET enabled = ?",
        )
        .bind(user_id)
        .bind(feature_name)
        .bind(enabled_val)
        .bind(enabled_val)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Check if a feature is enabled for a user
    pub async fn is_feature_enabled(&self, user_id: i64, feature_name: &str) -> Result<bool> {
        let result = sqlx::query_scalar::<_, i32>(
            "SELECT enabled FROM feature_flags WHERE user_id = ? AND feature_name = ?",
        )
        .bind(user_id)
        .bind(feature_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.unwrap_or(0) != 0)
    }

    /// Get all feature flags for a user
    pub async fn get_user_features(&self, user_id: i64) -> Result<Vec<(String, bool)>> {
        let rows = sqlx::query_as::<_, (String, i32)>(
            "SELECT feature_name, enabled FROM feature_flags WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(name, enabled)| (name, enabled != 0))
            .collect())
    }

    // Rate limiting implementation
    /// Check if user has exceeded rate limit (configurable actions per hour)
    pub async fn check_rate_limit(
        &self,
        user_id: i64,
        max_actions: i64,
        window_seconds: i64,
    ) -> Result<bool> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        // Get current rate limit status
        let current = sqlx::query_as::<_, (i64, i64, i64)>(
            "SELECT action_count, window_start, last_action FROM rate_limits WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        match current {
            Some((count, window_start, _last_action)) => {
                // Check if we're still in the same window
                if now - window_start < window_seconds {
                    if count >= max_actions {
                        // Rate limit exceeded
                        return Ok(false);
                    }
                    // Increment counter
                    sqlx::query(
                        "UPDATE rate_limits SET action_count = action_count + 1, last_action = ? WHERE user_id = ?"
                    )
                    .bind(now)
                    .bind(user_id)
                    .execute(&self.pool)
                    .await?;
                } else {
                    // New window, reset counter
                    sqlx::query(
                        "UPDATE rate_limits SET action_count = 1, window_start = ?, last_action = ? WHERE user_id = ?"
                    )
                    .bind(now)
                    .bind(now)
                    .bind(user_id)
                    .execute(&self.pool)
                    .await?;
                }
            }
            None => {
                // First action, create entry
                sqlx::query(
                    "INSERT INTO rate_limits (user_id, action_count, window_start, last_action) VALUES (?, 1, ?, ?)"
                )
                .bind(user_id)
                .bind(now)
                .bind(now)
                .execute(&self.pool)
                .await?;
            }
        }

        Ok(true)
    }

    /// Get rate limit status for a user
    pub async fn get_rate_limit_status(&self, user_id: i64) -> Result<Option<(i64, i64)>> {
        let result = sqlx::query_as::<_, (i64, i64)>(
            "SELECT action_count, window_start FROM rate_limits WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    /// Reset rate limit for a user (admin function)
    pub async fn reset_rate_limit(&self, user_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM rate_limits WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
