use crate::redirects::LookupHit;
use async_trait::async_trait;

#[cfg_attr(any(test, feature = "test-utils"), mockall::automock)]
#[async_trait]
pub trait RedirectProvider: Send + Sync {
    /// Look up alternative frontends for a given URL.
    async fn lookup(&self, url: &str) -> Vec<LookupHit>;

    /// Check if a domain is supported by any alternative frontend.
    fn is_supported(&self, domain: &str) -> bool;
}
