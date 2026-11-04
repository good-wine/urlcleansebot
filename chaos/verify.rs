//! Chaos Verification Binary - Comprehensive post-chaos validation
//! Run with: cargo run --bin chaos_verify -- [scenario]

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VerificationConfig {
    health_endpoint: String,
    metrics_endpoint: String,
    alertmanager_url: Option<String>,
    slo: SloConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SloConfig {
    availability_target: f64,
    max_error_rate: f64,
    max_p99_latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VerificationResult {
    scenario: String,
    timestamp: String,
    passed: bool,
    checks: Vec<CheckResult>,
    duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CheckResult {
    name: String,
    passed: bool,
    message: String,
    details: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct HealthResponse {
    status: String,
    #[serde(default)]
    checks: HashMap<String, serde_json::Value>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();
    let scenario = args.get(1).map(|s| s.as_str()).unwrap_or("all");

    let config = VerificationConfig {
        health_endpoint: std::env::var("HEALTH_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:3000/health".to_string()),
        metrics_endpoint: std::env::var("METRICS_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:3000/metrics".to_string()),
        alertmanager_url: std::env::var("ALERTMANAGER_URL").ok(),
        slo: SloConfig { availability_target: 0.995, max_error_rate: 0.05, max_p99_latency_ms: 5000 },
    };

    let client = Client::builder().timeout(Duration::from_secs(10)).build()?;

    let start = Instant::now();
    let mut results = Vec::new();

    match scenario {
        "db_latency" => results.extend(verify_db_latency(&client, &config).await),
        "db_unavailable" => results.extend(verify_db_unavailable(&client, &config).await),
        "ai_failure" => results.extend(verify_ai_failure(&client, &config).await),
        "rule_fetch_failure" => results.extend(verify_rule_fetch_failure(&client, &config).await),
        "network_partition" => results.extend(verify_network_partition(&client, &config).await),
        "memory_pressure" => results.extend(verify_memory_pressure(&client, &config).await),
        "cpu_throttling" => results.extend(verify_cpu_throttling(&client, &config).await),
        "disk_full" => results.extend(verify_disk_full(&client, &config).await),
        "telegram_flood" => results.extend(verify_telegram_flood(&client, &config).await),
        "all" => {
            results.extend(verify_db_latency(&client, &config).await);
            results.extend(verify_db_unavailable(&client, &config).await);
            results.extend(verify_ai_failure(&client, &config).await);
            results.extend(verify_rule_fetch_failure(&client, &config).await);
            results.extend(verify_network_partition(&client, &config).await);
            results.extend(verify_memory_pressure(&client, &config).await);
            results.extend(verify_cpu_throttling(&client, &config).await);
            results.extend(verify_disk_full(&client, &config).await);
            results.extend(verify_telegram_flood(&client, &config).await);
        },
        _ => {
            error!("Unknown scenario: {}", scenario);
            std::process::exit(1);
        },
    }

    let duration = start.elapsed().as_millis() as u64;
    let all_passed = results.iter().all(|r| r.passed);

    let report = VerificationResult {
        scenario: scenario.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        passed: all_passed,
        checks: results,
        duration_ms: duration,
    };

    // Output JSON report
    let json = serde_json::to_string_pretty(&report)?;
    println!("{}", json);

    // Write to file
    let report_dir = std::env::var("REPORT_DIR").unwrap_or_else(|_| "/tmp/chaos-reports".to_string());
    std::fs::create_dir_all(&report_dir)?;
    let report_file = format!(
        "{}/chaos_verify_{}_{}.json",
        report_dir,
        scenario,
        chrono::Utc::now().timestamp()
    );
    std::fs::write(&report_file, &json)?;
    info!("Report written to {}", report_file);

    if !all_passed {
        std::process::exit(1);
    }

    Ok(())
}

async fn check_health(client: &Client, url: &str) -> CheckResult {
    match client.get(url).send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                match resp.json::<HealthResponse>().await {
                    Ok(health) => CheckResult {
                        name: "health_endpoint".to_string(),
                        passed: true,
                        message: format!("Health check OK: {}", health.status),
                        details: serde_json::to_value(health).ok(),
                    },
                    Err(e) => CheckResult {
                        name: "health_endpoint".to_string(),
                        passed: false,
                        message: format!("Health check returned 200 but invalid JSON: {}", e),
                        details: None,
                    },
                }
            } else {
                CheckResult {
                    name: "health_endpoint".to_string(),
                    passed: false,
                    message: format!("Health check returned {}", status),
                    details: None,
                }
            }
        },
        Err(e) => CheckResult {
            name: "health_endpoint".to_string(),
            passed: false,
            message: format!("Health check failed: {}", e),
            details: None,
        },
    }
}

async fn check_metrics(client: &Client, url: &str) -> CheckResult {
    match client.get(url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                CheckResult {
                    name: "metrics_endpoint".to_string(),
                    passed: true,
                    message: "Metrics endpoint accessible".to_string(),
                    details: None,
                }
            } else {
                CheckResult {
                    name: "metrics_endpoint".to_string(),
                    passed: false,
                    message: format!("Metrics endpoint returned {}", resp.status()),
                    details: None,
                }
            }
        },
        Err(e) => CheckResult {
            name: "metrics_endpoint".to_string(),
            passed: false,
            message: format!("Metrics check failed: {}", e),
            details: None,
        },
    }
}

