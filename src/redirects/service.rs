//! High-level redirect service: fetches, caches and queries the LibRedirect
//! and Farside catalogues.
//!
//! The service is intentionally side-effect-free at construction time: the
//! first call to [`RedirectService::lookup`] (or its more specific siblings)
//! triggers the lazy fetch of upstream data, which is then memoized in the
//! TTL cache.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use url::Url;

use super::cache::{SingleEntryCache, DEFAULT_TTL};
use super::models::{Frontend, FrontendSource, LibRedirectDoc, LookupHit};
use crate::http_utils::retry_http_request;

pub const LIBREDIRECT_URL: &str =
    "https://raw.githubusercontent.com/libredirect/instances/main/data.json";
pub const FARSIDE_URL: &str =
    "https://raw.githubusercontent.com/benbusby/farside/refs/heads/main/services-full.json";

/// HTTP timeout for upstream catalogue fetches. Both endpoints are static
/// files on `raw.githubusercontent.com`; 10s is generous.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Mapping from target domains to the frontend kinds that replace them.
/// Since the new LibRedirect format no longer ships `targets`, we maintain
/// this mapping locally.
fn domain_to_kinds() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        (
            "youtube.com",
            &[
                "invidious",
                "piped",
                "pipedMaterial",
                "cloudtube",
                "materialious",
                "poke",
                "hyperpipe",
            ],
        ),
        (
            "youtu.be",
            &[
                "invidious",
                "piped",
                "pipedMaterial",
                "cloudtube",
                "materialious",
                "poke",
                "hyperpipe",
            ],
        ),
        ("music.youtube.com", &["hyperpipe", "invidious", "piped"]),
        ("twitter.com", &["nitter", "twineo"]),
        ("x.com", &["nitter", "twineo"]),
        ("mobile.twitter.com", &["nitter", "twineo"]),
        ("reddit.com", &["redlib", "libreddit", "teddit"]),
        ("old.reddit.com", &["redlib", "libreddit", "teddit"]),
        ("new.reddit.com", &["redlib", "libreddit", "teddit"]),
        ("www.reddit.com", &["redlib", "libreddit", "teddit"]),
        ("instagram.com", &["proxigram", "kittygram"]),
        ("www.instagram.com", &["proxigram", "kittygram"]),
        ("tiktok.com", &["proxiTok"]),
        ("www.tiktok.com", &["proxiTok"]),
        ("vm.tiktok.com", &["proxiTok"]),
        ("medium.com", &["scribe", "libMedium"]),
        ("*.medium.com", &["scribe", "libMedium"]),
        ("stackoverflow.com", &["anonymousOverflow"]),
        ("stackexchange.com", &["anonymousOverflow"]),
        ("*.stackexchange.com", &["anonymousOverflow"]),
        ("serverfault.com", &["anonymousOverflow"]),
        ("superuser.com", &["anonymousOverflow"]),
        ("askubuntu.com", &["anonymousOverflow"]),
        ("en.wikipedia.org", &["wikiless"]),
        ("*.wikipedia.org", &["wikiless"]),
        ("quora.com", &["quetre"]),
        ("www.quora.com", &["quetre"]),
        ("imdb.com", &["libremdb"]),
        ("www.imdb.com", &["libremdb"]),
        ("fandom.com", &["breezeWiki"]),
        ("*.fandom.com", &["breezeWiki"]),
        ("twitch.tv", &["safetwitch"]),
        ("www.twitch.tv", &["safetwitch"]),
        ("pixiv.net", &["pixivFe", "liteXiv"]),
        ("www.pixiv.net", &["pixivFe", "liteXiv"]),
        ("openstreetmap.org", &["osm"]),
        ("www.openstreetmap.org", &["osm"]),
        ("osm.org", &["osm"]),
        ("www.osm.org", &["osm"]),
        ("bandcamp.com", &["tent"]),
        ("*.bandcamp.com", &["tent"]),
        ("github.com", &["gothub"]),
        ("tumblr.com", &["priviblur"]),
        ("*.tumblr.com", &["priviblur"]),
        ("imgur.com", &["rimgo"]),
        ("i.imgur.com", &["rimgo"]),
        ("genius.com", &["dumb"]),
        (
            "translate.google.com",
            &[
                "simplyTranslate",
                "lingva",
                "mozhi",
                "libreTranslate",
                "transLite",
            ],
        ),
        ("deezer.com", &["simplyTranslate"]),
        ("pastebin.com", &["privateBin"]),
        ("reuters.com", &["neuters"]),
        ("www.reuters.com", &["neuters"]),
        ("urbandictionary.com", &["ruralDictionary"]),
        ("www.urbandictionary.com", &["ruralDictionary"]),
        ("odysee.com", &["librarian"]),
        ("lbry.tv", &["librarian"]),
        ("deviantart.com", &["skunkyArt"]),
        ("tenor.com", &["mezzo"]),
        ("pinterest.com", &["koub"]),
        ("www.pinterest.com", &["koub"]),
        ("soundcloud.com", &["soundcloak"]),
        ("vimeo.com", &["libremdb"]),
        ("bilibili.com", &["vixipy"]),
    ]
}

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
    farside: SingleEntryCache<Vec<super::models::FarsideService>>,
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
        let known_kinds: HashSet<String> = frontends.iter().map(|f| f.kind.clone()).collect();
        for fs in far.iter() {
            if known_kinds.contains(&fs.kind) {
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
                let resp = retry_http_request(|| http.get(&url), "fetch libredirect catalogue")
                    .await
                    .context("GET libredirect")?;
                let status = resp.status();
                if !status.is_success() {
                    return Err(anyhow!("libredirect HTTP {status}"));
                }
                let body = resp.text().await.context("read libredirect body")?;
                serde_json::from_str::<LibRedirectDoc>(&body).context("parse libredirect JSON")
            })
            .await
    }

    async fn fetch_farside(&self) -> Result<Arc<Vec<super::models::FarsideService>>> {
        let http = self.inner.http.clone();
        let url = self.inner.farside_url.clone();
        self.inner
            .farside
            .get_or_try_insert_with(|| async move {
                let resp = retry_http_request(|| http.get(&url), "fetch farside catalogue")
                    .await
                    .context("GET farside")?;
                let status = resp.status();
                if !status.is_success() {
                    return Err(anyhow!("farside HTTP {status}"));
                }
                let body = resp.text().await.context("read farside body")?;
                serde_json::from_str::<Vec<super::models::FarsideService>>(&body)
                    .context("parse farside JSON")
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

/// Walk the LibRedirect catalogue and find frontends for the given host
/// using the local domain-to-kinds mapping.
pub(crate) fn find_libredirect_match(
    doc: &LibRedirectDoc,
    host: &str,
) -> Option<(String, Vec<Frontend>)> {
    // Find which frontend kinds match this host.
    let matching_kinds: Vec<&str> = domain_to_kinds()
        .iter()
        .filter_map(|(pattern, kinds)| {
            if host_matches_pattern(host, pattern) {
                Some(*kinds)
            } else {
                None
            }
        })
        .flat_map(|kinds| kinds.iter().copied())
        .collect();

    if matching_kinds.is_empty() {
        return None;
    }

    let mut frontends = Vec::new();
    // Use the first matching kind's name as the service slug for display.
    let service_slug = matching_kinds.first().unwrap_or(&"unknown").to_string();

    for kind in matching_kinds {
        if let Some(entry) = doc.get(kind) {
            for url in &entry.clearnet {
                frontends.push(Frontend {
                    service: service_slug.clone(),
                    kind: kind.to_string(),
                    url: url.clone(),
                    source: FrontendSource::LibRedirect,
                });
            }
        }
    }

    if frontends.is_empty() {
        return None;
    }

    Some((service_slug, frontends))
}

/// Check if a host matches a pattern (supports `*.` prefix wildcards).
fn host_matches_pattern(host: &str, pattern: &str) -> bool {
    if pattern == host {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }
    false
}

/// Extract YouTube video ID from various URL formats.
fn extract_youtube_video_id(url: &Url) -> Option<String> {
    let host = url.host_str().unwrap_or("");

    // youtu.be short URL: path is the video ID
    if host == "youtu.be" || host.ends_with(".youtu.be") {
        let id = url.path().trim_start_matches('/').trim_end_matches('/');
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    // /watch?v=VIDEO_ID
    if url.path() == "/watch" {
        if let Some(v) = url
            .query_pairs()
            .find(|(k, _)| k == "v")
            .map(|(_, v)| v.to_string())
        {
            return Some(v);
        }
    }

    // /shorts/VIDEO_ID
    if let Some(id) = url.path().strip_prefix("/shorts/") {
        let id = id.trim_end_matches('/');
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    // /embed/VIDEO_ID
    if let Some(id) = url.path().strip_prefix("/embed/") {
        let id = id.trim_end_matches('/');
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    // /v/VIDEO_ID (old format)
    if let Some(id) = url.path().strip_prefix("/v/") {
        let id = id.trim_end_matches('/');
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    None
}

/// Extract TikTok video identifier (@user/video/ID or just ID).
fn extract_tiktok_info(url: &Url) -> Option<String> {
    let path = url.path();
    // /@user/video/ID or /video/ID
    if let Some(rest) = path.strip_prefix("/@") {
        // /@user/video/ID
        if let Some(video_part) = rest.split("/video/").nth(1) {
            return Some(video_part.trim_end_matches('/').to_string());
        }
    }
    if let Some(id) = path.strip_prefix("/video/") {
        return Some(id.trim_end_matches('/').to_string());
    }
    None
}

/// Extract Twitter/X tweet info (username + status ID).
fn extract_twitter_info(url: &Url) -> Option<(String, String)> {
    let parts: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() >= 3 && parts[1] != "i" && parts[1] != "explore" {
        // /username/status/ID
        let username = parts[0];
        let status_id = parts[2];
        Some((username.to_string(), status_id.to_string()))
    } else {
        None
    }
}

/// Extract Reddit post info (subreddit + post ID).
fn extract_reddit_info(url: &Url) -> Option<(String, String)> {
    let path = url.path();
    // /r/SUBREDDIT/comments/POST_ID/...
    if let Some(rest) = path.strip_prefix("/r/") {
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() >= 3 && parts[1] == "comments" {
            return Some((parts[0].to_string(), parts[2].to_string()));
        }
    }
    None
}

/// Build a full frontend URL by combining the frontend base URL with the
/// content extracted from the original user URL, using the correct format
/// for each specific frontend kind.
pub fn build_frontend_url(frontend_base: &str, original_url: &str, kind: &str) -> String {
    let Ok(parsed) =
        Url::parse(original_url).or_else(|_| Url::parse(&format!("https://{original_url}")))
    else {
        return frontend_base.to_string();
    };

    match kind {
        // ─── YouTube video frontends ───
        "invidious" => {
            if let Some(video_id) = extract_youtube_video_id(&parsed) {
                return format!("{frontend_base}/watch?v={video_id}");
            }
            // Fallback: preserve path
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }
        "piped" => {
            if let Some(video_id) = extract_youtube_video_id(&parsed) {
                return format!("{frontend_base}/watch?v={video_id}");
            }
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }
        "pipedMaterial" => {
            if let Some(video_id) = extract_youtube_video_id(&parsed) {
                return format!("{frontend_base}/watch?v={video_id}");
            }
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }
        "cloudtube" => {
            if let Some(video_id) = extract_youtube_video_id(&parsed) {
                return format!("{frontend_base}/video/{video_id}");
            }
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }
        "materialious" => {
            if let Some(video_id) = extract_youtube_video_id(&parsed) {
                return format!("{frontend_base}/watch/{video_id}");
            }
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }
        "hyperpipe" => {
            if let Some(video_id) = extract_youtube_video_id(&parsed) {
                return format!("{frontend_base}/watch?v={video_id}");
            }
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }
        "suds" => {
            if let Some(video_id) = extract_youtube_video_id(&parsed) {
                return format!("{frontend_base}/watch/{video_id}");
            }
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }
        "poke" => {
            if let Some(video_id) = extract_youtube_video_id(&parsed) {
                return format!("{frontend_base}/watch?v={video_id}");
            }
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }
        "vixipy" => {
            if let Some(video_id) = extract_youtube_video_id(&parsed) {
                return format!("{frontend_base}/watch/{video_id}");
            }
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Twitter/X frontends ───
        "nitter" | "twineo" => {
            if let Some((username, status_id)) = extract_twitter_info(&parsed) {
                return format!("{frontend_base}/{username}/status/{status_id}");
            }
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Reddit frontends ───
        "redlib" | "libreddit" | "teddit" => {
            if let Some((subreddit, post_id)) = extract_reddit_info(&parsed) {
                return format!("{frontend_base}/r/{subreddit}/comments/{post_id}");
            }
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── TikTok frontends ───
        "proxiTok" => {
            if let Some(video_info) = extract_tiktok_info(&parsed) {
                return format!("{frontend_base}/video/{video_info}");
            }
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Medium frontends ───
        "scribe" | "libMedium" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Wikipedia frontends ───
        "wikiless" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Fandom frontend ───
        "breezeWiki" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Quora frontend ───
        "quetre" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── IMDb frontend ───
        "libremdb" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Twitch frontend ───
        "safetwitch" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Imgur frontend ───
        "rimgo" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Pixiv frontends ───
        "pixivFe" | "liteXiv" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Bandcamp frontend ───
        "tent" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Tumblr frontend ───
        "priviblur" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── GitHub frontend ───
        "gothub" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── StackOverflow frontend ───
        "anonymousOverflow" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Genius frontend ───
        "dumb" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Translate frontends ───
        "simplyTranslate" | "lingva" | "mozhi" | "libreTranslate" | "transLite" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── SoundCloud frontend ───
        "soundcloak" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Tenor frontend ───
        "mezzo" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Instagram frontends ───
        "proxigram" | "kittygram" => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }

        // ─── Generic fallback: preserve path and query ───
        _ => {
            format!(
                "{}{}",
                frontend_base.trim_end_matches('/'),
                build_path_and_query(&parsed)
            )
        }
    }
}

fn build_path_and_query(url: &Url) -> String {
    let path = url.path();
    let query = url.query().map(|q| format!("?{q}")).unwrap_or_default();
    format!("{path}{query}")
}

/// Format a [`LookupHit`] as a Telegram-friendly HTML message.
///
/// Caps the number of instances per frontend kind to keep messages small.
/// `original_url` is used to construct full redirect links with the correct
/// format for each frontend kind.
pub fn format_hit_html(hit: &LookupHit, instances_per_kind: usize, original_url: &str) -> String {
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
            let full_url = build_frontend_url(&f.url, original_url, kind);
            out.push_str(&format!(
                "  ↳ <a href=\"{}\">{}</a>  <i>({})</i>\n",
                teloxide_html_escape(&full_url),
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
    use crate::redirects::models::{FarsideService, LibRedirectFrontend};
    use std::collections::BTreeMap;

    fn sample_libredirect() -> LibRedirectDoc {
        let mut doc = BTreeMap::new();
        doc.insert(
            "invidious".to_string(),
            LibRedirectFrontend {
                clearnet: vec!["https://yewtu.be".into(), "https://invidious.io".into()],
            },
        );
        doc.insert(
            "piped".to_string(),
            LibRedirectFrontend {
                clearnet: vec!["https://piped.video".into()],
            },
        );
        doc.insert(
            "nitter".to_string(),
            LibRedirectFrontend {
                clearnet: vec!["https://nitter.net".into()],
            },
        );
        doc.insert(
            "cloudtube".to_string(),
            LibRedirectFrontend {
                clearnet: vec!["https://tube.cadence.moe".into()],
            },
        );
        doc.insert(
            "materialious".to_string(),
            LibRedirectFrontend {
                clearnet: vec!["https://materialious.example".into()],
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
        assert_eq!(
            extract_host("https://www.youtube.com./watch").unwrap(),
            "youtube.com"
        );
    }

    #[test]
    fn libredirect_match_youtube() {
        let doc = sample_libredirect();
        let (svc, frontends) = find_libredirect_match(&doc, "youtube.com").unwrap();
        assert!(svc == "invidious" || svc == "piped");
        assert_eq!(frontends.len(), 5);
        assert!(frontends
            .iter()
            .all(|f| f.source == FrontendSource::LibRedirect));
    }

    #[test]
    fn libredirect_match_youtube_short() {
        let doc = sample_libredirect();
        let (svc, frontends) = find_libredirect_match(&doc, "youtu.be").unwrap();
        assert!(svc == "invidious" || svc == "piped");
        assert_eq!(frontends.len(), 5);
    }

    #[test]
    fn libredirect_match_subdomain() {
        assert!(host_matches_pattern("music.youtube.com", "*.youtube.com"));
    }

    #[test]
    fn libredirect_match_wikipedia_subdomain() {
        assert!(host_matches_pattern("en.wikipedia.org", "*.wikipedia.org"));
        assert!(host_matches_pattern("wikipedia.org", "*.wikipedia.org"));
        assert!(!host_matches_pattern("fakepedia.org", "*.wikipedia.org"));
    }

    #[test]
    fn libredirect_no_match() {
        let doc = sample_libredirect();
        assert!(find_libredirect_match(&doc, "example.org").is_none());
    }

    #[test]
    fn format_hit_html_includes_service_and_kind() {
        use crate::redirects::models::{Frontend, FrontendSource, LookupHit};

        let hit = LookupHit {
            service: "youtube".into(),
            frontends: vec![Frontend {
                service: "youtube".into(),
                kind: "invidious".into(),
                url: "https://yewtu.be".into(),
                source: FrontendSource::LibRedirect,
            }],
        };
        let out = format_hit_html(&hit, 5, "https://youtube.com/watch?v=test123");
        assert!(out.contains("youtube"));
        assert!(out.contains("invidious"));
        assert!(out.contains("yewtu.be"));
        assert!(out.contains("LibRedirect"));
        assert!(out.contains("watch?v=test123"));
    }

    #[test]
    fn format_hit_html_caps_instances_per_kind() {
        use crate::redirects::models::{Frontend, FrontendSource, LookupHit};

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
        let out = format_hit_html(&hit, 3, "https://twitter.com/user/status/123");
        assert!(out.contains("<b>• nitter</b>"));
        assert!(out.contains("nitter0.example"));
        assert!(!out.contains("nitter9.example"));
    }

    #[test]
    fn host_matches_pattern_exact() {
        assert!(host_matches_pattern("youtube.com", "youtube.com"));
        assert!(!host_matches_pattern("youtube.com", "twitter.com"));
    }

    #[test]
    fn host_matches_pattern_wildcard() {
        assert!(host_matches_pattern("en.wikipedia.org", "*.wikipedia.org"));
        assert!(host_matches_pattern("fr.wikipedia.org", "*.wikipedia.org"));
        assert!(host_matches_pattern("wikipedia.org", "*.wikipedia.org"));
        assert!(!host_matches_pattern(
            "wikipediafake.org",
            "*.wikipedia.org"
        ));
    }

    // ─── build_frontend_url tests ───

    #[test]
    fn build_frontend_url_invidious_watch() {
        let out = build_frontend_url(
            "https://yewtu.be",
            "https://www.youtube.com/watch?v=RbBnf8U-q94&feature=share",
            "invidious",
        );
        assert_eq!(out, "https://yewtu.be/watch?v=RbBnf8U-q94");
    }

    #[test]
    fn build_frontend_url_invidious_shorts() {
        let out = build_frontend_url(
            "https://yewtu.be",
            "https://www.youtube.com/shorts/abc123",
            "invidious",
        );
        assert_eq!(out, "https://yewtu.be/watch?v=abc123");
    }

    #[test]
    fn build_frontend_url_invidious_youtu_be() {
        let out = build_frontend_url(
            "https://yewtu.be",
            "https://youtu.be/RbBnf8U-q94",
            "invidious",
        );
        assert_eq!(out, "https://yewtu.be/watch?v=RbBnf8U-q94");
    }

    #[test]
    fn build_frontend_url_piped() {
        let out = build_frontend_url(
            "https://piped.video",
            "https://www.youtube.com/watch?v=XYZ789&list=PLabc",
            "piped",
        );
        assert_eq!(out, "https://piped.video/watch?v=XYZ789");
    }

    #[test]
    fn build_frontend_url_cloudtube() {
        let out = build_frontend_url(
            "https://tube.cadence.moe",
            "https://www.youtube.com/watch?v=abc123",
            "cloudtube",
        );
        assert_eq!(out, "https://tube.cadence.moe/video/abc123");
    }

    #[test]
    fn build_frontend_url_materialious() {
        let out = build_frontend_url(
            "https://materialious.example",
            "https://www.youtube.com/watch?v=test456",
            "materialious",
        );
        assert_eq!(out, "https://materialious.example/watch/test456");
    }

    #[test]
    fn build_frontend_url_hyperpipe() {
        let out = build_frontend_url(
            "https://hyperpipe.example",
            "https://www.youtube.com/watch?v=song123",
            "hyperpipe",
        );
        assert_eq!(out, "https://hyperpipe.example/watch?v=song123");
    }

    #[test]
    fn build_frontend_url_poke() {
        let out = build_frontend_url(
            "https://poketube.fun",
            "https://www.youtube.com/watch?v=test456",
            "poke",
        );
        assert_eq!(out, "https://poketube.fun/watch?v=test456");
    }

    #[test]
    fn build_frontend_url_nitter() {
        let out = build_frontend_url(
            "https://nitter.net",
            "https://twitter.com/elonmusk/status/1234567890",
            "nitter",
        );
        assert_eq!(out, "https://nitter.net/elonmusk/status/1234567890");
    }

    #[test]
    fn build_frontend_url_nitter_x_domain() {
        let out = build_frontend_url(
            "https://nitter.net",
            "https://x.com/user/status/9876543210",
            "nitter",
        );
        assert_eq!(out, "https://nitter.net/user/status/9876543210");
    }

    #[test]
    fn build_frontend_url_redlib() {
        let out = build_frontend_url(
            "https://redlib.example",
            "https://www.reddit.com/r/rust/comments/abc123/my_post/",
            "redlib",
        );
        assert_eq!(out, "https://redlib.example/r/rust/comments/abc123");
    }

    #[test]
    fn build_frontend_url_proxitok() {
        let out = build_frontend_url(
            "https://proxitok.example",
            "https://www.tiktok.com/@user/video/7123456789012345678",
            "proxiTok",
        );
        assert_eq!(out, "https://proxitok.example/video/7123456789012345678");
    }

    #[test]
    fn build_frontend_url_preserves_generic_path() {
        let out = build_frontend_url(
            "https://scribe.example",
            "https://medium.com/@user/article-title-abc123",
            "scribe",
        );
        assert_eq!(out, "https://scribe.example/@user/article-title-abc123");
    }

    #[test]
    fn build_frontend_url_fallback() {
        let out = build_frontend_url(
            "https://example.org",
            "https://unknown.com/some/path?q=1",
            "unknown_kind",
        );
        assert_eq!(out, "https://example.org/some/path?q=1");
    }

    #[test]
    fn build_frontend_url_invalid_original() {
        let out = build_frontend_url("https://yewtu.be", "not a valid url at all!!!", "invidious");
        assert_eq!(out, "https://yewtu.be");
    }

    #[tokio::test]
    async fn lookup_returns_none_on_unknown_host() {
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
        assert!(hit.service == "invidious" || hit.service == "piped");
        assert!(
            hit.frontends
                .iter()
                .any(|f| f.url == "https://farside-inv.example"
                    && f.source == FrontendSource::Farside)
        );
    }
}
