#!/bin/bash
# DB Unavailability Injection - Kill/pause container to simulate DB down
# For SQLite, the DB is a file, so we simulate by making the file inaccessible
# or by killing the bot container and letting restart policy handle it

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

COMPOSE_FILE="${COMPOSE_FILE:-compose.yml}"
SERVICE_NAME="${SERVICE_NAME:-url_cleanse_bot}"
DURATION="${DURATION:-60}"
METHOD="${METHOD:-kill}" # kill, pause, rename_db_file

inject_kill() {
    log_info "Killing container $SERVICE_NAME to simulate DB unavailability"

    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -z "$container_id" ]; then
        log_error "Container not found"
        return 1
    fi

    log_info "Container ID: $container_id"
    docker kill "$container_id"

    register_cleanup "restart_container"
}

inject_pause() {
    log_info "Pausing container $SERVICE_NAME"

    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -z "$container_id" ]; then
        log_error "Container not found"
        return 1
    fi

    docker pause "$container_id"

    register_cleanup "unpause_container"
}

inject_rename_db() {
    log_info "Renaming DB file to simulate unavailability"

    # The DB file is mounted at ./bot.db -> /app/bot.db
    if [ -f "./bot.db" ]; then
        mv ./bot.db ./bot.db.chaos_backup
        log_info "DB file renamed to bot.db.chaos_backup"
    else
        log_warn "DB file not found at ./bot.db"
    fi

    register_cleanup "restore_db_file"
}

restart_container() {
    log_info "Restarting container via docker compose"
    docker compose -f "$COMPOSE_FILE" up -d "$SERVICE_NAME"
    wait_for_health "http://localhost:3000/health" 60 5
}

unpause_container() {
    log_info "Unpausing container"
    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -n "$container_id" ]; then
        docker unpause "$container_id"
    fi
    wait_for_health "http://localhost:3000/health" 60 5
}

restore_db_file() {
    log_info "Restoring DB file"
    if [ -f "./bot.db.chaos_backup" ]; then
        mv ./bot.db.chaos_backup ./bot.db
        log_info "DB file restored"
    fi
    # Restart to reconnect
    docker compose -f "$COMPOSE_FILE" restart "$SERVICE_NAME"
    wait_for_health "http://localhost:3000/health" 60 5
}

main() {
    log_info "=== DB Unavailable Chaos Injection Started ==="
    log_info "Method: $METHOD, Duration: ${DURATION}s"

    ensure_tools

    capture_baseline_metrics "/tmp/baseline_db_unavailable.json"

    case "$METHOD" in
    kill)
        inject_kill
        ;;
    pause)
        inject_pause
        ;;
    rename_db)
        inject_rename_db
        ;;
    *)
        log_error "Unknown method: $METHOD"
        exit 1
        ;;
    esac

    log_info "Chaos injected. Waiting ${DURATION}s for recovery..."
    sleep "$DURATION"

    capture_chaos_metrics "/tmp/chaos_db_unavailable.json" "during"

    local error_rate=$(calculate_error_rate "/tmp/chaos_db_unavailable.json.key")
    local p99_latency=$(calculate_p99_latency "/tmp/chaos_db_unavailable.json.key")

    log_info "Results: error_rate=$error_rate p99_latency=${p99_latency}ms"

    # For DB unavailable, we expect some errors but quick recovery
    verify_slo "$error_rate" 0.10 "$p99_latency" 10000

    generate_report "db_unavailable" "passed" "$error_rate" "$p99_latency" "$DURATION" "/tmp/chaos-reports"

    log_success "=== DB Unavailable Chaos Injection Completed ==="
}

main "$@"
