use moka::sync::Cache as SyncCache;
use std::sync::LazyLock;
use teloxide::RequestError;
use teloxide::prelude::*;
use teloxide::types::CallbackQuery;
use tracing;

use crate::presentation::telegram::helpers;
use crate::presentation::telegram::settings;
use crate::constants::CALLBACK_DEDUP_TTL_SECS;
use crate::i18n;
use crate::metrics;
use crate::shared::security::{check_rate_limit, sanitize_callback};

static CALLBACK_CACHE: LazyLock<SyncCache<String, ()>> = LazyLock::new(|| {
    SyncCache::builder()
        .max_capacity(50_000)
        .time_to_live(std::time::Duration::from_secs(CALLBACK_DEDUP_TTL_SECS))
        .build()
});

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
        metrics::RATE_LIMIT_HITS.inc();
        return Ok(());
    }
    metrics::REQUESTS_CALLBACK.inc();
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
