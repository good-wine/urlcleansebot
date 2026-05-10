//! Modern command dispatcher using trait-based pattern.
//!
//! This module provides a clean, extensible dispatcher for Telegram commands
//! that centralizes all command routing logic.

use teloxide::prelude::*;
use teloxide::types::ChatId;
use tracing::{debug, warn};

use crate::config::Config;
use crate::db::Db;
use crate::db::models::UserConfig;
use crate::i18n::Translations;
use crate::sanitizer::{AiEngine, RuleEngine};
use crate::shared::error::{AppError, AppResult};

use super::commands;

/// Represents the context needed to handle any command.
#[derive(Clone)]
pub struct CommandContext {
    pub bot: Bot,
    pub chat_id: ChatId,
    pub user_id: i64,
    pub db: Db,
    pub rules: RuleEngine,
    pub ai: AiEngine,
    pub config: Config,
    pub tr: Translations,
    pub user_config: UserConfig,
}

/// Define all available commands as an enum for type-safe routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Start,
    Help,
    Stats,
    History,
    Leaderboard,
    Trending,
    Domains,
    Privacy,
    Export,
    Settings,
    Menu,
    HideKbd,
    Language,
    Whitelist,
    Limits,
}

impl Command {
    /// Parse a command string to identify the command type.
    ///
    /// # Arguments
    /// * `text` - Raw command text (e.g., "/start", "/stats@bot_username")
    ///
    /// # Returns
    /// `Some(Command)` if recognized, `None` otherwise.
    pub fn parse(text: &str) -> Option<Self> {
        let cmd = text.split('@').next().unwrap_or("").to_lowercase();
        match cmd.as_str() {
            "/start" => Some(Command::Start),
            "/help" => Some(Command::Help),
            "/stats" => Some(Command::Stats),
            "/history" => Some(Command::History),
            "/leaderboard" => Some(Command::Leaderboard),
            "/trending" => Some(Command::Trending),
            "/domains" => Some(Command::Domains),
            "/privacy" => Some(Command::Privacy),
            "/export" => Some(Command::Export),
            "/settings" => Some(Command::Settings),
            "/menu" => Some(Command::Menu),
            "/hidekbd" => Some(Command::HideKbd),
            "/language" => Some(Command::Language),
            "/setlang" => Some(Command::Language), // Handle both /language and /setlang
            "/whitelist" => Some(Command::Whitelist),
            "/limits" => Some(Command::Limits),
            _ => None,
        }
    }

    /// Dispatch command execution to appropriate handler.
    ///
    /// # Returns
    /// `Ok(())` if successful, `AppError` otherwise
    pub async fn execute(&self, ctx: &CommandContext) -> AppResult<()> {
        match self {
            Command::Start => commands::handle_start(&ctx.bot, ctx.chat_id, ctx.user_id, &ctx.tr).await,
            Command::Help => commands::handle_help(&ctx.bot, ctx.chat_id, &ctx.tr).await,
            Command::Stats => {
                commands::handle_stats(&ctx.bot, ctx.chat_id, ctx.user_id, &ctx.db, &ctx.user_config, &ctx.tr).await
            }
            Command::History => {
                commands::handle_history(&ctx.bot, ctx.chat_id, ctx.user_id, &ctx.db, &ctx.tr).await
            }
            Command::Leaderboard => {
                commands::handle_leaderboard(&ctx.bot, ctx.chat_id, &ctx.db, &ctx.tr).await
            }
            Command::Trending => {
                commands::handle_trending(&ctx.bot, ctx.chat_id, &ctx.db, &ctx.tr).await
            }
            Command::Domains => {
                commands::handle_domains(&ctx.bot, ctx.chat_id, ctx.user_id, &ctx.db, &ctx.tr).await
            }
            Command::Privacy => commands::handle_privacy(&ctx.bot, ctx.chat_id, &ctx.tr).await,
            Command::Export => {
                commands::handle_export(&ctx.bot, ctx.chat_id, ctx.user_id, &ctx.db, &ctx.tr).await
            }
            Command::Settings => {
                commands::handle_settings(
                    &ctx.bot,
                    ctx.chat_id,
                    ctx.user_id,
                    &ctx.db,
                    &ctx.config,
                    &ctx.tr,
                )
                .await
            }
            Command::Menu => commands::handle_menu(&ctx.bot, ctx.chat_id, &ctx.tr).await,
            Command::HideKbd => commands::handle_hidekbd(&ctx.bot, ctx.chat_id).await,
            Command::Language => commands::handle_language(&ctx.bot, ctx.chat_id, &ctx.tr).await,
            Command::Whitelist => commands::handle_whitelist(&ctx.bot, ctx.chat_id, &ctx.tr).await,
            Command::Limits => commands::handle_limits(&ctx.bot, ctx.chat_id, &ctx.user_config, &ctx.tr).await,
        }
    }

    /// Get human-readable command name
    pub fn name(&self) -> &'static str {
        match self {
            Command::Start => "/start",
            Command::Help => "/help",
            Command::Stats => "/stats",
            Command::History => "/history",
            Command::Leaderboard => "/leaderboard",
            Command::Trending => "/trending",
            Command::Domains => "/domains",
            Command::Privacy => "/privacy",
            Command::Export => "/export",
            Command::Settings => "/settings",
            Command::Menu => "/menu",
            Command::HideKbd => "/hidekbd",
            Command::Language => "/language",
            Command::Whitelist => "/whitelist",
            Command::Limits => "/limits",
        }
    }
}

/// Main dispatcher for command routing.
/// Handles command parsing, throttling, and execution.
pub async fn dispatch_command(
    text: &str,
    ctx: &CommandContext,
) -> AppResult<bool> {
    // Parse command
    match Command::parse(text) {
        Some(cmd) => {
            debug!(command = %cmd.name(), user_id = ctx.user_id, "Esecuzione comando");

            // Execute command
            if let Err(e) = cmd.execute(ctx).await {
                warn!(command = %cmd.name(), error = %e, "Errore nell'esecuzione del comando");
                // Send error message to user
                let _ = ctx
                    .bot
                    .send_message(ctx.chat_id, "❌ Errore nell'esecuzione del comando")
                    .await;
            }

            Ok(true) // Command was handled
        }
        None => Ok(false), // Not a command, continue processing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_parsing() {
        assert_eq!(Command::parse("/start"), Some(Command::Start));
        assert_eq!(Command::parse("/stats"), Some(Command::Stats));
        assert_eq!(Command::parse("/help@bot_username"), Some(Command::Help));
        assert_eq!(Command::parse("/nonexistent"), None);
    }

    #[test]
    fn test_command_names() {
        assert_eq!(Command::Start.name(), "/start");
        assert_eq!(Command::Stats.name(), "/stats");
    }

    #[test]
    fn test_case_insensitive_parsing() {
        assert_eq!(Command::parse("/START"), Some(Command::Start));
        assert_eq!(Command::parse("/StAtS"), Some(Command::Stats));
    }
}
