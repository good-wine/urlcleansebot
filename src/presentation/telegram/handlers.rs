use std::collections::HashSet;

use crate::constants::CALLBACK_DEDUP_TTL_SECS;
use crate::db::models::UserConfig;
use crate::i18n;
use crate::metrics;
use crate::redirects::RedirectService;
use crate::sanitizer::{AiEngine, RuleEngine, linkumori::LinkumoriEngine};
use crate::shared::security::{
    check_rate_limit, hash_user_id, is_safe_url_scheme, sanitize_callback, sanitize_input,
};
use moka::sync::Cache as SyncCache;
use std::sync::LazyLock;
use teloxide::RequestError;
use teloxide::prelude::*;
use teloxide::types::{
    CallbackQuery, ChatAction, ChatId, ChosenInlineResult, InlineQuery, Message, ParseMode,
    ReplyParameters,
};
use teloxide::utils::html;

use super::command_dispatcher;
use super::helpers;
use super::security_scan;
use super::settings;

static CALLBACK_CACHE: LazyLock<SyncCache<String, ()>> = LazyLock::new(|| {
    SyncCache::builder()
        .max_capacity(50_000)
        .time_to_live(std::time::Duration::from_secs(CALLBACK_DEDUP_TTL_SECS))
        .build()
});

pub async fn run_bot(
    bot: Bot,
    db: crate::db::Db,
    rules: RuleEngine,
    ai: AiEngine,
    linkumori: LinkumoriEngine,
    config: crate::config::Config,
    event_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
) {
    let handler = dptree::entry()
        .branch(Update::filter_message().endpoint(handle_message))
        .branch(Update::filter_edited_message().endpoint(handle_edited_message))
        .branch(Update::filter_inline_query().endpoint(handle_inline_query))
        .branch(Update::filter_chosen_inline_result().endpoint(handle_chosen_inline_result))
        .branch(Update::filter_callback_query().endpoint(handle_callback));

    let webhook_url = config.webhook_url.clone();
    let webhook_secret = config.webhook_secret.clone();
    let port = config.port;

    let redirect_service =
        match RedirectService::from_config(&config.libredirect_url, &config.farside_url) {
            Ok(svc) => svc,
            Err(e) => {
                tracing::error!("Impossibile inizializzare RedirectService: {e}");
                return;
            },
        };

    let mut dispatcher = Dispatcher::builder(bot.clone(), handler)
        .dependencies(dptree::deps![
            db.clone(),
            rules,
            ai,
            linkumori,
            config,
            event_tx,
            redirect_service
        ])
        .enable_ctrlc_handler()
        .build();

    match webhook_url {
        Some(url) => {
            use teloxide::update_listeners::webhooks;
            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            let parsed = match url::Url::parse(&url) {
                Ok(u) => u,
                Err(e) => {
                    tracing::error!("WEBHOOK_URL non valido ({url}): {e}");
                    return;
                },
            };
            let mut opts = webhooks::Options::new(addr, parsed);
            if let Some(secret) = webhook_secret {
                opts = opts.secret_token(secret);
            }
            tracing::info!("Avvio in modalita' WEBHOOK: bind={addr}, public_url={url}");

            let (listener, shutdown_future, telegram_router) =
                match webhooks::axum_to_router(bot, opts).await {
                    Ok(result) => result,
                    Err(e) => {
                        tracing::error!("Impossibile avviare il webhook: {e}");
                        return;
                    },
                };

            let health_db = db.clone();
            let health_router = axum::Router::new()
                .route("/health", axum::routing::get(health_liveness))
                .route(
                    "/ready",
                    axum::routing::get({
                        let health_db = health_db.clone();
                        move || health_readiness(health_db)
                    }),
                )
                .route("/metrics", axum::routing::get(metrics_handler));

            let app = telegram_router.merge(health_router);

            let server = axum::serve(
                tokio::net::TcpListener::bind(addr)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::error!("Impossibile bind alla porta {}: {e}", port);
                        std::process::exit(1);
                    }),
                app,
            )
            .with_graceful_shutdown(shutdown_future);

            tokio::select! {
                _ = dispatcher.dispatch_with_listener(
                    listener,
                    teloxide::error_handlers::LoggingErrorHandler::with_custom_text(
                        "Errore webhook listener",
                    ),
                ) => {},
                result = server => {
                    if let Err(e) = result {
                        tracing::error!("Server webhook terminato con errore: {e}");
                    }
                }
            }
        },
        None => {
            tracing::info!("Avvio in modalita' LONG-POLLING");
            dispatcher.dispatch().await;
        },
    }
}

