use std::collections::HashSet;

use crate::constants::{CALLBACK_DEDUP_TTL_SECS, URL_CACHE_MAX_CAPACITY};
use crate::db::models::UserConfig;
use crate::i18n;
use crate::redirects::RedirectService;
use crate::sanitizer::{AiEngine, RuleEngine};
use crate::shared::security::RATE_LIMITER;
use crate::shared::security::{
    hash_user_id, is_safe_url_scheme, sanitize_callback, sanitize_input,
};
use moka::future::Cache;
use moka::sync::Cache as SyncCache;
use once_cell::sync::Lazy;
use teloxide::prelude::*;
use teloxide::types::{
    CallbackQuery, ChatAction, ChatId, ChosenInlineResult, InlineQuery, Message, ParseMode,
    ReplyParameters,
};
use teloxide::utils::html;
use teloxide::RequestError;

use super::helpers;
use super::security_scan;
use super::settings;

static URL_CACHE: Lazy<Cache<String, String>> = Lazy::new(|| {
    Cache::builder()
        .max_capacity(URL_CACHE_MAX_CAPACITY)
        .build()
});

static CALLBACK_CACHE: Lazy<SyncCache<String, ()>> = Lazy::new(|| {
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
            }
        };

    let mut dispatcher = Dispatcher::builder(bot.clone(), handler)
        .dependencies(dptree::deps![
            db.clone(),
            rules,
            ai,
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
                }
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
                    }
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
                );

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
        }
        None => {
            tracing::info!("Avvio in modalita' LONG-POLLING");
            dispatcher.dispatch().await;
        }
    }
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
        }
    }
}

#[tracing::instrument(skip(bot, db, rules, ai, config), fields(user_id = %q.from.id.0))]
pub async fn handle_inline_query(
    bot: Bot,
    q: InlineQuery,
    db: crate::db::Db,
    rules: RuleEngine,
    ai: AiEngine,
    config: crate::config::Config,
) -> Result<(), RequestError> {
    let user_id = i64::try_from(q.from.id.0).unwrap_or(0);
    if !RATE_LIMITER.check(user_id) {
        return Ok(());
    }
    let query = sanitize_input(q.query.trim());
    let lang_code = helpers::get_user_language(&db, user_id, q.from.language_code.as_deref()).await;

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

        if user_config.is_ai_enabled() && config.ai_api_key.is_some() {
            if let Ok(Some(ai_cleaned)) = ai.sanitize(&final_url).await {
                final_url = ai_cleaned;
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
            helpers::build_inline_clean_result(rank, cleaned, *removed_params, &lang_code);
        results.push(article);
    }

    if results.is_empty() {
        let article = helpers::build_inline_no_results(&query, &lang_code);
        results.push(article);
    }

    helpers::send_inline_results(&bot, &q, results).await
}

#[tracing::instrument(skip(_bot), fields(user_id = %chosen.from.id.0))]
pub async fn handle_chosen_inline_result(
    _bot: Bot,
    chosen: ChosenInlineResult,
) -> Result<(), RequestError> {
    tracing::info!(
        user_id = chosen.from.id.0,
        result_id = %chosen.result_id,
        query = %chosen.query,
        "Risultato inline selezionato"
    );
    Ok(())
}

#[tracing::instrument(
    skip(bot, db, rules, ai, config, event_tx, redirect_service),
    fields(chat_id = %msg.chat.id, user_id)
)]
pub async fn handle_edited_message(
    bot: Bot,
    msg: Message,
    db: crate::db::Db,
    rules: RuleEngine,
    ai: AiEngine,
    config: crate::config::Config,
    event_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    redirect_service: RedirectService,
) -> Result<(), RequestError> {
    tracing::info!(chat_id = %msg.chat.id, msg_id = %msg.id, "Elaborazione messaggio modificato");
    handle_message(bot, msg, db, rules, ai, config, event_tx, redirect_service).await
}

