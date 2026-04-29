use crate::db::models::UserConfig;
use crate::i18n;
use crate::presentation::telegram::helpers;
use teloxide::prelude::*;
use teloxide::types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup, MessageId};
use teloxide::RequestError;

use super::helpers::{
    callback_target_user_id, settings_back_keyboard, show_no_permission_view, single_back_keyboard,
    upsert_settings_view,
};

pub struct CallbackContext {
    pub bot: Bot,
    pub chat_id: ChatId,
    pub message_id: Option<MessageId>,
    pub user_id: i64,
}

pub async fn handle_start_command(
    bot: Bot,
    chat_id: ChatId,
    user_id: i64,
    tr: &i18n::Translations,
    _config: &crate::config::Config,
    message_id: Option<MessageId>,
) -> Result<(), RequestError> {
    let welcome_text = tr.welcome.replace("{}", &user_id.to_string());

    let keyboard = InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback(tr.start_open_settings, format!("settings:{}", user_id)),
        InlineKeyboardButton::callback(
            tr.start_view_stats,
            format!("user_setting:stats:{}", user_id),
        ),
    ]]);

    upsert_settings_view(
        &bot,
        chat_id,
        message_id,
        welcome_text,
        Some(keyboard),
        true,
    )
    .await?;

    if message_id.is_none() {
        bot.send_message(chat_id, tr.reply_keyboard_opened)
            .reply_markup(helpers::main_reply_keyboard(tr))
            .await?;
    }

    Ok(())
}

pub async fn handle_settings_callback(
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    user_id: i64,
    _db: crate::db::Db,
    config: crate::config::Config,
    tr: &i18n::Translations,
) -> Result<(), RequestError> {
    let is_admin = user_id == config.admin_id;

    let role = if is_admin {
        tr.s_role_admin
    } else {
        tr.s_role_user
    };

    let settings_text = format!(
        "<b>\u{2699}\u{1fe0f}  {}</b>\n\n\
        <b>\u{1f464} Profilo:</b>\n\
        ID: <code>{}</code>\n\
        Ruolo: <b>{}</b>\n\n\
        <b>\u{1f4cb} Impostazioni disponibili:</b>",
        tr.s_menu_title, user_id, role
    );

    let mut keyboard_rows = vec![
        vec![
            InlineKeyboardButton::callback(
                format!("\u{1f514} {}", tr.s_notifications),
                format!("user_setting:notifications:{}", user_id),
            ),
            InlineKeyboardButton::callback(
                format!("\u{1f916} {}", tr.s_ai_settings),
                format!("user_setting:ai:{}", user_id),
            ),
        ],
        vec![
            InlineKeyboardButton::callback(
                format!("\u{1f512} {}", tr.s_privacy),
                format!("user_setting:privacy:{}", user_id),
            ),
            InlineKeyboardButton::callback(
                format!("\u{26a1} {}", tr.s_link_processing),
                format!("user_setting:links:{}", user_id),
            ),
        ],
        vec![InlineKeyboardButton::callback(
            format!("\u{1f310} {}", tr.s_language),
            format!("user_setting:language:{}", user_id),
        )],
    ];

    if is_admin {
        keyboard_rows.push(vec![
            InlineKeyboardButton::callback(
                format!("\u{1f6e0}\u{1fe0f}  {}", tr.s_admin_panel),
                format!("admin_setting:panel:{}", user_id),
            ),
            InlineKeyboardButton::callback(
                format!("\u{1f4ca} {}", tr.s_statistics),
                format!("admin_setting:stats:{}", user_id),
            ),
        ]);
    }

    keyboard_rows.push(vec![InlineKeyboardButton::callback(
        format!("\u{25c0}\u{1fe0f}  {}", tr.s_back_to_main),
        format!("back_to_main:{}", user_id),
    )]);

    let keyboard = InlineKeyboardMarkup::new(keyboard_rows);

    upsert_settings_view(
        &bot,
        chat_id,
        message_id,
        settings_text,
        Some(keyboard),
        true,
    )
    .await
}

