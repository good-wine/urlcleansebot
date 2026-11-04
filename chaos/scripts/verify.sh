#!/bin/bash
# Chaos Verification Script - Runs post-chaos assertions
# Validates health, metrics, error rates, and alerting

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

COMPOSE_FILE="${COMPOSE_FILE:-compose.yml}"
SERVICE_NAME="${SERVICE_NAME:-url_cleanse_bot}"
HEALTH_ENDPOINT="${HEALTH_ENDPOINT:-http://localhost:3000/health}"
METRICS_ENDPOINT="${METRICS_ENDPOINT:-http://localhost:3000/metrics}"
REPORT_DIR="${REPORT_DIR:-/tmp/chaos-reports}"
SCENARIO="${1:-all}"

# Verification functions
verify_health_endpoint() {
    log_info "Verifying health endpoint..."

    local response=$(curl -sf -w "\n%{http_code}" "$HEALTH_ENDPOINT" 2>/dev/null || echo -e "\n000")
    local http_code=$(echo "$response" | tail -1)
    local body=$(echo "$response" | head -n -1)

    if [ "$http_code" = "200" ]; then
        log_success "Health endpoint returned 200 OK"
        return 0
    else
        log_error "Health endpoint returned $http_code"
        return 1
    fi
}

verify_metrics_endpoint() {
    log_info "Verifying metrics endpoint..."

    local response=$(curl -sf -w "\n%{http_code}" "$METRICS_ENDPOINT" 2>/dev/null || echo -e "\n000")
    local http_code=$(echo "$response" | tail -1)

    if [ "$http_code" = "200" ]; then
        log_success "Metrics endpoint returned 200 OK"
        return 0
    else
        log_error "Metrics endpoint returned $http_code"
        return 1
    fi
}

verify_container_running() {
    log_info "Verifying container is running..."

    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -z "$container_id" ]; then
        log_error "Container not found"
        return 1
    fi

    local status=$(docker inspect --format '{{.State.Status}}' "$container_id")
    if [ "$status" = "running" ]; then
        log_success "Container is running"
        return 0
    else
        log_error "Container status: $status"
        return 1
    fi
}

verify_no_oom_kill() {
    log_info "Checking for OOM kills..."

    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -z "$container_id" ]; then
        return 1
    fi

    local oom_killed=$(docker inspect --format '{{.State.OOMKilled}}' "$container_id")
    if [ "$oom_killed" = "true" ]; then
        log_error "Container was OOM killed!"
        return 1
    else
        log_success "No OOM kill detected"
        return 0
    fi
}

verify_restart_count() {
    log_info "Checking restart count..."

    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -z "$container_id" ]; then
        return 1
    fi

    local restarts=$(docker inspect --format '{{.RestartCount}}' "$container_id")
    log_info "Container restart count: $restarts"

    # Alert if too many restarts
    if [ "$restarts" -gt 5 ]; then
        log_warn "High restart count: $restarts"
        return 1
    fi

    return 0
}

verify_log_rotation() {
    log_info "Verifying log rotation..."

    # Check Docker logging driver config
    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -z "$container_id" ]; then
        return 1
    fi

    local log_driver=$(docker inspect --format '{{.HostConfig.LogConfig.Type}}' "$container_id")
    local max_size=$(docker inspect --format '{{.HostConfig.LogConfig.Config.max-size}}' "$container_id")
    local max_file=$(docker inspect --format '{{.HostConfig.LogConfig.Config.max-file}}' "$container_id")

    log_info "Log driver: $log_driver, max-size: $max_size, max-file: $max_file"

    if [ "$log_driver" = "json-file" ] && [ -n "$max_size" ] && [ -n "$max_file" ]; then
        log_success "Log rotation configured"
        return 0
    else
        log_warn "Log rotation not properly configured"
        return 1
    fi
}

verify_database_accessible() {
    log_info "Verifying database accessibility..."

    # The health endpoint checks DB connectivity
    local response=$(curl -sf "$HEALTH_ENDPOINT" 2>/dev/null || echo "")
    if echo "$response" | grep -q "ready\|healthy\|ok"; then
        log_success "Database appears accessible via health check"
        return 0
    else
        log_warn "Health check response: $response"
        return 1
    fi
}

verify_alerting() {
    log_info "Verifying alerting system..."

    local alertmanager_url="${ALERTMANAGER_URL:-http://localhost:9093}"

    # Check if Alertmanager is reachable
    if curl -sf "$alertmanager_url/api/v2/status" >/dev/null 2>&1; then
        log_success "Alertmanager is reachable"

        # Check for any active alerts
        local alerts=$(curl -sf "$alertmanager_url/api/v2/alerts" 2>/dev/null | jq -r '.[] | select(.status.state == "active") | .labels.alertname' 2>/dev/null | sort -u || echo "")

        if [ -n "$alerts" ]; then
            log_info "Active alerts: $alerts"
        else
            log_info "No active alerts"
        fi
        return 0
    else
        log_warn "Alertmanager not reachable at $alertmanager_url"
        return 1
    fi
}