async fn parse_metrics(client: &Client, url: &str) -> Result<(f64, u64), Box<dyn std::error::Error>> {
    let text = client.get(url).send().await?.text().await?;

    let mut total_requests = 0.0;
    let mut error_requests = 0.0;
    let mut p99_latency_ms = 0u64;

    for line in text.lines() {
        if line.starts_with("urlcleansebot_requests_total") {
            if let Some(value_str) = line.split_whitespace().nth(1) {
                if let Ok(value) = value_str.parse::<f64>() {
                    total_requests += value;
                    if line.contains("status=\"5") || line.contains("status=\"429") || line.contains("status=\"timeout")
                    {
                        error_requests += value;
                    }
                }
            }
        } else if line.starts_with("urlcleansebot_request_duration_seconds_bucket") && line.contains("le=\"5\"") {
            if let Some(value_str) = line.split_whitespace().nth(1) {
                if let Ok(value) = value_str.parse::<f64>() {
                    p99_latency_ms = (value * 1000.0) as u64;
                }
            }
        }
    }

    let error_rate = if total_requests > 0.0 {
        error_requests / total_requests
    } else {
        0.0
    };
    Ok((error_rate, p99_latency_ms))
}

async fn check_error_rate(client: &Client, config: &VerificationConfig) -> CheckResult {
    match parse_metrics(client, &config.metrics_endpoint).await {
        Ok((error_rate, _)) => {
            let passed = error_rate <= config.slo.max_error_rate;
            CheckResult {
                name: "error_rate".to_string(),
                passed,
                message: format!(
                    "Error rate: {:.2}% (SLO: < {:.1}%)",
                    error_rate * 100.0,
                    config.slo.max_error_rate * 100.0
                ),
                details: Some(serde_json::json!({"error_rate": error_rate, "threshold": config.slo.max_error_rate})),
            }
        },
        Err(e) => CheckResult {
            name: "error_rate".to_string(),
            passed: false,
            message: format!("Failed to parse metrics: {}", e),
            details: None,
        },
    }
}

async fn check_latency(client: &Client, config: &VerificationConfig) -> CheckResult {
    match parse_metrics(client, &config.metrics_endpoint).await {
        Ok((_, p99_ms)) => {
            let passed = p99_ms <= config.slo.max_p99_latency_ms;
            CheckResult {
                name: "p99_latency".to_string(),
                passed,
                message: format!("P99 latency: {}ms (SLO: < {}ms)", p99_ms, config.slo.max_p99_latency_ms),
                details: Some(serde_json::json!({"p99_ms": p99_ms, "threshold_ms": config.slo.max_p99_latency_ms})),
            }
        },
        Err(e) => CheckResult {
            name: "p99_latency".to_string(),
            passed: false,
            message: format!("Failed to parse metrics: {}", e),
            details: None,
        },
    }
}

