//! Command handlers for Telegram bot.
//!
//! Extracts individual command logic from the main handler to improve readability and testability.

use teloxide::prelude::*;
use teloxide::types::{ChatId, ParseMode, ReplyParameters};
use teloxide::utils::html;
use tracing::error;

use crate::shared::error::{AppError, AppResult};

use crate::db::Db;
use crate::db::models::UserConfig;
use crate::i18n::{self, Translations};
use crate::sanitizer::{AiEngine, RuleEngine};
use crate::config::Config;

use super::helpers;
use super::security_scan;
use super::settings;

/// Represents a command execution result.
pub type CommandResult = AppResult<()>;

/// Handles the `/start` command.
///
/// # Arguments
///
/// * `bot` - Telegram bot instance
/// * `chat_id` - Target chat ID
/// * `user_id` - User ID
/// * `tr` - Translations for current language
pub async fn handle_start(bot: &Bot, chat_id: ChatId, user_id: i64, tr: &Translations) -> CommandResult {
    let msg = tr.welcome.replace("{}", &user_id.to_string());
    bot.send_message(chat_id, msg)
        .parse_mode(ParseMode::Html)
        .await
        .map_err(|e| {
            error!(?e, "Errore nell'invio del messaggio di benvenuto");
            AppError::Telegram(e)
        })?;
    Ok(())
}

/// Handles the `/stats` command with activity metrics.
pub async fn handle_stats(
    bot: &Bot,
    chat_id: ChatId,
    user_id: i64,
    db: &Db,
    user_config: &UserConfig,
    tr: &Translations,
) -> CommandResult {
    let stats_text = match db.get_user_config(user_id).await {
        Ok(config) => {
            let activity_level = (config.cleaned_count.min(100) / 10) as usize;
            let progress_bar = "\u{2588}".repeat(activity_level) + &"\u{2591}".repeat(10 - activity_level);

            let global_stats = db.get_global_stats().await.ok();
            let total_users = global_stats.as_ref().map(|(u, _)| *u).unwrap_or(0);
            let total_cleaned = global_stats.as_ref().map(|(_, c)| *c).unwrap_or(0);

            let user_rank = if total_cleaned > 0 {
                let top_users = db.get_top_users(10).await.ok();
                top_users
                    .as_ref()
                    .and_then(|users| users.iter().position(|(uid, _)| *uid == user_id))
                    .map(|pos| format!("#{}", pos + 1))
                    .unwrap_or_else(|| ">10".to_string())
            } else if config.cleaned_count > 0 {
                "#1".to_string()
            } else {
                "N/A".to_string()
            };

            format!(
                "<b>\u{1f4ca} Le Tue Statistiche</b>\n\n\
                \u{1f517} URL Elaborati: <code>{}</code>\n\
                \u{2705} Pulizie Riuscite: <code>{}</code>\n\
                \u{1f3c6} Ranking: <b>{}</b>\n\n\
                <b>Attività ({}/10)</b>\n{}\n\n\
                \u{1f30d} Lingua: <b>{}</b>\n\n\
                <b>\u{1f527} Configurazione:</b>\n\
                \u{1f916} AI Sanitizer: <b>{}</b>\n\
                \u{1f512} Privacy Mode: <b>{}</b>\n\
                \u{1f5c2}\u{1fe0f}  Modalità: <b>{}</b>\n\n\
                \u{1f4ca} <b>Globale:</b> {} utenti | {} URL puliti\n\n\
                \u{1f4a1} <i>Invia URL per pulirli automaticamente</i>",
                config.cleaned_count,
                config.cleaned_count,
                user_rank,
                activity_level,
                progress_bar,
                if config.language == "it" { "Italiano \u{1f1ee}\u{1f1f9}" } else { "English \u{1f1ec}\u{1f1e7}" },
                if config.is_ai_enabled() { "Attivo \u{2728}" } else { "Disattivo" },
                if config.privacy_mode != 0 { "Attivo" } else { "Disattivo" },
                if config.mode == "delete" { "Elimina msg" } else { "Rispondi" },
                total_users,
                total_cleaned
            )
        }
        Err(e) => {
            error!(?e, "Errore nel recupero delle statistiche utente");
            tr.s_not_found.to_string()
        }
    };

    bot.send_message(chat_id, stats_text)
        .parse_mode(ParseMode::Html)
        .await
        .map_err(|e| {
            error!(?e, "Errore nell'invio delle statistiche");
            AppError::Telegram(e)
        })?;
    Ok(())
}

