#!/bin/bash
# Common utilities for chaos injection scripts
# Source this file in other scripts: source "$(dirname "$0")/common.sh"

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() { echo -e "${BLUE}[INFO]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }
log_success() { echo -e "${GREEN}[OK]${NC} $*"; }

# Global cleanup tracking
CLEANUP_FUNCTIONS=()
CHAOS_START_TIME=$(date +%s)

# Register a cleanup function to be called on exit
register_cleanup() {
    CLEANUP_FUNCTIONS+=("$1")
}

# Execute all registered cleanup functions
run_cleanup() {
    log_info "Running cleanup functions..."
    for func in "${CLEANUP_FUNCTIONS[@]}"; do
        log_info "Executing cleanup: $func"
        $func || log_warn "Cleanup function $func failed"
    done
}

# Trap EXIT to ensure cleanup
trap run_cleanup EXIT

# Get container ID for a service
get_container_id() {
    local service_name="$1"
    docker compose -f "${COMPOSE_FILE:-compose.yml}" ps -q "$service_name" 2>/dev/null | head -n1
}

# Wait for health endpoint to be healthy
wait_for_health() {
    local endpoint="${1:-http://localhost:3000/health}"
    local timeout="${2:-60}"
    local interval="${3:-5}"
    local elapsed=0

    log_info "Waiting for health endpoint $endpoint (timeout: ${timeout}s)..."

    while [ $elapsed -lt $timeout ]; do
        if curl -sf -o /dev/null --max-time 5 "$endpoint" 2>/dev/null; then
            log_success "Health endpoint is healthy"
            return 0
        fi
        sleep $interval
        elapsed=$((elapsed + interval))
    done

    log_error "Health endpoint did not become healthy within ${timeout}s"
    return 1
}

# Get baseline metrics before chaos
capture_baseline_metrics() {
    local output_file="${1:-/tmp/baseline_metrics.json}"
    log_info "Capturing baseline metrics to $output_file"

    curl -sf "http://localhost:3000/metrics" 2>/dev/null |
        grep -E '^(urlcleansebot_|process_|go_|rust_)' |
        head -100 >"$output_file" || true

    # Also capture key metrics as JSON
    curl -sf "http://localhost:3000/metrics" 2>/dev/null |
        awk '/^urlcleansebot_requests_total/ {print} /^urlcleansebot_request_duration_seconds/ {print} /^urlcleansebot_errors_total/ {print}' \
            >"${output_file}.key" || true

    log_success "Baseline metrics captured"
}

# Capture metrics during/after chaos
capture_chaos_metrics() {
    local output_file="${1:-/tmp/chaos_metrics.json}"
    local label="${2:-during}"

    log_info "Capturing ${label} chaos metrics to $output_file"

    curl -sf "http://localhost:3000/metrics" 2>/dev/null |
        awk '/^urlcleansebot_requests_total/ {print} /^urlcleansebot_request_duration_seconds/ {print} /^urlcleansebot_errors_total/ {print}' \
            >"$output_file" || true
}

# Calculate error rate from metrics
calculate_error_rate() {
    local metrics_file="$1"

    if [ ! -f "$metrics_file" ]; then
        echo "0"
        return
    fi

    local total_requests=$(grep 'urlcleansebot_requests_total' "$metrics_file" | awk '{sum+=$2} END {print sum}')
    local error_requests=$(grep 'urlcleansebot_requests_total{.*status="5' "$metrics_file" 2>/dev/null | awk '{sum+=$2} END {print sum}')
    error_requests=$(grep 'urlcleansebot_requests_total{.*status="429' "$metrics_file" 2>/dev/null | awk -v e="$error_requests" '{sum+=$2} END {print sum+e}')
    error_requests=$(grep 'urlcleansebot_requests_total{.*status="timeout' "$metrics_file" 2>/dev/null | awk -v e="$error_requests" '{sum+=$2} END {print sum+e}')

    total_requests=${total_requests:-0}
    error_requests=${error_requests:-0}

    if [ "$total_requests" -gt 0 ]; then
        awk "BEGIN {printf \"%.4f\", $error_requests/$total_requests}"
    else
        echo "0"
    fi
}

# Calculate p99 latency from histogram
calculate_p99_latency() {
    local metrics_file="$1"

    if [ ! -f "$metrics_file" ]; then
        echo "0"
        return
    fi

    # Extract histogram buckets and calculate p99
    # This is a simplified version - in production use promql
    grep 'urlcleansebot_request_duration_seconds_bucket' "$metrics_file" |
        awk -F'[{}"]' '/le="5"/ {print $4}' | sort -n | tail -1 2>/dev/null || echo "0"
}

# Verify SLO compliance
verify_slo() {
    local error_rate="$1"
    local max_error_rate="${2:-0.05}"
    local p99_latency="$3"
    local max_p99_ms="${4:-5000}"

    local passed=true

    if (($(echo "$error_rate > $max_error_rate" | bc -l))); then
        log_error "SLO VIOLATION: Error rate $error_rate exceeds maximum $max_error_rate"
        passed=false
    else
        log_success "SLO OK: Error rate $error_rate within limit $max_error_rate"
    fi

    if (($(echo "$p99_latency > $max_p99_ms" | bc -l))); then
        log_error "SLO VIOLATION: P99 latency ${p99_latency}ms exceeds maximum ${max_p99_ms}ms"
        passed=false
    else
        log_success "SLO OK: P99 latency ${p99_latency}ms within limit ${max_p99_ms}ms"
    fi

    if [ "$passed" = true ]; then
        return 0
    else
        return 1
    fi
}

# Check if alert fired in Prometheus/Alertmanager
check_alert_fired() {
    local alert_name="$1"
    local alertmanager_url="${ALERTMANAGER_URL:-http://localhost:9093}"

    log_info "Checking if alert $alert_name fired..."

    # Query Alertmanager for active alerts
    local alerts=$(curl -sf "$alertmanager_url/api/v2/alerts" 2>/dev/null |
        jq -r ".[] | select(.labels.alertname == \"$alert_name\") | .labels.alertname" 2>/dev/null || echo "")

    if [ -n "$alerts" ]; then
        log_success "Alert $alert_name is firing"
        return 0
    else
        log_warn "Alert $alert_name is NOT firing"
        return 1
    fi
}

# Generate chaos test report
generate_report() {
    local scenario_name="$1"
    local status="$2" # passed/failed
    local error_rate="$3"
    local p99_latency="$4"
    local duration="$5"
    local output_dir="${6:-/tmp/chaos-reports}"

    mkdir -p "$output_dir"

    local timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    local report_file="$output_dir/chaos-${scenario_name}-$(date +%s).json"

    cat >"$report_file" <<EOF
{
  "scenario": "$scenario_name",
  "timestamp": "$timestamp",
  "status": "$status",
  "duration_seconds": $duration,
  "metrics": {
    "error_rate": $error_rate,
    "p99_latency_ms": $p99_latency
  },
  "slo": {
    "availability_target": 0.995,
    "max_error_rate": 0.05,
    "max_p99_latency_ms": 5000
  },
  "verification": {
    "error_rate_within_slo": $(echo "$error_rate <= 0.05" | bc -l),
    "latency_within_slo": $(echo "$p99_latency <= 5000" | bc -l)
  }
}
EOF

    # Also generate markdown report
    local md_file="$output_dir/chaos-${scenario_name}-$(date +%s).md"
    cat >"$md_file" <<EOF
# Chaos Test Report: $scenario_name

**Timestamp:** $timestamp
**Status:** $status
**Duration:** ${duration}s

## Metrics
| Metric | Value | SLO Target | Pass |
|--------|-------|------------|------|
| Error Rate | $(printf "%.2f%%" $(echo "$error_rate * 100" | bc -l)) | < 5% | $(echo "$error_rate <= 0.05" | bc -l) |
| P99 Latency | ${p99_latency}ms | < 5000ms | $(echo "$p99_latency <= 5000" | bc -l) |

## Verification
- Error rate within SLO: $(echo "$error_rate <= 0.05" | bc -l)
- Latency within SLO: $(echo "$p99_latency <= 5000" | bc -l)

## Scenario Configuration
See \`chaos/chaos.toml\` for scenario details.
EOF

    log_success "Report generated: $report_file and $md_file"
    echo "$report_file"
}

# Check if running as root (needed for tc/iptables)
check_root() {
    if [ "$EUID" -ne 0 ]; then
        log_error "This script must be run as root (needed for tc/iptables)"
        return 1
    fi
    return 0
}

# Check if running inside container
check_container() {
    if [ -f /.dockerenv ] || grep -q docker /proc/1/cgroup 2>/dev/null; then
        log_warn "Running inside container - some injections may not work"
        return 0
    fi
    return 1
}

# Install required tools if missing
ensure_tools() {
    local tools=("tc" "iptables" "ip" "curl" "jq" "bc" "stress-ng" "fallocate")
    local missing=()

    for tool in "${tools[@]}"; do
        if ! command -v "$tool" &>/dev/null; then
            missing+=("$tool")
        fi
    done

    if [ ${#missing[@]} -gt 0 ]; then
        log_warn "Missing tools: ${missing[*]}"
        log_info "Installing missing tools..."
        apt-get update && apt-get install -y iproute2 iptables iputils-ping curl jq bc stress-ng util-linux 2>/dev/null ||
            apk add --no-cache iproute2 iptables curl jq bc stress-ng util-linux 2>/dev/null ||
            log_error "Could not install tools automatically"
    fi
}