pub async fn handle_quick_callback(
    ctx: CallbackContext,
    callback_data: &str,
    db: crate::db::Db,
    config: crate::config::Config,
    tr: &i18n::Translations,
) -> Result<(), RequestError> {
    let parts: Vec<&str> = callback_data.split(':').collect();
    if parts.len() < 3 {
        return Ok(());
    }

    let target_user_id = callback_target_user_id(&parts, ctx.user_id);
    if target_user_id != ctx.user_id {
        show_no_permission_view(&ctx.bot, ctx.chat_id, ctx.message_id, tr).await?;
        return Ok(());
    }

    match parts[1] {
        "settings" => {
            handle_settings_callback(
                ctx.bot,
                ctx.chat_id,
                ctx.message_id,
                ctx.user_id,
                db,
                config,
                tr,
            )
            .await
        }
        "stats" => {
            let user_config = db.get_user_config(ctx.user_id).await.unwrap_or_default();
            let stats_text = tr
                .stats_text
                .replace("{}", &user_config.cleaned_count.to_string());
            upsert_settings_view(
                &ctx.bot,
                ctx.chat_id,
                ctx.message_id,
                stats_text,
                Some(helpers::quick_actions_inline_keyboard(tr, ctx.user_id)),
                true,
            )
            .await
        }
        "help" => {
            upsert_settings_view(
                &ctx.bot,
                ctx.chat_id,
                ctx.message_id,
                tr.help_text.to_string(),
                Some(helpers::quick_actions_inline_keyboard(tr, ctx.user_id)),
                true,
            )
            .await
        }
        "language" => {
            let user_config = db.get_user_config(ctx.user_id).await.unwrap_or_default();
            let language_text = format!(
                "<b>{}</b>\n\n{} <b>{}</b>",
                tr.s_language_title, tr.s_language_current, user_config.language
            );
            upsert_settings_view(
                &ctx.bot,
                ctx.chat_id,
                ctx.message_id,
                language_text,
                Some(helpers::language_inline_keyboard(tr, ctx.user_id)),
                true,
            )
            .await
        }
        _ => Ok(()),
    }
}

