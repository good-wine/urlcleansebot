//! Telegram bot handlers using the application layer.

use crate::application::commands::handlers::*;
use crate::application::queries::*;
use crate::application::services::*;
use crate::shared::error::AppResult;
use crate::shared::security::*;
use teloxide::prelude::*;
use teloxide::types::Message;
use std::sync::Arc;

/// Application services container for dependency injection.
#[derive(Clone)]
pub struct AppServices {
    pub clean_url_command_handler: Arc<dyn CleanUrlCommandHandler>,
    pub update_user_preferences_handler: Arc<dyn UpdateUserPreferencesCommandHandler>,
    pub update_user_language_handler: Arc<dyn UpdateUserLanguageCommandHandler>,
    pub manage_whitelist_handler: Arc<dyn ManageWhitelistCommandHandler>,
    pub get_user_profile_handler: Arc<dyn GetUserProfileQueryHandler>,
    pub get_global_statistics_handler: Arc<dyn GetGlobalStatisticsQueryHandler>,
    pub get_whitelist_handler: Arc<dyn GetWhitelistQueryHandler>,
}

impl AppServices {
    pub fn new(
        clean_url_command_handler: Arc<dyn CleanUrlCommandHandler>,
        update_user_preferences_handler: Arc<dyn UpdateUserPreferencesCommandHandler>,
        update_user_language_handler: Arc<dyn UpdateUserLanguageCommandHandler>,
        manage_whitelist_handler: Arc<dyn ManageWhitelistCommandHandler>,
        get_user_profile_handler: Arc<dyn GetUserProfileQueryHandler>,
        get_global_statistics_handler: Arc<dyn GetGlobalStatisticsQueryHandler>,
        get_whitelist_handler: Arc<dyn GetWhitelistQueryHandler>,
    ) -> Self {
        Self {
            clean_url_command_handler,
            update_user_preferences_handler,
            update_user_language_handler,
            manage_whitelist_handler,
            get_user_profile_handler,
            get_global_statistics_handler,
            get_whitelist_handler,
        }
    }
}

/// Handle URL cleaning requests.
pub async fn handle_url_cleaning(
    bot: Bot,
    msg: Message,
    services: AppServices,
) -> AppResult<()> {
    if let Some(text) = msg.text() {
        let user_id = msg.chat.id.0;

        // Check rate limit first
        if let Err(SecurityError::RateLimitExceeded) = check_rate_limit(user_id) {
            bot.send_message(msg.chat.id, "❌ Troppe richieste. Riprova tra un minuto.").await?;
            return Ok(());
        }

        // Validate user ID
        if validate_user_id(user_id).is_err() {
            bot.send_message(msg.chat.id, "❌ ID utente non valido").await?;
            return Ok(());
        }

        // Validate and sanitize input
        let validated_url = match validate_url(text) {
            Ok(url) => url,
            Err(e) => {
                let error_msg = format!("❌ Errore di sicurezza: {}", e);
                bot.send_message(msg.chat.id, error_msg).await?;
                return Ok(());
            }
        };

        let command = CleanUrlCommand {
            user_id,
            url: validated_url,
        };

        match services.clean_url_command_handler.handle(command).await {
            Ok(result) => {
                // Sanitize output for Telegram
                let safe_url = sanitize_telegram_text(&result.cleaned_url);
                let response = format!(
                    "🧹 URL pulita:\n\n{}",
                    safe_url
                );
                bot.send_message(msg.chat.id, response).await?;
            }
            Err(e) => {
                // Don't leak internal error details
                tracing::error!("URL cleaning error for user {}: {}", user_id, e);
                bot.send_message(msg.chat.id, "❌ Errore durante la pulizia dell'URL").await?;
            }
        }
    }

    Ok(())
}

/// Handle /start command.
pub async fn handle_start(
    bot: Bot,
    msg: Message,
    _services: AppServices,
) -> AppResult<()> {
    let welcome_text = "👋 Benvenuto nel ClearURLs Bot!\n\n\
        Invia un URL da pulire e io rimuoverò tutti i parametri di tracciamento.\n\n\
        Comandi disponibili:\n\
        /stats - Statistiche globali\n\
        /whitelist - Gestisci whitelist\n\
        /settings - Impostazioni personali";

    bot.send_message(msg.chat.id, welcome_text).await?;
    Ok(())
}

/// Handle /stats command.
pub async fn handle_stats(
    bot: Bot,
    msg: Message,
    services: AppServices,
) -> AppResult<()> {
    let user_id = msg.chat.id.0;

    // Validate user ID
    if validate_user_id(user_id).is_err() {
        bot.send_message(msg.chat.id, "❌ ID utente non valido").await?;
        return Ok(());
    }

    match services.get_global_statistics_handler.handle(GetGlobalStatisticsQuery).await {
        Ok(stats) => {
            let response = format!(
                "📊 Statistiche globali:\n\n\
                👥 Utenti totali: {}\n\
                🔗 URL pulite: {}",
                stats.total_users,
                stats.total_urls_cleaned
            );
            bot.send_message(msg.chat.id, response).await?;
        }
        Err(e) => {
            tracing::error!("Statistics error for user {}: {}", user_id, e);
            bot.send_message(msg.chat.id, "❌ Errore nel recupero delle statistiche").await?;
        }
    }

    Ok(())
}

/// Handle /whitelist command.
pub async fn handle_whitelist(
    bot: Bot,
    msg: Message,
    services: AppServices,
) -> AppResult<()> {
    let user_id = msg.chat.id.0;

    // Validate user ID
    if validate_user_id(user_id).is_err() {
        bot.send_message(msg.chat.id, "❌ ID utente non valido").await?;
        return Ok(());
    }

    match services.get_whitelist_handler.handle(GetWhitelistQuery).await {
        Ok(whitelist) => {
            if whitelist.is_empty() {
                bot.send_message(msg.chat.id, "📝 La whitelist è vuota.").await?;
            } else {
                let domains: Vec<String> = whitelist.into_iter()
                    .map(|domain| sanitize_telegram_text(&domain))
                    .collect();
                let domains_str = domains.join("\n• ");
                let response = format!("📝 Domini in whitelist:\n\n• {}", domains_str);
                bot.send_message(msg.chat.id, response).await?;
            }
        }
        Err(e) => {
            tracing::error!("Whitelist error for user {}: {}", user_id, e);
            bot.send_message(msg.chat.id, "❌ Errore nel recupero della whitelist").await?;
        }
    }

    Ok(())
}

/// Handle /settings command.
pub async fn handle_settings(
    bot: Bot,
    msg: Message,
    services: AppServices,
) -> AppResult<()> {
    let user_id = msg.chat.id.0;

    // Validate user ID
    if validate_user_id(user_id).is_err() {
        bot.send_message(msg.chat.id, "❌ ID utente non valido").await?;
        return Ok(());
    }

    match services.get_user_profile_handler.handle(GetUserProfileQuery { user_id }).await {
        Ok(user) => {
            let response = format!(
                "⚙️ Impostazioni:\n\n\
                🌐 Lingua: {:?}\n\
                🔧 Preferenze: {}",
                user.language,
                sanitize_telegram_text(&serde_json::to_string_pretty(&user.preferences).unwrap_or_default())
            );
            bot.send_message(msg.chat.id, response).await?;
        }
        Err(e) => {
            tracing::error!("Settings error for user {}: {}", user_id, e);
            bot.send_message(msg.chat.id, "❌ Errore nel recupero delle impostazioni").await?;
        }
    }

    Ok(())
}