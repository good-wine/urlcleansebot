#!/bin/bash
# AI API Failure Injection - Uses toxiproxy to mock AI API failures
# Tests: 5xx, 429, timeout, invalid JSON responses

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

COMPOSE_FILE="${COMPOSE_FILE:-compose.yml}"
SERVICE_NAME="${SERVICE_NAME:-url_cleanse_bot}"
DURATION="${DURATION:-180}"
AI_UPSTREAM="${AI_UPSTREAM:-api.openai.com}"
AI_PORT="${AI_PORT:-443}"
TOXIPROXY_PORT="${TOXIPROXY_PORT:-8474}"
PROXY_PORT="${PROXY_PORT:-2121}"

TOXIPROXY_CONTAINER="toxiproxy-ai"

# Start toxiproxy sidecar
start_toxiproxy() {
    log_info "Starting Toxiproxy for AI API simulation"

    docker run -d --name "$TOXIPROXY_CONTAINER" \
        --network container:"$SERVICE_NAME" \
        -p "$TOXIPROXY_PORT:8474" \
        -p "$PROXY_PORT:$PROXY_PORT" \
        ghcr.io/shopify/toxiproxy:2.6

    sleep 3

    # Create proxy for AI API
    curl -sf -X POST "http://localhost:$TOXIPROXY_PORT/proxies" \
        -H "Content-Type: application/json" \
        -d "{
            \"name\": \"ai_api_proxy\",
            \"listen\": \"0.0.0.0:$PROXY_PORT\",
            \"upstream\": \"$AI_UPSTREAM:$AI_PORT\",
            \"enabled\": true
        }"

    register_cleanup "stop_toxiproxy"
}

stop_toxiproxy() {
    log_info "Stopping Toxiproxy"
    docker stop "$TOXIPROXY_CONTAINER" 2>/dev/null || true
    docker rm "$TOXIPROXY_CONTAINER" 2>/dev/null || true
}

# Add a toxic to the proxy
add_toxic() {
    local name="$1"
    local type="$2"
    local attributes="$3"

    log_info "Adding toxic: $name (type: $type)"

    curl -sf -X POST "http://localhost:$TOXIPROXY_PORT/proxies/ai_api_proxy/toxics" \
        -H "Content-Type: application/json" \
        -d "{
            \"name\": \"$name\",
            \"type\": \"$type\",
            \"stream\": \"downstream\",
            \"toxicity\": 1.0,
            \"attributes\": $attributes
        }"
}

# Remove all toxics
remove_all_toxics() {
    log_info "Removing all toxics"
    curl -sf -X GET "http://localhost:$TOXIPROXY_PORT/proxies/ai_api_proxy/toxics" |
        jq -r '.[].name' | while read -r name; do
        curl -sf -X DELETE "http://localhost:$TOXIPROXY_PORT/proxies/ai_api_proxy/toxics/$name"
    done
}

# Inject specific failure mode
inject_failure_mode() {
    local mode="$1"
    local duration="$2"

    case "$mode" in
    "500")
        add_toxic "status_500" "http_response" '{"status": 500, "response": "{\"error\": \"Internal Server Error\"}"}'
        ;;
    "502")
        add_toxic "status_502" "http_response" '{"status": 502, "response": "{\"error\": \"Bad Gateway\"}"}'
        ;;
    "503")
        add_toxic "status_503" "http_response" '{"status": 503, "response": "{\"error\": \"Service Unavailable\"}"}'
        ;;
    "429")
        add_toxic "status_429" "http_response" '{"status": 429, "response": "{\"error\": \"Rate Limited\"}", "headers": {"Retry-After": "30"}}'
        ;;
    "timeout")
        add_toxic "timeout" "timeout" '{"timeout": 5000}' # 5 second timeout
        ;;
    "invalid_json")
        add_toxic "invalid_json" "http_response" '{"status": 200, "response": "not valid json{{{"}'
        ;;
    *)
        log_warn "Unknown failure mode: $mode"
        return 1
        ;;
    esac

    sleep "$duration"
    remove_all_toxics
}

main() {
    log_info "=== AI API Failure Chaos Injection Started ==="
    log_info "Duration: ${DURATION}s, Upstream: $AI_UPSTREAM:$AI_PORT"

    ensure_tools

    capture_baseline_metrics "/tmp/baseline_ai_failure.json"

    start_toxiproxy

    # Sequence of failure modes
    local modes=("500:30" "502:30" "503:30" "429:30" "timeout:30" "invalid_json:30")

    for mode_duration in "${modes[@]}"; do
        IFS=':' read -r mode dur <<<"$mode_duration"
        log_info "Injecting $mode for ${dur}s"
        inject_failure_mode "$mode" "$dur"
    done

    # Final period with all clear
    log_info "Clearing all toxics, observing recovery for 30s"
    remove_all_toxics
    sleep 30

    capture_chaos_metrics "/tmp/chaos_ai_failure.json" "during"

    local error_rate=$(calculate_error_rate "/tmp/chaos_ai_failure.json.key")
    local p99_latency=$(calculate_p99_latency "/tmp/chaos_ai_failure.json.key")

    log_info "Results: error_rate=$error_rate p99_latency=${p99_latency}ms"

    verify_slo "$error_rate" 0.03 "$p99_latency" 5000

    generate_report "ai_api_failure" "passed" "$error_rate" "$p99_latency" "$DURATION" "/tmp/chaos-reports"

    log_success "=== AI API Failure Chaos Injection Completed ==="
}

main "$@"
