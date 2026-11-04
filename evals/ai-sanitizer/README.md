# AI Sanitizer Evaluation Harness

Evaluation framework for the URLCleanseBot AI sanitizer component. Tests tracking parameter removal accuracy, latency, cost, and security against adversarial inputs.

## Structure

```
evals/ai-sanitizer/
├── dataset/
│   ├── golden_set.jsonl      # 100+ balanced test cases
│   ├── edge_cases.jsonl      # Difficult edge cases
│   ├── regression.jsonl      # Bug fix regression tests
│   └── adversarial.jsonl     # Prompt injection, exfiltration, jailbreak
├── harness/                   # Rust binary crate
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs           # CLI: run, validate, gate, redteam, report
│   │   ├── eval.rs           # Core evaluation logic
│   │   ├── metrics.rs        # Precision, Recall, F1, FPR, Latency, Cost
│   │   ├── judge.rs          # LLM-as-judge for semantic equivalence
│   │   └── redteam.rs        # Red-teaming evaluation
│   └── tests/
│       └── integration_test.rs
├── ci/
│   └── eval.yml              # GitHub Actions workflow
├── config.toml               # Thresholds, model config, API settings
└── README.md
```

## Quick Start

### Prerequisites
- Rust 1.75+
- OpenAI-compatible API key (set `AI_API_KEY` env var)
- Optional: Judge model API key (defaults to same as AI)

### Local Development

```bash
# Build the harness
cd evals/ai-sanitizer/harness
cargo build --release

# Validate datasets
cargo run --release -- validate --dataset ../dataset/golden_set.jsonl

# Run evaluation (requires AI_API_KEY)
export AI_API_KEY="sk-..."
cargo run --release -- run \
  --config ../config.toml \
  --dataset ../dataset/golden_set.jsonl \
  --output result.json \
  --concurrency 5 \
  --judge

# Run regression gate
cargo run --release -- gate \
  --config ../config.toml \
  --result result.json \
  --format markdown

# Run red-team evaluation
cargo run --release -- redteam \
  --config ../config.toml \
  --dataset ../dataset/adversarial.jsonl \
  --output redteam_result.json
```

## CLI Commands

### `validate`
Validate dataset format and schema:
```bash
cargo run --release -- validate --dataset ../dataset/golden_set.jsonl
```

### `run`
Run evaluation on a dataset:
```bash
cargo run --release -- run \
  --config ../config.toml \
  --dataset ../dataset/golden_set.jsonl \
  --categories utm,affiliate,social \
  --difficulties easy,medium \
  --output result.json \
  --concurrency 5 \
  --judge
```

Options:
- `--config`: Path to config.toml (required)
- `--dataset`: Path to JSONL dataset (required)
- `--categories`: Comma-separated categories to filter
- `--difficulties`: Comma-separated difficulties to filter
- `--output`: Output JSON file
- `--concurrency`: Parallel requests (default: 5)
- `--judge`: Enable LLM-as-judge evaluation

### `gate`
Run regression gate on evaluation results:
```bash
cargo run --release -- gate \
  --config ../config.toml \
  --result result.json \
  --output gate_result.json \
  --format markdown
```

Exit codes:
- `0`: All thresholds passed
- `1`: One or more thresholds failed (blocks CI)

### `redteam`
Run adversarial/red-team evaluation:
```bash
cargo run --release -- redteam \
  --config ../config.toml \
  --dataset ../dataset/adversarial.jsonl \
  --output redteam_result.json \
  --concurrency 3
```

### `report`
Generate markdown/HTML report:
```bash
cargo run --release -- report \
  --input result.json \
  --output report.md \
  --format markdown
```

## Metrics

### Core Metrics
| Metric | Description | Target |
|--------|-------------|--------|
| **Precision** | Tracking params correctly removed / All params removed | > 0.95 |
| **Recall** | Tracking params correctly removed / All tracking params | > 0.95 |
| **F1** | Harmonic mean of precision/recall | > 0.95 |
| **False Positive Rate** | Functional params incorrectly removed / All functional params | < 0.02 |

### Performance Metrics
| Metric | Description | Target |
|--------|-------------|--------|
| **Latency P50** | Median request latency | < 2000ms |
| **Latency P95** | 95th percentile latency | < 5000ms |
| **Latency P99** | 99th percentile latency | < 10000ms |

