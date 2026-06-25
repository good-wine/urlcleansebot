pub mod clearurls;
pub mod expand;
pub mod github;
pub mod redact;
pub mod ssrf;

use crate::constants::AGGRESSIVE_TRACKERS;
use crate::db::models::CustomRule;
use crate::http_utils::retry_with_backoff;
use crate::shared::error::{AppError, AppResult};
use clearurls::{ClearUrlsData, CompiledProvider};
use moka::future::Cache;
use std::sync::{Arc, RwLock};
use tracing;
use url::Url;

#[derive(Clone)]
pub struct RuleEngine {
    providers: Arc<RwLock<Vec<CompiledProvider>>>,
    source_url: String,
    cache: Cache<String, String>,
}

impl RuleEngine {
    pub fn new_lazy(source_url: &str) -> Self {
        Self {
            providers: Arc::new(RwLock::new(Vec::new())),
            source_url: source_url.to_string(),
            cache: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(std::time::Duration::from_secs(3600))
                .build(),
        }
    }

    pub async fn new(source_url: &str) -> AppResult<Self> {
        let engine = Self::new_lazy(source_url);
        engine.refresh().await?;
        Ok(engine)
    }

    pub async fn refresh(&self) -> AppResult<()> {
        tracing::info!("Scaricamento regole da {}", self.source_url);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let resp = retry_with_backoff(
            || async {
                let req = client.get(&self.source_url);
                req.send().await.map_err(|e| e.to_string())
            },
            "download ClearURLs rules",
        )
        .await
        .map_err(|e| AppError::Internal(format!("ClearURLs rules download failed after retries: {e}")))?
        .text()
        .await?;

        let data: ClearUrlsData = serde_json::from_str(&resp).map_err(|e| {
            AppError::Internal(format!("Impossibile analizzare il JSON di ClearURLs: {e}"))
        })?;

        let compiled = clearurls::compile_providers(data);
        let count = compiled.len();

        {
            if let Ok(mut w) = self.providers.write() {
                *w = compiled;
            } else {
                tracing::error!("Impossibile ottenere il lock in scrittura per i provider");
                return Err(AppError::Internal("Errore lock scrittura provider".into()));
            }
        }

        tracing::info!("Caricati {} provider", count);
        Ok(())
    }

    pub fn is_supported_by_clearurls(&self, text: &str) -> bool {
        if let Ok(providers) = self.providers.read() {
            for provider in providers.iter() {
                if provider.url_pattern.is_match(text) {
                    return true;
                }
            }
        }
        false
    }

    pub fn redact_sensitive(&self, text: &str) -> String {
        redact::redact_sensitive(text)
    }

    pub async fn expand_url(&self, input_url: &str) -> String {
        expand::expand_url(input_url, &self.cache).await
    }