async fn check_alertmanager(client: &Client, config: &VerificationConfig) -> CheckResult {
    let Some(url) = &config.alertmanager_url else {
        return CheckResult {
            name: "alertmanager".to_string(),
            passed: true,
            message: "Alertmanager URL not configured, skipping".to_string(),
            details: None,
        };
    };

    match client.get(&format!("{}/api/v2/status", url)).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                CheckResult {
                    name: "alertmanager".to_string(),
                    passed: true,
                    message: "Alertmanager reachable".to_string(),
                    details: None,
                }
            } else {
                CheckResult {
                    name: "alertmanager".to_string(),
                    passed: false,
                    message: format!("Alertmanager returned {}", resp.status()),
                    details: None,
                }
            }
        },
        Err(e) => CheckResult {
            name: "alertmanager".to_string(),
            passed: false,
            message: format!("Alertmanager unreachable: {}", e),
            details: None,
        },
    }
}

// Scenario-specific verifications

async fn verify_db_latency(client: &Client, config: &VerificationConfig) -> Vec<CheckResult> {
    info!("Verifying DB Latency scenario");
    vec![
        check_health(client, &config.health_endpoint).await,
        check_metrics(client, &config.metrics_endpoint).await,
        check_error_rate(client, config).await,
        check_latency(client, config).await,
        check_alertmanager(client, config).await,
    ]
}

async fn verify_db_unavailable(client: &Client, config: &VerificationConfig) -> Vec<CheckResult> {
    info!("Verifying DB Unavailable scenario");
    vec![
        check_health(client, &config.health_endpoint).await,
        check_metrics(client, &config.metrics_endpoint).await,
        check_alertmanager(client, config).await,
    ]
}

async fn verify_ai_failure(client: &Client, config: &VerificationConfig) -> Vec<CheckResult> {
    info!("Verifying AI API Failure scenario");
    vec![
        check_health(client, &config.health_endpoint).await,
        check_metrics(client, &config.metrics_endpoint).await,
        check_error_rate(client, config).await,
        check_latency(client, config).await,
        check_alertmanager(client, config).await,
    ]
}

async fn verify_rule_fetch_failure(client: &Client, config: &VerificationConfig) -> Vec<CheckResult> {
    info!("Verifying Rule Fetch Failure scenario");
    vec![
        check_health(client, &config.health_endpoint).await,
        check_metrics(client, &config.metrics_endpoint).await,
        check_error_rate(client, config).await,
        check_alertmanager(client, config).await,
    ]
}

async fn verify_network_partition(client: &Client, config: &VerificationConfig) -> Vec<CheckResult> {
    info!("Verifying Network Partition scenario");
    vec![
        check_health(client, &config.health_endpoint).await,
        check_metrics(client, &config.metrics_endpoint).await,
        check_alertmanager(client, config).await,
    ]
}

async fn verify_memory_pressure(client: &Client, config: &VerificationConfig) -> Vec<CheckResult> {
    info!("Verifying Memory Pressure scenario");
    vec![
        check_health(client, &config.health_endpoint).await,
        check_metrics(client, &config.metrics_endpoint).await,
        check_error_rate(client, config).await,
        check_latency(client, config).await,
        check_alertmanager(client, config).await,
    ]
}

async fn verify_cpu_throttling(client: &Client, config: &VerificationConfig) -> Vec<CheckResult> {
    info!("Verifying CPU Throttling scenario");
    vec![
        check_health(client, &config.health_endpoint).await,
        check_metrics(client, &config.metrics_endpoint).await,
        check_error_rate(client, config).await,
        check_latency(client, config).await,
        check_alertmanager(client, config).await,
    ]
}

async fn verify_disk_full(client: &Client, config: &VerificationConfig) -> Vec<CheckResult> {
    info!("Verifying Disk Full scenario");
    vec![
        check_health(client, &config.health_endpoint).await,
        check_metrics(client, &config.metrics_endpoint).await,
        check_error_rate(client, config).await,
        check_alertmanager(client, config).await,
    ]
}

async fn verify_telegram_flood(client: &Client, config: &VerificationConfig) -> Vec<CheckResult> {
    info!("Verifying Telegram FloodWait scenario");
    vec![
        check_health(client, &config.health_endpoint).await,
        check_metrics(client, &config.metrics_endpoint).await,
        check_error_rate(client, config).await,
        check_latency(client, config).await,
        check_alertmanager(client, config).await,
    ]
}
