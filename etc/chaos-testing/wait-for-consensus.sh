#!/usr/bin/env bash
#
# Wait for Tendermint consensus to stabilize
# Used before running chaos tests to ensure all validators are active
#
# Usage:
#   ./wait-for-consensus.sh                    # Wait with default timeout (120s)
#   ./wait-for-consensus.sh --timeout 300      # Custom timeout
#   ./wait-for-consensus.sh --min-blocks 10    # Wait for N blocks

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Configuration
NODE_NAMES=("alys-node-1" "alys-node-2" "alys-node-3")
NODE_RPC_PORTS=(3001 3011 3021)
TIMEOUT_SECONDS=120
MIN_BLOCKS=5
VERBOSE=false

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $*"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*"
}

get_consensus_height() {
    local port="$1"
    curl -s -X POST "http://localhost:$port" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"tendermint_consensusState","params":[],"id":1}' \
        2>/dev/null | jq -r '.result.height // 0'
}

check_all_nodes_responding() {
    local responding=0
    for i in "${!NODE_NAMES[@]}"; do
        local port="${NODE_RPC_PORTS[$i]}"
        local height=$(get_consensus_height "$port")
        if [[ "$height" != "0" ]] && [[ -n "$height" ]]; then
            ((responding++))
        fi
    done
    echo "$responding"
}

check_all_nodes_at_same_height() {
    local heights=()
    for i in "${!NODE_NAMES[@]}"; do
        local port="${NODE_RPC_PORTS[$i]}"
        local height=$(get_consensus_height "$port")
        heights+=("$height")
    done

    # Check if all heights are the same and non-zero
    local first="${heights[0]}"
    if [[ "$first" == "0" ]] || [[ -z "$first" ]]; then
        return 1
    fi

    for h in "${heights[@]}"; do
        if [[ "$h" != "$first" ]]; then
            return 1
        fi
    done
    return 0
}

wait_for_consensus() {
    local start_time=$(date +%s)
    local initial_height=""

    log_info "Waiting for Tendermint consensus to stabilize..."
    log_info "  Timeout: ${TIMEOUT_SECONDS}s"
    log_info "  Min blocks: ${MIN_BLOCKS}"

    # Phase 1: Wait for all nodes to respond
    while true; do
        local responding=$(check_all_nodes_responding)
        if [[ "$responding" -eq ${#NODE_NAMES[@]} ]]; then
            log_info "All ${#NODE_NAMES[@]} nodes responding"
            break
        fi

        local elapsed=$(($(date +%s) - start_time))
        if [[ $elapsed -ge $TIMEOUT_SECONDS ]]; then
            log_error "Timeout: Only $responding nodes responding after ${elapsed}s"
            return 1
        fi

        if [[ "$VERBOSE" == "true" ]]; then
            log_warn "Waiting for nodes... ($responding/${#NODE_NAMES[@]} responding)"
        fi
        sleep 2
    done

    # Phase 2: Wait for nodes to be at same height
    while true; do
        if check_all_nodes_at_same_height; then
            initial_height=$(get_consensus_height "${NODE_RPC_PORTS[0]}")
            log_info "All nodes synchronized at height $initial_height"
            break
        fi

        local elapsed=$(($(date +%s) - start_time))
        if [[ $elapsed -ge $TIMEOUT_SECONDS ]]; then
            log_error "Timeout: Nodes not synchronized after ${elapsed}s"
            return 1
        fi

        sleep 2
    done

    # Phase 3: Wait for minimum block production
    log_info "Waiting for $MIN_BLOCKS blocks to be produced..."
    local target_height=$((initial_height + MIN_BLOCKS))

    while true; do
        local current_height=$(get_consensus_height "${NODE_RPC_PORTS[0]}")
        if [[ "$current_height" -ge "$target_height" ]]; then
            log_info "Consensus stabilized: $current_height blocks produced"
            break
        fi

        local elapsed=$(($(date +%s) - start_time))
        if [[ $elapsed -ge $TIMEOUT_SECONDS ]]; then
            log_error "Timeout: Only $((current_height - initial_height)) blocks after ${elapsed}s"
            return 1
        fi

        if [[ "$VERBOSE" == "true" ]]; then
            echo -n "."
        fi
        sleep 1
    done

    local total_time=$(($(date +%s) - start_time))
    log_info "Consensus ready in ${total_time}s"
    return 0
}

main() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --timeout)
                TIMEOUT_SECONDS="$2"
                shift 2
                ;;
            --min-blocks)
                MIN_BLOCKS="$2"
                shift 2
                ;;
            --verbose)
                VERBOSE=true
                shift
                ;;
            --help)
                echo "Usage: $0 [--timeout SECONDS] [--min-blocks N] [--verbose]"
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done

    wait_for_consensus
}

main "$@"