    #[tracing::instrument(skip(self, custom_rules, ignored_domains))]
    pub fn sanitize(
        &self,
        text: &str,
        custom_rules: &[CustomRule],
        ignored_domains: &[String],
    ) -> Option<(String, String)> {
        tracing::debug!(url = %self.redact_sensitive(text), "Avvio sanitizzazione");

        let mut url_to_parse = text.to_string();
        if !url_to_parse.contains("://") && !url_to_parse.starts_with("mailto:") {
            url_to_parse = format!("http://{}", url_to_parse);
        }

        if let Ok(mut url) = Url::parse(&url_to_parse) {
            if let Some(host) = url.host_str()
                && ignored_domains.iter().any(|d| host.contains(d))
            {
                tracing::debug!(host = %host, "Host URL in domini ignorati");
                return None;
            }

            let mut provider_name = String::from("Custom/Other");
            let github_changed = github::clean_github_url(&mut url);
            if github_changed {
                provider_name = "GitHub (Repo Root)".to_string();
            }

            // 1. Apply Custom User Rules FIRST
            let mut custom_changed = false;
            if let Some(_query) = url.query() {
                let query_pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
                let mut new_query = url::form_urlencoded::Serializer::new(String::new());
                let mut any_kept = false;

                for (key, value) in query_pairs {
                    let mut keep = true;
                    for crule in custom_rules {
                        if key.contains(&crule.pattern) {
                            keep = false;
                            custom_changed = true;
                            tracing::debug!(param = %key, rule = %crule.pattern, "Regola personalizzata trovata");
                            break;
                        }
                    }
                    if keep {
                        new_query.append_pair(&key, &value);
                        any_kept = true;
                    }
                }

                if custom_changed {
                    if any_kept {
                        url.set_query(Some(&new_query.finish()));
                    } else {
                        url.set_query(None);
                    }
                }
            }

            // 2. Identify Provider
            {
                if let Ok(providers) = self.providers.read() {
                    for p in providers.iter() {
                        if p.url_pattern.is_match(text) {
                            provider_name = p.name.clone();
                            tracing::debug!(provider = %provider_name, "Provider identificato");
                            break;
                        }
                    }
                }
            }

            // 3. Apply Extended Algorithm
            let mut changed = self.clean_url_in_place(&mut url);

            // 4. Aggressive Fallback for common trackers not in the ruleset
            if let Some(_query) = url.query() {
                let query_pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
                let mut new_query = url::form_urlencoded::Serializer::new(String::new());
                let mut aggressive_changed = false;
                let mut any_kept = false;

                for (key, value) in query_pairs {
                    if AGGRESSIVE_TRACKERS.contains(&key.as_str()) {
                        aggressive_changed = true;
                        tracing::debug!(param = %key, "Tracker rimosso (aggressivo)");
                        continue;
                    }
                    new_query.append_pair(&key, &value);
                    any_kept = true;
                }

                if aggressive_changed {
                    changed = true;
                    if any_kept {
                        url.set_query(Some(&new_query.finish()));
                    } else {
                        url.set_query(None);
                    }
                }
            }

            if changed || custom_changed || github_changed {
                let cleaned = url.to_string();
                tracing::info!(
                    original = %self.redact_sensitive(text),
                    cleaned = %cleaned,
                    provider = %provider_name,
                    "URL pulito con successo"
                );
                return Some((cleaned, provider_name));
            }
        }
        None
    }

