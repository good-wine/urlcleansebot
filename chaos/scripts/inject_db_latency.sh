#!/bin/bash
# DB Latency Injection - Uses tc/netem to add latency to database traffic
# For SQLite, we simulate via a proxy or by adding latency to localhost loopback

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

COMPOSE_FILE="${COMPOSE_FILE:-compose.yml}"
SERVICE_NAME="${SERVICE_NAME:-url_cleanse_bot}"
LATENCY_MIN="${LATENCY_MIN:-100}"
LATENCY_MAX="${LATENCY_MAX:-500}"
JITTER="${JITTER:-50}"
DURATION="${DURATION:-120}"
INTERFACE="${INTERFACE:-lo}"

# For SQLite, we use a different approach since it's file-based
# We'll create a proxy that adds latency, or use a FUSE wrapper
# Here we use tc on loopback to simulate network DB latency

inject_latency() {
    log_info "Injecting DB latency: ${LATENCY_MIN}-${LATENCY_MAX}ms with ${JITTER}ms jitter on $INTERFACE"

    # Add netem qdisc to loopback
    tc qdisc add dev "$INTERFACE" root netem delay "${LATENCY_MIN}ms" "${JITTER}ms" distribution normal 2>/dev/null || {
        log_warn "tc qdisc already exists, replacing..."
        tc qdisc change dev "$INTERFACE" root netem delay "${LATENCY_MIN}ms" "${JITTER}ms" distribution normal
    }

    # Verify
    tc qdisc show dev "$INTERFACE"

    register_cleanup "remove_latency"
}

remove_latency() {
    log_info "Removing DB latency injection"
    tc qdisc del dev "$INTERFACE" root 2>/dev/null || true
    tc qdisc show dev "$INTERFACE"
}

# Alternative: Use a proxy container with toxiproxy for more realistic DB latency
# This is more realistic for actual network databases
inject_latency_toxiproxy() {
    log_info "Starting Toxiproxy for DB latency simulation"

    # Start toxiproxy sidecar
    docker run -d --name toxiproxy-db \
        --network container:"$SERVICE_NAME" \
        -p 8474:8474 \
        -p 2121:2121 \
        ghcr.io/shopify/toxiproxy:2.6 2>/dev/null || true

    sleep 2

    # Create proxy for SQLite (simulated via localhost:2121 -> actual DB)
    # Note: For SQLite file-based, this is conceptual - in production with PostgreSQL/MySQL this would be real
    curl -sf -X POST "http://localhost:8474/proxies" \
        -H "Content-Type: application/json" \
        -d '{
            "name": "db_proxy",
            "listen": "0.0.0.0:2121",
            "upstream": "localhost:5432",  # Would be real DB port
            "enabled": true
        }' 2>/dev/null || log_warn "Toxiproxy proxy creation failed (expected for SQLite)"

    # Add latency toxic
    curl -sf -X POST "http://localhost:8474/proxies/db_proxy/toxics" \
        -H "Content-Type: application/json" \
        -d "{
            \"name\": \"latency_downstream\",
            \"type\": \"latency\",
            \"stream\": \"downstream\",
            \"toxicity\": 1.0,
            \"attributes\": {
                \"latency\": $(($LATENCY_MAX * 1000)),
                \"jitter\": $(($JITTER * 1000))
            }
        }" 2>/dev/null || log_warn "Toxiproxy toxic creation failed"

    register_cleanup "remove_toxiproxy"
}

remove_toxiproxy() {
    log_info "Removing Toxiproxy"
    docker stop toxiproxy-db 2>/dev/null || true
    docker rm toxiproxy-db 2>/dev/null || true
}

# Main execution
main() {
    log_info "=== DB Latency Chaos Injection Started ==="
    log_info "Config: min=${LATENCY_MIN}ms max=${LATENCY_MAX}ms jitter=${JITTER}ms duration=${DURATION}s"

    ensure_tools

    # Capture baseline
    capture_baseline_metrics "/tmp/baseline_db_latency.json"

    # Inject latency (use tc for loopback simulation)
    inject_latency

    log_info "Latency injected. Running for ${DURATION}s..."
    sleep "$DURATION"

    # Capture chaos metrics
    capture_chaos_metrics "/tmp/chaos_db_latency.json" "during"

    # Verify
    local error_rate=$(calculate_error_rate "/tmp/chaos_db_latency.json.key")
    local p99_latency=$(calculate_p99_latency "/tmp/chaos_db_latency.json.key")

    log_info "Results: error_rate=$error_rate p99_latency=${p99_latency}ms"

    verify_slo "$error_rate" 0.02 "$p99_latency" 5000

    # Generate report
    generate_report "db_latency" "passed" "$error_rate" "$p99_latency" "$DURATION" "/tmp/chaos-reports"

    log_success "=== DB Latency Chaos Injection Completed ==="
}

main "$@"