pub async fn handle_user_settings_callback(
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    user_id: i64,
    callback_data: &str,
    db: crate::db::Db,
    tr: &i18n::Translations,
) -> Result<(), RequestError> {
    let parts: Vec<&str> = callback_data.split(':').collect();
    if parts.len() < 3 {
        return Ok(());
    }

    let target_user_id = callback_target_user_id(&parts, user_id);
    if target_user_id != user_id {
        show_no_permission_view(&bot, chat_id, message_id, tr).await?;
        return Ok(());
    }

    let setting_type = parts[1];
    let user_config: UserConfig = db.get_user_config(user_id).await.unwrap_or_default();

    let (message_text, keyboard) = match setting_type {
        "notifications" => {
            let current_status = if user_config.is_enabled() {
                tr.s_notif_enabled
            } else {
                tr.s_notif_disabled
            };
            let message = format!(
                "<b>{}</b>\n\n{} <b>{}</b>\n\n{}",
                tr.s_notif_title, tr.s_notif_current_status, current_status, tr.s_notif_desc
            );
            let keyboard = InlineKeyboardMarkup::new(vec![
                vec![
                    InlineKeyboardButton::callback(
                        format!("\u{2705} {}", tr.s_enabled),
                        format!("user_setting:toggle:notif:1:{}", user_id),
                    ),
                    InlineKeyboardButton::callback(
                        format!("\u{274c} {}", tr.s_disabled),
                        format!("user_setting:toggle:notif:0:{}", user_id),
                    ),
                ],
                vec![InlineKeyboardButton::callback(
                    tr.s_back,
                    format!("settings:{}", user_id),
                )],
            ]);
            (message, keyboard)
        }
        "ai" => {
            let ai_status = if user_config.is_ai_enabled() {
                tr.s_ai_status_enabled
            } else {
                tr.s_ai_status_disabled
            };
            let message = format!(
                "<b>{}</b>\n\n{} <b>{}</b>\n\n{}",
                tr.s_ai_title, tr.s_ai_current_status, ai_status, tr.s_ai_desc
            );
            let toggle_text = if user_config.is_ai_enabled() {
                format!("\u{1f534} {}", tr.s_disabled)
            } else {
                format!("\u{1f7e2} {}", tr.s_enabled)
            };
            let keyboard = InlineKeyboardMarkup::new(vec![
                vec![InlineKeyboardButton::callback(
                    toggle_text,
                    format!("user_setting:toggle:ai:{}", user_id),
                )],
                vec![InlineKeyboardButton::callback(
                    tr.s_back,
                    format!("settings:{}", user_id),
                )],
            ]);
            (message, keyboard)
        }
        "privacy" => (
            format!("<b>{}</b>\n\n{}", tr.s_privacy_title, tr.s_privacy_desc),
            InlineKeyboardMarkup::new(vec![
                vec![InlineKeyboardButton::callback(
                    tr.s_clear_history,
                    format!("user_setting:clear_history:{}", user_id),
                )],
                vec![InlineKeyboardButton::callback(
                    tr.s_back,
                    format!("settings:{}", user_id),
                )],
            ]),
        ),
        "links" => {
            let mode_label = match user_config.mode.as_str() {
                "reply" => tr.s_reply_mode,
                "delete" => tr.s_delete_mode,
                _ => user_config.mode.as_str(),
            };
            let message = format!(
                "<b>{}</b>\n\n{}: <b>{}</b>\n\n{}",
                tr.s_links_title, tr.s_action_mode, mode_label, tr.s_links_desc
            );
            let keyboard = InlineKeyboardMarkup::new(vec![
                vec![
                    InlineKeyboardButton::callback(
                        tr.s_reply_mode,
                        format!("user_setting:set_mode:reply:{}", user_id),
                    ),
                    InlineKeyboardButton::callback(
                        tr.s_delete_mode,
                        format!("user_setting:set_mode:delete:{}", user_id),
                    ),
                ],
                vec![InlineKeyboardButton::callback(
                    tr.s_back,
                    format!("settings:{}", user_id),
                )],
            ]);
            (message, keyboard)
        }
        "language" => {
            let message = format!(
                "<b>{}</b>\n\n{} <b>{}</b>",
                tr.s_language_title, tr.s_language_current, user_config.language
            );
            let keyboard = helpers::language_inline_keyboard(tr, user_id);
            (message, keyboard)
        }
        "lang" if parts.len() >= 4 => {
            let language = parts[2];
            let mut updated = user_config.clone();
            let mut ok = true;
            let lang_codes = crate::presentation::telegram::helpers::SUPPORTED_LANGUAGES;
            let new_translations = if lang_codes.contains(&language) {
                i18n::get_translations(language)
            } else {
                tr.clone()
            };
            let new_tr = &new_translations;

            if lang_codes.contains(&language) {
                updated.language = language.to_string();
                if let Err(e) = db.save_user_config(&updated).await {
                    tracing::error!(error = %e, user_id, "Errore nel salvataggio lingua");
                    ok = false;
                }
            } else {
                ok = false;
            }

            let text = if ok {
                format!(
                    "✅ <b>Lingua cambiata a {}</b>",
                    crate::presentation::telegram::helpers::language_name(language)
                )
            } else {
                tr.s_setting_update_failed.to_string()
            };
            let keyboard = settings_back_keyboard(new_tr, user_id);
            (text, keyboard)
        }
        "stats" => {
            let stats_text = tr
                .stats_text
                .replace("{}", &user_config.cleaned_count.to_string());
            let keyboard = settings_back_keyboard(tr, user_id);
            (stats_text, keyboard)
        }
        "set_mode" if parts.len() >= 4 => {
            let mode = parts[2];
            let mut mode_save_ok = true;
            if mode == "reply" || mode == "delete" {
                let mut updated = user_config.clone();
                updated.mode = mode.to_string();
                if let Err(e) = db.save_user_config(&updated).await {
                    tracing::error!(error = %e, user_id, "Errore nel salvataggio modalita' link");
                    mode_save_ok = false;
                }
            }

            let refreshed = db.get_user_config(user_id).await.unwrap_or_default();
            let mode_label = match refreshed.mode.as_str() {
                "reply" => tr.s_reply_mode,
                "delete" => tr.s_delete_mode,
                _ => refreshed.mode.as_str(),
            };

            let message = format!(
                "<b>{}</b>\n\n{}: <b>{}</b>\n\n{}",
                tr.s_links_title,
                tr.s_action_mode,
                mode_label,
                if mode_save_ok {
                    tr.s_links_desc
                } else {
                    tr.s_setting_update_failed
                }
            );
            let keyboard = InlineKeyboardMarkup::new(vec![
                vec![
                    InlineKeyboardButton::callback(
                        tr.s_reply_mode,
                        format!("user_setting:set_mode:reply:{}", user_id),
                    ),
                    InlineKeyboardButton::callback(
                        tr.s_delete_mode,
                        format!("user_setting:set_mode:delete:{}", user_id),
                    ),
                ],
                vec![InlineKeyboardButton::callback(
                    tr.s_back,
                    format!("settings:{}", user_id),
                )],
            ]);
            (message, keyboard)
        }
        "clear_history" => {
            let message = format!(
                "<b>⚠️ {}</b>\n\n\
                {}\n\n\
                {}",
                tr.s_clear_history,
                "Questa azione cancellerà permanentemente tutta la cronologia dei link puliti e azzererà il contatore. Questa operazione non può essere annullata.",
                "Sei sicuro di voler procedere?"
            );
            let keyboard = InlineKeyboardMarkup::new(vec![vec![
                InlineKeyboardButton::callback(
                    "✅ Sì, cancella tutto",
                    format!("user_setting:clear_history_confirm:{}", user_id),
                ),
                InlineKeyboardButton::callback(
                    tr.s_back,
                    format!("user_setting:privacy:{}", user_id),
                ),
            ]]);
            (message, keyboard)
        }
        "clear_history_confirm" => {
            let mut clear_ok = true;
            if let Err(e) = db.clear_history(user_id).await {
                tracing::error!(error = %e, user_id, "Errore nella cancellazione cronologia");
                clear_ok = false;
            }
            let mut updated = user_config.clone();
            updated.cleaned_count = 0;
            if let Err(e) = db.save_user_config(&updated).await {
                tracing::error!(error = %e, user_id, "Errore nel reset contatore pulizie");
                clear_ok = false;
            }

            let keyboard = settings_back_keyboard(tr, user_id);
            (
                if clear_ok {
                    "✅ Cronologia cancellata con successo e contatore azzerato.".to_string()
                } else {
                    tr.s_setting_update_failed.to_string()
                },
                keyboard,
            )
        }
        "toggle" if parts.len() >= 4 => {
            let setting = parts[2];
            let value = parts[3];

            handle_setting_toggle(bot, chat_id, message_id, user_id, setting, value, db, tr)
                .await?;
            return Ok(());
        }
        _ => (
            tr.s_not_found.to_string(),
            settings_back_keyboard(tr, user_id),
        ),
    };

    upsert_settings_view(
        &bot,
        chat_id,
        message_id,
        message_text,
        Some(keyboard),
        true,
    )
    .await?;

    Ok(())
}

