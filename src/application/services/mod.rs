//! Application layer - Use cases and business workflows.
//!
//! This layer contains:
//! - Commands: Write operations
//! - Queries: Read operations
//! - Services: Application orchestration

use crate::domain::entities::*;
use async_trait::async_trait;
use anyhow::Result;

/// Command to clean a URL.
#[derive(Debug)]
pub struct CleanUrlCommand {
    pub url: String,
    pub user_id: i64,
}

/// Result of cleaning a URL.
#[derive(Debug)]
pub struct CleanUrlResult {
    pub original_url: String,
    pub cleaned_url: String,
    pub warnings: Vec<String>,
    pub alternatives: Vec<AlternativeFrontend>,
}

/// Command handler for URL cleaning.
#[async_trait]
pub trait CleanUrlCommandHandler: Send + Sync {
    async fn handle(&self, command: CleanUrlCommand) -> Result<CleanUrlResult>;
}