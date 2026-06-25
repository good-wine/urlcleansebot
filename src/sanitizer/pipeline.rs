use std::collections::HashSet;

use crate::constants::URL_CACHE_MAX_CAPACITY;
use crate::db::models::{CustomRule, UserConfig};
use crate::metrics;
use crate::sanitizer::honor_creator as honor_creator_mod;
use crate::sanitizer::linkumori::LinkumoriEngine;
use crate::sanitizer::{AiEngine, RuleEngine};
use crate::shared::security::is_safe_url_scheme;
use moka::future::Cache;
use std::sync::LazyLock;

static PIPELINE_URL_CACHE: LazyLock<Cache<String, String>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(URL_CACHE_MAX_CAPACITY)
        .build()
});

#[derive(Debug, Clone)]
pub struct SanitizedUrl {
    pub original_url: String,
    pub cleaned_url: String,
    pub provider: String,
    pub removed_params: usize,
    pub param_names: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_sanitization_pipeline(
    url_str: &str,
    rules: &RuleEngine,
    ai: &AiEngine,
    linkumori: &LinkumoriEngine,
    user_config: &UserConfig,
    custom_rules: &[CustomRule],
    ignored_domains: &[String],
    _dry_run: bool,
) -> Option<SanitizedUrl> {
    let expanded_url = rules.expand_url(url_str).await;
    if !rules.is_supported_by_clearurls(&expanded_url) {
        return None;
    }

    if let Some(cached) = PIPELINE_URL_CACHE.get(&expanded_url).await {
        let removed_params = crate::presentation::telegram::helpers::removed_query_params_count(
            url_str,
            &cached,
        );
        return Some(SanitizedUrl {
            original_url: url_str.to_string(),
            cleaned_url: cached,
            provider: "CACHE".to_string(),
            removed_params,
            param_names: Vec::new(),
        });
    }

    let (cleaned_url, provider) = match rules.sanitize(&expanded_url, custom_rules, ignored_domains) {
        Some(result) => result,
        None => return None,
    };

    if cleaned_url == *url_str && cleaned_url == expanded_url {
        return None;
    }

    metrics::SANITIZATIONS_CLEANED.inc();

    let mut current_url = cleaned_url;
    let mut current_provider = provider;

    if user_config.is_ai_enabled()
        && let Ok(Some(ai_cleaned)) = ai.sanitize(&current_url).await
    {
        metrics::AI_SANITIZATIONS.inc();
        current_url = ai_cleaned;
        current_provider = format!("AI ({current_provider})");
    }

    if user_config.is_honor_creator()
        && let Some(honor_cleaned) = honor_creator_mod::clean_keeping_affiliates(&current_url)
    {
        current_url = honor_cleaned;
    }

    if user_config.is_aggressive()
        && let Some(agg_cleaned) = crate::sanitizer::aggressive::sanitize_aggressive(&current_url)
    {
        current_url = agg_cleaned;
    }

    if linkumori.source_count() > 0
        && let Ok(parsed_url) = url::Url::parse(&current_url)
    {
        let pairs: Vec<(String, String)> = parsed_url
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let original_count = pairs.len();
        let keep: Vec<_> = pairs
            .into_iter()
            .filter(|(name, _)| !linkumori.should_remove_param(name, &current_url))
            .collect();
        if keep.len() < original_count {
            let mut clean_url = parsed_url;
            clean_url.query_pairs_mut().clear();
            for (name, value) in &keep {
                clean_url.query_pairs_mut().append_pair(name, value);
            }
            current_url = clean_url.to_string();
        }
    }

    PIPELINE_URL_CACHE
        .insert(expanded_url.clone(), current_url.clone())
        .await;

    let removed_params =
        crate::presentation::telegram::helpers::removed_query_params_count(url_str, &current_url);
    let param_names = crate::sanitizer::aggressive::extract_removed_params(url_str, &current_url)
        .unwrap_or_default();

    Some(SanitizedUrl {
        original_url: url_str.to_string(),
        cleaned_url: current_url,
        provider: current_provider,
        removed_params,
        param_names,
    })
}

pub fn build_response_text(
    sanitized: &[SanitizedUrl],
    is_group_context: bool,
    user_name: &str,
    tr: &crate::i18n::Translations,
) -> String {
    let mut response = if is_group_context {
        tr.cleaned_for.replace("{}", user_name)
    } else {
        String::from(tr.cleaned_links)
    };

    let mut total_params_removed = 0;
    let mut all_param_names: Vec<String> = Vec::new();
    for s in sanitized {
        total_params_removed += s.removed_params;
        for p in &s.param_names {
            if !all_param_names.contains(p) {
                all_param_names.push(p.clone());
            }
        }
    }

    let stats_line = format!(
        "\n{}\n{}\n{}\n{}",
        tr.stat_cleaning_completed,
        tr.stat_separator,
        tr.stat_statistics,
        tr.stat_urls_cleaned.replace("{}", &sanitized.len().to_string()),
    );

    if total_params_removed > 0 {
        response.push_str(&stats_line);
        response.push_str(&format!(
            "{}\n",
            tr.stat_params_removed.replace("{}", &total_params_removed.to_string())
        ));
        if !all_param_names.is_empty() && all_param_names.len() <= 7 {
            response.push_str(&format!(
                "{}\n\n",
                tr.stat_removed.replace("{}", &all_param_names.join(", "))
            ));
        } else {
            response.push('\n');
        }
    } else {
        response.push_str(&stats_line);
        response.push('\n');
    }

    if !response.ends_with('\n') {
        response.push('\n');
    }

    response.push_str(tr.cleaned_links);

    const MAX_RESPONSE_LENGTH: usize = 4000;

    if sanitized.len() == 1 {
        let clean = sanitized[0].cleaned_url.trim();
        let escaped_url = crate::shared::security::sanitize_telegram_text(clean);
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
        for (idx, s) in sanitized.iter().enumerate() {
            let clean = s.cleaned_url.trim();
            let escaped_url = crate::shared::security::sanitize_telegram_text(clean);
            let link_entry = if is_safe_url_scheme(clean) {
                format!(
                    "{} <a href=\"{escaped_url}\">{escaped_url}</a>\n",
                    if idx == sanitized.len() - 1 {
                        "\u{2514}\u{2500}"
                    } else {
                        "\u{251c}\u{2500}"
                    }
                )
            } else {
                format!(
                    "{} <code>{escaped_url}</code>\n",
                    if idx == sanitized.len() - 1 {
                        "\u{2514}\u{2500}"
                    } else {
                        "\u{251c}\u{2500}"
                    }
                )
            };

            if response.len() + link_entry.len() > MAX_RESPONSE_LENGTH {
                response.push_str(&format!("\u{2514}\u{2500} <i>{}</i>\n", tr.stat_and_others));
                response.push_str(tr.truncated);
                break;
            }
            response.push_str(&link_entry);
        }
    }

    response
}

pub fn deduplicate_urls(urls: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    urls.into_iter().filter(|u| seen.insert(u.clone())).collect()
}