pub async fn handle_admin_settings_callback(
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    user_id: i64,
    callback_data: &str,
    db: crate::db::Db,
    config: &crate::config::Config,
    tr: &i18n::Translations,
) -> Result<(), RequestError> {
    if user_id != config.admin_id {
        show_no_permission_view(&bot, chat_id, message_id, tr).await?;
        return Ok(());
    }

    let parts: Vec<&str> = callback_data.split(':').collect();
    if parts.len() < 3 {
        return Ok(());
    }

    let target_user_id = callback_target_user_id(&parts, user_id);
    if target_user_id != user_id {
        show_no_permission_view(&bot, chat_id, message_id, tr).await?;
        return Ok(());
    }

    let admin_action = parts[1];

    let (message_text, keyboard) = match admin_action {
        "panel" => {
            let message = format!(
                "<b>{}</b>\n\n{}",
                tr.s_admin_panel_title, tr.s_admin_panel_desc
            );
            let keyboard = InlineKeyboardMarkup::new(vec![
                vec![
                    InlineKeyboardButton::callback(
                        tr.s_user_management,
                        format!("admin_setting:users:{}", user_id),
                    ),
                    InlineKeyboardButton::callback(
                        tr.s_system_settings,
                        format!("admin_setting:system:{}", user_id),
                    ),
                ],
                vec![
                    InlineKeyboardButton::callback(
                        tr.s_global_stats,
                        format!("admin_setting:global_stats:{}", user_id),
                    ),
                    InlineKeyboardButton::callback(
                        tr.s_maintenance,
                        format!("admin_setting:maintenance:{}", user_id),
                    ),
                ],
                vec![InlineKeyboardButton::callback(
                    tr.s_back,
                    format!("settings:{}", user_id),
                )],
            ]);
            (message, keyboard)
        }
        "stats" | "refresh_stats" => {
            let (total_cleaned, total_users) = db.get_global_stats().await.unwrap_or((0, 0));
            let message = helpers::admin_global_stats_message(tr, total_users, total_cleaned);
            let back_data = if admin_action == "stats" {
                format!("settings:{}", user_id)
            } else {
                format!("admin_setting:panel:{}", user_id)
            };
            let keyboard = helpers::admin_global_stats_keyboard(tr, user_id, back_data);
            (message, keyboard)
        }
        "users" => {
            let (_, total_users) = db.get_global_stats().await.unwrap_or((0, 0));
            let message = helpers::admin_users_message(tr, total_users);
            let keyboard =
                single_back_keyboard(tr.s_back, format!("admin_setting:panel:{}", user_id));
            (message, keyboard)
        }
        "system" => {
            let message = helpers::admin_system_message(tr);
            let keyboard =
                single_back_keyboard(tr.s_back, format!("admin_setting:panel:{}", user_id));
            (message, keyboard)
        }
        "global_stats" => {
            let (total_cleaned, total_users) = db.get_global_stats().await.unwrap_or((0, 0));
            let message = helpers::admin_global_stats_message(tr, total_users, total_cleaned);
            let keyboard = helpers::admin_global_stats_keyboard(
                tr,
                user_id,
                format!("admin_setting:panel:{}", user_id),
            );
            (message, keyboard)
        }
        "maintenance" => {
            let message = helpers::admin_maintenance_message(tr);
            let keyboard = helpers::admin_maintenance_keyboard(tr, user_id);
            (message, keyboard)
        }
        "clear_all_history" => {
            let message = tr.s_admin_server_only_op.to_string();
            let keyboard =
                single_back_keyboard(tr.s_back, format!("admin_setting:maintenance:{}", user_id));
            (message, keyboard)
        }
        _ => {
            let message = tr.s_admin_option_not_found.to_string();
            let keyboard = settings_back_keyboard(tr, user_id);
            (message, keyboard)
        }
    };

    upsert_settings_view(
        &bot,
        chat_id,
        message_id,
        message_text,
        Some(keyboard),
        true,
    )
    .await?;

    Ok(())
}

