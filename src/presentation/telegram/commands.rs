//! Command handlers for Telegram bot.
//!
//! Extracts individual command logic from the main handler to improve readability and testability.

use teloxide::prelude::*;
use teloxide::types::{ChatId, ParseMode};
use tracing::error;

use crate::shared::error::{AppError, AppResult};

use crate::db::Db;
use crate::db::models::UserConfig;
use crate::i18n::Translations;
use crate::shared::security::{RATE_LIMIT_REQUESTS, RATE_LIMIT_WINDOW};

use super::helpers;

/// Represents a command execution result.
pub type CommandResult = AppResult<()>;

/// Handles the `/start` command.
pub async fn handle_start(
    bot: &Bot,
    chat_id: ChatId,
    user_id: i64,
    tr: &Translations,
    _args: &[&str],
) -> CommandResult {
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
#[allow(clippy::too_many_arguments)]
pub async fn handle_stats(
    bot: &Bot,
    chat_id: ChatId,
    user_id: i64,
    db: &Db,
    _user_config: &UserConfig,
    tr: &Translations,
    lang_code: &str,
    _args: &[&str],
) -> CommandResult {
    let stats_text = match db.get_user_config(user_id).await {
        Ok(config) => {
            let activity_level = (config.cleaned_count.min(100) / 10) as usize;
            let progress_bar =
                "\u{2588}".repeat(activity_level) + &"\u{2591}".repeat(10 - activity_level);

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

            let lang_name = helpers::language_name(lang_code);

            {
                let mut text = String::from(tr.cmd_stats_title);
                text.push_str(&format!("\n\n{}", tr.cmd_stats_urls.replace("{}", &config.cleaned_count.to_string())));
                text.push_str(&format!("\n{}", tr.cmd_stats_cleaned.replace("{}", &config.cleaned_count.to_string())));
                text.push_str(&format!("\n{}", tr.cmd_stats_ranking.replace("{}", &user_rank)));
                text.push_str(&format!("\n\n{}", tr.cmd_stats_activity.replace("{}", &activity_level.to_string())));
                text.push_str(&format!("\n{}", progress_bar));
                text.push_str(&format!("\n\n{}", tr.cmd_stats_language.replace("{}", &lang_name)));
                text.push_str(&format!("\n\n{}", tr.cmd_stats_config));
                text.push_str(&format!("\n{}", tr.cmd_stats_ai.replace("{}", if config.is_ai_enabled() { tr.cmd_stats_ai_enabled } else { tr.cmd_stats_ai_disabled })));
                text.push_str(&format!("\n{}", tr.cmd_stats_privacy.replace("{}", if config.privacy_mode != 0 { tr.cmd_stats_privacy_on } else { tr.cmd_stats_privacy_off })));
                text.push_str(&format!("\n{}", tr.cmd_stats_mode.replace("{}", if config.mode == "delete" { tr.cmd_stats_mode_delete } else { tr.cmd_stats_mode_reply })));
                text.push_str(&format!("\n\n{}", tr.cmd_stats_global.replace("{}", &total_users.to_string()).replace("{}", &total_cleaned.to_string())));
                text.push_str(&format!("\n\n{}", tr.cmd_stats_hint));
                text
            }
        },
        Err(e) => {
            error!(?e, "Errore nel recupero delle statistiche utente");
            tr.s_not_found.to_string()
        },
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
pub async fn handle_history(
    bot: &Bot,
    chat_id: ChatId,
    user_id: i64,
    db: &Db,
    tr: &Translations,
    _args: &[&str],
) -> CommandResult {
    let history_text = match db.get_history(user_id, 10).await {
        Ok(links) if links.is_empty() => {
            tr.cmd_history_empty.to_string()
        },
        Ok(links) => {
            let mut text = String::from(tr.cmd_history_title);
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
        },
        Err(e) => {
            error!(?e, "Errore nel caricamento della cronologia");
            tr.cmd_history_error.to_string()
        },
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
pub async fn handle_leaderboard(
    bot: &Bot,
    chat_id: ChatId,
    db: &Db,
    tr: &Translations,
    _args: &[&str],
) -> CommandResult {
    let result = db.get_top_users(10).await;
    match result {
        Ok(top_users) if top_users.is_empty() => {
            bot.send_message(chat_id, tr.cmd_leaderboard_empty)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio leaderboard vuota");
                    AppError::Telegram(e)
                })?;
        },
        Ok(top_users) => {
            let mut msg = String::from(tr.cmd_leaderboard_title);
            for (idx, (_, count)) in top_users.iter().enumerate() {
                let medal = match idx {
                    0 => "\u{1f947}",
                    1 => "\u{1f948}",
                    2 => "\u{1f949}",
                    _ => "  ",
                };
                msg.push_str(
                    &tr.cmd_leaderboard_row
                        .replace("{}", medal)
                        .replace("{}", &(idx + 1).to_string())
                        .replace("{}", &count.to_string()),
                );
            }
            bot.send_message(chat_id, msg)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio leaderboard");
                    AppError::Telegram(e)
                })?;
        },
        Err(e) => {
            error!(?e, "Errore nel caricamento della leaderboard");
            bot.send_message(chat_id, tr.cmd_leaderboard_error)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio errore leaderboard");
                    AppError::Telegram(e)
                })?;
        },
    }
    Ok(())
}

