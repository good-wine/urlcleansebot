//! Linkumori rule format adapter.
//!
//! Linkumori is the active fork of ClearURLs for Firefox/Chrome.
//! It extends the ClearURLs rule format with:
//! - Custom rules that override built-ins
//! - Multiple remote rule sources
//! - Overload mode (merge rule sets)
//! - Per-rule enable/disable
//!
//! This module provides parsing and merging of Linkumori-compatible
//! rule sources alongside the existing ClearURLs catalog.
//!
//! Reference: https://github.com/Linkumori/Linkumori-Addon

use crate::http_utils::retry_http_request;
use crate::shared::error::{AppError, AppResult};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

/// Linkumori rule source configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct LinkumoriSource {
    /// URL to fetch rules from.
    pub url: String,
    /// Priority: lower = higher priority for overrides.
    #[serde(default)]
    pub priority: i32,
    /// Whether rules from this source override built-in ones.
    #[serde(default)]
    pub is_override: bool,
}

/// A single Linkumori rule.
#[derive(Debug, Clone, Deserialize)]
pub struct LinkumoriRule {
    /// The tracking parameter name or pattern to remove.
    pub name: String,
    /// Optional regex pattern for URL matching.
    #[serde(default)]
    pub url_pattern: Option<String>,
    /// Priority for override ordering.
    #[serde(default = "default_rule_priority")]
    pub priority: i32,
    /// Whether this rule is disabled.
    #[serde(default)]
    pub disabled: bool,
    /// Source identifier for attribution.
    #[serde(default)]
    pub source: String,
}

fn default_rule_priority() -> i32 {
    0
}

/// Compiled Linkumori rule ready for matching.
#[derive(Debug, Clone)]
struct CompiledLinkumoriRule {
    name: String,
    url_regex: Option<Regex>,
    _priority: i32,
    _source: String,
}

/// Compiled Linkumori rule set.
#[derive(Debug, Clone)]
struct CompiledSource {
    _source_url: String,
    _priority: i32,
    _is_override: bool,
    rules: Vec<CompiledLinkumoriRule>,
}

/// Linkumori rule engine that can merge multiple sources.
#[derive(Clone)]
pub struct LinkumoriEngine {
    sources: Arc<RwLock<Vec<CompiledSource>>>,
    /// Merged rule lookup: parameter name -> sources
    parameter_rules: Arc<RwLock<HashMap<String, Vec<CompiledLinkumoriRule>>>>,
}

