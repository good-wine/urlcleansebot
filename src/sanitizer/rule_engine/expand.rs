use crate::constants::{SHORTENER_DOMAINS, SHORTENER_REGEX_PATTERNS};
use crate::http_utils::retry_with_backoff;
use crate::sanitizer::rule_engine::ssrf;
use moka::future::Cache;
use regex::Regex;
use tracing;
use url::Url;

pub async fn expand_url(
    input_url: &str,
    cache: &Cache<String, String>,
) -> String {
    if let Some(cached) = cache.get(input_url).await {
        tracing::debug!(url = %input_url, "Cache hit per espansione URL");
        return cached;
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return input_url.to_string(),
    };

    let url_lower = input_url.to_lowercase();
    let is_shortener = SHORTENER_DOMAINS.iter().any(|s| url_lower.contains(s))
        || SHORTENER_REGEX_PATTERNS
            .iter()
            .any(|p| Regex::new(p).is_ok_and(|re| re.is_match(input_url)));

    if is_shortener {
        tracing::debug!(url = %input_url, "Tentativo di espansione URL corto");
        let resp = retry_with_backoff(
            || async { client.head(input_url).send().await.map_err(|e| e.to_string()) },
            "expand short URL",
        )
        .await;
        if let Ok(resp) = resp {
            let final_url = resp.url().to_string();
            if final_url != input_url {
                if let Ok(parsed) = Url::parse(&final_url)
                    && let Some(host) = parsed.host_str()
                    && !ssrf::resolve_and_check_ssrf(host).await
                {
                    tracing::warn!(
                        url = %final_url,
                        "SSRF blocked: redirect to private/reserved IP"
                    );
                    cache
                        .insert(input_url.to_string(), input_url.to_string())
                        .await;
                    return input_url.to_string();
                }
                tracing::info!(original = %input_url, expanded = %final_url, "URL espanso con successo");
                cache
                    .insert(input_url.to_string(), final_url.clone())
                    .await;
                return final_url;
            }
        }
    }

    if is_shortener {
        cache
            .insert(input_url.to_string(), input_url.to_string())
            .await;
    }

    input_url.to_string()
}
