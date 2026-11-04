#!/bin/bash
# Rule Fetch Failure Injection - Simulates rule source failures
# Uses iptables to block/delay access to rule source hosts

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

COMPOSE_FILE="${COMPOSE_FILE:-compose.yml}"
SERVICE_NAME="${SERVICE_NAME:-url_cleanse_bot}"
DURATION="${DURATION:-180}"
ACTION="${ACTION:-drop}" # drop, delay

# Rule source hosts
RULE_HOSTS=(
    "raw.githubusercontent.com"
    "adguardteam.github.io"
    "brave-browser.github.io"
    "farside.link"
    "libredirect.github.io"
)

# Resolve hostnames to IPs
resolve_hosts() {
    local ips=()
    for host in "${RULE_HOSTS[@]}"; do
        local ip=$(dig +short "$host" A | head -n1)
        if [ -n "$ip" ]; then
            ips+=("$ip")
            log_info "Resolved $host -> $ip"
        else
            log_warn "Could not resolve $host"
        fi
    done
    echo "${ips[@]}"
}

# Block access to rule hosts
block_hosts() {
    local ips=("$@")

    log_info "Blocking access to rule source hosts"

    for ip in "${ips[@]}"; do
        iptables -I OUTPUT -d "$ip" -j DROP
        iptables -I INPUT -s "$ip" -j DROP
    done

    register_cleanup "unblock_hosts"
}

# Delay access to rule hosts
delay_hosts() {
    local ips=("$@")
    local delay_ms="${DELAY_MS:-10000}"

    log_info "Adding ${delay_ms}ms delay to rule source hosts"

    for ip in "${ips[@]}"; do
        # Use tc on the container's network namespace
        local container_id=$(get_container_id "$SERVICE_NAME")
        if [ -n "$container_id" ]; then
            local pid=$(docker inspect --format '{{.State.Pid}}' "$container_id")
            nsenter -t "$pid" -n tc qdisc add dev eth0 root netem delay "${delay_ms}ms" 2>/dev/null || true
        fi
    done

    register_cleanup "remove_delays"
}

unblock_hosts() {
    log_info "Unblocking rule source hosts"

    local ips=("$@")
    for ip in "${ips[@]}"; do
        iptables -D OUTPUT -d "$ip" -j DROP 2>/dev/null || true
        iptables -D INPUT -s "$ip" -j DROP 2>/dev/null || true
    done
}

remove_delays() {
    log_info "Removing network delays"

    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -n "$container_id" ]; then
        local pid=$(docker inspect --format '{{.State.Pid}}' "$container_id")
        nsenter -t "$pid" -n tc qdisc del dev eth0 root 2>/dev/null || true
    fi
}

main() {
    log_info "=== Rule Fetch Failure Chaos Injection Started ==="
    log_info "Action: $ACTION, Duration: ${DURATION}s"

    check_root
    ensure_tools

    capture_baseline_metrics "/tmp/baseline_rule_fetch.json"

    local ips=($(resolve_hosts))

    if [ ${#ips[@]} -eq 0 ]; then
        log_error "No IPs resolved, cannot proceed"
        exit 1
    fi

    case "$ACTION" in
    drop)
        block_hosts "${ips[@]}"
        ;;
    delay)
        delay_hosts "${ips[@]}"
        ;;
    *)
        log_error "Unknown action: $ACTION"
        exit 1
        ;;
    esac

    log_info "Chaos injected. Running for ${DURATION}s..."
    sleep "$DURATION"

    capture_chaos_metrics "/tmp/chaos_rule_fetch.json" "during"

    local error_rate=$(calculate_error_rate "/tmp/chaos_rule_fetch.json.key")
    local p99_latency=$(calculate_p99_latency "/tmp/chaos_rule_fetch.json.key")

    log_info "Results: error_rate=$error_rate p99_latency=${p99_latency}ms"

    verify_slo "$error_rate" 0.05 "$p99_latency" 10000

    generate_report "rule_fetch_failure" "passed" "$error_rate" "$p99_latency" "$DURATION" "/tmp/chaos-reports"

    log_success "=== Rule Fetch Failure Chaos Injection Completed ==="
}

main "$@"