async fn metrics_handler() -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::OK, metrics::render_prometheus())
}

async fn health_liveness() -> axum::http::StatusCode {
    axum::http::StatusCode::OK
}

async fn health_readiness(db: crate::db::Db) -> axum::http::StatusCode {
    match sqlx::query("SELECT 1").fetch_one(&db.pool).await {
        Ok(_) => axum::http::StatusCode::OK,
        Err(e) => {
            tracing::error!("Health check readiness fallito: {e}");
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        },
    }
}

#[tracing::instrument(skip(bot, db, rules, ai, linkumori, config), fields(user_id = %q.from.id.0))]
pub async fn handle_inline_query(
    bot: Bot,
    q: InlineQuery,
    db: crate::db::Db,
    rules: RuleEngine,
    ai: AiEngine,
    linkumori: LinkumoriEngine,
    config: crate::config::Config,
) -> Result<(), RequestError> {
    let user_id = i64::try_from(q.from.id.0).unwrap_or(0);
    if check_rate_limit(user_id).await.is_err() {
        metrics::RATE_LIMIT_HITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return Ok(());
    }
    metrics::REQUESTS_INLINE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let query = sanitize_input(q.query.trim());
    let lang_code = helpers::get_user_language(&db, user_id, q.from.language_code.as_deref()).await;
    let tr = i18n::get_translations(&lang_code);

    if query.is_empty() {
        let article = helpers::build_inline_help_article(&lang_code);
        helpers::send_inline_results(&bot, &q, vec![article]).await?;
        return Ok(());
    }

    let user_config = db.get_user_config(user_id).await.unwrap_or_default();
    let ignored_domains: Vec<String> = user_config
        .ignored_domains
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let custom_rules = db.get_custom_rules(user_id).await.unwrap_or_default();

    let urls = helpers::extract_url_candidates(&query);
    let mut ranked_cleaned: Vec<(usize, String, String, usize)> = Vec::new();

    for (idx, original) in urls.iter().enumerate() {
        let expanded = rules.expand_url(original).await;
        if !rules.is_supported_by_clearurls(&expanded) {
            continue;
        }
        let mut final_url = expanded.clone();

        if let Some((cleaned, _provider)) =
            rules.sanitize(&expanded, &custom_rules, &ignored_domains)
        {
            final_url = cleaned;
        }

        if user_config.is_ai_enabled()
            && config.ai_api_key.is_some()
            && let Ok(Some(ai_cleaned)) = ai.sanitize(&final_url).await
        {
            final_url = ai_cleaned;
        }

        if user_config.is_honor_creator()
            && let Some(honor_cleaned) = crate::sanitizer::honor_creator::clean_keeping_affiliates(&final_url)
        {
            final_url = honor_cleaned;
        }

        // Aggressive mode for inline queries
        if user_config.is_aggressive()
            && let Some(agg_cleaned) = crate::sanitizer::aggressive::sanitize_aggressive(&final_url)
        {
            final_url = agg_cleaned;
        }

        // Linkumori rule check for inline queries
        if linkumori.source_count() > 0
            && let Ok(parsed_url) = url::Url::parse(&final_url)
        {
            let pairs: Vec<(String, String)> = parsed_url
                .query_pairs()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let original_count = pairs.len();
            let keep: Vec<_> = pairs
                .into_iter()
                .filter(|(name, _)| !linkumori.should_remove_param(name, &final_url))
                .collect();
            if keep.len() < original_count {
                let mut clean_url = parsed_url;
                clean_url.query_pairs_mut().clear();
                for (name, value) in &keep {
                    clean_url.query_pairs_mut().append_pair(name, value);
                }
                final_url = clean_url.to_string();
            }
        }

        if final_url == *original {
            continue;
        }

        let removed_params = helpers::removed_query_params_count(&expanded, &final_url);
        ranked_cleaned.push((idx, original.clone(), final_url, removed_params));
    }

    ranked_cleaned.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| a.0.cmp(&b.0)));

    let mut results: Vec<teloxide::types::InlineQueryResult> = Vec::new();

    for (rank, (_source_idx, _original, cleaned, removed_params)) in ranked_cleaned
        .iter()
        .take(config.inline_max_results)
        .enumerate()
    {
        let article =
            helpers::build_inline_clean_result(rank, cleaned, *removed_params, &tr);
        results.push(article);
    }

    if results.is_empty() {
        let article = helpers::build_inline_no_results(&query, &tr);
        results.push(article);
    }

    helpers::send_inline_results(&bot, &q, results).await
}

