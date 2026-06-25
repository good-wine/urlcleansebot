use crate::i18n;
use crate::redirects::RedirectService;
use regex::Regex;
use std::sync::LazyLock;
use teloxide::prelude::*;
use teloxide::types::{
    ChatId, InlineKeyboardButton, InlineKeyboardMarkup, InlineQuery, InlineQueryResult,
    InlineQueryResultArticle, InputMessageContent, InputMessageContentText, KeyboardButton,
    KeyboardMarkup, Message, MessageEntityKind, MessageId, ParseMode,
};

use std::collections::HashSet;

static URL_FALLBACK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:https?://|www\.)[a-zA-Z0-9\-\.]+\.[a-zA-Z]{2,}(?:/[^\s]*)?").unwrap()
});

pub fn extract_url_candidates(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for m in URL_FALLBACK_REGEX.find_iter(text) {
        let candidate = m.as_str().to_string();
        if !urls.contains(&candidate) {
            urls.push(candidate);
        }
    }
    urls
}

pub fn removed_query_params_count(original: &str, cleaned: &str) -> usize {
    let original_count = query_params_count(original);
    let cleaned_count = query_params_count(cleaned);
    original_count.saturating_sub(cleaned_count)
}

pub fn query_params_count(raw_url: &str) -> usize {
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

pub fn extract_domain(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let url_str = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{}", url)
    };

    let url_obj = url::Url::parse(&url_str)?;
    Ok(url_obj.host_str().unwrap_or("").to_string())
}

pub fn is_message_not_modified_error(error_text: &str) -> bool {
    error_text
        .to_lowercase()
        .contains("message is not modified")
}

pub async fn get_user_language(
    _db: &crate::db::Db,
    _user_id: i64,
    telegram_lang: Option<&str>,
) -> String {
    if let Some(l) = telegram_lang {
        for &code in SUPPORTED_LANGUAGES {
            if l.starts_with(code) {
                return code.to_string();
            }
        }
    }

    "en".to_string()
}

pub fn callback_target_user_id(parts: &[&str], fallback_user_id: i64) -> i64 {
    parts
        .last()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(fallback_user_id)
}

pub fn main_reply_keyboard(tr: &i18n::Translations) -> KeyboardMarkup {
    KeyboardMarkup::new(vec![
        vec![
            KeyboardButton::new(tr.rk_settings),
            KeyboardButton::new(tr.rk_stats),
        ],
        vec![
            KeyboardButton::new(tr.rk_help),
            KeyboardButton::new(tr.rk_hidekbd),
        ],
    ])
    .resize_keyboard()
}

#[derive(Clone, Copy)]
pub enum QuickReplyAction {
    Settings,
    Stats,
    Help,
    HideKeyboard,
}

pub fn quick_reply_action(text: &str, tr: &i18n::Translations) -> Option<QuickReplyAction> {
    let trimmed = text.trim();
    if trimmed == tr.rk_settings {
        Some(QuickReplyAction::Settings)
    } else if trimmed == tr.rk_stats {
        Some(QuickReplyAction::Stats)
    } else if trimmed == tr.rk_help {
        Some(QuickReplyAction::Help)
    } else if trimmed == tr.rk_hidekbd {
        Some(QuickReplyAction::HideKeyboard)
    } else {
        None
    }
}

pub fn quick_actions_inline_keyboard(
    tr: &i18n::Translations,
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
        vec![InlineKeyboardButton::callback(
            tr.s_back_to_main,
            format!("back_to_main:{}", user_id),
        )],
    ])
}

pub const SUPPORTED_LANGUAGES: &[&str] = &[
    "it", "en", "es", "fr", "de", "pt", "ru", "ar", "hi", "zh", "ja", "ko", "tr", "nl", "pl",
];

pub fn language_name(code: &str) -> String {
    match code {
        "it" => "Italiano 🇮🇹",
        "en" => "English 🇬🇧",
        "es" => "Español 🇪🇸",
        "fr" => "Français 🇫🇷",
        "de" => "Deutsch 🇩🇪",
        "pt" => "Português 🇧🇷",
        "ru" => "Русский 🇷🇺",
        "ar" => "العربية 🇸🇦",
        "hi" => "हिन्दी 🇮🇳",
        "zh" => "中文 🇨🇳",
        "ja" => "日本語 🇯🇵",
        "ko" => "한국어 🇰🇷",
        "tr" => "Türkçe 🇹🇷",
        "nl" => "Nederlands 🇳🇱",
        "pl" => "Polski 🇵🇱",
        other => other,
    }
    .to_string()
}

pub fn single_back_keyboard(label: &str, callback_data: String) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        label,
        callback_data,
    )]])
}

