#!/bin/bash
# Telegram FloodWait Injection - Uses toxiproxy to simulate 429 responses
# Tests backoff handling, queue management, user notification

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

COMPOSE_FILE="${COMPOSE_FILE:-compose.yml}"
SERVICE_NAME="${SERVICE_NAME:-url_cleanse_bot}"
DURATION="${DURATION:-180}"
TG_UPSTREAM="api.telegram.org"
TG_PORT=443
TOXIPROXY_PORT=8475
PROXY_PORT=2122

TOXIPROXY_CONTAINER="toxiproxy-telegram"

start_toxiproxy() {
    log_info "Starting Toxiproxy for Telegram API simulation"

    docker run -d --name "$TOXIPROXY_CONTAINER" \
        --network container:"$SERVICE_NAME" \
        -p "$TOXIPROXY_PORT:8474" \
        -p "$PROXY_PORT:$PROXY_PORT" \
        ghcr.io/shopify/toxiproxy:2.6

    sleep 3

    # Create proxy for Telegram API
    curl -sf -X POST "http://localhost:$TOXIPROXY_PORT/proxies" \
        -H "Content-Type: application/json" \
        -d "{
            \"name\": \"telegram_api_proxy\",
            \"listen\": \"0.0.0.0:$PROXY_PORT\",
            \"upstream\": \"$TG_UPSTREAM:$TG_PORT\",
            \"enabled\": true
        }"

    register_cleanup "stop_toxiproxy"
}

stop_toxiproxy() {
    log_info "Stopping Toxiproxy"
    docker stop "$TOXIPROXY_CONTAINER" 2>/dev/null || true
    docker rm "$TOXIPROXY_CONTAINER" 2>/dev/null || true
}

add_flood_wait_toxic() {
    local retry_after="$1"
    local name="flood_wait_${retry_after}"

    log_info "Adding FloodWait toxic: Retry-After: $retry_after"

    curl -sf -X POST "http://localhost:$TOXIPROXY_PORT/proxies/telegram_api_proxy/toxics" \
        -H "Content-Type: application/json" \
        -d "{
            \"name\": \"$name\",
            \"type\": \"http_response\",
            \"stream\": \"downstream\",
            \"toxicity\": 1.0,
            \"attributes\": {
                \"status\": 429,
                \"response\": \"{\\\"ok\\\":false,\\\"error_code\\\":429,\\\"description\\\":\\\"Too Many Requests: retry after $retry_after\\\"}\",
                \"headers\": {
                    \"Retry-After\": \"$retry_after\",
                    \"Content-Type\": \"application/json\"
                }
            }
        }"
}

remove_all_toxics() {
    log_info "Removing all toxics"
    curl -sf -X GET "http://localhost:$TOXIPROXY_PORT/proxies/telegram_api_proxy/toxics" |
        jq -r '.[].name' | while read -r name; do
        curl -sf -X DELETE "http://localhost:$TOXIPROXY_PORT/proxies/telegram_api_proxy/toxics/$name"
    done
}

main() {
    log_info "=== Telegram FloodWait Chaos Injection Started ==="
    log_info "Duration: ${DURATION}s"

    ensure_tools

    capture_baseline_metrics "/tmp/baseline_telegram_flood.json"

    start_toxiproxy

    # Sequence of flood wait scenarios
    local scenarios=("30:60" "60:60" "10:60") # retry_after:duration

    for scenario in "${scenarios[@]}"; do
        IFS=':' read -r retry_after dur <<<"$scenario"
        log_info "Injecting FloodWait with Retry-After: ${retry_after}s for ${dur}s"
        add_flood_wait_toxic "$retry_after"
        sleep "$dur"
        remove_all_toxics
        sleep 5 # Brief recovery between scenarios
    done

    # Final recovery period
    log_info "All toxics cleared, observing recovery for 30s"
    sleep 30

    capture_chaos_metrics "/tmp/chaos_telegram_flood.json" "during"

    local error_rate=$(calculate_error_rate "/tmp/chaos_telegram_flood.json.key")
    local p99_latency=$(calculate_p99_latency "/tmp/chaos_telegram_flood.json.key")

    log_info "Results: error_rate=$error_rate p99_latency=${p99_latency}ms"

    verify_slo "$error_rate" 0.05 "$p99_latency" 10000

    generate_report "telegram_flood_wait" "passed" "$error_rate" "$p99_latency" "$DURATION" "/tmp/chaos-reports"

    log_success "=== Telegram FloodWait Chaos Injection Completed ==="
}

main "$@"
