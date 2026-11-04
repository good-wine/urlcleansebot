// Integration tests for the eval harness binary
// These tests exercise the CLI commands through the binary

use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn get_binary_path() -> std::path::PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    Path::new(&manifest_dir)
        .join("target")
        .join(profile)
        .join("eval-harness")
}

fn run_harness(args: &[&str]) -> std::process::Output {
    let binary = get_binary_path();
    Command::new(&binary)
        .args(args)
        .current_dir(Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join(".."))
        .output()
        .expect("Failed to execute harness binary")
}

#[test]
fn test_validate_command_valid_dataset() {
    let tmpdir = tempdir().unwrap();
    let dataset_path = tmpdir.path().join("valid.jsonl");
    std::fs::write(&dataset_path, r#"{"input_url": "https://example.com?utm_source=test", "expected_output": "https://example.com", "category": "utm", "difficulty": "easy", "description": "test"}"#).unwrap();

    let output = run_harness(&["validate", "--dataset", dataset_path.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "Validate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_validate_command_invalid_dataset() {
    let tmpdir = tempdir().unwrap();
    let dataset_path = tmpdir.path().join("invalid.jsonl");
    std::fs::write(&dataset_path, r#"{"input_url": "", "expected_output": "https://example.com", "category": "utm", "difficulty": "easy", "description": "test"}"#).unwrap();

    let output = run_harness(&["validate", "--dataset", dataset_path.to_str().unwrap()]);
    assert!(!output.status.success(), "Validate should fail for invalid dataset");
}

#[test]
fn test_validate_command_invalid_category() {
    let tmpdir = tempdir().unwrap();
    let dataset_path = tmpdir.path().join("invalid_cat.jsonl");
    std::fs::write(&dataset_path, r#"{"input_url": "https://example.com?utm_source=test", "expected_output": "https://example.com", "category": "invalid", "difficulty": "easy", "description": "test"}"#).unwrap();

    let output = run_harness(&["validate", "--dataset", dataset_path.to_str().unwrap()]);
    assert!(!output.status.success(), "Validate should fail for invalid category");
}

#[test]
fn test_help_command() {
    let output = run_harness(&["--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("eval-harness"));
    assert!(stdout.contains("validate"));
    assert!(stdout.contains("run"));
    assert!(stdout.contains("gate"));
    assert!(stdout.contains("redteam"));
    assert!(stdout.contains("report"));
}

#[test]
fn test_run_command_help() {
    let output = run_harness(&["run", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--dataset"));
    assert!(stdout.contains("--output"));
    assert!(stdout.contains("--concurrency"));
    assert!(stdout.contains("--judge"));
}

#[test]
fn test_gate_command_help() {
    let output = run_harness(&["gate", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--dataset"));
    assert!(stdout.contains("--output"));
    assert!(stdout.contains("--format"));
}

#[test]
fn test_redteam_command_help() {
    let output = run_harness(&["redteam", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--dataset"));
    assert!(stdout.contains("--output"));
    assert!(stdout.contains("--concurrency"));
}

#[test]
fn test_report_command_help() {
    let output = run_harness(&["report", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--input"));
    assert!(stdout.contains("--output"));
    assert!(stdout.contains("--format"));
}

#[test]
fn test_gate_threshold_flags() {
    let output = run_harness(&["gate", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Check for threshold flags (actual names from CLI)
    assert!(stdout.contains("--threshold-f1"));
    assert!(stdout.contains("--threshold-fpr"));
    assert!(stdout.contains("--threshold-latency-p95"));
    assert!(stdout.contains("--threshold-fallback"));
    assert!(stdout.contains("--threshold-cost"));
}
