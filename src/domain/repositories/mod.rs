//! Repository interfaces for domain data access.
//!
//! These define the contracts that infrastructure implementations must fulfill.

use crate::domain::entities::*;
use anyhow::Result;
use async_trait::async_trait;

/// Repository for user data operations.
#[async_trait]
pub trait UserRepository: Send + Sync {
    /// Get a user by ID.
    async fn get_user(&self, user_id: i64) -> Result<User>;

    /// Save user information.
    async fn save_user(&self, user: &User) -> Result<()>;
}

/// Repository for URL cleaning history.
#[async_trait]
pub trait UrlHistoryRepository: Send + Sync {
    /// Save URL history entry.
    async fn save_url_history(&self, history: &UrlHistory) -> Result<()>;

    /// Get user history, ordered by most recent first.
    async fn get_user_history(&self, user_id: i64, limit: usize) -> Result<Vec<UrlHistory>>;
}

/// Repository for whitelist operations.
#[async_trait]
pub trait WhitelistRepository: Send + Sync {
    /// Add domain to user's whitelist.
    async fn add_to_whitelist(&self, user_id: i64, domain: &str) -> Result<()>;

    /// Remove domain from user's whitelist.
    async fn remove_from_whitelist(&self, user_id: i64, domain: &str) -> Result<()>;

    /// Get all whitelisted domains for a user.
    async fn get_whitelist(&self, user_id: i64) -> Result<Vec<String>>;

    /// Check if domain is whitelisted by a specific user.
    async fn is_whitelisted(&self, user_id: i64, domain: &str) -> Result<bool>;
}

/// Repository for statistics.
#[async_trait]
pub trait StatisticsRepository: Send + Sync {
    /// Get global statistics.
    async fn get_global_statistics(&self) -> Result<GlobalStatistics>;
}
