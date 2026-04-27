//! Command handlers implementation.

use crate::application::services::*;
use crate::domain::entities::*;
use crate::domain::repositories::*;
use crate::redirects::RedirectService;
use crate::sanitizer::RuleEngine;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

/// Command for updating user preferences.
#[derive(Debug)]
pub struct UpdateUserPreferencesCommand {
    pub user_id: i64,
    pub preferences: UserPreferences,
}

#[async_trait]
pub trait UpdateUserPreferencesCommandHandler: Send + Sync {
    async fn handle(&self, command: UpdateUserPreferencesCommand) -> Result<()>;
}

/// Command for updating user language.
#[derive(Debug)]
pub struct UpdateUserLanguageCommand {
    pub user_id: i64,
    pub language: Language,
}

#[async_trait]
pub trait UpdateUserLanguageCommandHandler: Send + Sync {
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
pub trait ManageWhitelistCommandHandler: Send + Sync {
    async fn handle(&self, command: ManageWhitelistCommand) -> Result<()>;
}

/// Handler for cleaning URLs.
pub struct CleanUrlCommandHandlerImpl {
    history_repository: Arc<dyn UrlHistoryRepository>,
    _whitelist_repository: Arc<dyn WhitelistRepository>,
    rule_engine: Arc<RuleEngine>,
    redirect_service: RedirectService,
}

impl CleanUrlCommandHandlerImpl {
    pub fn new(
        history_repository: Arc<dyn UrlHistoryRepository>,
        _whitelist_repository: Arc<dyn WhitelistRepository>,
        rule_engine: Arc<RuleEngine>,
        redirect_service: RedirectService,
    ) -> Self {
        Self {
            history_repository,
            _whitelist_repository,
            rule_engine,
            redirect_service,
        }
    }
}

#[async_trait]
impl CleanUrlCommandHandler for CleanUrlCommandHandlerImpl {
    async fn handle(&self, command: CleanUrlCommand) -> Result<CleanUrlResult> {
        // Parse the URL
        let mut url = url::Url::parse(&command.url)?;

        // Store original URL
        let original_url = url.to_string();

        // Clean the URL using the rule engine
        let changed = self.rule_engine.clean_url_in_place(&mut url);
        let cleaned_url = url.to_string();

        // Get alternative frontends
        let alternatives = if let Ok(Some(hit)) = self.redirect_service.lookup(&cleaned_url).await {
            hit.frontends
                .into_iter()
                .take(3)
                .map(|frontend| {
                    let service = frontend.service.clone();
                    AlternativeFrontend {
                        service: frontend.service,
                        frontend: frontend.kind,
                        url: frontend.url,
                        description: format!("Alternative frontend for {}", service),
                    }
                })
                .collect()
        } else {
            vec![]
        };

        // Create result
        let result = CleanUrlResult {
            original_url: original_url.clone(),
            cleaned_url: cleaned_url.clone(),
            warnings: if changed {
                vec!["URL cleaned".to_string()]
            } else {
                vec![]
            },
            alternatives,
        };

        // Save to history
        let history = UrlHistory {
            user_id: command.user_id,
            original_url,
            cleaned_url,
            timestamp: chrono::Utc::now(),
        };

        self.history_repository.save_url_history(&history).await?;

        Ok(result)
    }
}

/// Handler for updating user preferences.
pub struct UpdateUserPreferencesCommandHandlerImpl {
    user_repository: Arc<dyn UserRepository>,
}

impl UpdateUserPreferencesCommandHandlerImpl {
    pub fn new(user_repository: Arc<dyn UserRepository>) -> Self {
        Self { user_repository }
    }
}

#[async_trait]
impl UpdateUserPreferencesCommandHandler for UpdateUserPreferencesCommandHandlerImpl {
    async fn handle(&self, command: UpdateUserPreferencesCommand) -> Result<()> {
        let mut user = self.user_repository.get_user(command.user_id).await?;
        user.preferences = command.preferences;
        self.user_repository.save_user(&user).await
    }
}

/// Handler for updating user language.
pub struct UpdateUserLanguageCommandHandlerImpl {
    user_repository: Arc<dyn UserRepository>,
}

impl UpdateUserLanguageCommandHandlerImpl {
    pub fn new(user_repository: Arc<dyn UserRepository>) -> Self {
        Self { user_repository }
    }
}

#[async_trait]
impl UpdateUserLanguageCommandHandler for UpdateUserLanguageCommandHandlerImpl {
    async fn handle(&self, command: UpdateUserLanguageCommand) -> Result<()> {
        let mut user = self.user_repository.get_user(command.user_id).await?;
        user.language = command.language;
        self.user_repository.save_user(&user).await
    }
}

/// Handler for managing whitelist.
pub struct ManageWhitelistCommandHandlerImpl {
    whitelist_repository: Arc<dyn WhitelistRepository>,
}

impl ManageWhitelistCommandHandlerImpl {
    pub fn new(whitelist_repository: Arc<dyn WhitelistRepository>) -> Self {
        Self {
            whitelist_repository,
        }
    }
}

#[async_trait]
impl ManageWhitelistCommandHandler for ManageWhitelistCommandHandlerImpl {
    async fn handle(&self, command: ManageWhitelistCommand) -> Result<()> {
        match command.action {
            WhitelistAction::Add => {
                self.whitelist_repository
                    .add_to_whitelist(&command.domain)
                    .await
            }
            WhitelistAction::Remove => {
                self.whitelist_repository
                    .remove_from_whitelist(&command.domain)
                    .await
            }
        }
    }
}