/// Handles the `/history` command to show cleaned URL history.
pub async fn handle_history(bot: &Bot, chat_id: ChatId, user_id: i64, db: &Db) -> CommandResult {
    let history_text = match db.get_history(user_id, 10).await {
        Ok(links) if links.is_empty() => {
            "\u{1f550} <b>Cronologia Vuota</b>\n\nAncora non hai pulito nessun URL".to_string()
        }
        Ok(links) => {
            let mut text = String::from("<b>\u{1f550} Ultimi URL Puliti</b>\n\n");
            for (idx, link) in links.iter().enumerate() {
                let original_clean = if link.original_url.len() > 40 {
                    format!("{}...", &link.original_url[..37])
                } else {
                    link.original_url.clone()
                };
                let cleaned_clean = if link.cleaned_url.len() > 40 {
                    format!("{}...", &link.cleaned_url[..37])
                } else {
                    link.cleaned_url.clone()
                };

                text.push_str(&format!(
                    "{}. <code>{}</code>\n   → <code>{}</code>\n   via <b>{}</b>\n\n",
                    idx + 1,
                    original_clean,
                    cleaned_clean,
                    link.provider_name.as_deref().unwrap_or("Unknown")
                ));
            }
            text
        }
        Err(e) => {
            error!(?e, "Errore nel caricamento della cronologia");
            "\u{274c} Errore nel caricamento della cronologia".to_string()
        }
    };

    bot.send_message(chat_id, history_text)
        .parse_mode(ParseMode::Html)
        .await
        .map_err(|e| {
            error!(?e, "Errore nell'invio della cronologia");
            AppError::Telegram(e)
        })?;
    Ok(())
}

/// Handles the `/leaderboard` command.
pub async fn handle_leaderboard(bot: &Bot, chat_id: ChatId, db: &Db, tr: &Translations) -> CommandResult {
    let result = db.get_top_users(10).await;
    match result {
        Ok(top_users) if top_users.is_empty() => {
            bot.send_message(
                chat_id,
                "\u{1f3c6} <b>Leaderboard</b>\n\nAncora nessun utente. Invia il primo URL!",
            )
            .parse_mode(ParseMode::Html)
            .await
            .map_err(|e| {
                error!(?e, "Errore nell'invio leaderboard vuota");
                AppError::Telegram(e)
            })?;
        }
        Ok(top_users) => {
            let mut msg = String::from("\u{1f3c6} <b>Top 10 Pulitori</b>\n\n");
            for (idx, (_, count)) in top_users.iter().enumerate() {
                let medal = match idx {
                    0 => "\u{1f947}",
                    1 => "\u{1f948}",
                    2 => "\u{1f949}",
                    _ => "  ",
                };
                msg.push_str(&format!("{} #{}. <code>{}</code> URL puliti\n", medal, idx + 1, count));
            }
            bot.send_message(chat_id, msg)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio leaderboard");
                    AppError::Telegram(e)
                })?;
        }
        Err(e) => {
            error!(?e, "Errore nel caricamento della leaderboard");
            bot.send_message(chat_id, "\u{274c} Errore nel caricamento della leaderboard")
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio errore leaderboard");
                    AppError::Telegram(e)
                })?;
        }
    }
    Ok(())
}

/// Handles the `/trending` command.
pub async fn handle_trending(bot: &Bot, chat_id: ChatId, db: &Db, tr: &Translations) -> CommandResult {
    let result = db.get_top_links(10).await;
    match result {
        Ok(top_links) if top_links.is_empty() => {
            bot.send_message(
                chat_id,
                "\u{1f4c8} <b>URL Trending</b>\n\nAncora nessun URL processato",
            )
            .parse_mode(ParseMode::Html)
            .await
            .map_err(|e| {
                error!(?e, "Errore nell'invio trending vuoto");
                AppError::Telegram(e)
            })?;
        }
        Ok(top_links) => {
            let mut msg = String::from("\u{1f4c8} <b>Top 10 URL Più Puliti</b>\n\n");
            for (idx, (url, count)) in top_links.iter().enumerate() {
                let url_short = if url.len() > 50 {
                    format!("{}...", &url[..47])
                } else {
                    url.clone()
                };
                msg.push_str(&format!("{}. <code>{}</code> ({} volte)\n", idx + 1, url_short, count));
            }
            bot.send_message(chat_id, msg)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio trending");
                    AppError::Telegram(e)
                })?;
        }
        Err(e) => {
            error!(?e, "Errore nel caricamento dei trending");
            bot.send_message(chat_id, "\u{274c} Errore nel caricamento dei trending")
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio errore trending");
                    AppError::Telegram(e)
                })?;
        }
    }
    Ok(())
}