/// Handles the `/trending` command.
pub async fn handle_trending(
    bot: &Bot,
    chat_id: ChatId,
    db: &Db,
    tr: &Translations,
    _args: &[&str],
) -> CommandResult {
    let result = db.get_top_links(10).await;
    match result {
        Ok(top_links) if top_links.is_empty() => {
            bot.send_message(chat_id, tr.cmd_trending_empty)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio trending vuoto");
                    AppError::Telegram(e)
                })?;
        },
        Ok(top_links) => {
            let mut msg = String::from(tr.cmd_trending_title);
            for (idx, (url, count)) in top_links.iter().enumerate() {
                let url_short = if url.len() > 50 {
                    format!("{}...", &url[..47])
                } else {
                    url.clone()
                };
                msg.push_str(
                    &tr.cmd_trending_row
                        .replace("{}", &(idx + 1).to_string())
                        .replace("{}", &url_short)
                        .replace("{}", &count.to_string()),
                );
            }
            bot.send_message(chat_id, msg)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio trending");
                    AppError::Telegram(e)
                })?;
        },
        Err(e) => {
            error!(?e, "Errore nel caricamento dei trending");
            bot.send_message(chat_id, tr.cmd_trending_error)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio errore trending");
                    AppError::Telegram(e)
                })?;
        },
    }
    Ok(())
}

/// Handles the `/domains` command for per-domain statistics.
pub async fn handle_domains(
    bot: &Bot,
    chat_id: ChatId,
    user_id: i64,
    db: &Db,
    tr: &Translations,
    _args: &[&str],
) -> CommandResult {
    let result = db.get_domain_cleanup_stats(user_id).await;
    match result {
        Ok(domains) if domains.is_empty() => {
            bot.send_message(chat_id, tr.cmd_domains_empty)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio domini vuoti");
                    AppError::Telegram(e)
                })?;
        },
        Ok(domains) => {
            let mut msg = String::from(tr.cmd_domains_title);
            for (idx, (domain, count)) in domains.iter().enumerate() {
                msg.push_str(
                    &tr.cmd_domains_row
                        .replace("{}", &(idx + 1).to_string())
                        .replace("{}", domain)
                        .replace("{}", &count.to_string()),
                );
            }
            bot.send_message(chat_id, msg)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio domini");
                    AppError::Telegram(e)
                })?;
        },
        Err(e) => {
            error!(?e, "Errore nel caricamento delle statistiche per dominio");
            bot.send_message(chat_id, tr.cmd_domains_error)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio errore domini");
                    AppError::Telegram(e)
                })?;
        },
    }
    Ok(())
}

/// Handles the `/help` command.
pub async fn handle_help(
    bot: &Bot,
    chat_id: ChatId,
    tr: &Translations,
    _args: &[&str],
) -> CommandResult {
    bot.send_message(chat_id, tr.help_text)
        .parse_mode(ParseMode::Html)
        .await
        .map_err(|e| {
            error!(?e, "Errore nell'invio dell'help");
            AppError::Telegram(e)
        })?;
    Ok(())
}

/// Handles the `/privacy` command.
pub async fn handle_privacy(
    bot: &Bot,
    chat_id: ChatId,
    tr: &Translations,
    _args: &[&str],
) -> CommandResult {
    bot.send_message(chat_id, tr.cmd_privacy)
        .parse_mode(ParseMode::Html)
        .await
        .map_err(|e| {
            error!(?e, "Errore nell'invio privacy");
            AppError::Telegram(e)
        })?;
    Ok(())
}

