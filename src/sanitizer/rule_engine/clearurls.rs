use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct RawProvider {
    #[serde(default)]
    pub urlPattern: String,
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub exceptions: Vec<String>,
    #[serde(default)]
    pub rawRules: Vec<String>,
    #[serde(default)]
    pub redirections: Vec<String>,
    #[serde(default)]
    pub referralMarketing: Vec<String>,
    #[serde(default)]
    pub forceRedirection: bool,
}

#[derive(Debug, Deserialize)]
pub struct ClearUrlsData {
    pub providers: HashMap<String, RawProvider>,
}

#[derive(Clone)]
pub struct CompiledProvider {
    pub name: String,
    pub url_pattern: Regex,
    pub rules: Vec<Regex>,
    pub exceptions: Vec<Regex>,
    pub raw_rules: Vec<Regex>,
    pub redirections: Vec<Regex>,
    pub referral_marketing: Vec<Regex>,
    pub _force_redirection: bool,
}

pub fn compile_providers(data: ClearUrlsData) -> Vec<CompiledProvider> {
    let mut compiled = Vec::new();

    for (name, provider) in data.providers {
        if provider.urlPattern.is_empty() {
            continue;
        }

        let url_pattern = match Regex::new(&provider.urlPattern) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let compile_list = |list: &[String]| -> Vec<Regex> {
            list.iter().filter_map(|s| Regex::new(s).ok()).collect()
        };

        compiled.push(CompiledProvider {
            name,
            url_pattern,
            rules: compile_list(&provider.rules),
            exceptions: compile_list(&provider.exceptions),
            raw_rules: compile_list(&provider.rawRules),
            redirections: compile_list(&provider.redirections),
            referral_marketing: compile_list(&provider.referralMarketing),
            _force_redirection: provider.forceRedirection,
        });
    }

    compiled
}
