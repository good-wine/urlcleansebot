use crate::{
    db::{models::UserConfig, Db},
    i18n,
    redirects::{format_hit_html, RedirectService},
    sanitizer::{AiEngine, RuleEngine},
    security::{sanitize_callback, sanitize_input, RATE_LIMITER},
};
use base64::prelude::*;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use regex::Regex;
use teloxide::prelude::*;
use teloxide::types::{
    CallbackQuery, ChosenInlineResult, InlineKeyboardButton, InlineKeyboardMarkup, InlineQuery,
    InlineQueryResult, InlineQueryResultArticle, InputMessageContent, InputMessageContentText,
    KeyboardButton, KeyboardMarkup, KeyboardRemove, MessageEntityKind, MessageId, ParseMode,
    ReplyParameters,
};
use teloxide::utils::html;
use whatlang::{detect, Lang};

const MAX_MESSAGE_LENGTH: usize = 4000;

pub async fn run_bot(
    bot: Bot,
    db: Db,
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

    let redirect_service = match RedirectService::new() {
        Ok(svc) => svc,
        Err(e) => {
            tracing::error!("Impossibile inizializzare RedirectService: {e}");
            return;
        }
    };

    let mut dispatcher = Dispatcher::builder(bot.clone(), handler)
        .dependencies(dptree::deps![
            db,
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
            match webhooks::axum(bot, opts).await {
                Ok(listener) => {
                    dispatcher
                        .dispatch_with_listener(
                            listener,
                            teloxide::error_handlers::LoggingErrorHandler::with_custom_text(
                                "Errore webhook listener",
                            ),
                        )
                        .await;
                }
                Err(e) => {
                    tracing::error!("Impossibile avviare il webhook: {e}");
                }
            }
        }
        None => {
            tracing::info!("Avvio in modalita' LONG-POLLING");
            dispatcher.dispatch().await;
        }
    }
}

#[tracing::instrument(skip(bot, db, rules, ai, config), fields(user_id = %q.from.id.0))]
pub async fn handle_inline_query(
    bot: Bot,
    q: InlineQuery,
    db: Db,
    rules: RuleEngine,
    ai: AiEngine,
    config: crate::config::Config,
) -> ResponseResult<()> {
    let user_id = i64::try_from(q.from.id.0).unwrap_or(0);
    // Rate limiting anti-flood
    if !RATE_LIMITER.check(user_id) {
        return Ok(()); // Silenziosamente ignora richieste flood
    }
    let query = sanitize_input(q.query.trim());
    let lang_code = get_user_language(&db, user_id, q.from.language_code.as_deref()).await;

    if query.is_empty() {
        let article = InlineQueryResultArticle::new(
            "inline-help",
            if lang_code == "it" {
                "Incolla un URL da pulire"
            } else {
                "Paste a URL to clean"
            },
            InputMessageContent::Text(InputMessageContentText::new(if lang_code == "it" {
                "Incolla un URL dopo @botusername per pulirlo in linea."
            } else {
                "Paste a URL after @botusername to clean it inline."
            })),
        );

        bot.answer_inline_query(q.id, vec![InlineQueryResult::Article(article)])
            .cache_time(1)
            .is_personal(true)
            .await?;
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

    let urls = extract_url_candidates(&query);
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

        let removed_params = removed_query_params_count(&expanded, &final_url);
        ranked_cleaned.push((idx, original.clone(), final_url, removed_params));
    }

    ranked_cleaned.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| a.0.cmp(&b.0)));

    let mut results: Vec<InlineQueryResult> = Vec::new();

    for (rank, (_source_idx, _original, cleaned, removed_params)) in ranked_cleaned
        .iter()
        .take(config.inline_max_results)
        .enumerate()
    {
        let title = if lang_code == "it" {
            if *removed_params > 0 {
                format!("URL pulito #{} (−{} param)", rank + 1, removed_params)
            } else {
                format!("URL pulito #{}", rank + 1)
            }
        } else if *removed_params > 0 {
            format!("Clean URL #{} (-{} params)", rank + 1, removed_params)
        } else {
            format!("Clean URL #{}", rank + 1)
        };

        let content = InputMessageContent::Text(InputMessageContentText::new(cleaned.clone()));
        let article = InlineQueryResultArticle::new(format!("clean-{}", rank), title, content)
            .description(cleaned.clone());

        results.push(InlineQueryResult::Article(article));
    }

    if results.is_empty() {
        let article = InlineQueryResultArticle::new(
            "inline-no-results",
            if lang_code == "it" {
                "Nessun URL da pulire"
            } else {
                "No cleanable URL found"
            },
            InputMessageContent::Text(InputMessageContentText::new(query.to_string())),
        );
        results.push(InlineQueryResult::Article(article));
    }

    bot.answer_inline_query(q.id, results)
        .cache_time(1)
        .is_personal(true)
        .await?;

    Ok(())
}

