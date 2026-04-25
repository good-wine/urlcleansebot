//! High-level redirect service: fetches, caches and queries the LibRedirect
//! and Farside catalogues.
//!
//! The service is intentionally side-effect-free at construction time: the
//! first call to [`RedirectService::lookup`] (or its more specific siblings)
//! triggers the lazy fetch of upstream data, which is then memoized in the
//! TTL cache.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use url::Url;

use super::cache::{SingleEntryCache, DEFAULT_TTL};
use super::models::{FarsideService, Frontend, FrontendSource, LibRedirectDoc, LookupHit};
use crate::http_utils::retry_http_request;

pub const LIBREDIRECT_URL: &str =
    "https://raw.githubusercontent.com/libredirect/instances/main/data.json";
pub const FARSIDE_URL: &str =
    "https://raw.githubusercontent.com/benbusby/farside/refs/heads/main/services-full.json";

/// HTTP timeout for upstream catalogue fetches. Both endpoints are static
/// files on `raw.githubusercontent.com`; 10s is generous.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Service that resolves a domain to alternative privacy-friendly frontends.
///
/// Cheap to clone (`Arc` internally). Inject as a Dispatcher dependency.
#[derive(Clone)]
pub struct RedirectService {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for RedirectService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedirectService")
            .field("libredirect_url", &self.inner.libredirect_url)
            .field("farside_url", &self.inner.farside_url)
            .finish()
    }
}

struct Inner {
    http: Client,
    libredirect: SingleEntryCache<LibRedirectDoc>,
    farside: SingleEntryCache<Vec<FarsideService>>,
    libredirect_url: String,
    farside_url: String,
}

impl RedirectService {
    /// Build a service with the production upstream URLs and the default TTL.
    pub fn new() -> Result<Self> {
        Self::with_urls(LIBREDIRECT_URL, FARSIDE_URL, DEFAULT_TTL)
    }

    /// Build a service with URLs from configuration.
    pub fn from_config(libredirect_url: &str, farside_url: &str) -> Result<Self> {
        Self::with_urls(libredirect_url, farside_url, DEFAULT_TTL)
    }

    /// Build a service overriding upstream URLs and TTL — used in tests
    /// against a local mock server.
    pub fn with_urls(libredirect_url: &str, farside_url: &str, ttl: Duration) -> Result<Self> {
        let http = Client::builder()
            .timeout(FETCH_TIMEOUT)
            .user_agent(concat!("clear_urls_bot/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("build redirect HTTP client")?;
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                libredirect: SingleEntryCache::new(ttl),
                farside: SingleEntryCache::new(ttl),
                libredirect_url: libredirect_url.to_string(),
                farside_url: farside_url.to_string(),
            }),
        })
    }

    /// Force-refresh both upstream caches on the next lookup.
    pub async fn invalidate(&self) {
        self.inner.libredirect.invalidate().await;
        self.inner.farside.invalidate().await;
    }

    /// Lookup alternative frontends for the given user-provided URL.
    ///
    /// Returns `Ok(None)` when no service matches the URL's host. Returns
    /// `Err` only on infrastructure failure (network down, malformed JSON);
    /// callers are expected to surface a friendly fallback message in that
    /// case.
    pub async fn lookup(&self, raw_url: &str) -> Result<Option<LookupHit>> {
        let host = extract_host(raw_url).context("parse input URL")?;
        self.lookup_by_host(&host).await
    }

    /// Lookup frontends for a bare hostname (no scheme).
    pub async fn lookup_by_host(&self, host: &str) -> Result<Option<LookupHit>> {
        let host = host.trim_start_matches("www.").to_ascii_lowercase();
        let lib = self.fetch_libredirect().await?;
        let far = self.fetch_farside().await?;

        let (matched_service, mut frontends) = match find_libredirect_match(&lib, &host) {
            Some(hit) => hit,
            None => return Ok(None),
        };

        // Append Farside instances whose `type` matches one of the LibRedirect
        // frontend kinds we already collected (e.g. `invidious`, `nitter`).
        let known_kinds: Vec<String> = frontends.iter().map(|f| f.kind.clone()).collect();
        for fs in far.iter() {
            if known_kinds.iter().any(|k| k == &fs.kind) {
                for url in &fs.instances {
                    frontends.push(Frontend {
                        service: matched_service.clone(),
                        kind: fs.kind.clone(),
                        url: url.clone(),
                        source: FrontendSource::Farside,
                    });
                }
            }
        }

        Ok(Some(LookupHit {
            service: matched_service,
            frontends,
        }))
    }

    async fn fetch_libredirect(&self) -> Result<Arc<LibRedirectDoc>> {
        let http = self.inner.http.clone();
        let url = self.inner.libredirect_url.clone();
        self.inner
            .libredirect
            .get_or_try_insert_with(|| async move {
                let resp = retry_http_request(
                    || http.get(&url),
                    "fetch libredirect catalogue"
                ).await.context("GET libredirect")?;
                let status = resp.status();
                if !status.is_success() {
                    return Err(anyhow!("libredirect HTTP {status}"));
                }
                let body = resp.text().await.context("read libredirect body")?;
                serde_json::from_str::<LibRedirectDoc>(&body).context("parse libredirect JSON")
            })
            .await
    }

    async fn fetch_farside(&self) -> Result<Arc<Vec<FarsideService>>> {
        let http = self.inner.http.clone();
        let url = self.inner.farside_url.clone();
        self.inner
            .farside
            .get_or_try_insert_with(|| async move {
                let resp = retry_http_request(
                    || http.get(&url),
                    "fetch farside catalogue"
                ).await.context("GET farside")?;
                let status = resp.status();
                if !status.is_success() {
                    return Err(anyhow!("farside HTTP {status}"));
                }
                let body = resp.text().await.context("read farside body")?;
                serde_json::from_str::<Vec<FarsideService>>(&body).context("parse farside JSON")
            })
            .await
    }
}