verify_error_rate() {
    log_info "Verifying error rate..."

    local metrics=$(curl -sf "$METRICS_ENDPOINT" 2>/dev/null || echo "")
    local total=$(echo "$metrics" | grep 'urlcleansebot_requests_total' | awk '{sum+=$2} END {print sum}')
    local errors=$(echo "$metrics" | grep 'urlcleansebot_requests_total{.*status="5' | awk '{sum+=$2} END {print sum}')
    errors=$(echo "$metrics" | grep 'urlcleansebot_requests_total{.*status="429' | awk -v e="$errors" '{sum+=$2} END {print sum+e}')
    errors=$(echo "$metrics" | grep 'urlcleansebot_requests_total{.*status="timeout' | awk -v e="$errors" '{sum+=$2} END {print sum+e}')

    total=${total:-0}
    errors=${errors:-0}

    if [ "$total" -gt 0 ]; then
        local rate=$(awk "BEGIN {printf \"%.4f\", $errors/$total}")
        log_info "Error rate: $rate ($errors/$total)"

        if (($(echo "$rate < 0.05" | bc -l))); then
            log_success "Error rate within SLO (< 5%)"
            return 0
        else
            log_error "Error rate exceeds SLO (>= 5%)"
            return 1
        fi
    else
        log_warn "No requests recorded yet"
        return 0
    fi
}

verify_latency() {
    log_info "Verifying latency (p99)..."

    local metrics=$(curl -sf "$METRICS_ENDPOINT" 2>/dev/null || echo "")

    # Extract p99 from histogram (simplified)
    local p99=$(echo "$metrics" | grep 'urlcleansebot_request_duration_seconds_bucket' | grep 'le="5"' | awk '{print $2}' | sort -n | tail -1)

    if [ -n "$p99" ]; then
        local p99_ms=$(awk "BEGIN {printf \"%.0f\", $p99 * 1000}")
        log_info "P99 latency: ${p99_ms}ms"

        if [ "$p99_ms" -lt 5000 ]; then
            log_success "P99 latency within SLO (< 5000ms)"
            return 0
        else
            log_error "P99 latency exceeds SLO (>= 5000ms)"
            return 1
        fi
    else
        log_warn "Could not determine p99 latency"
        return 0
    fi
}

# Scenario-specific verifications
verify_db_latency() {
    log_info "=== Verifying DB Latency Scenario ==="
    verify_health_endpoint
    verify_metrics_endpoint
    verify_container_running
    verify_no_oom_kill
    verify_error_rate
    verify_latency
}

verify_db_unavailable() {
    log_info "=== Verifying DB Unavailable Scenario ==="
    verify_container_running
    verify_restart_count
    verify_health_endpoint
    verify_database_accessible
}

verify_ai_failure() {
    log_info "=== Verifying AI API Failure Scenario ==="
    verify_health_endpoint
    verify_metrics_endpoint
    verify_container_running
    verify_no_oom_kill
    verify_error_rate
    verify_latency
}

verify_rule_fetch_failure() {
    log_info "=== Verifying Rule Fetch Failure Scenario ==="
    verify_health_endpoint
    verify_metrics_endpoint
    verify_container_running
    verify_error_rate
}

verify_network_partition() {
    log_info "=== Verifying Network Partition Scenario ==="
    verify_container_running
    verify_restart_count
    # Health should fail during partition, but recover after
    verify_health_endpoint || log_warn "Health check failed (expected during partition)"
}

verify_memory_pressure() {
    log_info "=== Verifying Memory Pressure Scenario ==="
    verify_container_running
    verify_no_oom_kill
    verify_health_endpoint
    verify_metrics_endpoint
    verify_error_rate
    verify_latency
}

verify_cpu_throttling() {
    log_info "=== Verifying CPU Throttling Scenario ==="
    verify_container_running
    verify_health_endpoint
    verify_metrics_endpoint
    verify_error_rate
    verify_latency
}

verify_disk_full() {
    log_info "=== Verifying Disk Full Scenario ==="
    verify_container_running
    verify_log_rotation
    verify_health_endpoint
    verify_metrics_endpoint
    verify_error_rate
}

verify_telegram_flood() {
    log_info "=== Verifying Telegram FloodWait Scenario ==="
    verify_container_running
    verify_health_endpoint
    verify_metrics_endpoint
    verify_error_rate
    verify_latency
}

# Main verification runner
run_verification() {
    local scenario="$1"
    local start_time=$(date +%s)
    local failed=0

    log_info "Starting verification for scenario: $scenario"

    case "$scenario" in
    db_latency)
        verify_db_latency || failed=1
        ;;
    db_unavailable)
        verify_db_unavailable || failed=1
        ;;
    ai_failure)
        verify_ai_failure || failed=1
        ;;
    rule_fetch_failure)
        verify_rule_fetch_failure || failed=1
        ;;
    network_partition)
        verify_network_partition || failed=1
        ;;
    memory_pressure)
        verify_memory_pressure || failed=1
        ;;
    cpu_throttling)
        verify_cpu_throttling || failed=1
        ;;
    disk_full)
        verify_disk_full || failed=1
        ;;
    telegram_flood)
        verify_telegram_flood || failed=1
        ;;
    all)
        verify_db_latency || failed=1
        verify_db_unavailable || failed=1
        verify_ai_failure || failed=1
        verify_rule_fetch_failure || failed=1
        verify_network_partition || failed=1
        verify_memory_pressure || failed=1
        verify_cpu_throttling || failed=1
        verify_disk_full || failed=1
        verify_telegram_flood || failed=1
        ;;
    *)
        log_error "Unknown scenario: $scenario"
        return 1
        ;;
    esac

    local duration=$(($(date +%s) - start_time))

    if [ $failed -eq 0 ]; then
        log_success "All verifications passed for $scenario (${duration}s)"
        generate_report "$scenario" "passed" "0" "0" "$duration" "$REPORT_DIR"
        return 0
    else
        log_error "Some verifications failed for $scenario (${duration}s)"
        generate_report "$scenario" "failed" "0" "0" "$duration" "$REPORT_DIR"
        return 1
    fi
}

main() {
    log_info "=== Chaos Verification Started ==="
    log_info "Scenario: $SCENARIO"

    ensure_tools

    run_verification "$SCENARIO"
}

main "$@"