#[tracing::instrument(skip(_bot), fields(user_id = %chosen.from.id.0))]
pub async fn handle_chosen_inline_result(
    _bot: Bot,
    chosen: ChosenInlineResult,
) -> ResponseResult<()> {
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
    db: Db,
    rules: RuleEngine,
    ai: AiEngine,
    config: crate::config::Config,
    event_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    redirect_service: RedirectService,
) -> ResponseResult<()> {
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
    db: Db,
    rules: RuleEngine,
    ai: AiEngine,
    config: crate::config::Config,
    event_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    redirect_service: RedirectService,
) -> ResponseResult<()> {
    let user_id = msg
        .from
        .as_ref()
        .map(|u| i64::try_from(u.id.0).unwrap_or(0))
        .unwrap_or(0);
    let chat_id = msg.chat.id;
    let msg_text = msg.text().map(|t| t.to_string()).unwrap_or_default();

    tracing::info!(
        user_id = user_id,
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

    let entities = msg.entities();
    let text = msg_text.as_str();

    // Detect language
    let detected_lang = if text.is_empty() {
        None
    } else {
        detect(text).map(|info| info.lang())
    };

    let telegram_lang = msg.from.as_ref().and_then(|u| u.language_code.as_deref());

    let lang_code = match (detected_lang, telegram_lang) {
        (Some(Lang::Ita), _) => "it",
        (Some(Lang::Eng), _) => "en",
        (_, Some(l)) if l.starts_with("it") => "it",
        (_, Some(l)) if l.starts_with("en") => "en",
        _ => &user_config.language,
    };

    // Save detected language to database if different from current
    if lang_code != user_config.language.as_str() {
        let mut updated_config = user_config.clone();
        updated_config.language = lang_code.to_string();
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

    let tr = i18n::get_translations(lang_code);

    let mut has_urls = false;

    // Check if message has URLs
    if let Some(ents) = entities.as_ref() {
        for entity in *ents {
            if matches!(
                entity.kind,
                MessageEntityKind::Url | MessageEntityKind::TextLink { .. }
            ) {
                has_urls = true;
                break;
            }
        }
    }

    // Fallback: alcune URL non vengono marcate come entity da Telegram
    if !has_urls {
        has_urls = !extract_url_candidates(text).is_empty();
    }

    if let Some(_e) = entities.as_ref() {
        // Handle Commands
        if msg_text.starts_with('/') {
            let cmd_parts: Vec<&str> = msg_text.split('@').collect();
            let cmd = cmd_parts[0];
            let _is_private = msg.chat.is_private();
            let bot_username = config.bot_username.to_lowercase();

            let _is_targeted = if cmd_parts.len() > 1 {
                cmd_parts[1].to_lowercase().starts_with(&bot_username)
            } else {
                true
            };

            match cmd {
                "/redirect" => {
                    let arg = msg_text
                        .splitn(2, char::is_whitespace)
                        .nth(1)
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let bot = bot.clone();
                    let svc = redirect_service.clone();
                    let tr = tr.clone();
                    tokio::spawn(async move {
                        let reply = build_redirect_reply(&svc, &arg, &tr).await;
                        let _ = bot
                            .send_message(chat_id, reply)
                            .parse_mode(ParseMode::Html)
                            .await;
                    });
                    return Ok(());
                }
                "/start" => {
                    tokio::spawn({
                        let bot = bot.clone();
                        let tr = tr.clone();
                        async move {
                            let _ = bot
                                .send_message(
                                    chat_id,
                                    tr.welcome.replace("{}", &user_id.to_string()),
                                )
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
                                // Generate progress bar (activity level based on cleaned count)
                                let activity_level = (config.cleaned_count.min(100) / 10) as usize;
                                let progress_bar =
                                    "█".repeat(activity_level) + &"░".repeat(10 - activity_level);

                                // Get additional stats
                                let global_stats = db.get_global_stats().await.ok();
                                let total_users =
                                    global_stats.as_ref().map(|(u, _)| *u).unwrap_or(0);
                                let total_cleaned =
                                    global_stats.as_ref().map(|(_, c)| *c).unwrap_or(0);

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
                                    "<b>📊 Le Tue Statistiche</b>\n\n\
                                    🔗 URL Elaborati: <code>{}</code>\n\
                                    ✅ Pulizie Riuscite: <code>{}</code>\n\
                                    🏆 Ranking: <b>{}</b>\n\n\
                                    <b>Attività ({}/10)</b>\n{}\n\n\
                                    🌍 Lingua: <b>{}</b>\n\n\
                                    <b>🔧 Configurazione:</b>\n\
                                    🤖 AI Sanitizer: <b>{}</b>\n\
                                    🔒 Privacy Mode: <b>{}</b>\n\
                                    🗂️  Modalità: <b>{}</b>\n\n\
                                    📊 <b>Globale:</b> {} utenti | {} URL puliti\n\n\
                                    💡 <i>Invia URL per pulirli automaticamente</i>",
                                    config.cleaned_count,
                                    config.cleaned_count,
                                    user_rank,
                                    activity_level,
                                    progress_bar,
                                    if config.language == "it" {
                                        "Italiano 🇮🇹"
                                    } else {
                                        "English 🇬🇧"
                                    },
                                    if config.is_ai_enabled() {
                                        "Attivo ✨"
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
                        let _tr = tr.clone();
                        async move {
                            let history_text = if let Ok(links) = db.get_history(user_id, 10).await
                            {
                                if links.is_empty() {
                                    "🕐 <b>Cronologia Vuota</b>\n\nAncora non hai pulito nessun URL"
                                        .to_string()
                                } else {
                                    let mut text = String::from("<b>🕐 Ultimi URL Puliti</b>\n\n");
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
                                            idx + 1, original_clean, cleaned_clean, link.provider_name.as_deref().unwrap_or("Unknown")
                                        ));
                                    }
                                    text
                                }
                            } else {
                                "❌ Errore nel caricamento della cronologia".to_string()
                            };
                            let _ = bot
                                .send_message(chat_id, history_text)
                                .parse_mode(ParseMode::Html)
                                .await;
                        }
                    });
                }
                "/whitelist" => {
                    let whitelist_text = "<b>⭐ Whitelist</b>\n\n\
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
                            "❌ Uso: <code>/whitelist_add example.com</code>",
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
                                            format!("✅ <b>{}</b> aggiunto alla whitelist", domain),
                                        )
                                        .parse_mode(ParseMode::Html)
                                        .await;
                                }
                                Err(_) => {
                                    let _ = bot
                                        .send_message(
                                            chat_id,
                                            format!("⚠️ <b>{}</b> è già nella whitelist", domain),
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
                            "❌ Uso: <code>/whitelist_remove example.com</code>",
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
                                            format!("✅ <b>{}</b> rimosso dalla whitelist", domain),
                                        )
                                        .parse_mode(ParseMode::Html)
                                        .await;
                                }
                                Err(_) => {
                                    let _ = bot
                                        .send_message(chat_id, "❌ Errore nella rimozione")
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
                                    "⭐ <b>La Tua Whitelist</b>\n\nVuota. Aggiungi domini con <code>/whitelist_add</code>".to_string()
                                } else {
                                    let items = domains
                                        .iter()
                                        .enumerate()
                                        .map(|(i, d)| format!("{}. <code>{}</code>", i + 1, d))
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    format!(
                                        "⭐ <b>La Tua Whitelist</b> ({})\n\n{}",
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
                                let _ =
                                    bot.send_message(chat_id, "❌ Errore nel caricamento").await;
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
                                    "<b>📥 Esportazione Dati</b>\n\n<pre>{}</pre>\n\n\
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
                                    .send_message(chat_id, "❌ Errore nell'esportazione")
                                    .await;
                            }
                        }
                    });
                }
                "/limits" => {
                    // Rate limit info - API call tracking (VirusTotal quota: 4/min standard, URLScan: 15/hour)
                    let limits_msg = "<b>⚡ Limiti API</b>\n\n\
                        <b>VirusTotal:</b>\n\
                        • Standard: 4 richieste/min\n\
                        • Elevate: ∞ (Premium)\n\n\
                        <b>URLScan.io:</b>\n\
                        • Pubblico: 15 scansioni/giorno\n\
                        • Elevate: ∞ (Premium)\n\n\
                        💡 Il bot <b>cerca scansioni esistenti</b> prima di sottomettere, \
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
                                    let _ = bot.send_message(chat_id, "🏆 <b>Leaderboard</b>\n\nAncora nessun utente. Invia il primo URL!").parse_mode(ParseMode::Html).await;
                                } else {
                                    let mut msg = String::from("🏆 <b>Top 10 Pulitori</b>\n\n");
                                    for (idx, (_, count)) in top_users.iter().enumerate() {
                                        let medal = match idx {
                                            0 => "🥇",
                                            1 => "🥈",
                                            2 => "🥉",
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
                                        "❌ Errore nel caricamento della leaderboard",
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
                                    let _ = bot.send_message(chat_id, "📈 <b>URL Trending</b>\n\nAncora nessun URL processato").parse_mode(ParseMode::Html).await;
                                } else {
                                    let mut msg =
                                        String::from("📈 <b>Top 10 URL Più Puliti</b>\n\n");
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
                                    .send_message(chat_id, "❌ Errore nel caricamento dei trending")
                                    .await;
                            }
                        }
                    });
                }
                "/domains" => {
                    // Smart URL grouping by domain
                    let db = db.clone();
                    tokio::spawn(async move {
                        match db.get_domain_cleanup_stats(user_id).await {
                            Ok(domains) => {
                                if domains.is_empty() {
                                    let _ = bot.send_message(chat_id, "🌐 <b>Statistiche per Dominio</b>\n\nAncora nessun URL processato").parse_mode(ParseMode::Html).await;
                                } else {
                                    let mut msg =
                                        String::from("🌐 <b>Tuoi Domini Più Puliti</b>\n\n");
                                    for (idx, (domain, count)) in domains.iter().enumerate() {
                                        msg.push_str(&format!(
                                            "{}. <code>{}</code> — <b>{}</b> pulizie\n",
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
                                        "❌ Errore nel caricamento delle statistiche per dominio",
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
                                .reply_markup(main_reply_keyboard(&tr))
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
                                .reply_markup(KeyboardRemove::new())
                                .parse_mode(ParseMode::Html)
                                .await;
                        }
                    });
                }
                "/settings" => {
                    handle_settings_callback(
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
                    let mut msg_text = String::from("<b>Lingue disponibili:</b>\n\n");
                    msg_text.push_str("🇮🇹 Italiano (/setlang it)\n🇬🇧 English (/setlang en)\n");
                    bot.send_message(chat_id, msg_text)
                        .parse_mode(ParseMode::Html)
                        .await
                        .ok();
                }
                "/setlang" => {
                    let parts: Vec<&str> = msg_text.split_whitespace().collect();
                    if parts.len() > 1 {
                        let lang = parts[1];
                        let mut updated_config = user_config.clone();
                        updated_config.language = lang.to_string();
                        db.save_user_config(&updated_config).await.ok();
                        let tr_new = i18n::get_translations(lang);
                        bot.send_message(chat_id, tr_new.s_language_updated)
                            .parse_mode(ParseMode::Html)
                            .await
                            .ok();
                    } else {
                        bot.send_message(
                            chat_id,
                            "❓ Specifica la lingua: /setlang it oppure /setlang en",
                        )
                        .parse_mode(ParseMode::Html)
                        .await
                        .ok();
                    }
                }
                "/topusers" => {
                    let top = db.get_top_users(10).await.unwrap_or_default();
                    let mut msg_text = String::from("<b>Top utenti per link puliti:</b>\n\n");
                    for (idx, (uid, count)) in top.iter().enumerate() {
                        msg_text.push_str(&format!(
                            "{}. <code>{}</code> — <b>{}</b>\n",
                            idx + 1,
                            uid,
                            count
                        ));
                    }
                    bot.send_message(chat_id, msg_text)
                        .parse_mode(ParseMode::Html)
                        .await
                        .ok();
                }
                "/toplinks" => {
                    let top = db.get_top_links(10).await.unwrap_or_default();
                    let mut msg_text = String::from("<b>Top link puliti:</b>\n\n");
                    for (idx, (url, count)) in top.iter().enumerate() {
                        msg_text.push_str(&format!(
                            "{}. <code>{}</code> — <b>{}</b>\n",
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
    }

    if let Some(text_val) = msg.text() {
        if let Some(action) = quick_reply_action(text_val, &tr) {
            match action {
                QuickReplyAction::Settings => {
                    handle_settings_callback(
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
                QuickReplyAction::Stats => {
                    let stats_text = tr
                        .stats_text
                        .replace("{}", &user_config.cleaned_count.to_string());
                    bot.send_message(chat_id, stats_text)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(quick_actions_inline_keyboard(&tr, user_id))
                        .await?;
                    return Ok(());
                }
                QuickReplyAction::Help => {
                    bot.send_message(chat_id, tr.help_text)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(quick_actions_inline_keyboard(&tr, user_id))
                        .await?;
                    return Ok(());
                }
                QuickReplyAction::HideKeyboard => {
                    bot.send_message(chat_id, tr.reply_keyboard_hidden)
                        .reply_markup(KeyboardRemove::new())
                        .await?;
                    return Ok(());
                }
                QuickReplyAction::Language => {
                    let language_text = format!(
                        "<b>{}</b>\n\n{} <b>{}</b>",
                        tr.s_language_title, tr.s_language_current, user_config.language
                    );
                    bot.send_message(chat_id, language_text)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(language_inline_keyboard(&tr, user_id))
                        .await?;
                    return Ok(());
                }
            }
        }
    }

    // Persist/Update chat info
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

        // Only save if it's new or title changed
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

    // Logic: In groups, only check if the group enabled the bot.
    // In private, check if the user enabled the bot.
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

    let mut url_candidates = Vec::new();

    tracing::info!("Inizio estrazione URL dal messaggio");

    // 1. Get URLs from Telegram Entities
    if let Some(ents) = entities {
        let utf16: Vec<u16> = text.encode_utf16().collect();
        for entity in ents {
            let url_str = match &entity.kind {
                MessageEntityKind::Url => {
                    let start = entity.offset;
                    let end = start + entity.length;
                    if end > utf16.len() {
                        continue;
                    }
                    String::from_utf16_lossy(&utf16[start..end])
                }
                MessageEntityKind::TextLink { url } => url.to_string(),
                _ => continue,
            };
            if !url_candidates.contains(&url_str) {
                tracing::debug!(url = %url_str, "URL trovato tramite entita' Telegram");
                url_candidates.push(url_str);
            }
        }
    }

    // 2. Supplement with Regex Detection
    let url_pattern = r"(?i)(?:https?://|www\.)[a-zA-Z0-9\-\.]+\.[a-zA-Z]{2,}(?:/[^\s]*)?";
    if let Ok(re) = Regex::new(url_pattern) {
        for mat in re.find_iter(text) {
            let url_str = mat.as_str().to_string();
            if !url_candidates.contains(&url_str) {
                tracing::debug!(url = %url_str, "URL trovato tramite regex di fallback");
                url_candidates.push(url_str);
            }
        }
    }

    if url_candidates.is_empty() {
        tracing::info!("Nessun URL candidato trovato nel messaggio");
        return Ok(());
    }

    tracing::info!(
        count = url_candidates.len(),
        "URL candidati trovati, inizio processing"
    );

    // 3. Process candidates
    for url_str in url_candidates {
        // 1. Expand shortened URLs first
        let expanded_url = rules.expand_url(&url_str).await;
        let original_url_str = url_str.clone();

        // Check whitelist before security checks
        let domain = extract_domain(&url_str)
            .or_else(|_| extract_domain(&expanded_url))
            .unwrap_or_default();

        let is_whitelisted = if !domain.is_empty() {
            db.is_whitelisted(user_id, &domain).await.unwrap_or(false)
        } else {
            false
        };

        // Combined security check with both VirusTotal and URLScan (unless whitelisted)
        if !is_whitelisted {
            if let Some(warning) = check_url_combined(&url_str).await {
                tracing::warn!("Security Alert: inviando allerta consolidata per URL originale");
                if let Err(e) = bot
                    .send_message(chat_id, warning.clone())
                    .parse_mode(ParseMode::Html)
                    .await
                {
                    tracing::error!(error = %e, "Errore nell'invio del messaggio di allerta consolidata");
                }
            }
            // Also check expanded URL if different from original
            if expanded_url != url_str {
                if let Some(warning) = check_url_combined(&expanded_url).await {
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
            tracing::info!(domain = %domain, "URL saltato: dominion in whitelist");
        }

        if !rules.is_supported_by_clearurls(&expanded_url) {
            tracing::debug!(url = %rules.redact_sensitive(&expanded_url), "URL non supportato da ClearURLs, skip pulizia");
            continue;
        }

        let mut current_url = expanded_url.clone();
        // Caching: se già pulito, usa cache
        if let Some(cached) = URL_CACHE.get(&expanded_url).await {
            current_url = cached;
            cleaned_urls.push((original_url_str, current_url.clone(), "CACHE".to_string()));
            continue;
        }
        // 2. Sanitization
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
            response.push_str(&format!("• <a href=\"{escaped}\">{escaped}</a>\n"));
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

    // Calculate statistics for improved UI
    let mut total_params_removed = 0;
    for (original, cleaned, _) in &cleaned_urls {
        total_params_removed += removed_query_params_count(original, cleaned);
    }

    // Enhanced header with emoji and stats
    let stats_line = format!(
        "\n✅ <b>Pulizia completata</b>\n━━━━━━━━━━━━━━━━\n\
        📊 <b>Statistiche:</b>\n\
        🔗 URL puliti: <code>{}</code>\n",
        cleaned_urls.len()
    );

    if total_params_removed > 0 {
        response.push_str(&stats_line);
        response.push_str(&format!(
            "🗑️  Parametri rimossi: <code>{}</code>\n\n",
            total_params_removed
        ));
    } else {
        response.push_str(&stats_line);
        response.push_str("\n");
    }

    if !response.ends_with('\n') {
        response.push('\n');
    }

    // Enhanced link display with visual separators
    response.push_str("🌐 <b>Link puliti:</b>\n");

    if cleaned_urls.len() == 1 {
        let clean = cleaned_urls[0].1.trim();
        let escaped_url = html::escape(clean);
        let link_entry = format!("〉 <a href=\"{escaped_url}\">{escaped_url}</a>");

        if response.len() + link_entry.len() < MAX_MESSAGE_LENGTH {
            response.push_str(&link_entry);
        }
    } else {
        for (idx, (_, cleaned, _)) in cleaned_urls.iter().enumerate() {
            let clean = cleaned.trim();
            let escaped_url = html::escape(clean);
            let link_entry = format!(
                "{} <a href=\"{escaped_url}\">{escaped_url}</a>\n",
                if idx == cleaned_urls.len() - 1 {
                    "└─"
                } else {
                    "├─"
                }
            );

            if response.len() + link_entry.len() > MAX_MESSAGE_LENGTH {
                response.push_str("└─ <i>... e altri URL</i>\n");
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

    // Support for Supergroup topics/threads
    if let Some(thread_id) = msg_clone.thread_id {
        request = request.message_thread_id(thread_id);
    }

    if let Err(e) = request.await {
        tracing::error!(chat_id = %chat_id, error = %e, "Errore nell'invio della risposta con URL puliti");
        return Err(e);
    }

    Ok(())
}

use moka::future::Cache;
pub static URL_CACHE: once_cell::sync::Lazy<Cache<String, String>> =
    once_cell::sync::Lazy::new(|| Cache::new(10000));

/// Check URL with both VirusTotal and URLScan services and consolidate results
///
/// This function calls both security scanning services and combines their results
/// into a single consolidated alert message instead of sending separate messages.
/// Returns Option<String> with the combined alert if either service detects a threat.
pub async fn check_url_combined(url: &str) -> Option<String> {
    // Call both services concurrently for efficiency
    let vt_result = check_url_virustotal(url);
    let urlscan_result = check_url_urlscan(url);

    let (vt_msg, urlscan_msg) = tokio::join!(vt_result, urlscan_result);

    // Only send a message if at least one service detected a threat
    if vt_msg.is_none() && urlscan_msg.is_none() {
        return None;
    }

    // Build the consolidated message
    let mut consolidated = String::from(
        "🚨 <b>ALLERTA SICUREZZA</b> 🚨\n\
        ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
        🔴 <b>MINACCIA RILEVATA - REPORT CONSOLIDATO</b>\n\n",
    );

    // Extract key information from VirusTotal alert if present
    if let Some(vt_alert) = vt_msg {
        consolidated.push_str("🛡️ <b>VirusTotal Security Scan:</b>\n");
        // Extract the relevant part after the header
        if let Some(content_start) = vt_alert.find("🔴 <b>LINK PERICOLOSO RILEVATO</b>") {
            let content = &vt_alert[content_start..];
            // Get lines up to the report link
            if let Some(report_idx) = content.find("📋 <a href=") {
                let summary = &content[..report_idx];
                consolidated.push_str(summary);
                // Extract and append the report link
                if let Some(link_end) = content[report_idx..].find("</a>") {
                    consolidated.push_str(&content[report_idx..report_idx + link_end + 4]);
                }
            } else {
                consolidated.push_str(content);
            }
        }
        consolidated.push_str("\n\n");
    }

    // Extract key information from URLScan alert if present
    if let Some(urlscan_alert) = urlscan_msg {
        consolidated.push_str("🌐 <b>URLScan.io Web Reputation:</b>\n");
        // Extract the relevant part after the header
        if let Some(content_start) = urlscan_alert.find("🔴 <b>LINK PERICOLOSO RILEVATO</b>") {
            let content = &urlscan_alert[content_start..];
            // Get lines up to the report link
            if let Some(report_idx) = content.find("📋 <a href=") {
                let summary = &content[..report_idx];
                consolidated.push_str(summary);
                // Extract and append the report link
                if let Some(link_end) = content[report_idx..].find("</a>") {
                    consolidated.push_str(&content[report_idx..report_idx + link_end + 4]);
                }
            } else {
                consolidated.push_str(content);
            }
        }
        consolidated.push_str("\n\n");
    }

    // Add final warning
    consolidated.push_str(
        "⚠️ <b>ATTENZIONE:</b> Questo link è stato segnalato come pericoloso.\n\
        Si consiglia di NON visitare la pagina.",
    );

    Some(consolidated)
}

/// Check URL with VirusTotal API v3
///
/// Returns a user-facing VirusTotal message with scan outcome.
/// Requires VIRUSTOTAL_API_KEY environment variable.
pub async fn check_url_virustotal(url: &str) -> Option<String> {
    let alert_only = std::env::var("VIRUSTOTAL_ALERT_ONLY")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            !matches!(normalized.as_str(), "0" | "false" | "no" | "off")
        })
        .unwrap_or(true);

    let api_key = match std::env::var("VIRUSTOTAL_API_KEY") {
        Ok(key) if !key.is_empty() && key != "your_virustotal_api_key_here" => key,
        _ => {
            tracing::debug!("VirusTotal: API key non configurata, scansione disabilitata");
            return None;
        }
    };

    tracing::info!(url = %url, "VirusTotal: Scansione in corso...");

    let encoded_url = base64::prelude::BASE64_URL_SAFE_NO_PAD.encode(url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let endpoint = format!("https://www.virustotal.com/api/v3/urls/{encoded_url}");

    let mut lookup_id = encoded_url.clone();

    let mut resp = match client
        .get(&endpoint)
        .header("x-apikey", &api_key)
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            tracing::warn!(error = %e, url = %url, "VirusTotal: richiesta fallita");
            if alert_only {
                return None;
            }
            return Some(
                "⚠️ <b>VirusTotal non raggiungibile</b>\nRiprova tra qualche minuto.".to_string(),
            );
        }
    };

    // Check if URL already exists in VirusTotal database
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        tracing::info!(url = %url, "VirusTotal: URL non presente, invio per analisi");

        let submit_resp = match client
            .post("https://www.virustotal.com/api/v3/urls")
            .header("x-apikey", &api_key)
            .form(&[("url", url)])
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) => {
                tracing::warn!(error = %e, url = %url, "VirusTotal: submit fallito");
                if alert_only {
                    return None;
                }
                return Some(
                    "⚠️ <b>VirusTotal: invio analisi fallito</b>\nRiprova tra qualche minuto."
                        .to_string(),
                );
            }
        };

        if submit_resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            tracing::warn!(url = %url, "VirusTotal: rate limit raggiunto");
            if alert_only {
                return None;
            }
            return Some("⏱️ <b>VirusTotal: limite richieste raggiunto</b>\nAttendi circa 1 minuto e riprova.".to_string());
        }

        if !submit_resp.status().is_success() {
            tracing::warn!(status = %submit_resp.status(), url = %url, "VirusTotal: submit API error");
            if alert_only {
                return None;
            }
            return Some(format!(
                "⚠️ <b>VirusTotal: errore API</b>\nCodice: {}",
                submit_resp.status()
            ));
        }

        if let Ok(submit_json) = submit_resp.json::<serde_json::Value>().await {
            if let Some(id) = submit_json["data"]["id"].as_str() {
                lookup_id = id.to_string();
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        let submitted_endpoint = format!("https://www.virustotal.com/api/v3/urls/{lookup_id}");
        resp = match client
            .get(&submitted_endpoint)
            .header("x-apikey", &api_key)
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) => {
                tracing::warn!(error = %e, url = %url, "VirusTotal: recupero report fallito dopo submit");
                if alert_only {
                    return None;
                }
                return Some("ℹ️ <b>VirusTotal</b>\nURL inviato per analisi. Report non ancora disponibile, riprova tra poco.".to_string());
            }
        };
    }

    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        tracing::warn!(url = %url, "VirusTotal: rate limit raggiunto");
        if alert_only {
            return None;
        }
        return Some(
            "⏱️ <b>VirusTotal: limite richieste raggiunto</b>\nAttendi circa 1 minuto e riprova."
                .to_string(),
        );
    }

    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), url = %url, "VirusTotal API error");
        if alert_only {
            return None;
        }
        return Some(format!(
            "⚠️ <b>VirusTotal: errore API</b>\nCodice: {}",
            resp.status()
        ));
    }

    // URL already exists in VirusTotal, use existing scan results
    if resp.status() == reqwest::StatusCode::OK && resp.status() != reqwest::StatusCode::NOT_FOUND {
        tracing::info!(url = %url, "VirusTotal: Scansione precedente trovata, utilizzo risultati");
    }

    let json: serde_json::Value = match resp.json().await {
        Ok(value) => value,
        Err(e) => {
            tracing::warn!(error = %e, url = %url, "VirusTotal: risposta JSON non valida");
            if alert_only {
                return None;
            }
            return Some(
                "⚠️ <b>VirusTotal</b>\nImpossibile leggere la risposta dell'analisi.".to_string(),
            );
        }
    };

    // Parse detection stats
    let stats = &json["data"]["attributes"]["last_analysis_stats"];
    let malicious = stats["malicious"].as_i64().unwrap_or(0);
    let suspicious = stats["suspicious"].as_i64().unwrap_or(0);
    let harmless = stats["harmless"].as_i64().unwrap_or(0);
    let undetected = stats["undetected"].as_i64().unwrap_or(0);
    let total = harmless + malicious + suspicious + undetected;

    // Get last analysis date if available
    let last_analysis_date = json["data"]["attributes"]["last_analysis_date"]
        .as_i64()
        .and_then(|ts| {
            use std::time::{Duration, SystemTime, UNIX_EPOCH};
            let analysis_time = UNIX_EPOCH + Duration::from_secs(ts as u64);
            SystemTime::now()
                .duration_since(analysis_time)
                .ok()
                .map(|elapsed| {
                    let hours = elapsed.as_secs() / 3600;
                    if hours < 1 {
                        "meno di 1 ora fa".to_string()
                    } else if hours < 24 {
                        format!("{} ore fa", hours)
                    } else {
                        format!("{} giorni fa", hours / 24)
                    }
                })
        });

    if malicious > 0 || suspicious > 2 {
        tracing::warn!(
            malicious = malicious,
            suspicious = suspicious,
            harmless = harmless,
            total = total,
            url = %url,
            "VirusTotal: Minaccia rilevata!"
        );

        let report_link = format!("https://www.virustotal.com/gui/url/{}", encoded_url);

        let msg = if malicious > 0 {
            let mut alert = format!(
                "🚨 <b>ALLERTA SICUREZZA</b> 🚨\n\
                ━━━━━━━━━━━━━━━━\n\
                🛡️ <b>VirusTotal Security Scan</b>\n\n\
                🔴 <b>LINK PERICOLOSO RILEVATO</b>\n\n\
                📊 <b>Risultati Scansione:</b>\n\
                🔴 Dannoso: <b>{}</b> motori\n",
                malicious
            );
            if suspicious > 0 {
                alert.push_str(&format!("🟡 Sospetto: <b>{}</b> motori\n", suspicious));
            }
            alert.push_str(&format!(
                "✅ Sicuro: <b>{}</b> motori\n\
                ⚪️ Non rilevato: {} motori\n\
                📈 Rilevazioni: <b>{}/{}</b> motori\n",
                harmless,
                undetected,
                malicious + suspicious,
                total
            ));
            if let Some(date) = last_analysis_date {
                alert.push_str(&format!("\n🕐 Ultima analisi: <i>{}</i>\n", date));
            }
            alert.push_str(&format!(
                "\n🔒 <b>ATTENZIONE: NON APRIRE QUESTO LINK!</b>\n\
                Contiene contenuti potenzialmente dannosi.\n\n\
                📋 <a href=\"{}\">Visualizza Report Dettagliato ›</a>",
                report_link
            ));
            alert
        } else {
            let mut warning = format!(
                "⚠️ <b>AVVISO SICUREZZA</b>\n\
                ━━━━━━━━━━━━━━━━\n\
                🛡️ <b>VirusTotal Security Scan</b>\n\n\
                🟡 <b>Link classificato come SOSPETTO</b>\n\n\
                📊 <b>Risultati Scansione:</b>\n\
                🟡 Sospetto: <b>{}</b> motori\n\
                ✅ Sicuro: <b>{}</b> motori\n\
                ⚪️ Non rilevato: {} motori\n\
                📈 Rilevazioni sospette: <b>{}/{}</b> motori\n",
                suspicious, harmless, undetected, suspicious, total
            );
            if let Some(date) = last_analysis_date {
                warning.push_str(&format!("\n🕐 Ultima analisi: <i>{}</i>\n", date));
            }
            warning.push_str(&format!(
                "\n⚠️ <b>Procedere con CAUTELA</b>\n\
                Questo link potrebbe non essere sicuro.\n\n\
                📋 <a href=\"{}\">Visualizza Report Dettagliato ›</a>",
                report_link
            ));
            warning
        };
        Some(msg)
    } else {
        tracing::info!(
            total = total,
            harmless = harmless,
            url = %url,
            "VirusTotal: URL sicuro (nessuna minaccia rilevata)"
        );
        if alert_only {
            return None;
        }

        let mut msg = format!(
            "✅ <b>URL VERIFICATO SICURO</b>\n\
            ───────────────────\n\
            🛡️ <b>VirusTotal Security Scan</b>\n\n\
            📊 <b>Risultati Scansione:</b>\n\
            ✅ Sicuro: <b>{}</b> motori\n\
            ⚪️ Non rilevato: {} motori\n\
            📈 Totale verifiche: <b>{}</b> motori\n",
            harmless, undetected, total
        );

        if let Some(date) = last_analysis_date {
            msg.push_str(&format!("\n🕐 Ultima analisi: <i>{}</i>\n", date));
        }

        msg.push_str(&format!(
            "\n✨ Nessuna minaccia rilevata\n\
            📋 <a href=\"https://www.virustotal.com/gui/url/{}\">Visualizza Report ›</a>",
            encoded_url
        ));

        Some(msg)
    }
}

/// Search for existing URLScan.io scans of a URL.
/// Returns the UUID of an existing scan if found, None otherwise.
async fn search_existing_urlscan(url: &str, api_key: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    // URLScan Search API: search for the exact URL using query parameter
    let search_query = format!("domain:{}", url.split('/').nth(2).unwrap_or(url));

    let search_resp = match client
        .get("https://urlscan.io/api/v1/search/")
        .header("API-Key", api_key)
        .query(&[("q", search_query.as_str())])
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return None,
    };

    if !search_resp.status().is_success() {
        return None;
    }

    let search_json: serde_json::Value = match search_resp.json().await {
        Ok(value) => value,
        Err(_) => return None,
    };

    // Get the first result (most recent) that matches the exact URL
    if let Some(results) = search_json["results"].as_array() {
        for result in results {
            if let Some(page_url) = result["page"]["url"].as_str() {
                if page_url == url {
                    if let Some(uuid) = result["_id"].as_str() {
                        tracing::info!(url = %url, uuid = %uuid, "URLScan.io: Scansione precedente trovata");
                        return Some(uuid.to_string());
                    }
                }
            }
        }
    }

    None
}

/// Check URL with URLScan.io API.
///
/// Returns a user-facing URLScan.io message with scan outcome.
/// Requires URLSCAN_API_KEY environment variable.
pub async fn check_url_urlscan(url: &str) -> Option<String> {
    let alert_only = std::env::var("URLSCAN_ALERT_ONLY")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            !matches!(normalized.as_str(), "0" | "false" | "no" | "off")
        })
        .unwrap_or(true);

    let api_key = match std::env::var("URLSCAN_API_KEY") {
        Ok(key) if !key.is_empty() && key != "your_urlscan_api_key_here" => key,
        _ => {
            tracing::debug!("URLScan.io: API key non configurata, scansione disabilitata");
            return None;
        }
    };

    tracing::info!(url = %url, "URLScan.io: Scansione in corso...");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .ok()?;

    // First, try to find an existing scan
    let mut uuid = search_existing_urlscan(url, &api_key).await;
    let mut result_link = "https://urlscan.io".to_string();

    // If not found, submit a new scan
    if uuid.is_none() {
        let submit_resp = match client
            .post("https://urlscan.io/api/v1/scan/")
            .header("API-Key", &api_key)
            .json(&serde_json::json!({ "url": url, "visibility": "private" }))
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) => {
                tracing::warn!(error = %e, url = %url, "URLScan.io: richiesta fallita");
                if alert_only {
                    return None;
                }
                return Some(
                    "⚠️ <b>URLScan.io non raggiungibile</b>\nRiprova tra qualche minuto."
                        .to_string(),
                );
            }
        };

        if submit_resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            tracing::warn!(url = %url, "URLScan.io: rate limit raggiunto");
            if alert_only {
                return None;
            }
            return Some(
                "⏱️ <b>URLScan.io: limite richieste raggiunto</b>\nAttendi e riprova.".to_string(),
            );
        }

        if !submit_resp.status().is_success() {
            let status_code = submit_resp.status();

            // Try to extract error details from response body
            let error_details = if let Ok(error_body) = submit_resp.text().await {
                // Check for specific error messages from URLScan.io

                // Technical errors that should respect alert_only mode
                if error_body.contains("URL is too long") || error_body.contains("length") {
                    tracing::warn!(url = %url, "URLScan.io: URL troppo lungo");
                    if alert_only {
                        return None;
                    }
                    return Some(
                        "⚠️ <b>ERRORE SCANSIONE</b>\n\
                        ━━━━━━━━━━━━━━━━\n\
                        📌 <b>URLScan.io</b>\n\n\
                        🔗 <b>URL troppo lungo</b>\n\n\
                        ℹ️ Questo link è troppo lungo per essere scansionato.\n\n\
                        💡 <b>Suggerimento:</b>\n\
                        Prova ad accorciare l'URL usando un servizio\n\
                        di URL shortener (es: bit.ly, tinyurl, ecc.)"
                            .to_string(),
                    );
                }

                // URLScan blocked the scan for technical reasons (not because URL is malicious)
                if error_body.contains("Scan prevented")
                    || error_body.contains("blocked from scanning")
                    || error_body.contains("URL was blocked")
                {
                    tracing::warn!(
                        url = %url,
                        error = %error_body,
                        "URLScan.io: Scansione bloccata per motivi tecnici (non sicurezza)"
                    );
                    // This is a technical limitation, not a security alert
                    // Always suppress this in alert_only mode
                    if alert_only {
                        return None;
                    }
                    // In full report mode, still don't show as security alert
                    // Just log it and skip
                    return None;
                }

                error_body
            } else {
                "Unknown error".to_string()
            };

            tracing::warn!(
                status = %status_code,
                error = %error_details,
                url = %url,
                "URLScan.io API error"
            );

            if alert_only {
                return None;
            }
            return Some(format!(
                "⚠️ <b>ERRORE SCANSIONE</b>\n\
                ━━━━━━━━━━━━━━━━\n\
                📌 <b>URLScan.io</b>\n\n\
                🔧 <b>Errore Tecnico</b>\n\n\
                <b>Codice errore:</b> {}\n\n\
                ℹ️ <i>Il servizio ha incontrato un errore durante la scansione.</i>\n\n\
                💡 <b>Prova:</b>\n\
                • Riprova tra qualche minuto\n\
                • Verifica che l'URL sia valido\n\
                • Contatta l'admin se il problema persiste",
                status_code
            ));
        }

        let submit_json: serde_json::Value = match submit_resp.json().await {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!(error = %e, url = %url, "URLScan.io: risposta submit non valida");
                if alert_only {
                    return None;
                }
                return Some(
                    "⚠️ <b>ERRORE SCANSIONE</b>\n\
                ━━━━━━━━━━━━━━━━\n\
                📌 <b>URLScan.io</b>\n\n\
                🔧 <b>Risposta non valida</b>\n\n\
                ℹ️ <i>Il servizio ha dato una risposta non riconoscibile.</i>\n\n\
                💡 <b>Prova:</b>\n\
                • Riprova tra 1-2 minuti\n\
                • Assicurati che l'URL sia valido"
                        .to_string(),
                );
            }
        };

        uuid = submit_json["uuid"].as_str().map(ToString::to_string);
        result_link = submit_json["result"]
            .as_str()
            .map(ToString::to_string)
            .unwrap_or_else(|| "https://urlscan.io".to_string());

        if uuid.is_none() {
            if alert_only {
                return None;
            }
            return Some(format!(
                "🕐 <b>ANALISI IN CORSO</b>\n\
                ━━━━━━━━━━━━━━━━\n\
                📌 <b>URLScan.io</b>\n\n\
                ⏳ <b>URL inviato per analisi</b>\n\n\
                ℹ️ <i>La scansione è in corso sul servizio.</i>\n\n\
                📋 <a href=\"{}\">Apri il report completo ›</a>\n\n\
                💡 <b>Nota:</b> Il rapporto sarà disponibile tra pochi istanti.",
                result_link
            ));
        }

        // Wait a bit for the scan to start processing before polling
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    }

    let Some(uuid_ref) = uuid.as_ref() else {
        return None;
    };

    let uuid_re = Regex::new(
        r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
    )
    .ok()?;
    if !uuid_re.is_match(uuid_ref) {
        return None;
    }
    let safe_uuid = utf8_percent_encode(uuid_ref, NON_ALPHANUMERIC).to_string();

    let mut malicious = false;
    let mut potentially_malicious = false;
    let mut score = 0.0_f64;

    for _ in 0..4 {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let mut result_endpoint = match reqwest::Url::parse("https://urlscan.io/") {
            Ok(url) => url,
            Err(_) => continue,
        };
        {
            let mut segments = match result_endpoint.path_segments_mut() {
                Ok(path) => path,
                Err(_) => continue,
            };
            segments.extend(["api", "v1", "result", &safe_uuid, ""]);
        }
        let result_resp = match client
            .get(result_endpoint)
            .header("API-Key", &api_key)
            .send()
            .await
        {
            Ok(response) => response,
            Err(_) => continue,
        };

        if !result_resp.status().is_success() {
            continue;
        }

        let result_json: serde_json::Value = match result_resp.json().await {
            Ok(value) => value,
            Err(_) => continue,
        };

        malicious = result_json["verdicts"]["overall"]["malicious"]
            .as_bool()
            .unwrap_or(false);
        let verdict_text = result_json["verdicts"]["overall"]["verdict"]
            .as_str()
            .or_else(|| result_json["verdicts"]["overall"]["classification"].as_str())
            .or_else(|| result_json["verdicts"]["overall"]["label"].as_str())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();

        potentially_malicious =
            verdict_text.contains("potentially malicious") || verdict_text.contains("suspicious");

        score = result_json["verdicts"]["overall"]["score"]
            .as_f64()
            .unwrap_or(0.0);
        break;
    }

    if malicious || potentially_malicious {
        tracing::warn!(
            url = %url,
            score = score,
            malicious = malicious,
            potentially_malicious = potentially_malicious,
            "URLScan.io: minaccia rilevata"
        );

        let verdict_label = if malicious {
            "MALICIOUS"
        } else {
            "POTENTIALLY MALICIOUS"
        };

        let msg = format!(
            "🚨 <b>ALLERTA SICUREZZA</b> 🚨\n\
            ━━━━━━━━━━━━━━━━\n\
            🌐 <b>URLScan.io Web Reputation</b>\n\n\
            🔴 <b>LINK PERICOLOSO RILEVATO</b>\n\n\
            📊 <b>Analisi Comportamentale:</b>\n\
            📈 Risk Score: <b>{:.1}/100</b>\n\
            🔴 Classificato come: <b>{}</b>\n\
            \n🔒 <b>ATTENZIONE:</b> Pagina web sospetta\n\
            Potrebbe contenere phishing o malware.\n\n\
            📋 <a href=\"{}\">Visualizza Scansione Completa ›</a>",
            score, verdict_label, result_link
        );

        return Some(msg);
    }

    tracing::info!(url = %url, score = score, "URLScan.io: URL senza segnali critici");
    if alert_only {
        return None;
    }

    let safety_level = if score == 0.0 {
        "✅ <b>COMPLETAMENTE SICURO</b>"
    } else if score < 25.0 {
        "✅ <b>BASSO RISCHIO</b>"
    } else {
        "🟢 <b>ACCETTABILE</b>"
    };

    Some(format!(
        "✅ <b>URL VERIFICATO</b>\n\
        ━━━━━━━━━━━━━━━━\n\
        🌐 <b>URLScan.io Web Reputation</b>\n\n\
        {}\n\n\
        📊 <b>Analisi Comportamentale:</b>\n\
        📈 Risk Score: <b>{:.1}/100</b>\n\
        🔍 Status: Nessuna minaccia rilevata\n\n\
        ✨ Pagina web verificata sicura\n\
        📋 <a href=\"{}\">Visualizza Scansione ›</a>",
        safety_level, score, result_link
    ))
}