pub async fn handle_setting_toggle(
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    user_id: i64,
    setting: &str,
    value: &str,
    db: crate::db::Db,
    tr: &i18n::Translations,
) -> Result<(), RequestError> {
    let mut user_config = db.get_user_config(user_id).await.unwrap_or_default();
    let mut save_ok = true;
    let result_message = match setting {
        "notif" => {
            user_config.enabled = if value == "1" { 1 } else { 0 };
            if let Err(e) = db.save_user_config(&user_config).await {
                tracing::error!(error = %e, user_id, "Errore nel salvataggio toggle notifiche");
                save_ok = false;
            }
            if !save_ok {
                tr.s_setting_update_failed
            } else if user_config.enabled == 1 {
                tr.s_notif_enabled
            } else {
                tr.s_notif_disabled
            }
        }
        "ai" => {
            user_config.ai_enabled = if user_config.ai_enabled == 0 { 1 } else { 0 };
            if let Err(e) = db.save_user_config(&user_config).await {
                tracing::error!(error = %e, user_id, "Errore nel salvataggio toggle AI");
                save_ok = false;
            }
            if save_ok {
                tr.s_ai_toggled
            } else {
                tr.s_setting_update_failed
            }
        }
        _ => tr.s_setting_updated,
    };

    let keyboard = settings_back_keyboard(tr, user_id);

    upsert_settings_view(
        &bot,
        chat_id,
        message_id,
        result_message.to_string(),
        Some(keyboard),
        true,
    )
    .await?;

    Ok(())
}
