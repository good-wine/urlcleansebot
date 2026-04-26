//! Query handlers implementation.

use crate::application::queries::*;
use crate::domain::entities::*;
use crate::domain::repositories::*;
use async_trait::async_trait;
use anyhow::Result;

/// Handler for getting user profile.
pub struct GetUserProfileQueryHandlerImpl<R: UserRepository> {
    user_repository: R,
}

impl<R: UserRepository> GetUserProfileQueryHandlerImpl<R> {
    pub fn new(user_repository: R) -> Self {
        Self { user_repository }
    }
}

#[async_trait]
impl<R: UserRepository + Sync> GetUserProfileQueryHandler for GetUserProfileQueryHandlerImpl<R> {
    async fn handle(&self, query: GetUserProfileQuery) -> Result<User> {
        self.user_repository.get_user(query.user_id).await
    }
}

/// Handler for getting global statistics.
pub struct GetGlobalStatisticsQueryHandlerImpl<S: StatisticsRepository> {
    statistics_repository: S,
}

impl<S: StatisticsRepository> GetGlobalStatisticsQueryHandlerImpl<S> {
    pub fn new(statistics_repository: S) -> Self {
        Self { statistics_repository }
    }
}

#[async_trait]
impl<S: StatisticsRepository + Sync> GetGlobalStatisticsQueryHandler for GetGlobalStatisticsQueryHandlerImpl<S> {
    async fn handle(&self, _query: GetGlobalStatisticsQuery) -> Result<GlobalStatistics> {
        self.statistics_repository.get_global_statistics().await
    }
}

/// Handler for getting whitelist.
pub struct GetWhitelistQueryHandlerImpl<W: WhitelistRepository> {
    whitelist_repository: W,
}

impl<W: WhitelistRepository> GetWhitelistQueryHandlerImpl<W> {
    pub fn new(whitelist_repository: W) -> Self {
        Self { whitelist_repository }
    }
}

#[async_trait]
impl<W: WhitelistRepository + Sync> GetWhitelistQueryHandler for GetWhitelistQueryHandlerImpl<W> {
    async fn handle(&self, _query: GetWhitelistQuery) -> Result<Vec<String>> {
        self.whitelist_repository.get_whitelist().await
    }
}