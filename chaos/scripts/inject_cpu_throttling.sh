#!/bin/bash
# CPU Throttling Injection - Uses stress-ng to consume CPU
# Verifies latency degradation with 0.5 CPU limit

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

COMPOSE_FILE="${COMPOSE_FILE:-compose.yml}"
SERVICE_NAME="${SERVICE_NAME:-url_cleanse_bot}"
DURATION="${DURATION:-120}"
CPU_WORKERS="${CPU_WORKERS:-4}"
CPU_LOAD="${CPU_LOAD:-80}"

inject_cpu_throttling() {
    log_info "Injecting CPU throttling: ${CPU_WORKERS} workers at ${CPU_LOAD}% load"

    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -z "$container_id" ]; then
        log_error "Container not found"
        return 1
    fi

    local pid=$(docker inspect --format '{{.State.Pid}}' "$container_id")

    # Run stress-ng in container namespace
    nsenter -t "$pid" -m -n -u -- stress-ng \
        --cpu "$CPU_WORKERS" \
        --cpu-load "$CPU_LOAD" \
        --timeout "${DURATION}s" \
        --metrics-brief &

    local stress_pid=$!
    register_cleanup "kill_stress $stress_pid"

    echo $stress_pid
}

kill_stress() {
    local pid=$1
    log_info "Stopping stress-ng (PID: $pid)"
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
}

main() {
    log_info "=== CPU Throttling Chaos Injection Started ==="
    log_info "Config: workers=$CPU_WORKERS load=${CPU_LOAD}% duration=${DURATION}s"

    ensure_tools

    capture_baseline_metrics "/tmp/baseline_cpu_throttling.json"

    local stress_pid=$(inject_cpu_throttling)

    log_info "CPU throttling injected. Running for ${DURATION}s..."
    sleep "$DURATION"

    capture_chaos_metrics "/tmp/chaos_cpu_throttling.json" "during"

    local error_rate=$(calculate_error_rate "/tmp/chaos_cpu_throttling.json.key")
    local p99_latency=$(calculate_p99_latency "/tmp/chaos_cpu_throttling.json.key")

    log_info "Results: error_rate=$error_rate p99_latency=${p99_latency}ms"

    verify_slo "$error_rate" 0.01 "$p99_latency" 15000

    generate_report "cpu_throttling" "passed" "$error_rate" "$p99_latency" "$DURATION" "/tmp/chaos-reports"

    log_success "=== CPU Throttling Chaos Injection Completed ==="
}

main "$@"
