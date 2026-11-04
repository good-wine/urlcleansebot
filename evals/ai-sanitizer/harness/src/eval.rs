use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;
use tracing::{info, warn};

use crate::judge::{JudgeClient, JudgeResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetEntry {
    pub input_url: String,
    pub expected_output: String,
    pub category: String,
    pub difficulty: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bug_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_behavior: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationCase {
    pub entry: DatasetEntry,
    pub actual_output: Option<String>,
    pub latency_ms: u64,
    pub success: bool,
    pub error: Option<String>,
    pub fallback_triggered: bool,
    pub tokens_used: Option<u32>,
    pub cost_usd: Option<f64>,
    pub judge_result: Option<JudgeResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    pub run_id: String,
    pub timestamp: String,
    pub model: String,
    pub total_cases: usize,
    pub cases: Vec<EvaluationCase>,
    pub metrics: AggregateMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateMetrics {
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub false_positive_rate: f64,
    pub latency_p50: u64,
    pub latency_p95: u64,
    pub latency_p99: u64,
    pub avg_latency: f64,
    pub cost_per_request: f64,
    pub total_cost: f64,
    pub fallback_rate: f64,
    pub success_rate: f64,
    pub by_category: HashMap<String, CategoryMetrics>,
    pub by_difficulty: HashMap<String, DifficultyMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryMetrics {
    pub total: usize,
    pub correct: usize,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub false_positive_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifficultyMetrics {
    pub total: usize,
    pub correct: usize,
    pub accuracy: f64,
}

#[derive(Debug, Clone, Default)]
pub struct EvaluationConfig {
    pub ai_api_base: String,
    pub ai_api_key: String,
    pub ai_model: String,
    pub judge_model: String,
    pub judge_api_base: String,
    pub judge_api_key: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub cost_per_1k_input_tokens: f64,
    pub cost_per_1k_output_tokens: f64,
}

impl EvaluationConfig {
    pub async fn load(path: &Path) -> anyhow::Result<Self> {
        let content = tokio::fs::read_to_string(path).await?;
        let config: ConfigFile = toml::from_str(&content)?;

        Ok(Self {
            ai_api_base: config.ai.api_base,
            ai_api_key: config.ai.api_key,
            ai_model: config.ai.model,
            judge_model: config.judge.model,
            judge_api_base: config.judge.api_base,
            judge_api_key: config.judge.api_key,
            timeout_secs: config.eval.timeout_secs,
            max_retries: config.eval.max_retries,
            cost_per_1k_input_tokens: config.eval.cost_per_1k_input_tokens,
            cost_per_1k_output_tokens: config.eval.cost_per_1k_output_tokens,
        })
    }
}

#[derive(Deserialize)]
struct ConfigFile {
    ai: AiConfig,
    judge: JudgeConfig,
    eval: EvalConfig,
}

#[derive(Deserialize)]
struct AiConfig {
    api_base: String,
    api_key: String,
    model: String,
}

#[derive(Deserialize)]
struct JudgeConfig {
    api_base: String,
    api_key: String,
    model: String,
}

#[derive(Deserialize)]
struct EvalConfig {
    timeout_secs: u64,
    max_retries: u32,
    cost_per_1k_input_tokens: f64,
    cost_per_1k_output_tokens: f64,
}

pub struct EvaluationHarness {
    config: EvaluationConfig,
    client: reqwest::Client,
    judge: Option<JudgeClient>,
    concurrency: usize,
}

impl EvaluationHarness {
    pub async fn new(config: EvaluationConfig, concurrency: usize, use_judge: bool) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()?;

        let judge = if use_judge {
            Some(
                JudgeClient::new(
                    config.judge_api_base.clone(),
                    config.judge_api_key.clone(),
                    config.judge_model.clone(),
                )
                .await?,
            )
        } else {
            None
        };

        Ok(Self { config, client, judge, concurrency })
    }

    pub async fn run(
        &mut self,
        dataset_path: &Path,
        categories: Option<String>,
        difficulties: Option<String>,
    ) -> anyhow::Result<EvaluationResult> {
        let entries = Self::load_dataset(dataset_path, categories, difficulties).await?;
        info!("Loaded {} evaluation cases", entries.len());

        let mut cases = Vec::with_capacity(entries.len());
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(self.concurrency));

        let mut handles = Vec::new();

        for entry in entries {
            let permit = semaphore.clone().acquire_owned().await?;
            let client = self.client.clone();
            let config = self.config.clone();
            let judge = self.judge.clone();

            let handle = tokio::spawn(async move {
                let _permit = permit;
                Self::evaluate_case(&client, &config, &entry, judge.as_ref()).await
            });

            handles.push(handle);
        }

        for handle in handles {
            cases.push(handle.await??);
        }

        let metrics = Self::calculate_metrics(&cases);

        Ok(EvaluationResult {
            run_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            model: self.config.ai_model.clone(),
            total_cases: cases.len(),
            cases,
            metrics,
        })
    }

    async fn load_dataset(
        path: &Path,
        categories: Option<String>,
        difficulties: Option<String>,
    ) -> anyhow::Result<Vec<DatasetEntry>> {
        let content = tokio::fs::read_to_string(path).await?;
        let mut entries = Vec::new();

        let category_filter: Option<Vec<String>> =
            categories.map(|c| c.split(',').map(|s| s.trim().to_string()).collect());
        let difficulty_filter: Option<Vec<String>> =
            difficulties.map(|d| d.split(',').map(|s| s.trim().to_string()).collect());

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<DatasetEntry>(line) {
                Ok(entry) => {
                    if let Some(ref cats) = category_filter {
                        if !cats.contains(&entry.category) {
                            continue;
                        }
                    }
                    if let Some(ref diffs) = difficulty_filter {
                        if !diffs.contains(&entry.difficulty) {
                            continue;
                        }
                    }
                    entries.push(entry);
                },
                Err(e) => {
                    warn!("Failed to parse line: {}", e);
                },
            }
        }

        Ok(entries)
    }

    async fn evaluate_case(
        client: &reqwest::Client,
        config: &EvaluationConfig,
        entry: &DatasetEntry,
        judge: Option<&JudgeClient>,
    ) -> anyhow::Result<EvaluationCase> {
        let start = Instant::now();

        let prompt = format!(
            "You are a URL sanitizer. Remove all tracking parameters from the following URL. \
            Tracking parameters are things like utm_source, fbclid, gclid, msclkid, etc. \
            Preserve all functional parameters like IDs, search queries, pagination, filters, etc. \
            Return only the sanitized URL, or the original URL if no tracking parameters are found.\n\nURL: {}",
            entry.input_url
        );

        let request_body = serde_json::json!({
            "model": config.ai_model,
            "messages": [
                {"role": "system", "content": "You are a URL sanitizer that removes tracking parameters while preserving functional parameters."},
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.0,
            "max_tokens": 500,
        });

        let mut last_error: Option<String> = None;
        let mut fallback_triggered = false;
        let mut actual_output: Option<String> = None;
        let mut tokens_used: Option<u32> = None;
        let mut cost_usd: Option<f64> = None;

        for attempt in 0..=config.max_retries {
            match Self::call_ai_api(client, config, &request_body).await {
                Ok(response) => {
                    if let Some(choices) = response.get("choices").and_then(|c| c.as_array()) {
                        if let Some(choice) = choices.first() {
                            if let Some(content) = choice
                                .get("message")
                                .and_then(|m| m.get("content"))
                                .and_then(|c| c.as_str())
                            {
                                actual_output = Some(content.trim().to_string());
                            }
                        }
                    }

                    if let Some(usage) = response.get("usage") {
                        let prompt_tokens = usage.get("prompt_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32;
                        let completion_tokens =
                            usage.get("completion_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32;
                        tokens_used = Some(prompt_tokens + completion_tokens);

                        cost_usd = Some(
                            (prompt_tokens as f64 / 1000.0) * config.cost_per_1k_input_tokens
                                + (completion_tokens as f64 / 1000.0) * config.cost_per_1k_output_tokens,
                        );
                    }

                    break;
                },
                Err(e) => {
                    last_error = Some(e.to_string());
                    if attempt < config.max_retries {
                        tokio::time::sleep(std::time::Duration::from_millis(100 * (attempt + 1) as u64)).await;
                    } else {
                        fallback_triggered = true;
                    }
                },
            }
        }

        let latency_ms = start.elapsed().as_millis() as u64;
        let success = actual_output.is_some();

        let judge_result = if let (Some(judge_client), Some(output)) = (judge, &actual_output) {
            Some(
                judge_client
                    .evaluate(&entry.input_url, output, &entry.expected_output)
                    .await?,
            )
        } else {
            None
        };

        Ok(EvaluationCase {
            entry: entry.clone(),
            actual_output,
            latency_ms,
            success,
            error: last_error,
            fallback_triggered,
            tokens_used,
            cost_usd,
            judge_result,
        })
    }

    async fn call_ai_api(
        client: &reqwest::Client,
        config: &EvaluationConfig,
        body: &serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let response = client
            .post(&format!("{}/chat/completions", config.ai_api_base))
            .header("Authorization", format!("Bearer {}", config.ai_api_key))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("API error: {}", error_text);
        }

        Ok(response.json().await?)
    }

    fn calculate_metrics(cases: &[EvaluationCase]) -> AggregateMetrics {
        let mut tp: u64 = 0;
        let mut fp: u64 = 0;
        let mut fn_: u64 = 0;
        let mut tn: u64 = 0;

        let mut latencies: Vec<u64> = Vec::new();
        let mut total_cost: f64 = 0.0;
        let mut fallback_count: u64 = 0;
        let mut success_count: u64 = 0;

        let mut by_category_data: HashMap<String, (u64, u64, u64, u64)> = HashMap::new();
        let mut by_difficulty_data: HashMap<String, (u64, u64)> = HashMap::new();

        for case in cases {
            latencies.push(case.latency_ms);

            if let Some(cost) = case.cost_usd {
                total_cost += cost;
            }

            if case.fallback_triggered {
                fallback_count += 1;
            }

            if case.success {
                success_count += 1;
            }

            let expected = &case.entry.expected_output;
            let actual = case.actual_output.as_deref().unwrap_or("");

            let is_correct = Self::urls_match(expected, actual);
            let has_tracking = expected != &case.entry.input_url;

            if has_tracking {
                if is_correct {
                    tp += 1;
                    let cat = &case.entry.category;
                    let data = by_category_data.entry(cat.clone()).or_insert((0, 0, 0, 0));
                    *data = (data.0 + 1, data.1, data.2, data.3);
                } else {
                    fn_ += 1;
                    let cat = &case.entry.category;
                    let data = by_category_data.entry(cat.clone()).or_insert((0, 0, 0, 0));
                    *data = (data.0, data.1, data.2 + 1, data.3);
                }
            } else {
                if is_correct {
                    tn += 1;
                    let cat = &case.entry.category;
                    let data = by_category_data.entry(cat.clone()).or_insert((0, 0, 0, 0));
                    *data = (data.0, data.1, data.2, data.3 + 1);
                } else {
                    fp += 1;
                    let cat = &case.entry.category;
                    let data = by_category_data.entry(cat.clone()).or_insert((0, 0, 0, 0));
                    *data = (data.0, data.1 + 1, data.2, data.3);
                }
            }

            let diff = &case.entry.difficulty;
            let data = by_difficulty_data.entry(diff.clone()).or_insert((0, 0));
            if is_correct {
                *data = (data.0 + 1, data.1 + 1);
            } else {
                *data = (data.0, data.1 + 1);
            }
        }

        latencies.sort();
        let latency_p50 = Self::percentile(&latencies, 50);
        let latency_p95 = Self::percentile(&latencies, 95);
        let latency_p99 = Self::percentile(&latencies, 99);
        let avg_latency = latencies.iter().sum::<u64>() as f64 / latencies.len().max(1) as f64;

        let precision = if tp + fp > 0 { tp as f64 / (tp + fp) as f64 } else { 1.0 };
        let recall = if tp + fn_ > 0 {
            tp as f64 / (tp + fn_) as f64
        } else {
            1.0
        };
        let f1 = if precision + recall > 0.0 {
            2.0 * precision * recall / (precision + recall)
        } else {
            0.0
        };
        let false_positive_rate = if fp + tn > 0 { fp as f64 / (fp + tn) as f64 } else { 0.0 };
        let fallback_rate = fallback_count as f64 / cases.len().max(1) as f64;
        let success_rate = success_count as f64 / cases.len().max(1) as f64;
        let cost_per_request = total_cost / cases.len().max(1) as f64;

        let mut category_metrics = HashMap::new();
        for (cat, (tp, fp, fn_, tn)) in by_category_data {
            let p = if tp + fp > 0 { tp as f64 / (tp + fp) as f64 } else { 1.0 };
            let r = if tp + fn_ > 0 {
                tp as f64 / (tp + fn_) as f64
            } else {
                1.0
            };
            let f1_cat = if p + r > 0.0 { 2.0 * p * r / (p + r) } else { 0.0 };
            let fpr = if fp + tn > 0 { fp as f64 / (fp + tn) as f64 } else { 0.0 };

            category_metrics.insert(
                cat,
                CategoryMetrics {
                    total: (tp + fp + fn_ + tn) as usize,
                    correct: (tp + tn) as usize,
                    precision: p,
                    recall: r,
                    f1: f1_cat,
                    false_positive_rate: fpr,
                },
            );
        }

        let mut difficulty_metrics = HashMap::new();
        for (diff, (correct, total)) in by_difficulty_data {
            difficulty_metrics.insert(
                diff,
                DifficultyMetrics {
                    total: total as usize,
                    correct: correct as usize,
                    accuracy: correct as f64 / total.max(1) as f64,
                },
            );
        }

        AggregateMetrics {
            precision,
            recall,
            f1,
            false_positive_rate,
            latency_p50,
            latency_p95,
            latency_p99,
            avg_latency,
            cost_per_request,
            total_cost,
            fallback_rate,
            success_rate,
            by_category: category_metrics,
            by_difficulty: difficulty_metrics,
        }
    }

    fn urls_match(expected: &str, actual: &str) -> bool {
        if let Ok(parsed) = url::Url::parse(expected) {
            if let Ok(actual_parsed) = url::Url::parse(actual) {
                let mut expected_pairs: Vec<_> = parsed.query_pairs().collect();
                let mut actual_pairs: Vec<_> = actual_parsed.query_pairs().collect();
                expected_pairs.sort_by(|a, b| a.0.cmp(&b.0));
                actual_pairs.sort_by(|a, b| a.0.cmp(&b.0));
                expected_pairs.eq(&actual_pairs)
            } else {
                false
            }
        } else {
            expected == actual
        }
    }

    fn percentile(sorted: &[u64], p: u8) -> u64 {
        if sorted.is_empty() {
            return 0;
        }
        let idx = ((sorted.len() - 1) as f64 * p as f64 / 100.0).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    pub fn normalize_url(url: &str) -> String {
        if let Ok(mut parsed) = url::Url::parse(url) {
            let mut pairs: Vec<_> = parsed.query_pairs().collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));

            let query = pairs
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");

            parsed.set_query(Some(&query));
            parsed.to_string()
        } else {
            url.to_string()
        }
    }
}

pub async fn validate_dataset(path: &Path) -> anyhow::Result<()> {
    let content = tokio::fs::read_to_string(path).await?;
    let mut line_num = 0;

    for line in content.lines() {
        line_num += 1;
        if line.trim().is_empty() {
            continue;
        }

        let entry: DatasetEntry =
            serde_json::from_str(line).map_err(|e| anyhow::anyhow!("Line {}: {}", line_num, e))?;

        if entry.input_url.is_empty() {
            anyhow::bail!("Line {}: input_url is empty", line_num);
        }
        if entry.expected_output.is_empty() {
            anyhow::bail!("Line {}: expected_output is empty", line_num);
        }
        if entry.category.is_empty() {
            anyhow::bail!("Line {}: category is empty", line_num);
        }
        if entry.difficulty.is_empty() {
            anyhow::bail!("Line {}: difficulty is empty", line_num);
        }

        let valid_categories = [
            "utm",
            "affiliate",
            "social",
            "email",
            "ads",
            "analytics",
            "session",
            "consent",
            "obfuscated",
            "nested",
            "new_pattern",
            "prompt_injection",
            "data_exfiltration",
            "jailbreak",
        ];
        if !valid_categories.contains(&entry.category.as_str()) {
            anyhow::bail!("Line {}: invalid category '{}'", line_num, entry.category);
        }

        let valid_difficulties = ["easy", "medium", "hard", "expert"];
        if !valid_difficulties.contains(&entry.difficulty.as_str()) {
            anyhow::bail!("Line {}: invalid difficulty '{}'", line_num, entry.difficulty);
        }

        if !entry.input_url.starts_with("http") {
            anyhow::bail!("Line {}: input_url must be a valid URL", line_num);
        }
        if !entry.expected_output.starts_with("http") {
            anyhow::bail!("Line {}: expected_output must be a valid URL", line_num);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn test_validate_dataset() {
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("test.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "{{\"input_url\": \"https://example.com?utm_source=test\", \"expected_output\": \"https://example.com\", \"category\": \"utm\", \"difficulty\": \"easy\", \"description\": \"test\"}}").unwrap();
        drop(file);

        validate_dataset(&path).await.unwrap();
    }

    #[tokio::test]
    async fn test_validate_dataset_invalid() {
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("test.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "{{\"input_url\": \"\", \"expected_output\": \"https://example.com\", \"category\": \"utm\", \"difficulty\": \"easy\", \"description\": \"test\"}}").unwrap();
        drop(file);

        assert!(validate_dataset(&path).await.is_err());
    }

    #[test]
    fn test_urls_match() {
        assert!(EvaluationHarness::urls_match(
            "https://example.com/page?a=1&b=2",
            "https://example.com/page?b=2&a=1"
        ));
        assert!(!EvaluationHarness::urls_match(
            "https://example.com/page?a=1",
            "https://example.com/page?a=2"
        ));
    }

    #[test]
    fn test_normalize_url() {
        assert_eq!(
            EvaluationHarness::normalize_url("https://example.com/page?b=2&a=1"),
            EvaluationHarness::normalize_url("https://example.com/page?a=1&b=2")
        );
    }
}