impl LinkumoriEngine {
    /// Create a new empty Linkumori engine.
    pub fn new() -> Self {
        Self {
            sources: Arc::new(RwLock::new(Vec::new())),
            parameter_rules: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a rule source from a URL.
    pub async fn add_source(&self, source: &LinkumoriSource) -> AppResult<()> {
        debug!("Downloading Linkumori rules from: {}", source.url);
        let response = retry_http_request(
            || reqwest::Client::new().get(&source.url),
            "Linkumori rules download",
        )
        .await?;

        let text = response.text().await.map_err(|e| {
            AppError::Internal(format!("Failed to read Linkumori rules body: {e}"))
        })?;

        self.parse_and_add_source(source, &text)
    }

    /// Parse rule JSON and add to the engine.
    pub fn parse_and_add_source(
        &self,
        source: &LinkumoriSource,
        json_text: &str,
    ) -> AppResult<()> {
        let raw_rules: Vec<LinkumoriRule> = serde_json::from_str(json_text).map_err(|e| {
            AppError::Internal(format!("Failed to parse Linkumori rules JSON: {e}"))
        })?;

        let mut compiled = Vec::new();
        for rule in &raw_rules {
            if rule.disabled {
                continue;
            }
            let url_regex = match &rule.url_pattern {
                Some(pattern) => match Regex::new(pattern) {
                    Ok(r) => Some(r),
                    Err(e) => {
                        debug!(pattern = %pattern, error = %e, "Invalid Linkumori URL pattern, skipping");
                        continue;
                    },
                },
                None => None,
            };
            compiled.push(CompiledLinkumoriRule {
                name: rule.name.clone(),
                url_regex,
                _priority: rule.priority,
                _source: rule.source.clone(),
            });
        }

        let compiled_source = CompiledSource {
            _source_url: source.url.clone(),
            _priority: source.priority,
            _is_override: source.is_override,
            rules: compiled,
        };

        // Merge into sources
        {
            let mut sources = self
                .sources
                .write()
                .unwrap_or_else(|e| e.into_inner());
            sources.push(compiled_source);
            sources.sort_by_key(|s| s._priority);
        }

        // Rebuild parameter lookup
        self.rebuild_index();

        info!("Added Linkumori rules from: {}", source.url);
        Ok(())
    }

    /// Rebuild the parameter name -> rules index.
    fn rebuild_index(&self) {
        let mut index: HashMap<String, Vec<CompiledLinkumoriRule>> = HashMap::new();
        if let Ok(sources) = self.sources.read() {
            for source in sources.iter() {
                for rule in &source.rules {
                    index
                        .entry(rule.name.clone())
                        .or_default()
                        .push(rule.clone());
                }
            }
        }
        if let Ok(mut rules) = self.parameter_rules.write() {
            *rules = index;
        }
    }

    /// Check if a parameter should be removed according to Linkumori rules.
    /// Returns true if the parameter should be removed.
    pub fn should_remove_param(&self, param_name: &str, url: &str) -> bool {
        if let Ok(index) = self.parameter_rules.read()
            && let Some(rules) = index.get(param_name)
        {
            for rule in rules {
                if let Some(ref url_re) = rule.url_regex {
                    if url_re.is_match(url) {
                        return true;
                    }
                } else {
                    // No URL pattern = matches all URLs
                    return true;
                }
            }
        }
        false
    }

    /// Get all parameter names known to Linkumori rules.
    pub fn known_params(&self) -> Vec<String> {
        if let Ok(index) = self.parameter_rules.read() {
            index.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Number of loaded rule sources.
    pub fn source_count(&self) -> usize {
        if let Ok(sources) = self.sources.read() {
            sources.len()
        } else {
            0
        }
    }

    /// Number of loaded rules.
    pub fn rule_count(&self) -> usize {
        if let Ok(index) = self.parameter_rules.read() {
            index.values().map(|v| v.len()).sum()
        } else {
            0
        }
    }
}

impl Default for LinkumoriEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Default Linkumori rule source URL.
/// Uses the Linkumori-maintained rule set.
pub const DEFAULT_LINKUMORI_SOURCE: &str =
    "https://raw.githubusercontent.com/Linkumori/Linkumori-Addon/main/data/rules.json";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_engine() {
        let engine = LinkumoriEngine::new();
        assert_eq!(engine.source_count(), 0);
        assert_eq!(engine.rule_count(), 0);
        assert!(!engine.should_remove_param("utm_source", "https://example.com"));
    }

    #[test]
    fn test_parse_rules() {
        let engine = LinkumoriEngine::new();
        let json = r#"[
            {"name": "utm_source", "priority": 0, "disabled": false, "source": "test"},
            {"name": "utm_medium", "priority": 0, "disabled": false, "source": "test"},
            {"name": "fbclid", "url_pattern": ".*facebook\\.com.*", "priority": 1, "disabled": false, "source": "test"}
        ]"#;

        let source = LinkumoriSource {
            url: "test://local".to_string(),
            priority: 0,
            is_override: false,
        };

        assert!(engine.parse_and_add_source(&source, json).is_ok());
        assert_eq!(engine.source_count(), 1);
        assert_eq!(engine.rule_count(), 3);

        assert!(engine.should_remove_param("utm_source", "https://example.com"));
        assert!(engine.should_remove_param("utm_medium", "https://example.com"));
        assert!(engine.should_remove_param("fbclid", "https://facebook.com/page"));
        assert!(!engine.should_remove_param("fbclid", "https://example.com"));
    }

    #[test]
    fn test_disabled_rule() {
        let engine = LinkumoriEngine::new();
        let json = r#"[
            {"name": "utm_source", "disabled": true, "source": "test"},
            {"name": "utm_medium", "disabled": false, "source": "test"}
        ]"#;

        let source = LinkumoriSource {
            url: "test://local".to_string(),
            priority: 0,
            is_override: false,
        };

        assert!(engine.parse_and_add_source(&source, json).is_ok());
        assert!(!engine.should_remove_param("utm_source", "https://example.com"));
        assert!(engine.should_remove_param("utm_medium", "https://example.com"));
    }

    #[test]
    fn test_multiple_sources_merging() {
        let engine = LinkumoriEngine::new();

        let source1 = LinkumoriSource {
            url: "source1".to_string(),
            priority: 0,
            is_override: false,
        };
        engine
            .parse_and_add_source(
                &source1,
                r#"[
                {"name": "param1", "source": "s1"},
                {"name": "param2", "source": "s1"}
            ]"#,
            )
            .unwrap();

        let source2 = LinkumoriSource {
            url: "source2".to_string(),
            priority: 1,
            is_override: true,
        };
        engine
            .parse_and_add_source(
                &source2,
                r#"[
                {"name": "param3", "source": "s2"}
            ]"#,
            )
            .unwrap();

        assert_eq!(engine.source_count(), 2);
        assert_eq!(engine.rule_count(), 3);
        assert!(engine.should_remove_param("param1", "https://example.com"));
        assert!(engine.should_remove_param("param3", "https://example.com"));
    }
}
