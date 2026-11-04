use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tabled::{Table, Tabled};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsReport {
    pub run_id: String,
    pub timestamp: String,
    pub model: String,
    pub total_cases: usize,
    pub aggregate: AggregateMetrics,
    pub category_breakdown: Vec<CategoryReport>,
    pub difficulty_breakdown: Vec<DifficultyReport>,
    pub latency_distribution: LatencyDistribution,
    pub cost_analysis: CostAnalysis,
    pub fallback_analysis: FallbackAnalysis,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct CategoryReport {
    #[tabled(rename = "Category")]
    pub category: String,
    #[tabled(rename = "Total")]
    pub total: usize,
    #[tabled(rename = "Correct")]
    pub correct: usize,
    #[tabled(rename = "Precision")]
    pub precision: f64,
    #[tabled(rename = "Recall")]
    pub recall: f64,
    #[tabled(rename = "F1")]
    pub f1: f64,
    #[tabled(rename = "FPR")]
    pub false_positive_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct DifficultyReport {
    #[tabled(rename = "Difficulty")]
    pub difficulty: String,
    #[tabled(rename = "Total")]
    pub total: usize,
    #[tabled(rename = "Correct")]
    pub correct: usize,
    #[tabled(rename = "Accuracy")]
    pub accuracy: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyDistribution {
    pub min: u64,
    pub max: u64,
    pub mean: f64,
    pub std_dev: f64,
    pub p10: u64,
    pub p25: u64,
    pub p50: u64,
    pub p75: u64,
    pub p90: u64,
    pub p95: u64,
    pub p99: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostAnalysis {
    pub total_cost_usd: f64,
    pub cost_per_request_usd: f64,
    pub estimated_monthly_cost_usd: f64,
    pub cost_by_category: HashMap<String, f64>,
    pub tokens_per_request: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackAnalysis {
    pub total_fallbacks: usize,
    pub fallback_rate: f64,
    pub fallback_by_category: HashMap<String, usize>,
    pub fallback_by_error: HashMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub passed: bool,
    pub timestamp: String,
    pub model: String,
    pub checks: Vec<GateCheck>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct GateCheck {
    #[tabled(rename = "Check")]
    pub name: String,
    #[tabled(rename = "Threshold")]
    pub threshold: String,
    #[tabled(rename = "Actual")]
    pub actual: String,
    #[tabled(rename = "Status")]
    pub status: String,
}

impl MetricsReport {
    pub fn from_result(result: &crate::eval::EvaluationResult) -> Self {
        let category_breakdown: Vec<CategoryReport> = result
            .metrics
            .by_category
            .iter()
            .map(|(cat, m)| CategoryReport {
                category: cat.clone(),
                total: m.total,
                correct: m.correct,
                precision: m.precision,
                recall: m.recall,
                f1: m.f1,
                false_positive_rate: m.false_positive_rate,
            })
            .collect();

        let difficulty_breakdown: Vec<DifficultyReport> = result
            .metrics
            .by_difficulty
            .iter()
            .map(|(diff, m)| DifficultyReport {
                difficulty: diff.clone(),
                total: m.total,
                correct: m.correct,
                accuracy: m.accuracy,
            })
            .collect();

        let latencies: Vec<u64> = result.cases.iter().map(|c| c.latency_ms).collect();
        let latency_distribution = Self::calculate_latency_distribution(&latencies);

        let cost_analysis = Self::calculate_cost_analysis(&result.cases, &result.metrics);
        let fallback_analysis = Self::calculate_fallback_analysis(&result.cases);

        Self {
            run_id: result.run_id.clone(),
            timestamp: result.timestamp.clone(),
            model: result.model.clone(),
            total_cases: result.total_cases,
            aggregate: AggregateMetrics {
                precision: result.metrics.precision,
                recall: result.metrics.recall,
                f1: result.metrics.f1,
                false_positive_rate: result.metrics.false_positive_rate,
                latency_p50: result.metrics.latency_p50,
                latency_p95: result.metrics.latency_p95,
                latency_p99: result.metrics.latency_p99,
                avg_latency: result.metrics.avg_latency,
                cost_per_request: result.metrics.cost_per_request,
                total_cost: result.metrics.total_cost,
                fallback_rate: result.metrics.fallback_rate,
                success_rate: result.metrics.success_rate,
            },
            category_breakdown,
            difficulty_breakdown,
            latency_distribution,
            cost_analysis,
            fallback_analysis,
        }
    }

    pub fn from_redteam_result(result: &crate::redteam::RedTeamResult) -> Self {
        let mut report = Self::default();
        report.run_id = result.run_id.clone();
        report.timestamp = result.timestamp.clone();
        report.model = result.model.clone();
        report.total_cases = result.total_cases;
        report.aggregate = AggregateMetrics {
            precision: 0.0,
            recall: 0.0,
            f1: 0.0,
            false_positive_rate: result.attack_success_rate,
            latency_p50: result.latency_p50,
            latency_p95: result.latency_p95,
            latency_p99: result.latency_p99,
            avg_latency: result.avg_latency,
            cost_per_request: result.cost_per_request,
            total_cost: result.total_cost,
            fallback_rate: 0.0,
            success_rate: 1.0 - result.attack_success_rate,
        };
        report
    }

    fn calculate_latency_distribution(latencies: &[u64]) -> LatencyDistribution {
        let mut sorted = latencies.to_vec();
        sorted.sort();

        let len = sorted.len();
        let sum: u64 = sorted.iter().sum();
        let mean = sum as f64 / len.max(1) as f64;

        let variance: f64 = sorted.iter().map(|&x| (x as f64 - mean).powi(2)).sum::<f64>() / len.max(1) as f64;
        let std_dev = variance.sqrt();

        LatencyDistribution {
            min: *sorted.first().unwrap_or(&0),
            max: *sorted.last().unwrap_or(&0),
            mean,
            std_dev,
            p10: Self::percentile(&sorted, 10),
            p25: Self::percentile(&sorted, 25),
            p50: Self::percentile(&sorted, 50),
            p75: Self::percentile(&sorted, 75),
            p90: Self::percentile(&sorted, 90),
            p95: Self::percentile(&sorted, 95),
            p99: Self::percentile(&sorted, 99),
        }
    }

    fn percentile(sorted: &[u64], p: u8) -> u64 {
        if sorted.is_empty() {
            return 0;
        }
        let idx = ((sorted.len() - 1) as f64 * p as f64 / 100.0).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    fn calculate_cost_analysis(
        cases: &[crate::eval::EvaluationCase],
        metrics: &crate::eval::AggregateMetrics,
    ) -> CostAnalysis {
        let mut cost_by_category = HashMap::new();
        let mut total_tokens = 0u64;
        let mut token_count = 0usize;

        for case in cases {
            if let Some(cost) = case.cost_usd {
                *cost_by_category.entry(case.entry.category.clone()).or_insert(0.0) += cost;
            }
            if let Some(tokens) = case.tokens_used {
                total_tokens += tokens as u64;
                token_count += 1;
            }
        }

        CostAnalysis {
            total_cost_usd: metrics.total_cost,
            cost_per_request_usd: metrics.cost_per_request,
            estimated_monthly_cost_usd: metrics.cost_per_request * 1_000_000.0, // 1M requests/month
            cost_by_category,
            tokens_per_request: if token_count > 0 {
                total_tokens as f64 / token_count as f64
            } else {
                0.0
            },
        }
    }

    fn calculate_fallback_analysis(cases: &[crate::eval::EvaluationCase]) -> FallbackAnalysis {
        let mut fallback_by_category = HashMap::new();
        let mut fallback_by_error = HashMap::new();
        let mut total_fallbacks = 0;

        for case in cases {
            if case.fallback_triggered {
                total_fallbacks += 1;
                *fallback_by_category.entry(case.entry.category.clone()).or_insert(0) += 1;
                if let Some(ref error) = case.error {
                    *fallback_by_error.entry(error.clone()).or_insert(0) += 1;
                }
            }
        }

        FallbackAnalysis {
            total_fallbacks,
            fallback_rate: total_fallbacks as f64 / cases.len().max(1) as f64,
            fallback_by_category,
            fallback_by_error,
        }
    }

    pub async fn save(&self, output_dir: &std::path::Path) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(output_dir).await?;

        let json_path = output_dir.join(format!("report_{}.json", self.run_id));
        let json_content = serde_json::to_string_pretty(self)?;
        tokio::fs::write(&json_path, json_content).await?;

        let md_path = output_dir.join(format!("report_{}.md", self.run_id));
        let md_content = self.to_markdown();
        tokio::fs::write(&md_path, md_content).await?;

        let csv_path = output_dir.join(format!("report_{}.csv", self.run_id));
        let csv_content = self.to_csv()?;
        tokio::fs::write(&csv_path, csv_content).await?;

        Ok(())
    }

    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str(&format!("# Evaluation Report\n\n"));
        md.push_str(&format!("**Run ID:** {}\n", self.run_id));
        md.push_str(&format!("**Timestamp:** {}\n", self.timestamp));
        md.push_str(&format!("**Model:** {}\n", self.model));
        md.push_str(&format!("**Total Cases:** {}\n\n", self.total_cases));

        md.push_str("## Aggregate Metrics\n\n");
        md.push_str(&format!("| Metric | Value |\n"));
        md.push_str(&format!("|--------|-------|\n"));
        md.push_str(&format!("| Precision | {:.4} |\n", self.aggregate.precision));
        md.push_str(&format!("| Recall | {:.4} |\n", self.aggregate.recall));
        md.push_str(&format!("| F1 Score | {:.4} |\n", self.aggregate.f1));
        md.push_str(&format!(
            "| False Positive Rate | {:.4} |\n",
            self.aggregate.false_positive_rate
        ));
        md.push_str(&format!("| Latency P50 | {} ms |\n", self.aggregate.latency_p50));
        md.push_str(&format!("| Latency P95 | {} ms |\n", self.aggregate.latency_p95));
        md.push_str(&format!("| Latency P99 | {} ms |\n", self.aggregate.latency_p99));
        md.push_str(&format!("| Avg Latency | {:.1} ms |\n", self.aggregate.avg_latency));
        md.push_str(&format!("| Cost/Request | ${:.6} |\n", self.aggregate.cost_per_request));
        md.push_str(&format!("| Total Cost | ${:.4} |\n", self.aggregate.total_cost));
        md.push_str(&format!("| Fallback Rate | {:.2}% |\n", self.aggregate.fallback_rate * 100.0));
        md.push_str(&format!("| Success Rate | {:.2}% |\n\n", self.aggregate.success_rate * 100.0));

        md.push_str("## By Category\n\n");
        let cat_table = Table::new(&self.category_breakdown).to_string();
        md.push_str(&cat_table);
        md.push_str("\n\n");

        md.push_str("## By Difficulty\n\n");
        let diff_table = Table::new(&self.difficulty_breakdown).to_string();
        md.push_str(&diff_table);
        md.push_str("\n\n");

        md.push_str("## Latency Distribution\n\n");
        md.push_str(&format!("| Percentile | Latency (ms) |\n"));
        md.push_str(&format!("|------------|--------------|\n"));
        md.push_str(&format!("| Min | {} |\n", self.latency_distribution.min));
        md.push_str(&format!("| P10 | {} |\n", self.latency_distribution.p10));
        md.push_str(&format!("| P25 | {} |\n", self.latency_distribution.p25));
        md.push_str(&format!("| P50 | {} |\n", self.latency_distribution.p50));
        md.push_str(&format!("| P75 | {} |\n", self.latency_distribution.p75));
        md.push_str(&format!("| P90 | {} |\n", self.latency_distribution.p90));
        md.push_str(&format!("| P95 | {} |\n", self.latency_distribution.p95));
        md.push_str(&format!("| P99 | {} |\n", self.latency_distribution.p99));
        md.push_str(&format!("| Max | {} |\n", self.latency_distribution.max));
        md.push_str(&format!("| Mean | {:.1} |\n", self.latency_distribution.mean));
        md.push_str(&format!("| Std Dev | {:.1} |\n\n", self.latency_distribution.std_dev));

        md.push_str("## Cost Analysis\n\n");
        md.push_str(&format!("| Metric | Value |\n"));
        md.push_str(&format!("|--------|-------|\n"));
        md.push_str(&format!("| Total Cost | ${:.4} |\n", self.cost_analysis.total_cost_usd));
        md.push_str(&format!("| Cost/Request | ${:.6} |\n", self.cost_analysis.cost_per_request_usd));
        md.push_str(&format!(
            "| Est. Monthly (1M req) | ${:.2} |\n",
            self.cost_analysis.estimated_monthly_cost_usd
        ));
        md.push_str(&format!(
            "| Tokens/Request | {:.1} |\n\n",
            self.cost_analysis.tokens_per_request
        ));

        if !self.cost_analysis.cost_by_category.is_empty() {
            md.push_str("### Cost by Category\n\n");
            md.push_str(&format!("| Category | Cost |\n"));
            md.push_str(&format!("|----------|------|\n"));
            for (cat, cost) in &self.cost_analysis.cost_by_category {
                md.push_str(&format!("| {} | ${:.6} |\n", cat, cost));
            }
            md.push_str("\n");
        }

        md.push_str("## Fallback Analysis\n\n");
        md.push_str(&format!("| Metric | Value |\n"));
        md.push_str(&format!("|--------|-------|\n"));
        md.push_str(&format!("| Total Fallbacks | {} |\n", self.fallback_analysis.total_fallbacks));
        md.push_str(&format!(
            "| Fallback Rate | {:.2}% |\n\n",
            self.fallback_analysis.fallback_rate * 100.0
        ));

        if !self.fallback_analysis.fallback_by_category.is_empty() {
            md.push_str("### Fallbacks by Category\n\n");
            md.push_str(&format!("| Category | Count |\n"));
            md.push_str(&format!("|----------|-------|\n"));
            for (cat, count) in &self.fallback_analysis.fallback_by_category {
                md.push_str(&format!("| {} | {} |\n", cat, count));
            }
            md.push_str("\n");
        }

        if !self.fallback_analysis.fallback_by_error.is_empty() {
            md.push_str("### Fallbacks by Error\n\n");
            md.push_str(&format!("| Error | Count |\n"));
            md.push_str(&format!("|-------|-------|\n"));
            for (error, count) in &self.fallback_analysis.fallback_by_error {
                md.push_str(&format!("| {} | {} |\n", error, count));
            }
            md.push_str("\n");
        }

        md
    }

    pub fn to_csv(&self) -> anyhow::Result<String> {
        let mut wtr = csv::Writer::from_writer(vec![]);

        for case in &self.category_breakdown {
            wtr.serialize(case)?;
        }

        Ok(String::from_utf8(wtr.into_inner()?)?)
    }

    pub fn summary(&self) -> String {
        format!(
            "F1: {:.2}% | FPR: {:.2}% | Latency P95: {}ms | Cost: ${:.6}/req | Fallback: {:.2}% | Success: {:.2}%",
            self.aggregate.f1 * 100.0,
            self.aggregate.false_positive_rate * 100.0,
            self.aggregate.latency_p95,
            self.aggregate.cost_per_request,
            self.aggregate.fallback_rate * 100.0,
            self.aggregate.success_rate * 100.0
        )
    }
}

impl GateResult {
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str(&format!("# Regression Gate Result\n\n"));
        md.push_str(&format!(
            "**Status:** {}\n",
            if self.passed { "✅ PASSED" } else { "❌ FAILED" }
        ));
        md.push_str(&format!("**Timestamp:** {}\n", self.timestamp));
        md.push_str(&format!("**Model:** {}\n", self.model));
        md.push_str(&format!("**Summary:** {}\n\n", self.summary));

        md.push_str("## Checks\n\n");
        let table = Table::new(&self.checks).to_string();
        md.push_str(&table);
        md.push_str("\n");

        md
    }
}

impl Default for MetricsReport {
    fn default() -> Self {
        Self {
            run_id: String::new(),
            timestamp: String::new(),
            model: String::new(),
            total_cases: 0,
            aggregate: AggregateMetrics {
                precision: 0.0,
                recall: 0.0,
                f1: 0.0,
                false_positive_rate: 0.0,
                latency_p50: 0,
                latency_p95: 0,
                latency_p99: 0,
                avg_latency: 0.0,
                cost_per_request: 0.0,
                total_cost: 0.0,
                fallback_rate: 0.0,
                success_rate: 0.0,
            },
            category_breakdown: Vec::new(),
            difficulty_breakdown: Vec::new(),
            latency_distribution: LatencyDistribution {
                min: 0,
                max: 0,
                mean: 0.0,
                std_dev: 0.0,
                p10: 0,
                p25: 0,
                p50: 0,
                p75: 0,
                p90: 0,
                p95: 0,
                p99: 0,
            },
            cost_analysis: CostAnalysis {
                total_cost_usd: 0.0,
                cost_per_request_usd: 0.0,
                estimated_monthly_cost_usd: 0.0,
                cost_by_category: HashMap::new(),
                tokens_per_request: 0.0,
            },
            fallback_analysis: FallbackAnalysis {
                total_fallbacks: 0,
                fallback_rate: 0.0,
                fallback_by_category: HashMap::new(),
                fallback_by_error: HashMap::new(),
            },
        }
    }
}

impl MetricsReport {
    pub fn check_gate(
        &self,
        threshold_f1: f64,
        threshold_fpr: f64,
        threshold_latency_p95: u64,
        threshold_cost: f64,
        threshold_fallback: f64,
        threshold_attack_success: f64,
    ) -> GateResult {
        let mut checks = Vec::new();
        let mut all_passed = true;

        let f1_passed = self.aggregate.f1 >= threshold_f1;
        checks.push(GateCheck {
            name: "F1 Score".to_string(),
            threshold: format!("≥ {:.2}", threshold_f1),
            actual: format!("{:.4}", self.aggregate.f1),
            status: if f1_passed { "✅ PASS" } else { "❌ FAIL" }.to_string(),
        });
        all_passed &= f1_passed;

        let fpr_passed = self.aggregate.false_positive_rate <= threshold_fpr;
        checks.push(GateCheck {
            name: "False Positive Rate".to_string(),
            threshold: format!("≤ {:.2}", threshold_fpr),
            actual: format!("{:.4}", self.aggregate.false_positive_rate),
            status: if fpr_passed { "✅ PASS" } else { "❌ FAIL" }.to_string(),
        });
        all_passed &= fpr_passed;

        let latency_passed = self.aggregate.latency_p95 <= threshold_latency_p95;
        checks.push(GateCheck {
            name: "Latency P95".to_string(),
            threshold: format!("≤ {} ms", threshold_latency_p95),
            actual: format!("{} ms", self.aggregate.latency_p95),
            status: if latency_passed { "✅ PASS" } else { "❌ FAIL" }.to_string(),
        });
        all_passed &= latency_passed;

        let cost_passed = self.aggregate.cost_per_request <= threshold_cost;
        checks.push(GateCheck {
            name: "Cost per Request".to_string(),
            threshold: format!("≤ ${:.4}", threshold_cost),
            actual: format!("${:.6}", self.aggregate.cost_per_request),
            status: if cost_passed { "✅ PASS" } else { "❌ FAIL" }.to_string(),
        });
        all_passed &= cost_passed;

        let fallback_passed = self.aggregate.fallback_rate <= threshold_fallback;
        checks.push(GateCheck {
            name: "Fallback Rate".to_string(),
            threshold: format!("≤ {:.2}", threshold_fallback),
            actual: format!("{:.4}", self.aggregate.fallback_rate),
            status: if fallback_passed { "✅ PASS" } else { "❌ FAIL" }.to_string(),
        });
        all_passed &= fallback_passed;

        let attack_passed = self.aggregate.false_positive_rate <= threshold_attack_success;
        checks.push(GateCheck {
            name: "Attack Success Rate".to_string(),
            threshold: format!("≤ {:.2}", threshold_attack_success),
            actual: format!("{:.4}", self.aggregate.false_positive_rate),
            status: if attack_passed { "✅ PASS" } else { "❌ FAIL" }.to_string(),
        });
        all_passed &= attack_passed;

        let summary = if all_passed {
            "All checks passed".to_string()
        } else {
            format!("{} checks failed", checks.iter().filter(|c| c.status.contains("FAIL")).count())
        };

        GateResult {
            passed: all_passed,
            timestamp: chrono::Utc::now().to_rfc3339(),
            model: self.model.clone(),
            checks,
            summary,
        }
    }
}