/// Handles the `/start` command.
///
/// # Errors
/// Returns an error if message sending fails.
async fn handle_start_command(
    bot: Bot,
    chat_id: ChatId,
    user_id: i64,
    tr: &crate::i18n::Translations,
    _config: &crate::config::Config,
    message_id: Option<MessageId>,
) -> ResponseResult<()> {
    let welcome_text = tr.welcome.replace("{}", &user_id.to_string());

    // Create inline keyboard with settings button
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
            .reply_markup(main_reply_keyboard(tr))
            .await?;
    }

    Ok(())
}

/// Helper function to get user's preferred language.
///
/// Retrieves language from user configuration or Telegram language code.
async fn get_user_language(db: &Db, user_id: i64, telegram_lang: Option<&str>) -> &'static str {
    // Try to get user config from database
    if let Ok(cfg) = db.get_user_config(user_id).await {
        if !cfg.language.is_empty() && cfg.language != "en" {
            // Return 'it' if language is Italian, otherwise default to 'en'
            if cfg.language == "it" || cfg.language.starts_with("it") {
                return "it";
            }
        }
    }

    // Fallback to Telegram language
    if let Some(l) = telegram_lang {
        if l.starts_with("it") {
            return "it";
        }
    }

    // Default to English
    "en"
}

/// Handles callback query from inline keyboard buttons.
///
/// # Errors
/// Returns an error if callback query handling fails.
#[tracing::instrument(skip(bot, db, config), fields(user_id, chat_id))]
async fn handle_callback(
    bot: Bot,
    q: CallbackQuery,
    db: Db,
    config: crate::config::Config,
) -> ResponseResult<()> {
    let user_id = q.from.id.0 as i64;
    // Rate limiting anti-flood anche sulle callback
    if !RATE_LIMITER.check(user_id) {
        return Ok(());
    }
    let callback_data = sanitize_callback(q.data.as_deref().unwrap_or(""));
    let chat_id = q
        .message
        .as_ref()
        .map(teloxide::types::MaybeInaccessibleMessage::chat)
        .map(|chat| chat.id);
    // message_id deve essere sempre propagato per editare il messaggio originale
    let message_id = q
        .message
        .as_ref()
        .map(teloxide::types::MaybeInaccessibleMessage::id);

    // Get user's preferred language
    let telegram_lang = q.from.language_code.as_deref();
    let lang_code = get_user_language(&db, user_id, telegram_lang).await;
    let tr = crate::i18n::get_translations(lang_code);

    if let Some(chat_id) = chat_id {
        if callback_data.starts_with("settings:") {
            let parts: Vec<&str> = callback_data.split(':').collect();
            let target_user_id = callback_target_user_id(&parts, user_id);
            if target_user_id != user_id {
                show_no_permission_view(&bot, chat_id, message_id, &tr).await?;
            } else {
                handle_settings_callback(
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
            handle_user_settings_callback(
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
            handle_admin_settings_callback(
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
            handle_quick_callback(
                CallbackContext {
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
            let target_user_id = callback_target_user_id(&parts, user_id);
            if target_user_id != user_id {
                show_no_permission_view(&bot, chat_id, message_id, &tr).await?;
            } else {
                handle_start_command(bot.clone(), chat_id, user_id, &tr, &config, message_id)
                    .await?;
            }
        }
    }

    // Answer callback to remove loading state
    bot.answer_callback_query(q.id).await?;

    Ok(())
}

async fn handle_settings_callback(
    // message_id deve essere sempre quello della callback per editare il messaggio
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    user_id: i64,
    db: Db,
    config: crate::config::Config,
    tr: &crate::i18n::Translations,
) -> ResponseResult<()> {
    let _user_config = db.get_user_config(user_id).await.unwrap_or_default();
    let is_admin = user_id == config.admin_id;

    let role = if is_admin {
        tr.s_role_admin
    } else {
        tr.s_role_user
    };

    let settings_text = format!(
        "<b>⚙️  {}</b>\n\n\
        <b>👤 Profilo:</b>\n\
        ID: <code>{}</code>\n\
        Ruolo: <b>{}</b>\n\n\
        <b>📋 Impostazioni disponibili:</b>",
        tr.s_menu_title, user_id, role
    );

    let mut keyboard_rows = vec![
        vec![
            InlineKeyboardButton::callback(
                format!("🔔 {}", tr.s_notifications),
                format!("user_setting:notifications:{}", user_id),
            ),
            InlineKeyboardButton::callback(
                format!("🤖 {}", tr.s_ai_settings),
                format!("user_setting:ai:{}", user_id),
            ),
        ],
        vec![
            InlineKeyboardButton::callback(
                format!("🔒 {}", tr.s_privacy),
                format!("user_setting:privacy:{}", user_id),
            ),
            InlineKeyboardButton::callback(
                format!("⚡ {}", tr.s_link_processing),
                format!("user_setting:links:{}", user_id),
            ),
        ],
        vec![InlineKeyboardButton::callback(
            format!("🌐 {}", tr.s_language),
            format!("user_setting:language:{}", user_id),
        )],
    ];

    // Add admin options if user is admin
    if is_admin {
        keyboard_rows.push(vec![
            InlineKeyboardButton::callback(
                format!("🛠️  {}", tr.s_admin_panel),
                format!("admin_setting:panel:{}", user_id),
            ),
            InlineKeyboardButton::callback(
                format!("📊 {}", tr.s_statistics),
                format!("admin_setting:stats:{}", user_id),
            ),
        ]);
    }

    keyboard_rows.push(vec![InlineKeyboardButton::callback(
        format!("◀️  {}", tr.s_back_to_main),
        format!("back_to_main:{}", user_id),
    )]);

    let keyboard = InlineKeyboardMarkup::new(keyboard_rows);

    // Risposta atomica: una sola edit/send
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

/// Context per callback handlers - riduce il numero di parametri
struct CallbackContext {
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    user_id: i64,
}

#[allow(clippy::too_many_arguments)]
async fn handle_quick_callback(
    ctx: CallbackContext,
    callback_data: &str,
    db: Db,
    config: crate::config::Config,
    tr: &crate::i18n::Translations,
) -> ResponseResult<()> {
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
                Some(quick_actions_inline_keyboard(tr, ctx.user_id)),
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
                Some(quick_actions_inline_keyboard(tr, ctx.user_id)),
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
                Some(language_inline_keyboard(tr, ctx.user_id)),
                true,
            )
            .await
        }
        _ => Ok(()),
    }
}

async fn handle_user_settings_callback(
    // message_id deve essere sempre quello della callback per editare il messaggio
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    user_id: i64,
    callback_data: &str,
    db: Db,
    tr: &crate::i18n::Translations,
) -> ResponseResult<()> {
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
    let user_config: crate::db::models::UserConfig =
        db.get_user_config(user_id).await.unwrap_or_default();

    let (message_text, keyboard) = match setting_type {
        "notifications" => (
            format!("<b>{}</b>\n\n{}", tr.s_notif_title, tr.s_notif_desc),
            InlineKeyboardMarkup::new(vec![
                vec![
                    InlineKeyboardButton::callback(
                        tr.s_enabled,
                        format!("user_setting:toggle:notif:1:{}", user_id),
                    ),
                    InlineKeyboardButton::callback(
                        tr.s_disabled,
                        format!("user_setting:toggle:notif:0:{}", user_id),
                    ),
                ],
                vec![InlineKeyboardButton::callback(
                    tr.s_back,
                    format!("settings:{}", user_id),
                )],
            ]),
        ),
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
            let keyboard = InlineKeyboardMarkup::new(vec![
                vec![InlineKeyboardButton::callback(
                    tr.s_toggle_ai,
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
            let keyboard = language_inline_keyboard(tr, user_id);
            (message, keyboard)
        }
        "lang" if parts.len() >= 4 => {
            let language = parts[2];
            let mut updated = user_config.clone();
            let mut ok = true;
            if language == "it" || language == "en" {
                updated.language = language.to_string();
                if let Err(e) = db.save_user_config(&updated).await {
                    tracing::error!(error = %e, user_id, "Errore nel salvataggio lingua");
                    ok = false;
                }
            }

            let text = if ok {
                format!("{}: <b>{}</b>", tr.s_language_updated, updated.language)
            } else {
                tr.s_setting_update_failed.to_string()
            };
            let keyboard = settings_back_keyboard(tr, user_id);
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
                    tr.s_setting_updated.to_string()
                } else {
                    tr.s_setting_update_failed.to_string()
                },
                keyboard,
            )
        }
        "toggle" if parts.len() >= 4 => {
            let setting = parts[2];
            let value = parts[3];

            // Handle toggle logic here (would update database)
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

#[allow(clippy::too_many_arguments)]
async fn handle_admin_settings_callback(
    // message_id deve essere sempre quello della callback per editare il messaggio
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    user_id: i64,
    callback_data: &str,
    db: Db,
    config: &crate::config::Config,
    tr: &crate::i18n::Translations,
) -> ResponseResult<()> {
    // Verify admin permissions
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
        "stats" => {
            let (total_cleaned, total_users) = db.get_global_stats().await.unwrap_or((0, 0));
            let message = admin_global_stats_message(tr, total_users, total_cleaned);
            let keyboard =
                admin_global_stats_keyboard(tr, user_id, format!("settings:{}", user_id));
            (message, keyboard)
        }
        "refresh_stats" => {
            let (total_cleaned, total_users) = db.get_global_stats().await.unwrap_or((0, 0));
            let message = admin_global_stats_message(tr, total_users, total_cleaned);
            let keyboard =
                admin_global_stats_keyboard(tr, user_id, format!("settings:{}", user_id));
            (message, keyboard)
        }
        "users" => {
            let (_, total_users) = db.get_global_stats().await.unwrap_or((0, 0));
            let message = admin_users_message(tr, total_users);
            let keyboard =
                single_back_keyboard(tr.s_back, format!("admin_setting:panel:{}", user_id));
            (message, keyboard)
        }
        "system" => {
            let message = admin_system_message(tr);
            let keyboard =
                single_back_keyboard(tr.s_back, format!("admin_setting:panel:{}", user_id));
            (message, keyboard)
        }
        "global_stats" => {
            let (total_cleaned, total_users) = db.get_global_stats().await.unwrap_or((0, 0));
            let message = admin_global_stats_message(tr, total_users, total_cleaned);
            let keyboard = admin_global_stats_keyboard(
                tr,
                user_id,
                format!("admin_setting:panel:{}", user_id),
            );
            (message, keyboard)
        }
        "maintenance" => {
            let message = admin_maintenance_message(tr);
            let keyboard = admin_maintenance_keyboard(tr, user_id);
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

#[allow(clippy::too_many_arguments)]
async fn handle_setting_toggle(
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    user_id: i64,
    setting: &str,
    value: &str,
    db: Db,
    tr: &crate::i18n::Translations,
) -> ResponseResult<()> {
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

async fn upsert_settings_view(
    bot: &Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    text: String,
    keyboard: Option<InlineKeyboardMarkup>,
    parse_html: bool,
) -> ResponseResult<()> {
    if let Some(message_id) = message_id {
        let mut edit = bot.edit_message_text(chat_id, message_id, text.clone());
        if parse_html {
            edit = edit.parse_mode(ParseMode::Html);
        }
        if let Some(kb) = keyboard.clone() {
            edit = edit.reply_markup(kb);
        }

        match edit.await {
            Ok(_) => return Ok(()),
            Err(err) => {
                if is_message_not_modified_error(&err.to_string()) {
                    return Ok(());
                }
            }
        }
    }

    let mut send = bot.send_message(chat_id, text);
    if parse_html {
        send = send.parse_mode(ParseMode::Html);
    }
    if let Some(kb) = keyboard {
        send = send.reply_markup(kb);
    }
    send.await?;

    Ok(())
}

fn callback_target_user_id(parts: &[&str], fallback_user_id: i64) -> i64 {
    parts
        .last()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(fallback_user_id)
}

fn extract_url_candidates(text: &str) -> Vec<String> {
    let url_pattern = r"(?i)(?:https?://|www\.)[a-zA-Z0-9\-\.]+\.[a-zA-Z]{2,}(?:/[^\s]*)?";
    let Ok(re) = Regex::new(url_pattern) else {
        return Vec::new();
    };

    let mut urls = Vec::new();
    for m in re.find_iter(text) {
        let candidate = m.as_str().to_string();
        if !urls.contains(&candidate) {
            urls.push(candidate);
        }
    }
    urls
}

fn removed_query_params_count(original: &str, cleaned: &str) -> usize {
    let original_count = query_params_count(original);
    let cleaned_count = query_params_count(cleaned);
    original_count.saturating_sub(cleaned_count)
}

fn query_params_count(raw_url: &str) -> usize {
    let normalized = if raw_url.starts_with("http://") || raw_url.starts_with("https://") {
        raw_url.to_string()
    } else {
        format!("https://{raw_url}")
    };

    let Ok(parsed) = url::Url::parse(&normalized) else {
        return 0;
    };
    parsed.query_pairs().count()
}

fn is_message_not_modified_error(error_text: &str) -> bool {
    error_text
        .to_lowercase()
        .contains("message is not modified")
}

fn main_reply_keyboard(tr: &crate::i18n::Translations) -> KeyboardMarkup {
    KeyboardMarkup::new(vec![
        vec![
            KeyboardButton::new(tr.rk_settings),
            KeyboardButton::new(tr.rk_stats),
        ],
        vec![
            KeyboardButton::new(tr.rk_help),
            KeyboardButton::new(tr.rk_language),
        ],
        vec![KeyboardButton::new(tr.rk_hidekbd)],
    ])
    .resize_keyboard()
}

#[derive(Clone, Copy)]
enum QuickReplyAction {
    Settings,
    Stats,
    Help,
    HideKeyboard,
    Language,
}

fn quick_reply_action(text: &str, tr: &crate::i18n::Translations) -> Option<QuickReplyAction> {
    let trimmed = text.trim();
    if trimmed == tr.rk_settings {
        Some(QuickReplyAction::Settings)
    } else if trimmed == tr.rk_stats {
        Some(QuickReplyAction::Stats)
    } else if trimmed == tr.rk_help {
        Some(QuickReplyAction::Help)
    } else if trimmed == tr.rk_hidekbd {
        Some(QuickReplyAction::HideKeyboard)
    } else if trimmed == tr.rk_language {
        Some(QuickReplyAction::Language)
    } else {
        None
    }
}

fn quick_actions_inline_keyboard(
    tr: &crate::i18n::Translations,
    user_id: i64,
) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback(
                tr.start_open_settings,
                format!("quick:settings:{}", user_id),
            ),
            InlineKeyboardButton::callback(tr.start_view_stats, format!("quick:stats:{}", user_id)),
        ],
        vec![
            InlineKeyboardButton::callback(tr.s_language, format!("quick:language:{}", user_id)),
            InlineKeyboardButton::callback(tr.s_back_to_main, format!("back_to_main:{}", user_id)),
        ],
    ])
}

fn language_inline_keyboard(tr: &crate::i18n::Translations, user_id: i64) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback(
                tr.s_language_it,
                format!("user_setting:lang:it:{}", user_id),
            ),
            InlineKeyboardButton::callback(
                tr.s_language_en,
                format!("user_setting:lang:en:{}", user_id),
            ),
        ],
        vec![InlineKeyboardButton::callback(
            tr.s_back,
            format!("settings:{}", user_id),
        )],
    ])
}

fn single_back_keyboard(label: &str, callback_data: String) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        label,
        callback_data,
    )]])
}

