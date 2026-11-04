#!/bin/bash
# Memory Pressure Injection - Uses stress-ng to consume memory
# Verifies OOM handling with 512M limit

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

COMPOSE_FILE="${COMPOSE_FILE:-compose.yml}"
SERVICE_NAME="${SERVICE_NAME:-url_cleanse_bot}"
DURATION="${DURATION:-180}"
WORKERS="${WORKERS:-2}"
VM_BYTES="${VM_BYTES:-400M}" # 80% of 512M limit

inject_memory_pressure() {
    log_info "Injecting memory pressure: ${WORKERS} workers, ${VM_BYTES} each"

    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -z "$container_id" ]; then
        log_error "Container not found"
        return 1
    fi

    local pid=$(docker inspect --format '{{.State.Pid}}' "$container_id")

    # Run stress-ng in container namespace
    nsenter -t "$pid" -m -n -u -- stress-ng \
        --vm "$WORKERS" \
        --vm-bytes "$VM_BYTES" \
        --vm-keep \
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
    log_info "=== Memory Pressure Chaos Injection Started ==="
    log_info "Config: workers=$WORKERS vm_bytes=$VM_BYTES duration=${DURATION}s"

    ensure_tools

    capture_baseline_metrics "/tmp/baseline_memory_pressure.json"

    local stress_pid=$(inject_memory_pressure)

    log_info "Memory pressure injected. Running for ${DURATION}s..."
    sleep "$DURATION"

    capture_chaos_metrics "/tmp/chaos_memory_pressure.json" "during"

    local error_rate=$(calculate_error_rate "/tmp/chaos_memory_pressure.json.key")
    local p99_latency=$(calculate_p99_latency "/tmp/chaos_memory_pressure.json.key")

    log_info "Results: error_rate=$error_rate p99_latency=${p99_latency}ms"

    verify_slo "$error_rate" 0.05 "$p99_latency" 10000

    generate_report "memory_pressure" "passed" "$error_rate" "$p99_latency" "$DURATION" "/tmp/chaos-reports"

    log_success "=== Memory Pressure Chaos Injection Completed ==="
}

main "$@"