#[tracing::instrument(
    skip(bot, db, rules, ai, config, event_tx, redirect_service),
    fields(chat_id = %msg.chat.id, user_id)
)]
#[allow(clippy::too_many_lines)]
pub async fn handle_message(
    bot: Bot,
    msg: Message,
    db: crate::db::Db,
    rules: RuleEngine,
    ai: AiEngine,
    config: crate::config::Config,
    event_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    redirect_service: RedirectService,
) -> Result<(), RequestError> {
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

    let lang_code = helpers::detect_language(text, &msg, &user_config);

    if lang_code != user_config.language.as_str() {
        let mut updated_config = user_config.clone();
        updated_config.language = lang_code.clone();
        if let Err(e) = db.save_user_config(&updated_config).await {
            tracing::warn!(error = %e, "Errore nel salvataggio lingua utente");
        } else {
            tracing::debug!(
                user_id = user_id,
                old_lang = %user_config.language,
                new_lang = lang_code,
                "Preferenza lingua utente aggiornata"
            );
        }
    }

    let tr = i18n::get_translations(&lang_code);

    let has_urls = helpers::has_url_entities(&msg, text);

    if msg_text.starts_with('/') {
        let cmd_parts: Vec<&str> = msg_text.split('@').collect();
        let cmd = cmd_parts[0];
        let bot_username = config.bot_username.to_lowercase();

        let _is_targeted = if cmd_parts.len() > 1 {
            cmd_parts[1].to_lowercase().starts_with(&bot_username)
        } else {
            true
        };

        match cmd {
            "/start" => {
                tokio::spawn({
                    let bot = bot.clone();
                    let tr = tr.clone();
                    async move {
                        let _ = bot
                            .send_message(chat_id, tr.welcome.replace("{}", &user_id.to_string()))
                            .parse_mode(ParseMode::Html)
                            .await;
                    }
                });
            }
            "/stats" => {
                tokio::spawn({
                    let bot = bot.clone();
                    let db = db.clone();
                    let tr = tr.clone();
                    async move {
                        let stats_text = if let Ok(config) = db.get_user_config(user_id).await {
                            let activity_level = (config.cleaned_count.min(100) / 10) as usize;
                            let progress_bar = "\u{2588}".repeat(activity_level)
                                + &"\u{2591}".repeat(10 - activity_level);

                            let global_stats = db.get_global_stats().await.ok();
                            let total_users = global_stats.as_ref().map(|(u, _)| *u).unwrap_or(0);
                            let total_cleaned = global_stats.as_ref().map(|(_, c)| *c).unwrap_or(0);

                            let user_rank = if total_cleaned > 0 {
                                let top_users = db.get_top_users(10).await.ok();
                                top_users
                                    .as_ref()
                                    .and_then(|users| {
                                        users.iter().position(|(uid, _)| *uid == user_id)
                                    })
                                    .map(|pos| format!("#{}", pos + 1))
                                    .unwrap_or_else(|| ">10".to_string())
                            } else {
                                if config.cleaned_count > 0 {
                                    "#1".to_string()
                                } else {
                                    "N/A".to_string()
                                }
                            };

                            format!(
                                "<b>\u{1f4ca} Le Tue Statistiche</b>\n\n\
                                \u{1f517} URL Elaborati: <code>{}</code>\n\
                                \u{2705} Pulizie Riuscite: <code>{}</code>\n\
                                \u{1f3c6} Ranking: <b>{}</b>\n\n\
                                <b>Attivit\u{00e0} ({}/10)</b>\n{}\n\n\
                                \u{1f30d} Lingua: <b>{}</b>\n\n\
                                <b>\u{1f527} Configurazione:</b>\n\
                                \u{1f916} AI Sanitizer: <b>{}</b>\n\
                                \u{1f512} Privacy Mode: <b>{}</b>\n\
                                \u{1f5c2}\u{1fe0f}  Modalit\u{00e0}: <b>{}</b>\n\n\
                                \u{1f4ca} <b>Globale:</b> {} utenti | {} URL puliti\n\n\
                                \u{1f4a1} <i>Invia URL per pulirli automaticamente</i>",
                                config.cleaned_count,
                                config.cleaned_count,
                                user_rank,
                                activity_level,
                                progress_bar,
                                if config.language == "it" {
                                    "Italiano \u{1f1ee}\u{1f1f9}"
                                } else {
                                    "English \u{1f1ec}\u{1f1e7}"
                                },
                                if config.is_ai_enabled() {
                                    "Attivo \u{2728}"
                                } else {
                                    "Disattivo"
                                },
                                if config.privacy_mode != 0 {
                                    "Attivo"
                                } else {
                                    "Disattivo"
                                },
                                if config.mode == "delete" {
                                    "Elimina msg"
                                } else {
                                    "Rispondi"
                                },
                                total_users,
                                total_cleaned
                            )
                        } else {
                            tr.s_not_found.to_string()
                        };
                        let _ = bot
                            .send_message(chat_id, stats_text)
                            .parse_mode(ParseMode::Html)
                            .await;
                    }
                });
            }
            "/history" => {
                tokio::spawn({
                    let bot = bot.clone();
                    let db = db.clone();
                    async move {
                        let history_text = if let Ok(links) = db.get_history(user_id, 10).await {
                            if links.is_empty() {
                                "\u{1f550} <b>Cronologia Vuota</b>\n\nAncora non hai pulito nessun URL"
                                    .to_string()
                            } else {
                                let mut text =
                                    String::from("<b>\u{1f550} Ultimi URL Puliti</b>\n\n");
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
                                        "{}. <code>{}</code>\n   \u{2192} <code>{}</code>\n   via <b>{}</b>\n\n",
                                        idx + 1, original_clean, cleaned_clean, link.provider_name.as_deref().unwrap_or("Unknown")
                                    ));
                                }
                                text
                            }
                        } else {
                            "\u{274c} Errore nel caricamento della cronologia".to_string()
                        };
                        let _ = bot
                            .send_message(chat_id, history_text)
                            .parse_mode(ParseMode::Html)
                            .await;
                    }
                });
            }
            "/whitelist" => {
                let whitelist_text = "<b>\u{2b50} Whitelist</b>\n\n\
                    Aggiungi domini fidati che non necessitano controllo VirusTotal:\n\n\
                    <code>/whitelist_add example.com</code>\n\
                    <code>/whitelist_remove example.com</code>\n\
                    <code>/whitelist_show</code>";
                bot.send_message(chat_id, whitelist_text)
                    .parse_mode(ParseMode::Html)
                    .await
                    .ok();
            }
            "/whitelist_add" => {
                let args: Vec<&str> = text.split_whitespace().collect();
                if args.len() < 2 {
                    bot.send_message(
                        chat_id,
                        "\u{274c} Uso: <code>/whitelist_add example.com</code>",
                    )
                    .parse_mode(ParseMode::Html)
                    .await
                    .ok();
                } else {
                    let domain = args[1].to_string();
                    let db = db.clone();
                    let bot = bot.clone();
                    tokio::spawn(async move {
                        match db.add_to_whitelist(user_id, &domain).await {
                            Ok(_) => {
                                let _ = bot
                                    .send_message(
                                        chat_id,
                                        format!(
                                            "\u{2705} <b>{}</b> aggiunto alla whitelist",
                                            domain
                                        ),
                                    )
                                    .parse_mode(ParseMode::Html)
                                    .await;
                            }
                            Err(_) => {
                                let _ = bot
                                    .send_message(
                                        chat_id,
                                        format!("\u{26a0}\u{1fe0f} <b>{}</b> \u{00e8} gi\u{00e0} nella whitelist", domain),
                                    )
                                    .parse_mode(ParseMode::Html)
                                    .await;
                            }
                        }
                    });
                }
            }
            "/whitelist_remove" => {
                let args: Vec<&str> = text.split_whitespace().collect();
                if args.len() < 2 {
                    bot.send_message(
                        chat_id,
                        "\u{274c} Uso: <code>/whitelist_remove example.com</code>",
                    )
                    .parse_mode(ParseMode::Html)
                    .await
                    .ok();
                } else {
                    let domain = args[1].to_string();
                    let db = db.clone();
                    tokio::spawn(async move {
                        match db.remove_from_whitelist(user_id, &domain).await {
                            Ok(_) => {
                                let _ = bot
                                    .send_message(
                                        chat_id,
                                        format!(
                                            "\u{2705} <b>{}</b> rimosso dalla whitelist",
                                            domain
                                        ),
                                    )
                                    .parse_mode(ParseMode::Html)
                                    .await;
                            }
                            Err(_) => {
                                let _ = bot
                                    .send_message(chat_id, "\u{274c} Errore nella rimozione")
                                    .await;
                            }
                        }
                    });
                }
            }
            "/whitelist_show" => {
                let db = db.clone();
                tokio::spawn(async move {
                    match db.get_whitelist(user_id).await {
                        Ok(domains) => {
                            let text = if domains.is_empty() {
                                "\u{2b50} <b>La Tua Whitelist</b>\n\nVuota. Aggiungi domini con <code>/whitelist_add</code>".to_string()
                            } else {
                                let items = domains
                                    .iter()
                                    .enumerate()
                                    .map(|(i, d)| format!("{}. <code>{}</code>", i + 1, d))
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                format!(
                                    "\u{2b50} <b>La Tua Whitelist</b> ({})\n\n{}",
                                    domains.len(),
                                    items
                                )
                            };
                            let _ = bot
                                .send_message(chat_id, text)
                                .parse_mode(ParseMode::Html)
                                .await;
                        }
                        Err(_) => {
                            let _ = bot
                                .send_message(chat_id, "\u{274c} Errore nel caricamento")
                                .await;
                        }
                    }
                });
            }
            "/export" => {
                let db = db.clone();
                tokio::spawn(async move {
                    match db.get_history(user_id, 50).await {
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

                            let json_str =
                                serde_json::to_string_pretty(&json_data).unwrap_or_default();
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

                            let _ = bot
                                .send_message(chat_id, export_msg)
                                .parse_mode(ParseMode::Html)
                                .await;
                        }
                        Err(_) => {
                            let _ = bot
                                .send_message(chat_id, "\u{274c} Errore nell'esportazione")
                                .await;
                        }
                    }
                });
            }
            "/limits" => {
                let limits_msg = "<b>\u{26a1} Limiti API</b>\n\n\
                    <b>VirusTotal:</b>\n\
                    \u{2022} Standard: 4 richieste/min\n\
                    \u{2022} Elevate: \u{221e} (Premium)\n\n\
                    <b>URLScan.io:</b>\n\
                    \u{2022} Pubblico: 15 scansioni/giorno\n\
                    \u{2022} Elevate: \u{221e} (Premium)\n\n\
                    \u{1f4a1} Il bot <b>cerca scansioni esistenti</b> prima di sottomettere, \
                    risparmiando quota del 70%";

                bot.send_message(chat_id, limits_msg)
                    .parse_mode(ParseMode::Html)
                    .await
                    .ok();
            }
            "/leaderboard" => {
                let db = db.clone();
                tokio::spawn(async move {
                    match db.get_top_users(10).await {
                        Ok(top_users) => {
                            if top_users.is_empty() {
                                let _ = bot.send_message(chat_id, "\u{1f3c6} <b>Leaderboard</b>\n\nAncora nessun utente. Invia il primo URL!").parse_mode(ParseMode::Html).await;
                            } else {
                                let mut msg = String::from("\u{1f3c6} <b>Top 10 Pulitori</b>\n\n");
                                for (idx, (_, count)) in top_users.iter().enumerate() {
                                    let medal = match idx {
                                        0 => "\u{1f947}",
                                        1 => "\u{1f948}",
                                        2 => "\u{1f949}",
                                        _ => "  ",
                                    };
                                    msg.push_str(&format!(
                                        "{} #{}. <code>{}</code> URL puliti\n",
                                        medal,
                                        idx + 1,
                                        count
                                    ));
                                }
                                let _ = bot
                                    .send_message(chat_id, msg)
                                    .parse_mode(ParseMode::Html)
                                    .await;
                            }
                        }
                        Err(_) => {
                            let _ = bot
                                .send_message(
                                    chat_id,
                                    "\u{274c} Errore nel caricamento della leaderboard",
                                )
                                .await;
                        }
                    }
                });
            }
            "/trending" => {
                let db = db.clone();
                tokio::spawn(async move {
                    match db.get_top_links(10).await {
                        Ok(top_links) => {
                            if top_links.is_empty() {
                                let _ = bot.send_message(chat_id, "\u{1f4c8} <b>URL Trending</b>\n\nAncora nessun URL processato").parse_mode(ParseMode::Html).await;
                            } else {
                                let mut msg = String::from(
                                    "\u{1f4c8} <b>Top 10 URL Pi\u{00f9} Puliti</b>\n\n",
                                );
                                for (idx, (url, count)) in top_links.iter().enumerate() {
                                    let url_short = if url.len() > 50 {
                                        format!("{}...", &url[..47])
                                    } else {
                                        url.clone()
                                    };
                                    msg.push_str(&format!(
                                        "{}. <code>{}</code> ({} volte)\n",
                                        idx + 1,
                                        url_short,
                                        count
                                    ));
                                }
                                let _ = bot
                                    .send_message(chat_id, msg)
                                    .parse_mode(ParseMode::Html)
                                    .await;
                            }
                        }
                        Err(_) => {
                            let _ = bot
                                .send_message(
                                    chat_id,
                                    "\u{274c} Errore nel caricamento dei trending",
                                )
                                .await;
                        }
                    }
                });
            }
            "/domains" => {
                let db = db.clone();
                tokio::spawn(async move {
                    match db.get_domain_cleanup_stats(user_id).await {
                        Ok(domains) => {
                            if domains.is_empty() {
                                let _ = bot.send_message(chat_id, "\u{1f310} <b>Statistiche per Dominio</b>\n\nAncora nessun URL processato").parse_mode(ParseMode::Html).await;
                            } else {
                                let mut msg = String::from(
                                    "\u{1f310} <b>Tuoi Domini Pi\u{00f9} Puliti</b>\n\n",
                                );
                                for (idx, (domain, count)) in domains.iter().enumerate() {
                                    msg.push_str(&format!(
                                        "{}. <code>{}</code> \u{2014} <b>{}</b> pulizie\n",
                                        idx + 1,
                                        domain,
                                        count
                                    ));
                                }
                                let _ = bot
                                    .send_message(chat_id, msg)
                                    .parse_mode(ParseMode::Html)
                                    .await;
                            }
                        }
                        Err(_) => {
                            let _ = bot
                                .send_message(
                                    chat_id,
                                    "\u{274c} Errore nel caricamento delle statistiche per dominio",
                                )
                                .await;
                        }
                    }
                });
            }
            "/help" => {
                tokio::spawn({
                    let bot = bot.clone();
                    let tr = tr.clone();
                    async move {
                        let _ = bot
                            .send_message(chat_id, tr.help_text)
                            .parse_mode(ParseMode::Html)
                            .await;
                    }
                });
            }
            "/menu" => {
                tokio::spawn({
                    let bot = bot.clone();
                    let tr = tr.clone();
                    async move {
                        let _ = bot
                            .send_message(chat_id, tr.reply_keyboard_opened)
                            .reply_markup(helpers::main_reply_keyboard(&tr))
                            .parse_mode(ParseMode::Html)
                            .await;
                    }
                });
            }
            "/hidekbd" => {
                tokio::spawn({
                    let bot = bot.clone();
                    let tr = tr.clone();
                    async move {
                        let _ = bot
                            .send_message(chat_id, tr.reply_keyboard_hidden)
                            .reply_markup(teloxide::types::KeyboardRemove::new())
                            .parse_mode(ParseMode::Html)
                            .await;
                    }
                });
            }
            "/privacy" => {
                tokio::spawn({
                    let bot = bot.clone();
                    let tr = tr.clone();
                    async move {
                        let privacy_text = format!(
                            "<b>🔒 {}</b>\n\n\
                            {}\n\n\
                            <b>📊 {}</b>\n\
                            • {}\n\
                            • {}\n\
                            • {}\n\n\
                            <b>🗑️ {}</b>\n\
                            • {}\n\
                            • <code>/clear_history</code> {}\n\n\
                            <b>📤 {}</b>\n\
                            • <code>/export</code> {}",
                            tr.s_privacy_title,
                            tr.s_privacy_desc,
                            tr.s_clear_history,
                            "I tuoi ID utente e chat sono hashed nei log per conformità GDPR",
                            "La cronologia dei link puliti viene memorizzata localmente",
                            "Nessun dato personale viene condiviso con servizi terzi",
                            "Gestione dati",
                            "Cancella tutta la cronologia dei link puliti",
                            "Cancella cronologia e reset contatore",
                            "Esportazione dati",
                            "Esporta i tuoi dati in formato JSON"
                        );
                        let _ = bot
                            .send_message(chat_id, privacy_text)
                            .parse_mode(ParseMode::Html)
                            .await;
                    }
                });
            }
            "/settings" => {
                settings::handle_settings_callback(
                    bot.clone(),
                    chat_id,
                    None,
                    user_id,
                    db.clone(),
                    config.clone(),
                    &tr,
                )
                .await
                .ok();
            }
            "/language" => {
                let current_lang = helpers::language_name(user_config.language.as_str());
                let mut msg_text = format!("<b>🌐 Lingua Attuale: {}</b>\n\n", current_lang);
                msg_text.push_str("Scegli la lingua:\n\n");
                for code in helpers::SUPPORTED_LANGUAGES {
                    msg_text.push_str(&format!(
                        "{} {}: <code>/setlang {}</code>\n",
                        helpers::language_name(code)
                            .split_whitespace()
                            .next()
                            .unwrap_or(""),
                        code,
                        code
                    ));
                }
                msg_text.push_str("\n💡 <i>La lingua influisce su tutti i messaggi del bot</i>");
                bot.send_message(chat_id, msg_text)
                    .parse_mode(ParseMode::Html)
                    .await
                    .ok();
            }
            "/setlang" => {
                let parts: Vec<&str> = msg_text.split_whitespace().collect();
                if parts.len() > 1 {
                    let lang = parts[1];
                    if helpers::SUPPORTED_LANGUAGES.contains(&lang) {
                        let mut updated_config = user_config.clone();
                        updated_config.language = lang.to_string();
                        db.save_user_config(&updated_config).await.ok();
                        let tr_new = i18n::get_translations(lang);
                        let msg = format!(
                            "✅ <b>Lingua cambiata a {}</b>\n\n{}",
                            helpers::language_name(lang),
                            tr_new.s_language_updated
                        );
                        bot.send_message(chat_id, msg)
                            .parse_mode(ParseMode::Html)
                            .await
                            .ok();
                    } else {
                        let langs_list = helpers::SUPPORTED_LANGUAGES.join(", ");
                        bot.send_message(
                            chat_id,
                            format!(
                                "❌ Lingua non supportata. Lingue disponibili: {}",
                                langs_list
                            ),
                        )
                        .parse_mode(ParseMode::Html)
                        .await
                        .ok();
                    }
                } else {
                    bot.send_message(
                        chat_id,
                        "❓ Specifica la lingua: /setlang <codice> (es. /setlang it)",
                    )
                    .parse_mode(ParseMode::Html)
                    .await
                    .ok();
                }
            }
            "/toplinks" => {
                let top = db.get_top_links(10).await.unwrap_or_default();
                let mut msg_text = String::from("<b>Top link puliti:</b>\n\n");
                for (idx, (url, count)) in top.iter().enumerate() {
                    msg_text.push_str(&format!(
                        "{}. <code>{}</code> \u{2014} <b>{}</b>\n",
                        idx + 1,
                        url,
                        count
                    ));
                }
                bot.send_message(chat_id, msg_text)
                    .parse_mode(ParseMode::Html)
                    .await
                    .ok();
            }
            _ => {
                bot.send_message(chat_id, tr.unknown_command)
                    .parse_mode(ParseMode::Html)
                    .await
                    .ok();
            }
        }
        return Ok(());
    }

    if let Some(text_val) = msg.text() {
        if let Some(action) = helpers::quick_reply_action(text_val, &tr) {
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
                }
                helpers::QuickReplyAction::Stats => {
                    let stats_text = tr
                        .stats_text
                        .replace("{}", &user_config.cleaned_count.to_string());
                    bot.send_message(chat_id, stats_text)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(helpers::quick_actions_inline_keyboard(&tr, user_id))
                        .await?;
                    return Ok(());
                }
                helpers::QuickReplyAction::Help => {
                    bot.send_message(chat_id, tr.help_text)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(helpers::quick_actions_inline_keyboard(&tr, user_id))
                        .await?;
                    return Ok(());
                }
                helpers::QuickReplyAction::HideKeyboard => {
                    bot.send_message(chat_id, tr.reply_keyboard_hidden)
                        .reply_markup(teloxide::types::KeyboardRemove::new())
                        .await?;
                    return Ok(());
                }
                helpers::QuickReplyAction::Language => {
                    let language_text = format!(
                        "<b>{}</b>\n\n{} <b>{}</b>",
                        tr.s_language_title, tr.s_language_current, user_config.language
                    );
                    bot.send_message(chat_id, language_text)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(helpers::language_inline_keyboard(&tr, user_id))
                        .await?;
                    return Ok(());
                }
            }
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

    let ignored_domains: Vec<String> = user_config
        .ignored_domains
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let custom_rules = db.get_custom_rules(user_id).await.unwrap_or_default();
    let msg_id = msg.id;
    let mut cleaned_urls = Vec::new();
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

    for url_str in url_candidates {
        let expanded_url = rules.expand_url(&url_str).await;
        if all_urls_seen.insert(expanded_url.clone()) {
            all_urls.push(expanded_url.clone());
        }
        let original_url_str = url_str.clone();

        let domain = helpers::extract_domain(&url_str)
            .or_else(|_| helpers::extract_domain(&expanded_url))
            .unwrap_or_default();

        let is_whitelisted = if !domain.is_empty() {
            db.is_whitelisted(user_id, &domain).await.unwrap_or(false)
        } else {
            false
        };

        if !is_whitelisted {
            if let Some(warning) = security_scan::check_url_combined(&url_str).await {
                tracing::warn!("Security Alert: inviando allerta consolidata per URL originale");
                if let Err(e) = bot
                    .send_message(chat_id, warning.clone())
                    .parse_mode(ParseMode::Html)
                    .await
                {
                    tracing::error!(error = %e, "Errore nell'invio del messaggio di allerta consolidata");
                }
            }
            if expanded_url != url_str {
                if let Some(warning) = security_scan::check_url_combined(&expanded_url).await {
                    tracing::warn!("Security Alert: inviando allerta consolidata per URL espanso");
                    if let Err(e) = bot
                        .send_message(chat_id, warning.clone())
                        .parse_mode(ParseMode::Html)
                        .await
                    {
                        tracing::error!(error = %e, "Errore nell'invio del messaggio di allerta consolidata");
                    }
                }
            }
        } else {
            tracing::info!(domain = %domain, "URL saltato: dominio in whitelist");
        }

        if !rules.is_supported_by_clearurls(&expanded_url) {
            tracing::debug!(url = %rules.redact_sensitive(&expanded_url), "URL non supportato da ClearURLs, skip pulizia");
            continue;
        }

        let mut current_url = expanded_url.clone();
        if let Some(cached) = URL_CACHE.get(&expanded_url).await {
            current_url = cached;
            cleaned_urls.push((original_url_str, current_url.clone(), "CACHE".to_string()));
            continue;
        }
        if let Some((cleaned, provider)) =
            rules.sanitize(&current_url, &custom_rules, &ignored_domains)
        {
            current_url = cleaned;
            tracing::info!(provider = %provider, "URL pulito dal motore");

            if user_config.is_ai_enabled() && config.ai_api_key.is_some() {
                if let Ok(Some(ai_cleaned)) = ai.sanitize(&current_url).await {
                    current_url = ai_cleaned;
                    let provider_name = format!("AI ({provider})");
                    URL_CACHE
                        .insert(expanded_url.clone(), current_url.clone())
                        .await;
                    cleaned_urls.push((original_url_str, current_url, provider_name));
                    continue;
                }
            }

            tracing::info!(
                original = %rules.redact_sensitive(&original_url_str),
                cleaned = %current_url,
                provider = %provider,
                "URL pulito dal motore"
            );
            URL_CACHE
                .insert(expanded_url.clone(), current_url.clone())
                .await;
            cleaned_urls.push((original_url_str, current_url, provider));
        } else {
            tracing::debug!(url = %rules.redact_sensitive(&current_url), "URL supportato ma senza parametri da pulire");
        }
    }

    if cleaned_urls.is_empty() {
        tracing::info!("Elaborazione completata: nessun URL da pulire (gia' puliti)");
        helpers::send_alternative_frontends(&bot, chat_id, &all_urls, &redirect_service).await?;
        return Ok(());
    }

    tracing::info!(
        count = cleaned_urls.len(),
        "URL puliti, preparazione risposta"
    );

    let _ = db
        .increment_cleaned_count(user_id, cleaned_urls.len() as i64)
        .await;
    for (orig, clean, prov) in &cleaned_urls {
        let _ = db.log_cleaned_link(user_id, orig, clean, prov).await;

        let _ = event_tx.send(serde_json::json!({
            "user_id": user_id,
            "original_url": orig,
            "cleaned_url": clean,
            "provider_name": prov,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        }));
    }

    const MAX_RESPONSE_LENGTH: usize = 4000;

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
        for (_, cleaned, _) in &cleaned_urls {
            let escaped = html::escape(cleaned);
            if is_safe_url_scheme(cleaned) {
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

    let mut response = if is_group_context {
        let user_name = msg_clone
            .from
            .as_ref()
            .map_or_else(|| tr.fallback_user.to_string(), |u| u.first_name.clone());
        tr.cleaned_for.replace("{}", &html::escape(&user_name))
    } else {
        String::from(tr.cleaned_links)
    };

    let mut total_params_removed = 0;
    for (original, cleaned, _) in &cleaned_urls {
        total_params_removed += helpers::removed_query_params_count(original, cleaned);
    }

    let stats_line = format!(
        "\n\u{2705} <b>Pulizia completata</b>\n\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\n\
        \u{1f4ca} <b>Statistiche:</b>\n\
        \u{1f517} URL puliti: <code>{}</code>\n",
        cleaned_urls.len()
    );

    if total_params_removed > 0 {
        response.push_str(&stats_line);
        response.push_str(&format!(
            "\u{1f5d1}\u{1fe0f}  Parametri rimossi: <code>{}</code>\n\n",
            total_params_removed
        ));
    } else {
        response.push_str(&stats_line);
        response.push('\n');
    }

    if !response.ends_with('\n') {
        response.push('\n');
    }

    response.push_str("\u{1f310} <b>Link puliti:</b>\n");

    if cleaned_urls.len() == 1 {
        let clean = cleaned_urls[0].1.trim();
        let escaped_url = html::escape(clean);
        if is_safe_url_scheme(clean) {
            let link_entry = format!("\u{3009} <a href=\"{escaped_url}\">{escaped_url}</a>");

            if response.len() + link_entry.len() < MAX_RESPONSE_LENGTH {
                response.push_str(&link_entry);
            }
        } else {
            let link_entry = format!("\u{3009} <code>{escaped_url}</code>");
            if response.len() + link_entry.len() < MAX_RESPONSE_LENGTH {
                response.push_str(&link_entry);
            }
        }
    } else {
        for (idx, (_, cleaned, _)) in cleaned_urls.iter().enumerate() {
            let clean = cleaned.trim();
            let escaped_url = html::escape(clean);
            let link_entry = if is_safe_url_scheme(clean) {
                format!(
                    "{} <a href=\"{escaped_url}\">{escaped_url}</a>\n",
                    if idx == cleaned_urls.len() - 1 {
                        "\u{2514}\u{2500}"
                    } else {
                        "\u{251c}\u{2500}"
                    }
                )
            } else {
                format!(
                    "{} <code>{escaped_url}</code>\n",
                    if idx == cleaned_urls.len() - 1 {
                        "\u{2514}\u{2500}"
                    } else {
                        "\u{251c}\u{2500}"
                    }
                )
            };

            if response.len() + link_entry.len() > MAX_RESPONSE_LENGTH {
                response.push_str("\u{2514}\u{2500} <i>... e altri URL</i>\n");
                response.push_str(tr.truncated);
                break;
            }
            response.push_str(&link_entry);
        }
    }

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
    if !RATE_LIMITER.check(user_id) {
        return Ok(());
    }
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