/// Extract the lowercased host (without `www.`) from a URL or bare host.
///
/// Accepts both `https://youtube.com/watch?v=...` and `youtube.com/watch?...`.
pub fn extract_host(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("empty URL"));
    }
    // url::Url requires a scheme; prepend one if missing.
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let parsed = Url::parse(&with_scheme).context("invalid URL")?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("URL has no host"))?
        .trim_end_matches('.')
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    Ok(host)
}

/// Walk the LibRedirect catalogue and find the service whose `targets` cover
/// the given host. Returns the service slug and the normalised `Frontend`s.
pub(crate) fn find_libredirect_match(
    doc: &LibRedirectDoc,
    host: &str,
) -> Option<(String, Vec<Frontend>)> {
    for (service, def) in doc.iter() {
        let matched = def.targets.iter().any(|t| {
            let target = t.to_ascii_lowercase();
            host == target || host.ends_with(&format!(".{target}"))
        });
        if !matched {
            continue;
        }
        let mut frontends = Vec::new();
        for (kind, set) in def.instances.iter() {
            for url in &set.clearnet {
                frontends.push(Frontend {
                    service: service.clone(),
                    kind: kind.clone(),
                    url: url.clone(),
                    source: FrontendSource::LibRedirect,
                });
            }
        }
        return Some((service.clone(), frontends));
    }
    None
}

/// Format a [`LookupHit`] as a Telegram-friendly HTML message.
///
/// Caps the number of instances per frontend kind to keep messages small.
pub fn format_hit_html(hit: &LookupHit, instances_per_kind: usize) -> String {
    use std::collections::BTreeMap;

    let mut by_kind: BTreeMap<&str, Vec<&Frontend>> = BTreeMap::new();
    for f in &hit.frontends {
        by_kind.entry(f.kind.as_str()).or_default().push(f);
    }

    let mut out = format!(
        "🔁 <b>Frontend alternativi per</b> <code>{}</code>\n",
        teloxide_html_escape(&hit.service)
    );
    for (kind, mut entries) in by_kind {
        entries.sort_by(|a, b| a.url.cmp(&b.url));
        entries.dedup_by(|a, b| a.url == b.url);
        out.push_str(&format!("\n<b>• {}</b>\n", teloxide_html_escape(kind)));
        for f in entries.iter().take(instances_per_kind) {
            out.push_str(&format!(
                "  ↳ <a href=\"{}\">{}</a>  <i>({})</i>\n",
                teloxide_html_escape(&f.url),
                teloxide_html_escape(&f.url),
                f.source.as_str()
            ));
        }
    }
    out
}

