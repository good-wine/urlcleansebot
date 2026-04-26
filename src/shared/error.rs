//! Common error types.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Telegram bot error: {0}")]
    Telegram(#[from] teloxide::RequestError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Security error: {0}")]
    Security(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl From<crate::shared::security::SecurityError> for AppError {
    fn from(error: crate::shared::security::SecurityError) -> Self {
        match error {
            crate::shared::security::SecurityError::RateLimitExceeded => AppError::RateLimitExceeded,
            _ => AppError::Security(error.to_string()),
        }
    }
}