pub fn settings_back_keyboard(tr: &i18n::Translations, user_id: i64) -> InlineKeyboardMarkup {
    single_back_keyboard(tr.s_back, format!("settings:{}", user_id))
}

pub async fn upsert_settings_view(
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
            },
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

pub async fn show_no_permission_view(
    bot: &Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    tr: &i18n::Translations,
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

pub fn admin_global_stats_message(
    tr: &i18n::Translations,
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

pub fn admin_users_message(tr: &i18n::Translations, total_users: i64) -> String {
    format!(
        "<b>{}</b>\n\n{}: <b>{}</b>",
        tr.s_user_management, tr.s_admin_users_total, total_users
    )
}

pub fn admin_system_message(tr: &i18n::Translations) -> String {
    format!(
        "<b>{}</b>\n\n{}",
        tr.s_system_settings, tr.s_admin_system_note
    )
}

pub fn admin_maintenance_message(tr: &i18n::Translations) -> String {
    format!(
        "<b>{}</b>\n\n{}",
        tr.s_maintenance, tr.s_admin_maintenance_none
    )
}

pub fn admin_global_stats_keyboard(
    tr: &i18n::Translations,
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

pub fn admin_maintenance_keyboard(tr: &i18n::Translations, user_id: i64) -> InlineKeyboardMarkup {
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

pub fn has_url_entities(msg: &Message, text: &str) -> bool {
    if let Some(ents) = msg.entities() {
        for entity in ents {
            if matches!(
                entity.kind,
                MessageEntityKind::Url | MessageEntityKind::TextLink { .. }
            ) {
                return true;
            }
        }
    }

    !extract_url_candidates(text).is_empty()
}

pub fn extract_urls_from_message(msg: &Message, text: &str) -> Vec<String> {
    let mut url_candidates = Vec::new();

    if let Some(ents) = msg.entities() {
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
                },
                MessageEntityKind::TextLink { url } => url.to_string(),
                _ => continue,
            };
            if !url_candidates.contains(&url_str) {
                url_candidates.push(url_str);
            }
        }
    }

    for mat in URL_FALLBACK_REGEX.find_iter(text) {
        let url_str = mat.as_str().to_string();
        if !url_candidates.contains(&url_str) {
            url_candidates.push(url_str);
        }
    }

    url_candidates
}

pub async fn send_inline_results(
    bot: &Bot,
    query: &InlineQuery,
    results: Vec<InlineQueryResult>,
) -> Result<(), teloxide::RequestError> {
    bot.answer_inline_query(query.id.clone(), results)
        .cache_time(1)
        .is_personal(true)
        .await
        .map(|_| ())
}

pub fn build_inline_help_article(lang_code: &str) -> InlineQueryResult {
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
    InlineQueryResult::Article(article)
}

pub fn build_inline_no_results(query: &str, tr: &i18n::Translations) -> InlineQueryResult {
    let article = InlineQueryResultArticle::new(
        "inline-no-results",
        tr.inline_no_results,
        InputMessageContent::Text(InputMessageContentText::new(query.to_string())),
    );
    InlineQueryResult::Article(article)
}

pub fn build_inline_clean_result(
    rank: usize,
    cleaned: &str,
    removed_params: usize,
    tr: &i18n::Translations,
) -> InlineQueryResult {
    let title = if removed_params > 0 {
        tr.inline_clean_params
            .replace("{}", &(rank + 1).to_string())
            .replace("{}", &removed_params.to_string())
    } else {
        tr.inline_clean.replace("{}", &(rank + 1).to_string())
    };

    let content = InputMessageContent::Text(InputMessageContentText::new(cleaned.to_string()));
    let article = InlineQueryResultArticle::new(format!("clean-{}", rank), title, content)
        .description(cleaned.to_string());

    InlineQueryResult::Article(article)
}

pub async fn send_alternative_frontends(
    bot: &Bot,
    chat_id: ChatId,
    urls: &[String],
    redirect_service: &RedirectService,
) -> Result<(), teloxide::RequestError> {
    let mut seen_hosts = HashSet::new();
    for url in urls {
        if let Ok(host) = crate::redirects::extract_host(url) {
            let host = host.trim_start_matches("www.").to_ascii_lowercase();
            if !seen_hosts.insert(host.clone()) {
                continue;
            }

            if let Ok(Some(hit)) = redirect_service.lookup_by_host(&host).await {
                let frontend_msg = crate::redirects::format_hit_html(&hit, 3, url);
                let _ = bot
                    .send_message(chat_id, frontend_msg)
                    .parse_mode(ParseMode::Html)
                    .await;
                break;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        admin_global_stats_message, admin_maintenance_message, admin_system_message,
        admin_users_message, callback_target_user_id, is_message_not_modified_error,
        removed_query_params_count,
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
