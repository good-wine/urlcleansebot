use crate::db::models::{ChatConfig, CleanedLink, CustomRule, UserConfig};
use crate::shared::error::AppResult;
use async_trait::async_trait;

#[cfg_attr(any(test, feature = "test-utils"), mockall::automock)]
#[async_trait]
pub trait DatabasePort: Send + Sync {
    async fn get_user_config(&self, user_id: i64) -> AppResult<UserConfig>;

    async fn save_user_config(&self, user_id: i64, config: &UserConfig) -> AppResult<()>;

    async fn get_chat_config(&self, chat_id: i64) -> AppResult<Option<ChatConfig>>;

    async fn save_chat_config(&self, chat_id: i64, config: &ChatConfig) -> AppResult<()>;

    async fn increment_cleaned_count(&self, user_id: i64) -> AppResult<()>;

    async fn log_cleaned_link(&self, link: &CleanedLink) -> AppResult<()>;

    async fn get_history(&self, user_id: i64, limit: i64) -> AppResult<Vec<CleanedLink>>;

    async fn get_top_users(&self, limit: i64) -> AppResult<Vec<(i64, i64)>>;

    async fn get_top_links(&self, limit: i64) -> AppResult<Vec<(String, i64)>>;

    async fn get_domain_stats(&self, user_id: i64) -> AppResult<Vec<(String, i64)>>;

    async fn get_custom_rules(&self, user_id: i64) -> AppResult<Vec<CustomRule>>;

    async fn save_custom_rule(&self, user_id: i64, rule: &CustomRule) -> AppResult<()>;

    async fn delete_custom_rule(&self, user_id: i64, rule_id: i64) -> AppResult<()>;

    async fn add_to_whitelist(&self, user_id: i64, domain: &str) -> AppResult<()>;

    async fn remove_from_whitelist(&self, user_id: i64, domain: &str) -> AppResult<()>;

    async fn get_whitelist(&self, user_id: i64) -> AppResult<Vec<String>>;

    async fn clear_history(&self, user_id: i64) -> AppResult<()>;

    async fn is_whitelisted(&self, user_id: i64, domain: &str) -> AppResult<bool>;

    async fn set_feature_flag(&self, user_id: i64, flag: &str, value: bool) -> AppResult<()>;

    async fn get_feature_flag(&self, user_id: i64, flag: &str) -> AppResult<Option<bool>>;

    async fn ping(&self) -> AppResult<()>;
}