/// Handles the `/domains` command for per-domain statistics.
pub async fn handle_domains(bot: &Bot, chat_id: ChatId, user_id: i64, db: &Db) -> CommandResult {
    let result = db.get_domain_cleanup_stats(user_id).await;
    match result {
        Ok(domains) if domains.is_empty() => {
            bot.send_message(
                chat_id,
                "\u{1f310} <b>Statistiche per Dominio</b>\n\nAncora nessun URL processato",
            )
            .parse_mode(ParseMode::Html)
            .await
            .map_err(|e| {
                error!(?e, "Errore nell'invio domini vuoti");
                AppError::Telegram(e)
            })?;
        }
        Ok(domains) => {
            let mut msg = String::from("\u{1f310} <b>Tuoi Domini Più Puliti</b>\n\n");
            for (idx, (domain, count)) in domains.iter().enumerate() {
                msg.push_str(&format!(
                    "{}. <code>{}</code> – <b>{}</b> pulizie\n",
                    idx + 1, domain, count
                ));
            }
            bot.send_message(chat_id, msg)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio domini");
                    AppError::Telegram(e)
                })?;
        }
        Err(e) => {
            error!(?e, "Errore nel caricamento delle statistiche per dominio");
            bot.send_message(
                chat_id,
                "\u{274c} Errore nel caricamento delle statistiche per dominio",
            )
            .await
            .map_err(|e| {
                error!(?e, "Errore nell'invio errore domini");
                AppError::Telegram(e)
            })?;
        }
    }
    Ok(())
}

/// Handles the `/help` command.
pub async fn handle_help(bot: &Bot, chat_id: ChatId, tr: &Translations) -> CommandResult {
    bot.send_message(chat_id, tr.help_text.clone())
        .parse_mode(ParseMode::Html)
        .await
        .map_err(|e| {
            error!(?e, "Errore nell'invio dell'help");
            AppError::Telegram(e)
        })?;
    Ok(())
}

/// Handles the `/privacy` command.
pub async fn handle_privacy(bot: &Bot, chat_id: ChatId, tr: &Translations) -> CommandResult {
    let privacy_text = format!(
        "<b>🔒 Privacy</b>\n\n\
        Protezione dei dati GDPR compliant\n\n\
        <b>📊 Cosa raccogliamo:</b>\n\
        • I tuoi ID utente e chat sono hashed nei log per conformità GDPR\n\
        • La cronologia dei link puliti viene memorizzata localmente\n\
        • Nessun dato personale viene condiviso con servizi terzi\n\n\
        <b>🗑️ Gestione dati:</b>\n\
        • Cancella tutta la cronologia dei link puliti\n\
        • <code>/clear_history</code> Cancella cronologia e reset contatore\n\n\
        <b>📤 Esportazione dati:</b>\n\
        • <code>/export</code> Esporta i tuoi dati in formato JSON"
    );
    bot.send_message(chat_id, privacy_text)
        .parse_mode(ParseMode::Html)
        .await
        .map_err(|e| {
            error!(?e, "Errore nell'invio privacy");
            AppError::Telegram(e)
        })?;
    Ok(())
}

/// Handles whitelist display command.
pub async fn handle_whitelist_show(bot: &Bot, chat_id: ChatId, user_id: i64, db: &Db) -> CommandResult {
    let result = db.get_whitelist(user_id).await;
    match result {
        Ok(domains) => {
            let text = if domains.is_empty() {
                "\u{2b50} <b>La Tua Whitelist</b>\n\nVuota. Aggiungi domini con <code>/whitelist_add</code>"
                    .to_string()
            } else {
                let items = domains
                    .iter()
                    .enumerate()
                    .map(|(i, d)| format!("{}. <code>{}</code>", i + 1, d))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("\u{2b50} <b>La Tua Whitelist</b> ({})\n\n{}", domains.len(), items)
            };
            bot.send_message(chat_id, text)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio whitelist");
                    AppError::Telegram(e)
                })?;
        }
        Err(e) => {
            error!(?e, "Errore nel caricamento whitelist");
            bot.send_message(chat_id, "\u{274c} Errore nel caricamento")
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio errore whitelist");
                    AppError::Telegram(e)
                })?;
        }
    }
    Ok(())
}

/// Handles export command for user data.
pub async fn handle_export(bot: &Bot, chat_id: ChatId, user_id: i64, db: &Db) -> CommandResult {
    let result = db.get_history(user_id, 50).await;
    match result {
        Ok(links) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let json_data = serde_json::json!({
                "user_id": user_id,
                "exported_at": now,
                "total_links": links.len(),
                "links": links.iter().map(|link| {
                    serde_json::json!({
                        "original_url": link.original_url,
                        "cleaned_url": link.cleaned_url,
                        "provider": link.provider_name.as_deref().unwrap_or("Unknown")
                    })
                }).collect::<Vec<_>>()
            });

            let json_str = serde_json::to_string_pretty(&json_data).unwrap_or_default();
            let truncated = if json_str.len() > 1000 {
                format!("{}...[truncated]", &json_str[..1000])
            } else {
                json_str
            };
            let export_msg = format!(
                "<b>\u{1f4e5} Esportazione Dati</b>\n\n<pre>{}</pre>\n\n\
                <i>Ultimi 50 URL. Per il bulk export, contatta l'admin.</i>",
                truncated
            );

            bot.send_message(chat_id, export_msg)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio export");
                    AppError::Telegram(e)
                })?;
        }
        Err(e) => {
            error!(?e, "Errore nell'esportazione");
            bot.send_message(chat_id, "\u{274c} Errore nell'esportazione")
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio errore export");
                    AppError::Telegram(e)
                })?;
        }
    }
    Ok(())
}
