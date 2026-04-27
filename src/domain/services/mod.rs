//! Domain services containing business logic.
//!
//! These services orchestrate domain operations and enforce business rules.

use crate::domain::entities::*;
use anyhow::Result;
use async_trait::async_trait;

/// Service for URL cleaning operations.
#[async_trait]
pub trait UrlCleaningService {
    /// Clean a URL and return the result.
    async fn clean_url(&self, url_to_clean: &UrlToClean) -> Result<CleaningResult>;

    /// Check if a URL should be processed (not whitelisted, etc.).
    async fn should_process_url(&self, url: &str) -> Result<bool>;
}

/// Service for security scanning.
#[async_trait]
pub trait SecurityService {
    /// Scan a URL for security threats.
    async fn scan_url(&self, url: &str) -> Result<Vec<SecurityWarning>>;

    /// Check if a URL is safe to visit.
    async fn is_url_safe(&self, url: &str) -> Result<bool>;
}

/// Service for finding alternative frontends.
#[async_trait]
pub trait FrontendService {
    /// Find alternative frontends for a URL.
    async fn find_alternatives(&self, url: &str) -> Result<Vec<AlternativeFrontend>>;
}

/// Service for user management.
pub trait UserService {
    /// Get or create a user.
    fn get_or_create_user(&self, user_id: i64) -> User;

    /// Update user language.
    fn update_language(&self, user: &mut User, language: Language);

    /// Update user preferences.
    fn update_preferences(&self, user: &mut User, preferences: UserPreferences);
}

/// Service for statistics calculation.
pub trait StatisticsService {
    /// Calculate user statistics.
    fn calculate_user_stats(
        &self,
        user_id: i64,
        cleaning_results: &[CleaningResult],
    ) -> UserStatistics;

    /// Calculate global statistics.
    fn calculate_global_stats(
        &self,
        all_users: &[User],
        all_results: &[CleaningResult],
    ) -> GlobalStatistics;
}