#[tracing::instrument(skip(bot), fields(user_id = %chosen.from.id.0))]
pub async fn handle_chosen_inline_result(
    bot: Bot,
    chosen: ChosenInlineResult,
) -> Result<(), RequestError> {
    tracing::info!(
        user_id = chosen.from.id.0,
        result_id = %chosen.result_id,
        query = %chosen.query,
        "Risultato inline selezionato"
    );
    let _ = bot
        .send_message(
            ChatId(chosen.from.id.0 as i64),
            format!(
                "✅ URL pulito: <code>{}</code>",
                html::escape(&chosen.result_id)
            ),
        )
        .parse_mode(ParseMode::Html)
        .await;
    Ok(())
}

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
    metrics::REQUESTS_EDITED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
    metrics::REQUESTS_MESSAGE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
        UserConfig::default()
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
            metrics::SANITIZATIONS_UNCHANGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

#[tracing::instrument(skip(bot, db, config), fields(user_id, chat_id))]
pub async fn handle_callback(
    bot: Bot,
    q: CallbackQuery,
    db: crate::db::Db,
    config: crate::config::Config,
) -> Result<(), RequestError> {
    let callback_id = q.id.0.clone();
    if CALLBACK_CACHE.get(&callback_id).is_some() {
        tracing::debug!(callback_id = %callback_id, "Callback query duplicated, ignoring");
        bot.answer_callback_query(q.id).await.ok();
        return Ok(());
    }
    CALLBACK_CACHE.insert(callback_id, ());

    let user_id = q.from.id.0 as i64;
    if check_rate_limit(user_id).await.is_err() {
        metrics::RATE_LIMIT_HITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return Ok(());
    }
    metrics::REQUESTS_CALLBACK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let callback_data = sanitize_callback(q.data.as_deref().unwrap_or(""));
    let chat_id = q
        .message
        .as_ref()
        .map(teloxide::types::MaybeInaccessibleMessage::chat)
        .map(|chat| chat.id);
    let message_id = q
        .message
        .as_ref()
        .map(teloxide::types::MaybeInaccessibleMessage::id);

    let telegram_lang = q.from.language_code.as_deref();
    let lang_code = helpers::get_user_language(&db, user_id, telegram_lang).await;
    let tr = i18n::get_translations(&lang_code);

    if let Some(chat_id) = chat_id {
        if callback_data.starts_with("settings:") {
            let parts: Vec<&str> = callback_data.split(':').collect();
            let target_user_id = helpers::callback_target_user_id(&parts, user_id);
            if target_user_id != user_id {
                helpers::show_no_permission_view(&bot, chat_id, message_id, &tr).await?;
            } else {
                settings::handle_settings_callback(
                    bot.clone(),
                    chat_id,
                    message_id,
                    user_id,
                    db,
                    config,
                    &tr,
                )
                .await?;
            }
        } else if callback_data.starts_with("user_setting:") {
            settings::handle_user_settings_callback(
                bot.clone(),
                chat_id,
                message_id,
                user_id,
                &callback_data,
                db,
                &tr,
            )
            .await?;
        } else if callback_data.starts_with("admin_setting:") {
            settings::handle_admin_settings_callback(
                bot.clone(),
                chat_id,
                message_id,
                user_id,
                &callback_data,
                db,
                &config,
                &tr,
            )
            .await?;
        } else if callback_data.starts_with("quick:") {
            settings::handle_quick_callback(
                settings::CallbackContext {
                    bot: bot.clone(),
                    chat_id,
                    message_id,
                    user_id,
                },
                &callback_data,
                db,
                config,
                &tr,
            )
            .await?;
        } else if callback_data.starts_with("back_to_main") {
            let parts: Vec<&str> = callback_data.split(':').collect();
            let target_user_id = helpers::callback_target_user_id(&parts, user_id);
            if target_user_id != user_id {
                helpers::show_no_permission_view(&bot, chat_id, message_id, &tr).await?;
            } else {
                settings::handle_start_command(
                    bot.clone(),
                    chat_id,
                    user_id,
                    &tr,
                    &config,
                    message_id,
                )
                .await?;
            }
        }
    }

    bot.answer_callback_query(q.id).await?;

    Ok(())
}
