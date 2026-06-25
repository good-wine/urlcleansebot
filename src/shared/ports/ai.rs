use async_trait::async_trait;

#[cfg_attr(any(test, feature = "test-utils"), mockall::automock)]
#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn sanitize(&self, url: &str) -> Result<Option<String>, AiError>;

    fn is_enabled(&self) -> bool;
}

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("AI provider not configured")]
    NotConfigured,
    #[error("API error: {0}")]
    Api(String),
    #[error("Rate limited")]
    RateLimited,
}
