use std::collections::HashSet;

use teloxide::RequestError;
use teloxide::prelude::*;
use teloxide::types::{ChatAction, ChatId, Message, ParseMode, ReplyParameters};
use teloxide::utils::html;
use tracing;

use crate::presentation::telegram::command_dispatcher;
use crate::presentation::telegram::helpers;
use crate::presentation::telegram::security_scan;
use crate::presentation::telegram::settings;
use crate::i18n;
use crate::metrics;
use crate::redirects::RedirectService;
use crate::sanitizer::{AiEngine, RuleEngine, linkumori::LinkumoriEngine};
use crate::shared::security::{hash_user_id, is_safe_url_scheme};

#[tracing::instrument(
    skip(bot, db, rules, ai, linkumori, config, event_tx, redirect_service),
    fields(chat_id = %msg.chat.id, user_id)
)]
#[allow(clippy::too_many_arguments)]
pub async fn handle_edited_message(
    bot: Bot,
    msg: Message,
    db: crate::db::Db,
    rules: RuleEngine,
    ai: AiEngine,
    linkumori: LinkumoriEngine,
    config: crate::config::Config,
    event_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    redirect_service: RedirectService,
) -> Result<(), RequestError> {
    metrics::REQUESTS_EDITED.inc();
    tracing::info!(chat_id = %msg.chat.id, msg_id = %msg.id, "Elaborazione messaggio modificato");
    handle_message(bot, msg, db, rules, ai, linkumori, config, event_tx, redirect_service).await
}

