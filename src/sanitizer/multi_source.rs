//! Multi-source URL sanitizer using url-sanitize-core.
//!
//! Merges rules from ClearURLs, AdGuard, Brave, and Firefox into a single
//! sanitization engine. Provides explainable results with stripped params,
//! redirect unwrapping, and provider attribution.
//!
//! See: https://github.com/antonio-orionus/url-sanitize

use crate::http_utils::retry_http_request;
use crate::shared::error::{AppError, AppResult};
use moka::future::Cache;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};
use url_sanitize_core::{Catalog, SanitizeResult, Sanitizer, SanitizerOptions};

/// Default URL for the merged multi-source catalog.
/// Published as `@url-sanitize/merged` on npm, mirrored on GitHub releases.
pub const DEFAULT_CATALOG_URL: &str =
    "https://github.com/antonio-orionus/url-sanitize/releases/latest/download/catalog.json";

/// Shared sanitizer instance with cached catalog.
#[derive(Clone)]
pub struct MultiSourceSanitizer {
    inner: Arc<Inner>,
}

struct Inner {
    sanitizer: RwLock<Option<Sanitizer>>,
    cache: Cache<String, Option<String>>,
}

impl MultiSourceSanitizer {
    /// Create a new lazily-initialized sanitizer.
    pub fn new_lazy() -> Self {
        Self {
            inner: Arc::new(Inner {
                sanitizer: RwLock::new(None),
                cache: Cache::builder()
                    .max_capacity(10_000)
                    .time_to_live(std::time::Duration::from_secs(3600))
                    .build(),
            }),
        }
    }

    /// Load catalog from a JSON string (e.g., from file or HTTP response).
    pub fn load_catalog(&self, json: &str) -> AppResult<()> {
        let catalog: Catalog = Catalog::from_json(json).map_err(|e| {
            AppError::Internal(format!("Failed to parse url-sanitize catalog: {e}"))
        })?;
        let sanitizer = catalog.compile(SanitizerOptions::default());
        let mut guard = self
            .inner
            .sanitizer
            .write()
            .unwrap_or_else(|e| e.into_inner());
        *guard = Some(sanitizer);
        info!("Caricato catalogo url-sanitize multi-source");
        Ok(())
    }

    /// Download and load the catalog from a URL.
    pub async fn fetch_catalog(&self, url: &str) -> AppResult<()> {
        debug!("Scaricamento catalogo url-sanitize da: {}", url);
        let response = retry_http_request(
            || reqwest::Client::new().get(url),
            "url-sanitize catalog download",
        )
        .await?;
        let json = response.text().await.map_err(|e| {
            AppError::Internal(format!("Failed to read catalog response body: {e}"))
        })?;
        self.load_catalog(&json)
    }

    /// Sanitize a URL using the multi-source rules.
    ///
    /// Returns `Some(cleaned_url)` if tracking parameters were removed.
    pub async fn sanitize(&self, url: &str) -> Option<String> {
        if let Some(cached) = self.inner.cache.get(url).await {
            return cached;
        }

        let cleaned = {
            let sanitizer = self
                .inner
                .sanitizer
                .read()
                .unwrap_or_else(|e| e.into_inner());
            let result = match sanitizer.as_ref() {
                Some(s) => s.sanitize(url),
                None => return None,
            };
            // sanitizer guard dropped here when block ends
            match result {
                SanitizeResult::Unchanged { .. } => None,
                SanitizeResult::Cleaned {
                    url: cleaned_url, ..
                } => {
                    if cleaned_url != url {
                        Some(cleaned_url)
                    } else {
                        None
                    }
                },
                SanitizeResult::Redirected { url: unwrapped, .. } => Some(unwrapped),
                SanitizeResult::Blocked { .. } => None,
            }
        };

        self.inner
            .cache
            .insert(url.to_string(), cleaned.clone())
            .await;
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_lazy_creates_empty() {
        let sanitizer = MultiSourceSanitizer::new_lazy();
        // Should not panic and should not have a sanitizer loaded
        assert!(
            sanitizer
                .inner
                .sanitizer
                .read()
                .expect("lock poisoned")
                .is_none()
        );
    }
}
