# Chaos Engineering Suite for URLCleanseBot

This directory contains the chaos engineering test suite for validating the resilience of URLCleanseBot running as a single-instance VPS deployment with Docker Compose.

## Directory Structure

```
chaos/
├── chaos.toml              # Scenario configurations and SLO thresholds
├── verify.rs               # Rust-based verification binary
├── scripts/
│   ├── common.sh           # Shared utilities (source this in other scripts)
│   ├── inject_db_latency.sh
│   ├── inject_db_unavailable.sh
│   ├── inject_ai_failure.sh
│   ├── inject_rule_fetch_failure.sh
│   ├── inject_network_partition.sh
│   ├── inject_memory_pressure.sh
│   ├── inject_cpu_throttling.sh
│   ├── inject_disk_full.sh
│   ├── inject_telegram_flood.sh
│   └── verify.sh           # Bash-based verification script
└── README.md               # This file
```

## Scenarios Tested

| Scenario | Description | Category | Duration |
|----------|-------------|----------|----------|
| `db_latency` | Inject 100-500ms latency on database | Dependency Failure | 120s |
| `db_unavailable` | Kill/pause container or hide DB file | Dependency Failure | 60s |
| `ai_failure` | Mock AI API: 5xx, 429, timeout, invalid JSON | External Dependency | 180s |
| `rule_fetch_failure` | Block/delay rule source hosts | External Dependency | 180s |
| `network_partition` | Isolate from internet except Telegram API | Network Failure | 120s |
| `memory_pressure` | Stress-ng VM workers (400M of 512M) | Resource Exhaustion | 180s |
| `cpu_throttling` | Stress-ng CPU workers (4x 0.5 CPU limit) | Resource Exhaustion | 120s |
| `disk_full` | Fill disk to 95% with sparse file | Resource Exhaustion | 120s |
| `telegram_flood` | Simulate 429 FloodWait with Retry-After | External Dependency | 180s |

## Prerequisites

### System Dependencies (Ubuntu/Debian)
```bash
sudo apt-get update && sudo apt-get install -y \
    iproute2 \
    iptables \
    iputils-ping \
    curl \
    jq \
    bc \
    stress-ng \
    util-linux \
    dnsutils \
    netcat-openbsd
```

### Rust Verification Binary
```bash
cargo build --release --bin chaos_verify
```

## Running Locally

### Quick Start (Run All Scenarios)
```bash
# From project root
cd chaos/scripts
sudo bash inject_db_latency.sh
sudo bash inject_db_unavailable.sh
sudo bash inject_ai_failure.sh
# ... etc
```

### Run Single Scenario
```bash
# Set environment variables
export COMPOSE_FILE=../compose.yml
export SERVICE_NAME=url_cleanse_bot
export HEALTH_ENDPOINT=http://localhost:3000/health
export METRICS_ENDPOINT=http://localhost:3000/metrics
export REPORT_DIR=/tmp/chaos-reports

# Run specific scenario (requires root for network injections)
sudo -E bash inject_db_latency.sh
```

### Run Verification Only
```bash
# Bash verifier
bash verify.sh db_latency

# Rust verifier (more detailed)
./target/release/chaos_verify db_latency

# All scenarios
bash verify.sh all
./target/release/chaos_verify all
```

### Using Docker Compose with Toxiproxy (Recommended for AI/Telegram tests)
```bash
# Start the stack with toxiproxy sidecars
docker compose -f compose.yml -f chaos/compose.chaos.yml up -d

# Run chaos tests against the instrumented stack
export TOXIPROXY_AI_URL=http://localhost:8474
export TOXIPROXY_TG_URL=http://localhost:8475
bash scripts/inject_ai_failure.sh
bash scripts/inject_telegram_flood.sh
```

## CI/CD Integration

The GitHub Actions workflow (`.github/workflows/chaos.yml`) runs:
- **Weekly** on Sunday 03:00 UTC
- **Manual dispatch** with scenario selection
- **Artifacts**: JSON + Markdown reports uploaded for 30 days
- **Summary**: Auto-generated markdown summary

### Manual Trigger
```bash
gh workflow run chaos.yml -f scenario=db_latency -f duration=60
```

## Configuration

Edit `chaos.toml` to adjust:
- SLO thresholds (error rate, latency, availability)
- Scenario durations and injection parameters
- Verification expectations

Key SLOs:
- **Availability**: ≥ 99.5% (30d rolling)
- **Error Rate**: < 5% during chaos, < 2% for DB latency
- **P99 Latency**: < 5000ms normal, < 10000ms under stress

## Safety & Cleanup

All injection scripts:
- Register cleanup functions via `trap EXIT`
- Restore original state on interruption (Ctrl+C, kill, timeout)
- Use `tc qdisc del`, `iptables -D`, `docker rm`, file restoration

**Always run with `sudo -E`** to preserve environment variables and allow network manipulation.

## Report Outputs

Reports generated in `$REPORT_DIR` (default `/tmp/chaos-reports`):

- `chaos-{scenario}-{timestamp}.json` - Machine-readable results
- `chaos-{scenario}-{timestamp}.md` - Human-readable summary
- `chaos_verify_{scenario}_{timestamp}.json` - Rust verifier output

## Extending Scenarios

1. Add scenario config to `chaos.toml`
2. Create injection script in `scripts/inject_{name}.sh`
3. Source `common.sh` and follow the pattern:
   - `capture_baseline_metrics`
   - Inject chaos
   - `sleep $DURATION`
   - `capture_chaos_metrics`
   - `verify_slo`
   - `generate_report`
4. Add verification logic to `verify.sh` and `verify.rs`
5. Update CI workflow if needed

## Troubleshooting

### "tc: command not found"
```bash
sudo apt-get install iproute2
```

### "iptables: permission denied"
Run with `sudo` or ensure `NET_ADMIN` capability.

### "Container not found"
Check `SERVICE_NAME` matches `compose.yml` service name.

### Health check never passes
```bash
# Check container logs
docker logs url_cleanse_bot
# Check health endpoint manually
curl -v http://localhost:3000/health
```

### Toxiproxy connection refused
Ensure toxiproxy container shares network namespace:
```yaml
# In docker-compose.override.yml or chaos/compose.chaos.yml
services:
  toxiproxy-ai:
    network_mode: "service:url_cleanse_bot"
```