fn teloxide_html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redirects::models::{
        FarsideService, LibRedirectDoc, LibRedirectInstanceSet, LibRedirectService,
    };
    use std::collections::BTreeMap;

    fn sample_libredirect() -> LibRedirectDoc {
        let mut youtube_instances = BTreeMap::new();
        youtube_instances.insert(
            "invidious".to_string(),
            LibRedirectInstanceSet {
                clearnet: vec!["https://yewtu.be".into(), "https://invidious.io".into()],
            },
        );
        youtube_instances.insert(
            "piped".to_string(),
            LibRedirectInstanceSet {
                clearnet: vec!["https://piped.video".into()],
            },
        );
        let mut doc = BTreeMap::new();
        doc.insert(
            "youtube".to_string(),
            LibRedirectService {
                instances: youtube_instances,
                targets: vec!["youtube.com".into(), "youtu.be".into()],
            },
        );
        doc
    }

    #[test]
    fn extract_host_with_scheme() {
        assert_eq!(
            extract_host("https://www.youtube.com/x").unwrap(),
            "youtube.com"
        );
    }

    #[test]
    fn extract_host_without_scheme() {
        assert_eq!(
            extract_host("youtube.com/watch?v=abc").unwrap(),
            "youtube.com"
        );
    }

    #[test]
    fn extract_host_strips_www() {
        assert_eq!(
            extract_host("https://www.twitter.com").unwrap(),
            "twitter.com"
        );
    }

    #[test]
    fn extract_host_rejects_empty() {
        assert!(extract_host("   ").is_err());
    }

    #[test]
    fn extract_host_rejects_non_url() {
        assert!(extract_host("not a url with spaces!").is_err());
    }

    #[test]
    fn extract_host_trims_trailing_dot() {
        assert_eq!(extract_host("https://www.youtube.com./watch").unwrap(), "youtube.com");
    }

    #[test]
    fn libredirect_match_exact() {
        let doc = sample_libredirect();
        let (svc, frontends) = find_libredirect_match(&doc, "youtube.com").unwrap();
        assert_eq!(svc, "youtube");
        // 2 invidious + 1 piped
        assert_eq!(frontends.len(), 3);
        assert!(frontends
            .iter()
            .all(|f| f.source == FrontendSource::LibRedirect));
    }

    #[test]
    fn libredirect_match_subdomain() {
        let doc = sample_libredirect();
        let (svc, _) = find_libredirect_match(&doc, "music.youtube.com").unwrap();
        assert_eq!(svc, "youtube");
    }

    #[test]
    fn libredirect_match_alias_target() {
        let doc = sample_libredirect();
        let (svc, _) = find_libredirect_match(&doc, "youtu.be").unwrap();
        assert_eq!(svc, "youtube");
    }

    #[test]
    fn libredirect_no_match() {
        let doc = sample_libredirect();
        assert!(find_libredirect_match(&doc, "example.org").is_none());
    }

    #[test]
    fn format_hit_html_includes_service_and_kind() {
        let hit = LookupHit {
            service: "youtube".into(),
            frontends: vec![Frontend {
                service: "youtube".into(),
                kind: "invidious".into(),
                url: "https://yewtu.be".into(),
                source: FrontendSource::LibRedirect,
            }],
        };
        let out = format_hit_html(&hit, 5);
        assert!(out.contains("youtube"));
        assert!(out.contains("invidious"));
        assert!(out.contains("yewtu.be"));
        assert!(out.contains("LibRedirect"));
    }

    #[test]
    fn format_hit_html_caps_instances_per_kind() {
        let mut frontends = Vec::new();
        for i in 0..10 {
            frontends.push(Frontend {
                service: "x".into(),
                kind: "nitter".into(),
                url: format!("https://nitter{i}.example"),
                source: FrontendSource::Farside,
            });
        }
        let hit = LookupHit {
            service: "x".into(),
            frontends,
        };
        let out = format_hit_html(&hit, 3);
        let count = out.matches("nitter").count();
        // 1 occurrence in the kind header + 3 in URLs * 2 (href + text) = 7
        assert!(count <= 9, "unexpected count: {count}\n{out}");
        assert!(out.contains("nitter0.example"));
        assert!(!out.contains("nitter9.example"));
    }

    #[tokio::test]
    async fn lookup_returns_none_on_unknown_host() {
        // Drive the full pipeline against a stub by pre-filling the cache.
        let svc = RedirectService::with_urls(
            "http://x.invalid",
            "http://x.invalid",
            Duration::from_secs(60),
        )
        .unwrap();
        // Pre-populate caches so we don't actually hit the network.
        svc.inner
            .libredirect
            .get_or_try_insert_with::<_, _, anyhow::Error>(|| async { Ok(sample_libredirect()) })
            .await
            .unwrap();
        svc.inner
            .farside
            .get_or_try_insert_with::<_, _, anyhow::Error>(|| async {
                Ok(Vec::<FarsideService>::new())
            })
            .await
            .unwrap();

        let hit = svc.lookup("https://example.org/path").await.unwrap();
        assert!(hit.is_none());
    }

    #[tokio::test]
    async fn lookup_merges_farside_instances() {
        let svc = RedirectService::with_urls(
            "http://x.invalid",
            "http://x.invalid",
            Duration::from_secs(60),
        )
        .unwrap();
        svc.inner
            .libredirect
            .get_or_try_insert_with::<_, _, anyhow::Error>(|| async { Ok(sample_libredirect()) })
            .await
            .unwrap();
        let farside = vec![FarsideService {
            kind: "invidious".into(),
            instances: vec!["https://farside-inv.example".into()],
        }];
        svc.inner
            .farside
            .get_or_try_insert_with::<_, _, anyhow::Error>(|| async { Ok(farside) })
            .await
            .unwrap();

        let hit = svc
            .lookup("https://youtube.com/watch?v=1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(hit.service, "youtube");
        assert!(
            hit.frontends
                .iter()
                .any(|f| f.url == "https://farside-inv.example"
                    && f.source == FrontendSource::Farside)
        );
    }
}
