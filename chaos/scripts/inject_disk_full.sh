#!/bin/bash
# Disk Full Injection - Fill disk to test logging rotation and DB write failures

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

COMPOSE_FILE="${COMPOSE_FILE:-compose.yml}"
SERVICE_NAME="${SERVICE_NAME:-url_cleanse_bot}"
DURATION="${DURATION:-120}"
TARGET_PATH="${TARGET_PATH:-/app}"
FILL_PERCENT="${FILL_PERCENT:-95}"

# Get available disk space
get_disk_info() {
    df "$TARGET_PATH" | tail -1 | awk '{print $2, $4}' # total, available in KB
}

fill_disk() {
    log_info "Filling disk at $TARGET_PATH to ${FILL_PERCENT}%"

    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -z "$container_id" ]; then
        log_error "Container not found"
        return 1
    fi

    local pid=$(docker inspect --format '{{.State.Pid}}' "$container_id")

    # Calculate fill size
    read -r total_kb avail_kb <<<$(nsenter -t "$pid" -m -- df "$TARGET_PATH" | tail -1 | awk '{print $2, $4}')
    local target_used_kb=$(echo "$total_kb * $FILL_PERCENT / 100" | bc)
    local fill_kb=$(echo "$target_used_kb - ($total_kb - $avail_kb)" | bc)

    if (($(echo "$fill_kb <= 0" | bc -l))); then
        log_warn "Disk already at or above ${FILL_PERCENT}%"
        fill_kb=102400 # 100MB minimum
    fi

    local fill_mb=$(echo "$fill_kb / 1024" | bc)
    log_info "Filling ${fill_mb}MB (${fill_kb}KB)"

    # Create sparse file quickly with fallocate
    nsenter -t "$pid" -m -- fallocate -l "${fill_kb}K" "$TARGET_PATH/chaos_disk_fill.tmp" 2>/dev/null || {
        log_warn "fallocate failed, using dd"
        nsenter -t "$pid" -m -- dd if=/dev/zero of="$TARGET_PATH/chaos_disk_fill.tmp" bs=1M count="$fill_mb" 2>/dev/null
    }

    register_cleanup "cleanup_disk_fill"
}

cleanup_disk_fill() {
    log_info "Cleaning up disk fill file"

    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -n "$container_id" ]; then
        local pid=$(docker inspect --format '{{.State.Pid}}' "$container_id")
        nsenter -t "$pid" -m -- rm -f "$TARGET_PATH/chaos_disk_fill.tmp" 2>/dev/null || true
    fi
}

main() {
    log_info "=== Disk Full Chaos Injection Started ==="
    log_info "Target: $TARGET_PATH, Fill: ${FILL_PERCENT}%, Duration: ${DURATION}s"

    ensure_tools

    capture_baseline_metrics "/tmp/baseline_disk_full.json"

    fill_disk

    log_info "Disk filled. Running for ${DURATION}s..."
    sleep "$DURATION"

    capture_chaos_metrics "/tmp/chaos_disk_full.json" "during"

    local error_rate=$(calculate_error_rate "/tmp/chaos_disk_full.json.key")
    local p99_latency=$(calculate_p99_latency "/tmp/chaos_disk_full.json.key")

    log_info "Results: error_rate=$error_rate p99_latency=${p99_latency}ms"

    verify_slo "$error_rate" 0.10 "$p99_latency" 30000

    generate_report "disk_full" "passed" "$error_rate" "$p99_latency" "$DURATION" "/tmp/chaos-reports"

    log_success "=== Disk Full Chaos Injection Completed ==="
}

main "$@"