/// Handles the `/export` command for user data.
pub async fn handle_export(
    bot: &Bot,
    chat_id: ChatId,
    user_id: i64,
    db: &Db,
    tr: &Translations,
    _args: &[&str],
) -> CommandResult {
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
            let export_msg = tr.cmd_export_text.replace("{}", &truncated);

            bot.send_message(chat_id, export_msg)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio export");
                    AppError::Telegram(e)
                })?;
        },
        Err(e) => {
            error!(?e, "Errore nell'esportazione");
            bot.send_message(chat_id, tr.cmd_export_error)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio errore export");
                    AppError::Telegram(e)
                })?;
        },
    }
    Ok(())
}

/// Handles the `/menu` command to show main keyboard.
pub async fn handle_menu(
    bot: &Bot,
    chat_id: ChatId,
    tr: &Translations,
    _args: &[&str],
) -> CommandResult {
    bot.send_message(chat_id, tr.reply_keyboard_opened)
        .reply_markup(super::helpers::main_reply_keyboard(tr))
        .parse_mode(ParseMode::Html)
        .await
        .map_err(|e| {
            error!(?e, "Errore nell'invio menu");
            AppError::Telegram(e)
        })?;
    Ok(())
}

/// Handles the `/hidekbd` command to hide keyboard.
pub async fn handle_hidekbd(bot: &Bot, chat_id: ChatId, _args: &[&str]) -> CommandResult {
    bot.send_message(chat_id, "⌨️ Keyboard hidden")
        .reply_markup(teloxide::types::KeyboardRemove::new())
        .await
        .map_err(|e| {
            error!(?e, "Errore nell'invio hidekbd");
            AppError::Telegram(e)
        })?;
    Ok(())
}

/// Handles the `/whitelist` command — shows whitelisted domains.
pub async fn handle_whitelist(
    bot: &Bot,
    chat_id: ChatId,
    user_id: i64,
    db: &Db,
    tr: &Translations,
    _args: &[&str],
) -> CommandResult {
    let result = db.get_whitelist(user_id).await;
    match result {
        Ok(domains) => {
            let text = if domains.is_empty() {
                format!(
                    "{}\n\n<i>{}\n{}</i>",
                    tr.cmd_whitelist_count.replace("{}", "0"),
                    tr.cmd_whitelist_add,
                    tr.cmd_whitelist_remove,
                )
            } else {
                let items = domains
                    .iter()
                    .enumerate()
                    .map(|(i, d)| format!("{}. <code>{}</code>", i + 1, d))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "{}\n\n{}\n\n<i>{}\n{}</i>",
                    tr.cmd_whitelist_count.replace("{}", &domains.len().to_string()),
                    items,
                    tr.cmd_whitelist_add,
                    tr.cmd_whitelist_remove,
                )
            };
            bot.send_message(chat_id, text)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio whitelist");
                    AppError::Telegram(e)
                })?;
        },
        Err(e) => {
            error!(?e, "Errore nel caricamento whitelist");
            bot.send_message(chat_id, tr.cmd_whitelist_error)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio errore whitelist");
                    AppError::Telegram(e)
                })?;
        },
    }
    Ok(())
}

/// Handles the `/settings` command.
#[allow(clippy::too_many_arguments)]
pub async fn handle_settings(
    bot: &Bot,
    chat_id: ChatId,
    user_id: i64,
    db: &Db,
    _user_config: &UserConfig,
    tr: &Translations,
    lang_code: &str,
    _args: &[&str],
) -> CommandResult {
    // Fetch fresh config in case settings were modified via callbacks
    let user_config = match db.get_user_config(user_id).await {
        Ok(config) => config,
        Err(e) => {
            error!(?e, user_id, "Failed to get user config for settings");
            bot.send_message(chat_id, tr.cmd_settings_error)
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio messaggio errore settings");
                    AppError::Telegram(e)
                })?;
            return Ok(());
        },
    };

    let enabled_status = if user_config.is_enabled() {
        tr.cmd_settings_enabled
    } else {
        tr.cmd_settings_disabled
    };
    let ai_status = if user_config.is_ai_enabled() {
        tr.cmd_settings_ai_enabled
    } else {
        tr.cmd_settings_ai_disabled
    };
    let privacy_status = if user_config.privacy_mode != 0 {
        tr.cmd_settings_privacy_on
    } else {
        tr.cmd_settings_privacy_off
    };
    let mode_text = match user_config.mode.as_str() {
        "reply" => tr.cmd_settings_mode_reply,
        "inline" => tr.cmd_settings_mode_inline,
        _ => tr.unknown,
    };

    let settings_text = format!(
        "{}\n\n{}\n{}\n{}\n{}\n{}\n{}\n\n{}",
        tr.cmd_settings_title,
        tr.cmd_settings_status.replace("{}", enabled_status),
        tr.cmd_settings_ai.replace("{}", ai_status),
        tr.cmd_settings_mode.replace("{}", mode_text),
        tr.cmd_settings_privacy.replace("{}", privacy_status),
        tr.cmd_settings_language.replace("{}", &lang_code.to_uppercase()),
        tr.cmd_settings_cleaned.replace("{}", &user_config.cleaned_count.to_string()),
        tr.cmd_settings_hint,
    );

    bot.send_message(chat_id, settings_text)
        .parse_mode(ParseMode::Html)
        .await
        .map_err(|e| {
            error!(?e, "Errore nell'invio settings");
            AppError::Telegram(e)
        })?;

    Ok(())
}

