use crate::sanitizer::pipeline::SanitizedUrl;
use async_trait::async_trait;

#[cfg_attr(any(test, feature = "test-utils"), mockall::automock)]
#[async_trait]
pub trait SanitizerService: Send + Sync {
    async fn sanitize(&self, url: &str, config: &SanitizeConfig) -> Option<SanitizedUrl>;

    async fn expand_url(&self, url: &str) -> String;

    fn is_supported_by_clearurls(&self, url: &str) -> bool;
}

#[derive(Debug, Clone)]
pub struct SanitizeConfig {
    pub enable_ai: bool,
    pub honor_creator: bool,
    pub aggressive_mode: bool,
    pub custom_rules: Vec<crate::db::models::CustomRule>,
    pub ignored_domains: Vec<String>,
    pub dry_run: bool,
}

impl Default for SanitizeConfig {
    fn default() -> Self {
        Self {
            enable_ai: false,
            honor_creator: false,
            aggressive_mode: false,
            custom_rules: Vec::new(),
            ignored_domains: Vec::new(),
            dry_run: false,
        }
    }
}