fn settings_back_keyboard(tr: &crate::i18n::Translations, user_id: i64) -> InlineKeyboardMarkup {
    single_back_keyboard(tr.s_back, format!("settings:{}", user_id))
}

async fn show_no_permission_view(
    bot: &Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    tr: &crate::i18n::Translations,
) -> ResponseResult<()> {
    upsert_settings_view(
        bot,
        chat_id,
        message_id,
        tr.s_admin_no_permission.to_string(),
        None,
        false,
    )
    .await
}

fn admin_global_stats_message(
    tr: &crate::i18n::Translations,
    total_users: i64,
    total_cleaned: i64,
) -> String {
    format!(
        "<b>{}</b>\n\n{}\n\n👥 {}: <b>{}</b>\n🔗 {}: <b>{}</b>",
        tr.s_global_stats_title,
        tr.s_global_stats_desc,
        tr.s_total_users_label,
        total_users,
        tr.s_total_cleaned_label,
        total_cleaned
    )
}

fn admin_users_message(tr: &crate::i18n::Translations, total_users: i64) -> String {
    format!(
        "<b>{}</b>\n\n{}: <b>{}</b>",
        tr.s_user_management, tr.s_admin_users_total, total_users
    )
}

fn admin_system_message(tr: &crate::i18n::Translations) -> String {
    format!(
        "<b>{}</b>\n\n{}",
        tr.s_system_settings, tr.s_admin_system_note
    )
}