/// Handles the `/limits` command — shows rate limiting info.
pub async fn handle_limits(
    bot: &Bot,
    chat_id: ChatId,
    _user_config: &UserConfig,
    _tr: &Translations,
    _args: &[&str],
) -> CommandResult {
    let msg = format!(
        "<b>\u{26a1} Limiti API</b>\n\n\
        \u{1f504} Richieste: <b>{}</b> al minuto\n\
        \u{23f1}\u{1fe0f} Finestra: <b>{}</b> secondi\n\n\
        \u{2139}\u{1fe0f} I limiti si azzerano automaticamente.",
        RATE_LIMIT_REQUESTS, RATE_LIMIT_WINDOW,
    );
    bot.send_message(chat_id, msg)
        .parse_mode(ParseMode::Html)
        .await
        .map_err(|e| {
            error!(?e, "Errore nell'invio limits");
            AppError::Telegram(e)
        })?;
    Ok(())
}

/// Handles `/whitelist_add <domain>`.
pub async fn handle_whitelist_add(
    bot: &Bot,
    chat_id: ChatId,
    user_id: i64,
    db: &Db,
    args: &[&str],
) -> CommandResult {
    if args.len() < 2 {
        bot.send_message(chat_id, "❌ Uso: /whitelist_add <dominio>")
            .parse_mode(ParseMode::Html)
            .await
            .map_err(|e| {
                error!(?e, "Errore nell'invio help whitelist_add");
                AppError::Telegram(e)
            })?;
        return Ok(());
    }

    let domain = args[1].to_string();
    match db.add_to_whitelist(user_id, &domain).await {
        Ok(_) => {
            bot.send_message(
                chat_id,
                format!("✅ <b>{}</b> aggiunto alla whitelist", domain),
            )
            .parse_mode(ParseMode::Html)
            .await
            .map_err(|e| {
                error!(?e, "Errore nell'invio conferma whitelist_add");
                AppError::Telegram(e)
            })?;
        },
        Err(e) => {
            error!(?e, "Errore nell'aggiunta alla whitelist");
            bot.send_message(
                chat_id,
                format!("⚠️ <b>{}</b> è già nella whitelist", domain),
            )
            .parse_mode(ParseMode::Html)
            .await
            .map_err(|e| {
                error!(?e, "Errore nell'invio errore whitelist_add");
                AppError::Telegram(e)
            })?;
        },
    }
    Ok(())
}

/// Handles `/whitelist_remove <domain>`.
pub async fn handle_whitelist_remove(
    bot: &Bot,
    chat_id: ChatId,
    user_id: i64,
    db: &Db,
    args: &[&str],
) -> CommandResult {
    if args.len() < 2 {
        bot.send_message(chat_id, "❌ Uso: /whitelist_remove <dominio>")
            .parse_mode(ParseMode::Html)
            .await
            .map_err(|e| {
                error!(?e, "Errore nell'invio help whitelist_remove");
                AppError::Telegram(e)
            })?;
        return Ok(());
    }

    let domain = args[1].to_string();
    match db.remove_from_whitelist(user_id, &domain).await {
        Ok(_) => {
            bot.send_message(
                chat_id,
                format!("✅ <b>{}</b> rimosso dalla whitelist", domain),
            )
            .parse_mode(ParseMode::Html)
            .await
            .map_err(|e| {
                error!(?e, "Errore nell'invio conferma whitelist_remove");
                AppError::Telegram(e)
            })?;
        },
        Err(e) => {
            error!(?e, "Errore nella rimozione dalla whitelist");
            bot.send_message(chat_id, "❌ Errore rimuovendo il dominio dalla whitelist")
                .parse_mode(ParseMode::Html)
                .await
                .map_err(|e| {
                    error!(?e, "Errore nell'invio errore whitelist_remove");
                    AppError::Telegram(e)
                })?;
        },
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
