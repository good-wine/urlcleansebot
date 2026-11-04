use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod eval;
mod judge;
mod metrics;
mod redteam;

use eval::{EvaluationConfig, EvaluationHarness, EvaluationResult};
use judge::JudgeClient;
use metrics::MetricsReport;
use redteam::RedTeamHarness;

#[derive(Parser)]
#[command(name = "eval-harness")]
#[command(about = "AI Evaluation Harness for URLCleanseBot AI Sanitizer")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run evaluation on a dataset
    Run {
        /// Dataset file to evaluate
        #[arg(short, long, default_value = "../../dataset/golden_set.jsonl")]
        dataset: PathBuf,

        /// Configuration file
        #[arg(short, long, default_value = "../../config.toml")]
        config: PathBuf,

        /// Output directory for reports
        #[arg(short, long, default_value = "./reports")]
        output: PathBuf,

        /// Run only specific categories (comma-separated)
        #[arg(long)]
        categories: Option<String>,

        /// Run only specific difficulties (comma-separated)
        #[arg(long)]
        difficulties: Option<String>,

        /// Maximum concurrent requests
        #[arg(long, default_value = "10")]
        concurrency: usize,

        /// Enable LLM-as-judge for semantic equivalence
        #[arg(long)]
        judge: bool,
    },

    /// Run regression gate (exit code 1 if thresholds not met)
    Gate {
        /// Dataset file to evaluate
        #[arg(short, long, default_value = "../../dataset/golden_set.jsonl")]
        dataset: PathBuf,

        /// Configuration file
        #[arg(short, long, default_value = "../../config.toml")]
        config: PathBuf,

        /// Minimum F1 score threshold
        #[arg(long, default_value = "0.95")]
        threshold_f1: f64,

        /// Maximum False Positive Rate threshold
        #[arg(long, default_value = "0.02")]
        threshold_fpr: f64,

        /// Maximum latency P95 threshold (ms)
        #[arg(long, default_value = "5000")]
        threshold_latency_p95: u64,

        /// Maximum cost per request threshold (USD)
        #[arg(long, default_value = "0.01")]
        threshold_cost: f64,

        /// Maximum fallback rate threshold
        #[arg(long, default_value = "0.05")]
        threshold_fallback: f64,

        /// Maximum attack success rate for red-teaming
        #[arg(long, default_value = "0.0")]
        threshold_attack_success: f64,

        /// Output format (json, markdown, both)
        #[arg(long, default_value = "both")]
        format: String,

        /// Output file for gate result
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Run red-teaming evaluation
    Redteam {
        /// Adversarial dataset file
        #[arg(short, long, default_value = "../../dataset/adversarial.jsonl")]
        dataset: PathBuf,

        /// Configuration file
        #[arg(short, long, default_value = "../../config.toml")]
        config: PathBuf,

        /// Output directory for reports
        #[arg(short, long, default_value = "./reports")]
        output: PathBuf,

        /// Maximum concurrent requests
        #[arg(long, default_value = "5")]
        concurrency: usize,
    },

    /// Generate evaluation report from previous run
    Report {
        /// Input results file (JSON)
        #[arg(short, long)]
        input: PathBuf,

        /// Output directory
        #[arg(short, long, default_value = "./reports")]
        output: PathBuf,

        /// Output format (json, markdown, both)
        #[arg(long, default_value = "both")]
        format: String,
    },

    /// Validate dataset format
    Validate {
        /// Dataset file to validate
        #[arg(short, long)]
        dataset: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run { dataset, config, output, categories, difficulties, concurrency, judge } => {
            let config = EvaluationConfig::load(&config).await?;
            let mut harness = EvaluationHarness::new(config, concurrency, judge).await?;
            let result = harness.run(&dataset, categories, difficulties).await?;

            let report = MetricsReport::from_result(&result);
            report.save(&output).await?;

            println!("Evaluation completed. Results saved to {}", output.display());
            println!("{}", report.summary());
        },

        Commands::Gate {
            dataset,
            config,
            threshold_f1,
            threshold_fpr,
            threshold_latency_p95,
            threshold_cost,
            threshold_fallback,
            threshold_attack_success,
            format,
            output,
        } => {
            let config = EvaluationConfig::load(&config).await?;
            let mut harness = EvaluationHarness::new(config, 10, true).await?;
            let result = harness.run(&dataset, None, None).await?;

            let report = MetricsReport::from_result(&result);
            let gate_result = report.check_gate(
                threshold_f1,
                threshold_fpr,
                threshold_latency_p95,
                threshold_cost,
                threshold_fallback,
                threshold_attack_success,
            );

            let output_content = match format.as_str() {
                "json" => serde_json::to_string_pretty(&gate_result)?,
                "markdown" => gate_result.to_markdown(),
                "both" => format!(
                    "{}\n\n{}",
                    serde_json::to_string_pretty(&gate_result)?,
                    gate_result.to_markdown()
                ),
                _ => anyhow::bail!("Invalid format: {}", format),
            };

            if let Some(output_path) = output {
                tokio::fs::write(&output_path, &output_content).await?;
                println!("Gate result saved to {}", output_path.display());
            } else {
                println!("{}", output_content);
            }

            if !gate_result.passed {
                eprintln!("❌ GATE FAILED: {}", gate_result.summary);
                std::process::exit(1);
            } else {
                println!("✅ GATE PASSED: {}", gate_result.summary);
            }
        },

        Commands::Redteam { dataset, config, output, concurrency } => {
            let config = EvaluationConfig::load(&config).await?;
            let mut redteam = RedTeamHarness::new(config, concurrency).await?;
            let result = redteam.run(&dataset).await?;

            let report = MetricsReport::from_redteam_result(&result);
            report.save(&output).await?;

            println!("Red-teaming completed. Results saved to {}", output.display());
            println!("{}", report.summary());

            if result.attack_success_rate > 0.0 {
                eprintln!(
                    "❌ RED-TEAMING FAILED: Attack success rate {:.2}% > 0%",
                    result.attack_success_rate * 100.0
                );
                std::process::exit(1);
            } else {
                println!("✅ RED-TEAMING PASSED: No successful attacks");
            }
        },

        Commands::Report { input, output, format } => {
            let result: EvaluationResult = serde_json::from_str(&tokio::fs::read_to_string(&input).await?)?;
            let report = MetricsReport::from_result(&result);
            report.save(&output).await?;
            println!("Report generated at {}", output.display());
        },

        Commands::Validate { dataset } => {
            eval::validate_dataset(&dataset).await?;
            println!("✅ Dataset validation passed");
        },
    }

    Ok(())
}
