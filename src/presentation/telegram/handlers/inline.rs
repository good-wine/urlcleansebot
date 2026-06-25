use teloxide::RequestError;
use teloxide::prelude::*;
use teloxide::types::{ChosenInlineResult, ChatId, InlineQuery, ParseMode};
use teloxide::utils::html;
use tracing;

use crate::presentation::telegram::helpers;
use crate::i18n;
use crate::metrics;
use crate::sanitizer::{AiEngine, RuleEngine, linkumori::LinkumoriEngine};
use crate::shared::security::{check_rate_limit, sanitize_input};

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
        metrics::RATE_LIMIT_HITS.inc();
        return Ok(());
    }
    metrics::REQUESTS_INLINE.inc();
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