#[tracing::instrument(
    skip(bot, db, rules, ai, linkumori, config, event_tx, redirect_service),
    fields(chat_id = %msg.chat.id, user_id)
)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
pub async fn handle_message(
    bot: Bot,
    msg: Message,
    db: crate::db::Db,
    rules: RuleEngine,
    ai: AiEngine,
    linkumori: LinkumoriEngine,
    config: crate::config::Config,
    event_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    redirect_service: RedirectService,
) -> Result<(), RequestError> {
    metrics::REQUESTS_MESSAGE.inc();
    let user_id = msg
        .from
        .as_ref()
        .map(|u| i64::try_from(u.id.0).unwrap_or(0))
        .unwrap_or(0);
    let chat_id = msg.chat.id;
    let msg_text = msg.text().map(|t| t.to_string()).unwrap_or_default();

    tracing::info!(
        user_id_hash = hash_user_id(user_id),
        chat_id = %chat_id,
        text_len = msg_text.len(),
        "Messaggio ricevuto"
    );
    let msg_clone = msg.clone();
    let user_config = db.get_user_config(user_id).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "Errore nel recupero config utente, uso default");
        if user_id != config.admin_id && config.admin_id != 0 {
            let admin_chat = ChatId(config.admin_id);
            let admin_msg = format!("[CRITICAL] Errore DB per user {}: {}", user_id, e);
            let bot_clone = bot.clone();
            tokio::spawn(async move {
                let _ = bot_clone.send_message(admin_chat, admin_msg).await;
            });
        }
        crate::db::models::UserConfig::default()
    });

    let text = msg_text.as_str();

    let lang_code = helpers::get_user_language(
        &db,
        user_id,
        msg.from.as_ref().and_then(|u| u.language_code.as_deref()),
    )
    .await;

    let tr = i18n::get_translations(&lang_code);

    let has_urls = helpers::has_url_entities(&msg, text);

    let command_ctx = command_dispatcher::CommandContext {
        bot: bot.clone(),
        chat_id,
        user_id,
        db: db.clone(),
        rules: rules.clone(),
        ai: ai.clone(),
        config: config.clone(),
        tr: tr.clone(),
        user_config: user_config.clone(),
        lang_code: lang_code.clone(),
    };

    if msg_text.starts_with('/')
        && let Err(err) = command_dispatcher::dispatch_command(text, &command_ctx).await
    {
        tracing::warn!(error = %err, "Errore nell'esecuzione del dispatcher comando");
        let _ = bot
            .send_message(chat_id, tr.cmd_internal_error)
            .await;
        return Ok(());
    }
    if let Some(text_val) = msg.text()
        && let Some(action) = helpers::quick_reply_action(text_val, &tr)
    {
        match action {
            helpers::QuickReplyAction::Settings => {
                settings::handle_settings_callback(
                    bot.clone(),
                    chat_id,
                    None,
                    user_id,
                    db.clone(),
                    config.clone(),
                    &tr,
                )
                .await?;
                return Ok(());
            },
            helpers::QuickReplyAction::Stats => {
                let stats_text = tr
                    .stats_text
                    .replace("{}", &user_config.cleaned_count.to_string());
                bot.send_message(chat_id, stats_text)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(helpers::quick_actions_inline_keyboard(&tr, user_id))
                    .await?;
                return Ok(());
            },
            helpers::QuickReplyAction::Help => {
                bot.send_message(chat_id, tr.help_text)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(helpers::quick_actions_inline_keyboard(&tr, user_id))
                    .await?;
                return Ok(());
            },
            helpers::QuickReplyAction::HideKeyboard => {
                bot.send_message(chat_id, tr.reply_keyboard_hidden)
                    .reply_markup(teloxide::types::KeyboardRemove::new())
                    .await?;
                return Ok(());
            },
        }
    }

    let is_group_context =
        msg_clone.chat.is_group() || msg_clone.chat.is_supergroup() || msg_clone.chat.is_channel();
    let mut chat_config = db
        .get_chat_config_or_default(chat_id.0)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "Errore nel recupero config chat, uso default");
            crate::db::models::ChatConfig::default()
        });

    if is_group_context {
        let title = msg_clone.chat.title().map(ToString::to_string);
        let chat_config_db = db.get_chat_config(chat_id.0).await.unwrap_or(None);
        let chat_exists = chat_config_db.is_some();

        if !chat_exists || chat_config.title != title {
            chat_config.title = title.clone();
            if !chat_exists {
                chat_config.added_by = user_id;
            }
            let _ = db.save_chat_config(&chat_config).await;
        }

        if !chat_exists && user_id != 0 && has_urls {
            let notify_text = tr.group_activated.replace(
                "{}",
                &html::escape(&title.clone().unwrap_or_else(|| tr.unknown.to_string())),
            );
            let _ = bot
                .send_message(ChatId(user_id), notify_text)
                .parse_mode(ParseMode::Html)
                .await;
        }
    }

    if !has_urls {
        tracing::info!("Nessun URL trovato, skip processing");
        return Ok(());
    }

    let is_enabled = if is_group_context {
        chat_config.is_enabled()
    } else {
        user_config.is_enabled()
    };

    tracing::info!(
        is_enabled,
        is_group_context,
        "Controllo stato attivazione bot"
    );

    if !is_enabled {
        tracing::info!(is_group_context, chat_id = %chat_id, "Bot disattivato per questo contesto (skip)");
        return Ok(());
    }

    let is_dry_run = user_config.dry_run != 0;
    let ignored_domains: Vec<String> = user_config
        .ignored_domains
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let custom_rules = db.get_custom_rules(user_id).await.unwrap_or_default();
    let msg_id = msg.id;
    let mut cleaned_urls: Vec<crate::sanitizer::pipeline::SanitizedUrl> = Vec::new();
    let mut all_urls = Vec::new();
    let mut all_urls_seen = HashSet::new();

    let url_candidates = helpers::extract_urls_from_message(&msg, text);

    tracing::info!(
        count = url_candidates.len(),
        "URL candidati trovati, inizio processing"
    );

    if url_candidates.is_empty() {
        tracing::info!("Nessun URL candidato trovato nel messaggio");
        return Ok(());
    }

    let _ = bot.send_chat_action(chat_id, ChatAction::Typing).await;

    for url_str in &url_candidates {
        let expanded_url = rules.expand_url(url_str).await;
        if all_urls_seen.insert(expanded_url.clone()) {
            all_urls.push(expanded_url.clone());
        }

        let domain = helpers::extract_domain(url_str)
            .or_else(|_| helpers::extract_domain(&expanded_url))
            .unwrap_or_default();

        let is_whitelisted = if !domain.is_empty() {
            db.is_whitelisted(user_id, &domain).await.unwrap_or(false)
        } else {
            false
        };

        if !is_whitelisted {
            if let Some(warning) = security_scan::check_url_combined(url_str).await {
                tracing::warn!("Security Alert: inviando allerta consolidata per URL originale");
                if let Err(e) = bot
                    .send_message(chat_id, warning.clone())
                    .parse_mode(ParseMode::Html)
                    .reply_parameters(ReplyParameters::new(msg_id))
                    .await
                {
                    tracing::error!(error = %e, "Errore nell'invio del messaggio di allerta consolidata");
                }
            }
            if expanded_url != *url_str
                && let Some(warning) = security_scan::check_url_combined(&expanded_url).await
            {
                tracing::warn!("Security Alert: inviando allerta consolidata per URL espanso");
                if let Err(e) = bot
                    .send_message(chat_id, warning.clone())
                    .parse_mode(ParseMode::Html)
                    .reply_parameters(ReplyParameters::new(msg_id))
                    .await
                {
                    tracing::error!(error = %e, "Errore nell'invio del messaggio di allerta consolidata");
                }
            }
        } else {
            tracing::info!(domain = %domain, "URL saltato: dominio in whitelist");
        }

        if is_dry_run {
            let supported = rules.is_supported_by_clearurls(&expanded_url);
            let domain_info = if !domain.is_empty() { domain } else { "unknown".to_string() };
            let dry_msg = format!(
                "🔍 <b>Dry-Run</b> — URL analizzato:\n\
                 <code>{}</code>\n\n\
                 🌐 Dominio: <b>{}</b>\n\
                 {}\n\
                 {}, {}",
                html::escape(url_str),
                html::escape(&domain_info),
                if supported {
                    "✅ Supportato da ClearURLs"
                } else {
                    "❌ Non supportato da ClearURLs"
                },
                if is_whitelisted {
                    "✅ In whitelist"
                } else {
                    "🔍 Verrà scansionato per sicurezza"
                },
                if user_config.is_ai_enabled() && config.ai_api_key.is_some() {
                    "🧠 AI abilitato"
                } else {
                    "🤖 Solo regole standard"
                },
            );
            let _ = bot
                .send_message(chat_id, dry_msg)
                .parse_mode(ParseMode::Html)
                .reply_parameters(ReplyParameters::new(msg_id))
                .await;
            continue;
        }

        if let Some(result) = crate::sanitizer::pipeline::run_sanitization_pipeline(
            url_str,
            &rules,
            &ai,
            &linkumori,
            &user_config,
            &custom_rules,
            &ignored_domains,
            is_dry_run,
        )
        .await
        {
            cleaned_urls.push(result);
        } else {
            metrics::SANITIZATIONS_UNCHANGED.inc();
            tracing::debug!(url = %rules.redact_sensitive(url_str), "URL supportato ma senza modifiche");
        }
    }

    if cleaned_urls.is_empty() && !is_dry_run {
        tracing::info!("Elaborazione completata: nessun URL da pulire (gia' puliti)");
        helpers::send_alternative_frontends(&bot, chat_id, &all_urls, &redirect_service).await?;
        return Ok(());
    }

    if is_dry_run {
        return Ok(());
    }

    tracing::info!(
        count = cleaned_urls.len(),
        "URL puliti, preparazione risposta"
    );

    let _ = db
        .increment_cleaned_count(user_id, cleaned_urls.len() as i64)
        .await;
    for s in &cleaned_urls {
        let _ = db.log_cleaned_link(user_id, &s.original_url, &s.cleaned_url, &s.provider).await;

        let _ = event_tx.send(serde_json::json!({
            "user_id": user_id,
            "original_url": s.original_url,
            "cleaned_url": s.cleaned_url,
            "provider_name": s.provider,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        }));
    }

    let mode = match chat_config.mode.as_str() {
        "default" | "" => user_config.mode.clone(),
        m => m.to_string(),
    };

    if mode == "delete" && bot.delete_message(chat_id, msg_id).await.is_ok() {
        let user_name = msg_clone
            .from
            .as_ref()
            .map_or_else(|| tr.fallback_user.to_string(), |u| u.first_name.clone());
        let mut response = tr.cleaned_for.replace("{}", &html::escape(&user_name));
        for s in &cleaned_urls {
            let escaped = html::escape(&s.cleaned_url);
            if is_safe_url_scheme(&s.cleaned_url) {
                response.push_str(&format!("\u{2022} <a href=\"{escaped}\">{escaped}</a>\n"));
            } else {
                response.push_str(&format!("\u{2022} <code>{escaped}</code>\n"));
            }
        }
        bot.send_message(chat_id, response)
            .parse_mode(ParseMode::Html)
            .await?;
        return Ok(());
    }

    let user_name = msg_clone
        .from
        .as_ref()
        .map_or_else(|| tr.fallback_user.to_string(), |u| u.first_name.clone());
    let response = crate::sanitizer::pipeline::build_response_text(
        &cleaned_urls,
        is_group_context,
        &user_name,
        &tr,
    );

    tracing::info!(chat_id = %chat_id, "Invio risposta con URL puliti");

    let mut request = bot
        .send_message(chat_id, response)
        .reply_parameters(ReplyParameters::new(msg_id))
        .parse_mode(ParseMode::Html);

    if let Some(thread_id) = msg_clone.thread_id {
        request = request.message_thread_id(thread_id);
    }

    if let Err(e) = request.await {
        tracing::error!(chat_id = %chat_id, error = %e, "Errore nell'invio della risposta con URL puliti");
        return Err(e);
    }

    helpers::send_alternative_frontends(&bot, chat_id, &all_urls, &redirect_service).await?;

    Ok(())
}
