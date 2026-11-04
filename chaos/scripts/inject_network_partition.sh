#!/bin/bash
# Network Partition Injection - Isolate bot from internet except Telegram API
# Uses iptables to create network partition

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

COMPOSE_FILE="${COMPOSE_FILE:-compose.yml}"
SERVICE_NAME="${SERVICE_NAME:-url_cleanse_bot}"
DURATION="${DURATION:-120}"

# Telegram API endpoints to allow
TELEGRAM_HOSTS=("api.telegram.org")
ALLOWED_IPS=("127.0.0.1")

# Resolve Telegram hosts
resolve_telegram() {
    local ips=()
    for host in "${TELEGRAM_HOSTS[@]}"; do
        local ip=$(dig +short "$host" A | head -n1)
        if [ -n "$ip" ]; then
            ips+=("$ip")
            log_info "Resolved $host -> $ip"
        fi
    done
    echo "${ips[@]}"
}

# Create network partition - block all except allowed
create_partition() {
    local allowed_ips=("$@")

    log_info "Creating network partition - allowing only: ${allowed_ips[*]}"

    # Get container PID for namespace operations
    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -z "$container_id" ]; then
        log_error "Container not found"
        return 1
    fi

    local pid=$(docker inspect --format '{{.State.Pid}}' "$container_id")

    # In container namespace: default deny all, then allow specific
    nsenter -t "$pid" -n iptables -P OUTPUT DROP
    nsenter -t "$pid" -n iptables -P INPUT DROP
    nsenter -t "$pid" -n iptables -P FORWARD DROP

    # Allow loopback
    nsenter -t "$pid" -n iptables -A OUTPUT -o lo -j ACCEPT
    nsenter -t "$pid" -n iptables -A INPUT -i lo -j ACCEPT

    # Allow established/related
    nsenter -t "$pid" -n iptables -A OUTPUT -m state --state ESTABLISHED,RELATED -j ACCEPT
    nsenter -t "$pid" -n iptables -A INPUT -m state --state ESTABLISHED,RELATED -j ACCEPT

    # Allow Telegram IPs
    for ip in "${allowed_ips[@]}"; do
        nsenter -t "$pid" -n iptables -A OUTPUT -d "$ip" -j ACCEPT
        nsenter -t "$pid" -n iptables -A INPUT -s "$ip" -j ACCEPT
    done

    # Allow local Docker network (for health checks)
    nsenter -t "$pid" -n iptables -A OUTPUT -d 172.16.0.0/12 -j ACCEPT
    nsenter -t "$pid" -n iptables -A INPUT -s 172.16.0.0/12 -j ACCEPT

    register_cleanup "remove_partition"
}

remove_partition() {
    log_info "Removing network partition"

    local container_id=$(get_container_id "$SERVICE_NAME")
    if [ -z "$container_id" ]; then
        return
    fi

    local pid=$(docker inspect --format '{{.State.Pid}}' "$container_id")

    # Flush all rules
    nsenter -t "$pid" -n iptables -F
    nsenter -t "$pid" -n iptables -X
    nsenter -t "$pid" -n iptables -P OUTPUT ACCEPT
    nsenter -t "$pid" -n iptables -P INPUT ACCEPT
    nsenter -t "$pid" -n iptables -P FORWARD ACCEPT
}

main() {
    log_info "=== Network Partition Chaos Injection Started ==="
    log_info "Duration: ${DURATION}s"

    check_root
    ensure_tools

    capture_baseline_metrics "/tmp/baseline_network_partition.json"

    local telegram_ips=($(resolve_telegram))
    local all_allowed=("${telegram_ips[@]}" "${ALLOWED_IPS[@]}")

    create_partition "${all_allowed[@]}"

    log_info "Network partition created. Running for ${DURATION}s..."
    sleep "$DURATION"

    capture_chaos_metrics "/tmp/chaos_network_partition.json" "during"

    local error_rate=$(calculate_error_rate "/tmp/chaos_network_partition.json.key")
    local p99_latency=$(calculate_p99_latency "/tmp/chaos_network_partition.json.key")

    log_info "Results: error_rate=$error_rate p99_latency=${p99_latency}ms"

    # Network partition should cause health checks to fail and graceful shutdown
    verify_slo "$error_rate" 0.50 "$p99_latency" 30000

    generate_report "network_partition" "passed" "$error_rate" "$p99_latency" "$DURATION" "/tmp/chaos-reports"

    log_success "=== Network Partition Chaos Injection Completed ==="
}

main "$@"
