pub mod handlers;

/// Query for user profile information.
#[derive(Debug)]
pub struct GetUserProfileQuery {
    pub user_id: i64,
}

#[async_trait::async_trait]
pub trait GetUserProfileQueryHandler: Send + Sync {
    async fn handle(
        &self,
        query: GetUserProfileQuery,
    ) -> anyhow::Result<crate::domain::entities::User>;
}

/// Query for global statistics.
#[derive(Debug)]
pub struct GetGlobalStatisticsQuery;

#[async_trait::async_trait]
pub trait GetGlobalStatisticsQueryHandler: Send + Sync {
    async fn handle(
        &self,
        _query: GetGlobalStatisticsQuery,
    ) -> anyhow::Result<crate::domain::entities::GlobalStatistics>;
}

/// Query for whitelisted domains.
#[derive(Debug)]
pub struct GetWhitelistQuery {
    pub user_id: i64,
}

#[async_trait::async_trait]
pub trait GetWhitelistQueryHandler: Send + Sync {
    async fn handle(&self, query: GetWhitelistQuery) -> anyhow::Result<Vec<String>>;
}