    #[tracing::instrument(skip(self, url))]
    pub fn clean_url_in_place(&self, url: &mut Url) -> bool {
        let mut changed = false;
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 5;

        while iterations < MAX_ITERATIONS {
            let url_str = url.to_string();
            let mut current_iteration_changed = false;

            if let Ok(providers) = self.providers.read() {
                for provider in providers.iter() {
                    if provider.url_pattern.is_match(&url_str) || provider.name == "generic" {
                        let mut provider_changed = false;

                        // Exceptions check
                        let mut is_exception = false;
                        for exception in &provider.exceptions {
                            if exception.is_match(&url_str) {
                                is_exception = true;
                                break;
                            }
                        }
                        if is_exception {
                            continue;
                        }

                        // Handle redirections
                        for redirection_regex in &provider.redirections {
                            if let Some(caps) = redirection_regex.captures(&url_str)
                                && let Some(m) = caps.get(1)
                                && let Ok(new_url) = Url::parse(m.as_str())
                            {
                                *url = new_url;
                                current_iteration_changed = true;
                                provider_changed = true;
                                changed = true;
                                break;
                            }
                        }

                        if provider_changed {
                            continue;
                        }

                        // Handle Query Parameters (with affiliate preservation)
                        if let Some(_query) = url.query() {
                            let query_pairs: Vec<(String, String)> =
                                url.query_pairs().into_owned().collect();
                            let mut new_query =
                                url::form_urlencoded::Serializer::new(String::new());
                            let mut params_removed = false;
                            let mut any_kept = false;

                            for (key, mut value) in query_pairs {
                                let mut keep = true;

                                for rule in &provider.rules {
                                    if rule.is_match(&key) {
                                        keep = false;
                                        break;
                                    }
                                }
                                if keep {
                                    for rule in &provider.referral_marketing {
                                        if rule.is_match(&key) {
                                            keep = false;
                                            break;
                                        }
                                    }
                                }

                                if keep {
                                    if value.starts_with("http")
                                        && let Ok(mut inner_url) = Url::parse(&value)
                                        && self.clean_url_in_place(&mut inner_url)
                                    {
                                        value = inner_url.to_string();
                                        changed = true;
                                    }
                                    new_query.append_pair(&key, &value);
                                    any_kept = true;
                                } else {
                                    params_removed = true;
                                }
                            }

                            if params_removed {
                                changed = true;
                                current_iteration_changed = true;
                                if any_kept {
                                    url.set_query(Some(&new_query.finish()));
                                } else {
                                    url.set_query(None);
                                }
                            }
                        }

                        // Handle Fragment (hash)
                        if let Some(fragment) = url.fragment()
                            && fragment.contains('=')
                        {
                            let frag_url_str = format!("http://localhost?{}", fragment);
                            if let Ok(mut frag_url) = Url::parse(&frag_url_str)
                                && self.clean_url_in_place(&mut frag_url)
                            {
                                if let Some(new_frag) = frag_url.query() {
                                    url.set_fragment(Some(new_frag));
                                } else {
                                    url.set_fragment(None);
                                }
                                changed = true;
                                current_iteration_changed = true;
                            }
                        }

                        // Raw rules
                        let mut intermediate_url_str = url.to_string();
                        let mut raw_changed = false;
                        for raw in &provider.raw_rules {
                            let new_str = raw.replace_all(&intermediate_url_str, "");
                            if new_str != intermediate_url_str {
                                intermediate_url_str = new_str.to_string();
                                raw_changed = true;
                            }
                        }

                        if raw_changed && let Ok(new_url) = Url::parse(&intermediate_url_str) {
                            *url = new_url;
                            changed = true;
                            current_iteration_changed = true;
                        }
                    }
                }
            }

            if !current_iteration_changed {
                break;
            }
            iterations += 1;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[tokio::test]
    async fn test_simple_cleaning() {
        let engine = RuleEngine::new_lazy("");

        {
            let mut w = match engine.providers.write() {
                Ok(w) => w,
                Err(poisoned) => {
                    tracing::error!("RuleEngine providers RwLock poisoned, recovering");
                    poisoned.into_inner()
                },
            };
            w.push(CompiledProvider {
                name: "generic".to_string(),
                url_pattern: Regex::new(".*").unwrap(),
                rules: vec![Regex::new("utm_.*").unwrap()],
                exceptions: vec![],
                raw_rules: vec![],
                redirections: vec![],
                referral_marketing: vec![],
                _force_redirection: false,
            });
        }

        let input = "https://example.com/?utm_source=test&foo=bar";
        let (cleaned, _) = engine.sanitize(input, &[], &[]).unwrap();
        assert_eq!(cleaned, "https://example.com/?foo=bar");
    }

    #[tokio::test]
    async fn test_redaction() {
        let engine = RuleEngine::new_lazy("");
        let input = "My email is test@example.com and my IP is 1.2.3.4";
        let redacted = engine.redact_sensitive(input);
        assert!(redacted.contains("[REDACTED EMAIL]"));
        assert!(redacted.contains("[REDACTED IPV4]"));
        assert!(!redacted.contains("test@example.com"));
    }

    #[tokio::test]
    async fn test_github_cleaning() {
        let engine = RuleEngine::new_lazy("");
        let input = "https://github.com/owner/repo/blob/main/README.md?foo=bar#L10";
        let (cleaned, provider) = engine.sanitize(input, &[], &[]).unwrap();
        assert_eq!(cleaned, "https://github.com/owner/repo");
        assert_eq!(provider, "GitHub (Repo Root)");
    }
}
