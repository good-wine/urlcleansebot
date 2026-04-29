//! Query handlers implementation.

use crate::application::queries::*;
use crate::domain::entities::*;
use crate::domain::repositories::*;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

/// Handler for getting user profile.
pub struct GetUserProfileQueryHandlerImpl {
    user_repository: Arc<dyn UserRepository>,
}

impl GetUserProfileQueryHandlerImpl {
    pub fn new(user_repository: Arc<dyn UserRepository>) -> Self {
        Self { user_repository }
    }
}

#[async_trait]
impl GetUserProfileQueryHandler for GetUserProfileQueryHandlerImpl {
    async fn handle(&self, query: GetUserProfileQuery) -> Result<User> {
        self.user_repository.get_user(query.user_id).await
    }
}

/// Handler for getting global statistics.
pub struct GetGlobalStatisticsQueryHandlerImpl {
    statistics_repository: Arc<dyn StatisticsRepository>,
}

impl GetGlobalStatisticsQueryHandlerImpl {
    pub fn new(statistics_repository: Arc<dyn StatisticsRepository>) -> Self {
        Self {
            statistics_repository,
        }
    }
}

#[async_trait]
impl GetGlobalStatisticsQueryHandler for GetGlobalStatisticsQueryHandlerImpl {
    async fn handle(&self, _query: GetGlobalStatisticsQuery) -> Result<GlobalStatistics> {
        self.statistics_repository.get_global_statistics().await
    }
}

/// Handler for getting whitelist.
pub struct GetWhitelistQueryHandlerImpl {
    whitelist_repository: Arc<dyn WhitelistRepository>,
}

impl GetWhitelistQueryHandlerImpl {
    pub fn new(whitelist_repository: Arc<dyn WhitelistRepository>) -> Self {
        Self {
            whitelist_repository,
        }
    }
}

#[async_trait]
impl GetWhitelistQueryHandler for GetWhitelistQueryHandlerImpl {
    async fn handle(&self, query: GetWhitelistQuery) -> Result<Vec<String>> {
        self.whitelist_repository.get_whitelist(query.user_id).await
    }
}