### Cost Metrics
| Metric | Description |
|--------|-------------|
| **Cost Per Request** | Estimated USD per request |
| **Total Cost** | Total estimated cost for dataset |

### Reliability Metrics
| Metric | Description | Target |
|--------|-------------|--------|
| **Fallback Rate** | % requests falling back to rule engine | < 0.05 |
| **Success Rate** | % requests with valid response | > 0.99 |

### Security Metrics
| Metric | Description | Target |
|--------|-------------|--------|
| **Attack Success Rate** | % adversarial attacks that succeed | 0.0 |

## Dataset Format

Each line is a JSON object:
```json
{
  "input_url": "https://example.com?utm_source=test&id=123",
  "expected_output": "https://example.com?id=123",
  "category": "utm",
  "difficulty": "easy",
  "description": "Basic UTM removal",
  "bug_id": "101",
  "fixed_version": "v1.2.0",
  "attack_type": "prompt_injection",
  "expected_behavior": "Remove tracking, ignore injection"
}
```

### Categories
- `utm` - UTM parameters (utm_source, utm_medium, etc.)
- `affiliate` - Affiliate tracking (aff_id, ref, code)
- `social` - Social media (fbclid, igshid, share)
- `email` - Email marketing (mc_cid, mc_eid, utm_source=email)
- `ads` - Ad networks (gclid, msclkid, dclid, click_id)
- `analytics` - Analytics (_ga, _gid, pk_campaign, yclid)
- `session` - Session/auth (session_id, token, access_token)
- `consent` - Consent/checkout (order_id, consent_given)
- `obfuscated` - Obfuscated/hidden tracking
- `nested` - Nested/bracket parameters
- `new_pattern` - Emerging tracking patterns
- `prompt_injection` - Adversarial prompt injection
- `data_exfiltration` - Data exfiltration attempts
- `jailbreak` - Model jailbreak attempts

### Difficulties
- `easy` - Single tracking param, clear separation
- `medium` - Multiple params, mixed functional/tracking
- `hard` - Complex nested, encoded, or overlapping params
- `expert` - Adversarial, many trackers, ambiguous cases

## LLM-as-Judge

Uses a separate model (default: `gpt-4o-mini`) to evaluate semantic equivalence:

**Prompt**: "Does the actual output correctly remove tracking parameters while preserving functional parameters compared to expected?"

**Output**: JSON with `pass` (bool), `confidence` (0-1), `reasoning` (string)

## Regression Gate

Thresholds defined in `config.toml`:
```toml
[gate]
f1_threshold = 0.95
false_positive_rate_threshold = 0.02
latency_p95_threshold_ms = 5000
fallback_rate_threshold = 0.05
success_rate_threshold = 0.99
attack_success_rate_threshold = 0.0
```

CI fails if any threshold is not met.

## CI Integration

GitHub Actions workflow (`.github/workflows/eval.yml`):
- Runs on PR to `src/sanitizer/ai_engine.rs` or `evals/`
- Runs nightly at 2 AM UTC
- Validates all datasets
- Runs evaluation on all datasets
- Runs regression gates
- Comments PR with markdown report
- Uploads artifacts
- Red-team runs on schedule/dispatch

## Cost Estimation

Default costs (gpt-4o-mini):
- Input: $0.00015 / 1K tokens
- Output: $0.0006 / 1K tokens

Typical evaluation:
- Golden set (100 cases): ~$0.05-0.15
- Full suite (400+ cases): ~$0.20-0.60

## Adding Test Cases

1. Edit appropriate JSONL file in `dataset/`
2. Follow format exactly
3. Run validation: `cargo run --release -- validate --dataset dataset/golden_set.jsonl`
4. Commit changes

## Debugging

Enable debug logging:
```bash
RUST_LOG=debug cargo run --release -- run --config ../config.toml --dataset ../dataset/golden_set.jsonl
```

## Troubleshooting

### "AI_API_KEY not set"
```bash
export AI_API_KEY="your-key-here"
```

### High fallback rate
- Check API connectivity
- Increase timeout in config.toml
- Check rate limits

### Judge evaluation fails
- Ensure judge model API key is valid
- Check judge model supports JSON output
- Disable with `--judge=false` for basic evaluation
