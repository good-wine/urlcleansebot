//! Concrete repository implementations using SQLx.

use crate::domain::entities::*;
use crate::domain::repositories::*;
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use anyhow::Result;

/// PostgreSQL implementation of UserRepository.
pub struct PostgresUserRepository {
    pool: PgPool,
}

impl PostgresUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for PostgresUserRepository {
    async fn get_user(&self, user_id: i64) -> Result<User> {
        let row = sqlx::query("SELECT user_id, language, preferences FROM users WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await?;

        let language_str: String = row.get("language");
        let language = match language_str.as_str() {
            "Italian" => Language::Italian,
            "English" => Language::English,
            "Spanish" => Language::Spanish,
            "French" => Language::French,
            "German" => Language::German,
            _ => Language::English, // default fallback
        };

        Ok(User {
            id: row.get("user_id"),
            username: None, // TODO: Add username column to database
            language,
            preferences: serde_json::from_value(row.get("preferences"))?,
        })
    }

    async fn save_user(&self, user: &User) -> Result<()> {
        let language_str = match user.language {
            Language::Italian => "Italian",
            Language::English => "English",
            Language::Spanish => "Spanish",
            Language::French => "French",
            Language::German => "German",
        };

        sqlx::query(
            "INSERT INTO users (user_id, language, preferences) VALUES ($1, $2, $3)
             ON CONFLICT (user_id) DO UPDATE SET language = $2, preferences = $3"
        )
        .bind(user.id)
        .bind(language_str)
        .bind(serde_json::to_value(&user.preferences)?)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

/// PostgreSQL implementation of UrlHistoryRepository.
pub struct PostgresUrlHistoryRepository {
    pool: PgPool,
}

impl PostgresUrlHistoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UrlHistoryRepository for PostgresUrlHistoryRepository {
    async fn save_url_history(&self, history: &UrlHistory) -> Result<()> {
        sqlx::query(
            "INSERT INTO url_history (user_id, original_url, cleaned_url, timestamp) VALUES ($1, $2, $3, $4)"
        )
        .bind(history.user_id)
        .bind(&history.original_url)
        .bind(&history.cleaned_url)
        .bind(history.timestamp)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_user_history(&self, user_id: i64, limit: usize) -> Result<Vec<UrlHistory>> {
        let rows = sqlx::query(
            "SELECT user_id, original_url, cleaned_url, timestamp FROM url_history
             WHERE user_id = $1 ORDER BY timestamp DESC LIMIT $2"
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut history = Vec::new();
        for row in rows {
            history.push(UrlHistory {
                user_id: row.get("user_id"),
                original_url: row.get("original_url"),
                cleaned_url: row.get("cleaned_url"),
                timestamp: row.get("timestamp"),
            });
        }

        Ok(history)
    }
}

/// PostgreSQL implementation of WhitelistRepository.
pub struct PostgresWhitelistRepository {
    pool: PgPool,
}

impl PostgresWhitelistRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WhitelistRepository for PostgresWhitelistRepository {
    async fn add_to_whitelist(&self, domain: &str) -> Result<()> {
        sqlx::query("INSERT INTO whitelist (domain) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(domain)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn remove_from_whitelist(&self, domain: &str) -> Result<()> {
        sqlx::query("DELETE FROM whitelist WHERE domain = $1")
            .bind(domain)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get_whitelist(&self) -> Result<Vec<String>> {
        let rows = sqlx::query("SELECT domain FROM whitelist ORDER BY domain")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|row| row.get("domain")).collect())
    }

    async fn is_whitelisted(&self, domain: &str) -> Result<bool> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM whitelist WHERE domain = $1")
            .bind(domain)
            .fetch_one(&self.pool)
            .await?;

        Ok(count.0 > 0)
    }
}

/// PostgreSQL implementation of StatisticsRepository.
pub struct PostgresStatisticsRepository {
    pool: PgPool,
}

impl PostgresStatisticsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl StatisticsRepository for PostgresStatisticsRepository {
    async fn get_global_statistics(&self) -> Result<GlobalStatistics> {
        let total_users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?;

        let total_urls: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM url_history")
            .fetch_one(&self.pool)
            .await?;

        Ok(GlobalStatistics {
            total_users: total_users.0 as usize,
            total_urls_cleaned: total_urls.0 as usize,
        })
    }
}