fn admin_maintenance_message(tr: &crate::i18n::Translations) -> String {
    format!(
        "<b>{}</b>\n\n{}",
        tr.s_maintenance, tr.s_admin_maintenance_none
    )
}

fn admin_global_stats_keyboard(
    tr: &crate::i18n::Translations,
    user_id: i64,
    back_callback_data: String,
) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            tr.s_refresh,
            format!("admin_setting:refresh_stats:{}", user_id),
        )],
        vec![InlineKeyboardButton::callback(
            tr.s_back,
            back_callback_data,
        )],
    ])
}

fn admin_maintenance_keyboard(
    tr: &crate::i18n::Translations,
    user_id: i64,
) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            tr.s_clear_history,
            format!("admin_setting:clear_all_history:{}", user_id),
        )],
        vec![InlineKeyboardButton::callback(
            tr.s_back,
            format!("admin_setting:panel:{}", user_id),
        )],
    ])
}

fn extract_domain(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let url_str = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{}", url)
    };

    let url_obj = url::Url::parse(&url_str)?;
    Ok(url_obj.host_str().unwrap_or("").to_string())
}

/// Build a Telegram-ready reply for the `/redirect <url>` command.
///
/// Kept as a free function (rather than inlined in the command match arm) so
/// it can be unit-tested against a [`RedirectService`] backed by stub data.
async fn build_redirect_reply(
    svc: &RedirectService,
    arg: &str,
    tr: &crate::i18n::Translations,
) -> String {
    let is_it = tr.welcome.contains("Benvenuto") || tr.welcome.contains("benvenuto");
    if arg.is_empty() {
        return if is_it {
            "ℹ️ Uso: <code>/redirect &lt;url&gt;</code>\nEsempio: <code>/redirect youtube.com</code>".into()
        } else {
            "ℹ️ Usage: <code>/redirect &lt;url&gt;</code>\nExample: <code>/redirect youtube.com</code>".into()
        };
    }
    if crate::redirects::extract_host(arg).is_err() {
        return if is_it {
            "⚠️ URL non valido. Usa <code>/redirect &lt;url&gt;</code> con un dominio valido.".into()
        } else {
            "⚠️ Invalid URL. Use <code>/redirect &lt;url&gt;</code> with a valid domain.".into()
        };
    }
    match svc.lookup(arg).await {
        Ok(Some(hit)) => format_hit_html(&hit, 5),
        Ok(None) => {
            if is_it {
                format!(
                    "🤷 Nessun frontend alternativo conosciuto per <code>{}</code>.",
                    html::escape(arg)
                )
            } else {
                format!(
                    "🤷 No known alternative frontend for <code>{}</code>.",
                    html::escape(arg)
                )
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, arg = %arg, "redirect lookup failed");
            if is_it {
                "⚠️ Impossibile contattare le sorgenti di redirect. Riprova più tardi.".into()
            } else {
                "⚠️ Could not reach redirect catalogues. Please retry later.".into()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        admin_global_stats_message, admin_maintenance_message, admin_system_message,
        admin_users_message, callback_target_user_id,
        is_message_not_modified_error, removed_query_params_count,
    };
    use crate::i18n;

    #[test]
    fn callback_target_user_id_uses_last_segment_when_numeric() {
        let parts = vec!["user_setting", "toggle", "ai", "42"];
        let user_id = callback_target_user_id(&parts, 7);
        assert_eq!(user_id, 42);
    }

    #[test]
    fn callback_target_user_id_falls_back_when_last_segment_is_not_numeric() {
        let parts = vec!["user_setting", "toggle", "ai", "abc"];
        let user_id = callback_target_user_id(&parts, 7);
        assert_eq!(user_id, 7);
    }

    #[test]
    fn callback_target_user_id_falls_back_on_empty_parts() {
        let parts: Vec<&str> = vec![];
        let user_id = callback_target_user_id(&parts, 15);
        assert_eq!(user_id, 15);
    }

    #[test]
    fn detects_message_not_modified_error_case_insensitive() {
        let error_text = "Bad Request: MESSAGE IS NOT MODIFIED";
        assert!(is_message_not_modified_error(error_text));
    }

    #[test]
    fn ignores_other_errors() {
        let error_text = "Bad Request: message to edit not found";
        assert!(!is_message_not_modified_error(error_text));
    }

    #[test]
    fn callback_target_user_id_reads_owner_from_settings_callback() {
        let parts = vec!["settings", "99"];
        let user_id = callback_target_user_id(&parts, 7);
        assert_eq!(user_id, 99);
    }

    #[test]
    fn admin_global_stats_message_includes_values_and_labels() {
        let tr = i18n::get_translations("it");
        let message = admin_global_stats_message(&tr, 12, 345);
        assert!(message.contains(tr.s_total_users_label));
        assert!(message.contains(tr.s_total_cleaned_label));
        assert!(message.contains("12"));
        assert!(message.contains("345"));
    }

    #[test]
    fn admin_users_message_includes_total_users() {
        let tr = i18n::get_translations("en");
        let message = admin_users_message(&tr, 27);
        assert!(message.contains(tr.s_user_management));
        assert!(message.contains(tr.s_admin_users_total));
        assert!(message.contains("27"));
    }

    #[test]
    fn admin_system_message_uses_localized_note() {
        let tr = i18n::get_translations("it");
        let message = admin_system_message(&tr);
        assert!(message.contains(tr.s_system_settings));
        assert!(message.contains(tr.s_admin_system_note));
    }

    #[test]
    fn admin_maintenance_message_uses_localized_note() {
        let tr = i18n::get_translations("en");
        let message = admin_maintenance_message(&tr);
        assert!(message.contains(tr.s_maintenance));
        assert!(message.contains(tr.s_admin_maintenance_none));
    }

    #[test]
    fn build_redirect_reply_invalid_url_returns_error_message() {
        let svc = crate::redirects::RedirectService::with_urls(
            "http://x.invalid",
            "http://x.invalid",
            std::time::Duration::from_secs(60),
        )
        .unwrap();
        let tr = i18n::get_translations("en");
        let out = futures::executor::block_on(super::build_redirect_reply(&svc, "not a url", &tr));
        assert!(out.contains("Invalid URL") || out.contains("URL non valido"));
    }

    #[test]
    fn removed_query_params_count_detects_removed_tracking_params() {
        let original = "https://example.com/path?a=1&b=2&utm_source=x";
        let cleaned = "https://example.com/path?a=1";
        assert_eq!(removed_query_params_count(original, cleaned), 2);
    }

    #[test]
    fn removed_query_params_count_handles_schemeless_urls() {
        let original = "www.example.com/?a=1&b=2";
        let cleaned = "www.example.com/?a=1";
        assert_eq!(removed_query_params_count(original, cleaned), 1);
    }
}
