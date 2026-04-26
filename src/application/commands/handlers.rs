//! Command handlers implementation.

use crate::application::services::*;
use crate::domain::entities::*;
use crate::domain::repositories::*;
use async_trait::async_trait;
use anyhow::Result;

/// Command for updating user preferences.
#[derive(Debug)]
pub struct UpdateUserPreferencesCommand {
    pub user_id: i64,
    pub preferences: UserPreferences,
}

#[async_trait]
pub trait UpdateUserPreferencesCommandHandler {
    async fn handle(&self, command: UpdateUserPreferencesCommand) -> Result<()>;
}

/// Command for updating user language.
#[derive(Debug)]
pub struct UpdateUserLanguageCommand {
    pub user_id: i64,
    pub language: Language,
}

#[async_trait]
pub trait UpdateUserLanguageCommandHandler {
    async fn handle(&self, command: UpdateUserLanguageCommand) -> Result<()>;
}

/// Command for managing whitelist.
#[derive(Debug)]
pub struct ManageWhitelistCommand {
    pub action: WhitelistAction,
    pub domain: String,
    pub user_id: i64,
}

#[derive(Debug)]
pub enum WhitelistAction {
    Add,
    Remove,
}

#[async_trait]
pub trait ManageWhitelistCommandHandler {
    async fn handle(&self, command: ManageWhitelistCommand) -> Result<()>;
}

/// Handler for cleaning URLs.
pub struct CleanUrlCommandHandlerImpl<H, W> {
    history_repository: H,
    whitelist_repository: W,
}

impl<H, W> CleanUrlCommandHandlerImpl<H, W>
where
    H: UrlHistoryRepository + Send + Sync,
    W: WhitelistRepository + Send + Sync,
{
    pub fn new(history_repository: H, whitelist_repository: W) -> Self {
        Self {
            history_repository,
            whitelist_repository,
        }
    }
}

#[async_trait]
impl<H, W> CleanUrlCommandHandler for CleanUrlCommandHandlerImpl<H, W>
where
    H: UrlHistoryRepository + Send + Sync,
    W: WhitelistRepository + Send + Sync,
{
    async fn handle(&self, command: CleanUrlCommand) -> Result<CleanUrlResult> {
        // TODO: Implement actual URL cleaning logic
        // For now, return a mock result
        let result = CleanUrlResult {
            original_url: command.url.clone(),
            cleaned_url: command.url.clone(), // Mock: no cleaning applied
            warnings: vec![],
            alternatives: vec![],
        };

        // Save to history
        let history = UrlHistory {
            user_id: command.user_id,
            original_url: result.original_url.clone(),
            cleaned_url: result.cleaned_url.clone(),
            timestamp: chrono::Utc::now(),
        };

        self.history_repository.save_url_history(&history).await?;

        Ok(result)
    }
}

/// Handler for updating user preferences.
pub struct UpdateUserPreferencesCommandHandlerImpl<R: UserRepository> {
    user_repository: R,
}

impl<R: UserRepository> UpdateUserPreferencesCommandHandlerImpl<R> {
    pub fn new(user_repository: R) -> Self {
        Self { user_repository }
    }
}

#[async_trait]
impl<R: UserRepository + Sync> UpdateUserPreferencesCommandHandler for UpdateUserPreferencesCommandHandlerImpl<R> {
    async fn handle(&self, command: UpdateUserPreferencesCommand) -> Result<()> {
        let mut user = self.user_repository.get_user(command.user_id).await?;
        user.preferences = command.preferences;
        self.user_repository.save_user(&user).await
    }
}

/// Handler for updating user language.
pub struct UpdateUserLanguageCommandHandlerImpl<R: UserRepository> {
    user_repository: R,
}

impl<R: UserRepository> UpdateUserLanguageCommandHandlerImpl<R> {
    pub fn new(user_repository: R) -> Self {
        Self { user_repository }
    }
}

#[async_trait]
impl<R: UserRepository + Sync> UpdateUserLanguageCommandHandler for UpdateUserLanguageCommandHandlerImpl<R> {
    async fn handle(&self, command: UpdateUserLanguageCommand) -> Result<()> {
        let mut user = self.user_repository.get_user(command.user_id).await?;
        user.language = command.language;
        self.user_repository.save_user(&user).await
    }
}

/// Handler for managing whitelist.
pub struct ManageWhitelistCommandHandlerImpl<R: WhitelistRepository> {
    whitelist_repository: R,
}

impl<R: WhitelistRepository> ManageWhitelistCommandHandlerImpl<R> {
    pub fn new(whitelist_repository: R) -> Self {
        Self { whitelist_repository }
    }
}

#[async_trait]
impl<R: WhitelistRepository + Sync> ManageWhitelistCommandHandler for ManageWhitelistCommandHandlerImpl<R> {
    async fn handle(&self, command: ManageWhitelistCommand) -> Result<()> {
        match command.action {
            WhitelistAction::Add => {
                self.whitelist_repository.add_to_whitelist(&command.domain).await
            }
            WhitelistAction::Remove => {
                self.whitelist_repository.remove_from_whitelist(&command.domain).await
            }
        }
    }
}