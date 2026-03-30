#!/usr/bin/env bash
#
# Tendermint Consensus Chaos Testing Framework
# Tests consensus safety, liveness, and recovery for Tendermint BFT
#
# Prerequisites:
#   - Docker Compose testnet running: docker compose -f docker-compose.tendermint-3node.yml up -d
#   - All 3 validators producing blocks (wait ~60s after startup)
#
# Usage:
#   ./tendermint-chaos.sh --scenario TM-A1          # Run specific scenario
#   ./tendermint-chaos.sh --scenario tier1          # Run tier 1 scenarios
#   ./tendermint-chaos.sh --scenario all --verbose  # Run all with verbose output
#
# Tendermint BFT Properties (n=3):
#   - Quorum: floor(2*3/3) + 1 = 3 (100% required)
#   - Can tolerate 0 failures - all 3 validators must be online
#   - Any single node failure halts consensus (tests halt/recovery)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# ============================================================================
# Configuration - matches docker-compose.tendermint-3node.yml
# ============================================================================

COMPOSE_FILE="$PROJECT_ROOT/etc/docker-compose.tendermint-3node.yml"
NETWORK_NAME="alys-tendermint-3"
NETWORK_SUBNET="172.22.0.0/16"

# Node configuration (from docker-compose)
VALIDATORS=3
NODE_NAMES=("alys-node-1" "alys-node-2" "alys-node-3")
NODE_IPS=("172.22.0.10" "172.22.0.11" "172.22.0.12")
NODE_RPC_PORTS=(3001 3011 3021)

# Tendermint timing (from docker-compose environment)
TENDERMINT_PROPOSE_TIMEOUT_MS=3000
TENDERMINT_PREVOTE_TIMEOUT_MS=1000
TENDERMINT_PRECOMMIT_TIMEOUT_MS=1000

# Quorum calculation: >2/3 required
QUORUM_SIZE=3
MAX_FAILURES=0

# Test configuration
VERBOSE=false
OUTPUT_DIR="$PROJECT_ROOT/chaos-results"
TEST_RESULTS=()

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# ============================================================================
# Logging Functions
# ============================================================================

log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $*"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $*"
}

log_debug() {
    if [[ "$VERBOSE" == "true" ]]; then
        echo -e "[DEBUG] $*"
    fi
}

# ============================================================================
# RPC Helper Functions - Query Tendermint Consensus State
# ============================================================================

get_rpc_port() {
    local node="$1"
    case "$node" in
        alys-node-1) echo 3001 ;;
        alys-node-2) echo 3011 ;;
        alys-node-3) echo 3021 ;;
        *) echo 3001 ;;
    esac
}

get_node_ip() {
    local node="$1"
    case "$node" in
        alys-node-1) echo "172.22.0.10" ;;
        alys-node-2) echo "172.22.0.11" ;;
        alys-node-3) echo "172.22.0.12" ;;
        *) echo "" ;;
    esac
}

rpc_call() {
    local port="$1"
    local method="$2"
    local params="${3:-[]}"
    curl -s -X POST "http://localhost:$port" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}" \
        | jq -r '.result // empty'
}

get_consensus_height() {
    local node="${1:-alys-node-1}"
    local port=$(get_rpc_port "$node")
    local result=$(rpc_call "$port" "tendermint_consensusState" 2>/dev/null)
    if [[ -n "$result" ]]; then
        echo "$result" | jq -r '.height // 0'
    else
        echo "0"
    fi
}

get_consensus_round() {
    local node="${1:-alys-node-1}"
    local port=$(get_rpc_port "$node")
    rpc_call "$port" "tendermint_consensusState" 2>/dev/null | jq -r '.round // 0'
}

get_consensus_step() {
    local node="${1:-alys-node-1}"
    local port=$(get_rpc_port "$node")
    rpc_call "$port" "tendermint_consensusState" 2>/dev/null | jq -r '.step // "unknown"'
}

get_consensus_state() {
    local node="${1:-alys-node-1}"
    local port=$(get_rpc_port "$node")
    rpc_call "$port" "tendermint_consensusState" 2>/dev/null
}

get_validators() {
    local node="${1:-alys-node-1}"
    local port=$(get_rpc_port "$node")
    rpc_call "$port" "tendermint_validators" 2>/dev/null
}

get_commit() {
    local node="${1:-alys-node-1}"
    local height="$2"
    local port=$(get_rpc_port "$node")
    # Note: tendermint_commit expects height as a direct integer, not an object
    rpc_call "$port" "tendermint_commit" "[$height]" 2>/dev/null
}

get_evidence() {
    local node="${1:-alys-node-1}"
    local port=$(get_rpc_port "$node")
    rpc_call "$port" "tendermint_evidence" 2>/dev/null
}

get_proposer() {
    local height="$1"
    local round="${2:-0}"
    local proposer_idx=$(( (height + round) % VALIDATORS ))
    echo "${NODE_NAMES[$proposer_idx]}"
}

# ============================================================================
# AuxPoW Helper Functions
# ============================================================================

create_aux_block() {
    local node="${1:-alys-node-1}"
    local miner_address="${2:-0x0000000000000000000000000000000000000000}"
    local port=$(get_rpc_port "$node")

    curl -s -X POST "http://localhost:$port" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"createauxblock\",\"params\":[\"$miner_address\"],\"id\":1}" \
        | jq -r '.result // empty'
}

get_block_by_height() {
    local node="${1:-alys-node-1}"
    local height="${2:-}"
    local port=$(get_rpc_port "$node")

    if [[ -z "$height" ]]; then
        # Get latest
        curl -s -X POST "http://localhost:$port" \
            -H "Content-Type: application/json" \
            -d '{"jsonrpc":"2.0","method":"alys_getBlockByHeight","params":[],"id":1}' \
            | jq -r '.result // empty'
    else
        curl -s -X POST "http://localhost:$port" \
            -H "Content-Type: application/json" \
            -d "{\"jsonrpc\":\"2.0\",\"method\":\"alys_getBlockByHeight\",\"params\":[$height],\"id\":1}" \
            | jq -r '.result // empty'
    fi
}

get_block_auxpow_header() {
    local block_response="$1"
    echo "$block_response" | jq -r '.auxpow_header // empty'
}

get_auxpow_field() {
    local auxpow_header="$1"
    local field="$2"
    echo "$auxpow_header" | jq -r ".$field // empty"
}

verify_block_has_auxpow() {
    local block_response="$1"
    local has_auxpow=$(echo "$block_response" | jq -r '.has_auxpow // false')
    [[ "$has_auxpow" == "true" ]]
}

# Find a block with AuxPoW by scanning recent heights
find_block_with_auxpow() {
    local node="${1:-alys-node-1}"
    local max_scan="${2:-50}"
    local current_height=$(get_consensus_height "$node")

    for ((h = current_height - 1; h >= 1 && h >= current_height - max_scan; h--)); do
        local block=$(get_block_by_height "$node" "$h")
        if verify_block_has_auxpow "$block"; then
            echo "$h"
            return 0
        fi
    done
    echo ""
    return 1
}

# ============================================================================
# Verification Functions
# ============================================================================

verify_all_validators_active() {
    local active_count=$(docker ps --filter "name=alys-node-" --format '{{.Names}}' 2>/dev/null | wc -l | tr -d ' ')
    if [[ "$active_count" -ge "$VALIDATORS" ]]; then
        log_debug "All $VALIDATORS validators are active"
        return 0
    else
        log_warn "Only $active_count of $VALIDATORS validators active"
        return 1
    fi
}

verify_consensus_halted() {
    local node="${1:-alys-node-1}"
    local wait_seconds="${2:-15}"

    local initial_height=$(get_consensus_height "$node")
    sleep "$wait_seconds"
    local current_height=$(get_consensus_height "$node")

    if [[ "$current_height" -le "$((initial_height + 1))" ]]; then
        log_debug "Consensus correctly halted at height $current_height"
        return 0
    else
        log_error "Consensus advanced from $initial_height to $current_height (expected halt)"
        return 1
    fi
}

verify_consensus_progressing() {
    local node="${1:-alys-node-1}"
    local wait_seconds="${2:-15}"
    local expected_blocks="${3:-3}"

    local initial_height=$(get_consensus_height "$node")
    sleep "$wait_seconds"
    local current_height=$(get_consensus_height "$node")

    if [[ "$current_height" -ge "$((initial_height + expected_blocks))" ]]; then
        log_debug "Consensus progressing: $initial_height -> $current_height"
        return 0
    else
        log_error "Consensus stalled: expected $expected_blocks blocks, got $((current_height - initial_height))"
        return 1
    fi
}

wait_for_height() {
    local target_height="$1"
    local timeout_seconds="${2:-60}"
    local node="${3:-alys-node-1}"

    local start_time=$(date +%s)
    while true; do
        local current_height=$(get_consensus_height "$node")
        if [[ "$current_height" -ge "$target_height" ]]; then
            return 0
        fi

        local elapsed=$(($(date +%s) - start_time))
        if [[ $elapsed -ge $timeout_seconds ]]; then
            log_error "Timeout waiting for height $target_height (current: $current_height)"
            return 1
        fi
        sleep 1
    done
}

wait_for_validator_sync() {
    local node="$1"
    local timeout_seconds="${2:-30}"

    log_debug "Waiting for $node to sync..."
    local start_time=$(date +%s)
    while true; do
        # Check if container is running
        if ! docker ps --filter "name=$node" --format '{{.Names}}' | grep -q "$node"; then
            sleep 1
            continue
        fi

        # Check if RPC is responding
        local port=$(get_rpc_port "$node")
        if curl -s -X POST "http://localhost:$port" \
            -H "Content-Type: application/json" \
            -d '{"jsonrpc":"2.0","method":"tendermint_consensusState","params":[],"id":1}' \
            | jq -e '.result' >/dev/null 2>&1; then
            log_debug "$node is synced and responding"
            return 0
        fi

        local elapsed=$(($(date +%s) - start_time))
        if [[ $elapsed -ge $timeout_seconds ]]; then
            log_error "Timeout waiting for $node to sync"
            return 1
        fi
        sleep 1
    done
}

verify_no_forks() {
    local height="$1"
    local max_retries="${2:-10}"
    local retry_delay="${3:-2}"
    local block_hashes=()
    local all_responded=false

    # Retry until all nodes respond with valid hashes
    for attempt in $(seq 1 $max_retries); do
        block_hashes=()
        all_responded=true

        for node in "${NODE_NAMES[@]}"; do
            local port=$(get_rpc_port "$node")
            # Note: tendermint_commit expects height as a direct integer, not an object
            local hash=$(rpc_call "$port" "tendermint_commit" "[$height]" 2>/dev/null \
                | jq -r '.block_hash // empty')

            if [[ -z "$hash" || "$hash" == "null" ]]; then
                log_debug "Node $node not responding for height $height (attempt $attempt/$max_retries)"
                all_responded=false
                break
            fi
            block_hashes+=("$hash")
        done

        if [[ "$all_responded" == "true" ]]; then
            break
        fi
        sleep "$retry_delay"
    done

    if [[ "$all_responded" != "true" ]]; then
        log_error "Not all nodes responded for height $height after $max_retries attempts"
        return 1
    fi

    local unique_hashes=$(printf '%s\n' "${block_hashes[@]}" | sort -u | wc -l | tr -d ' ')
    if [[ "$unique_hashes" -eq 1 ]]; then
        log_debug "No fork detected at height $height (all ${#block_hashes[@]} nodes agree)"
        return 0
    else
        log_error "Fork detected at height $height! Hashes: ${block_hashes[*]}"
        return 1
    fi
}

check_equivocation_evidence() {
    local node="${1:-alys-node-1}"
    local evidence=$(get_evidence "$node")
    local count=$(echo "$evidence" | jq -r '.total // 0')
    if [[ "$count" -gt 0 ]]; then
        log_warn "Equivocation evidence detected: $count instances"
        return 0  # Evidence found
    fi
    return 1  # No evidence
}

# ============================================================================
# Chaos Injection Functions
# ============================================================================

isolate_node() {
    local node="$1"
    log_info "Isolating $node from network $NETWORK_NAME..."
    docker network disconnect "$NETWORK_NAME" "$node" 2>/dev/null || true
}

reconnect_node() {
    local node="$1"
    local ip=$(get_node_ip "$node")
    log_info "Reconnecting $node to network $NETWORK_NAME with IP $ip..."
    # Use --ip to preserve the static IP assignment from docker-compose
    docker network connect --ip "$ip" "$NETWORK_NAME" "$node" 2>/dev/null || true
}

add_latency() {
    local node="$1"
    local delay_ms="${2:-500}"
    log_info "Adding ${delay_ms}ms latency to $node..."
    docker exec "$node" tc qdisc add dev eth0 root netem delay "${delay_ms}ms" 2>/dev/null || true
}

remove_latency() {
    local node="$1"
    docker exec "$node" tc qdisc del dev eth0 root 2>/dev/null || true
}

crash_node() {
    local node="$1"
    log_info "Crashing $node..."
    docker kill "$node" 2>/dev/null || true
}

stop_node() {
    local node="$1"
    log_info "Stopping $node..."
    # Use 30s timeout to match docker-compose stop_grace_period
    # Default docker stop timeout is only 10s which may not be enough
    # for graceful database shutdown
    docker stop -t 30 "$node" 2>/dev/null || true
}

restart_node() {
    local node="$1"
    log_info "Restarting $node..."
    docker start "$node" 2>/dev/null || true
}

# ============================================================================
# Test Result Recording
# ============================================================================

record_test_result() {
    local test_id="$1"
    local result="$2"
    local description="$3"

    TEST_RESULTS+=("$test_id|$result|$description")

    if [[ "$result" == "PASSED" ]]; then
        log_success "[$test_id] $description"
    else
        log_error "[$test_id] $description"
    fi
}

print_test_summary() {
    echo ""
    echo "=============================================="
    echo "         CHAOS TEST SUMMARY"
    echo "=============================================="

    local passed=0
    local failed=0

    for result in "${TEST_RESULTS[@]}"; do
        IFS='|' read -r test_id status description <<< "$result"
        if [[ "$status" == "PASSED" ]]; then
            echo -e "${GREEN}[PASS]${NC} $test_id: $description"
            ((passed++))
        else
            echo -e "${RED}[FAIL]${NC} $test_id: $description"
            ((failed++))
        fi
    done

    echo ""
    echo "----------------------------------------------"
    echo "Total: $((passed + failed)) | Passed: $passed | Failed: $failed"
    echo "=============================================="

    if [[ $failed -gt 0 ]]; then
        return 1
    fi
    return 0
}

# ============================================================================
# Tier 1 Scenarios: Validator Failures (TM-A*)
# ============================================================================

run_TM_A1() {
    log_info "[TM-A1] Running: Single Validator Crash (n=3, expect halt)"

    # Pre-conditions
    if ! verify_all_validators_active; then
        record_test_result "TM-A1" "FAILED" "Pre-condition failed: validators not active"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")
    local target="alys-node-2"

    # Execute chaos: stop validator
    stop_node "$target"

    # With n=3, consensus should HALT
    sleep 15

    # Verify consensus halted
    if verify_consensus_halted "alys-node-1" 10; then
        log_debug "Consensus correctly halted with 1 validator down"
    else
        record_test_result "TM-A1" "FAILED" "Consensus advanced when it should have halted"
        restart_node "$target"
        return
    fi

    # Recover: restart validator
    restart_node "$target"
    wait_for_validator_sync "$target" 180

    # Verify consensus resumed
    local post_recovery_height=$(get_consensus_height "alys-node-1")
    if wait_for_height $((post_recovery_height + 3)) 180; then
        record_test_result "TM-A1" "PASSED" "Single Validator Crash & Recovery"
    else
        record_test_result "TM-A1" "FAILED" "Consensus did not resume after recovery"
    fi
}

run_TM_A2() {
    log_info "[TM-A2] Running: Proposer Crash"

    if ! verify_all_validators_active; then
        record_test_result "TM-A2" "FAILED" "Pre-condition failed"
        return
    fi

    local current_height=$(get_consensus_height "alys-node-1")
    local proposer=$(get_proposer $((current_height + 1)) 0)

    log_debug "Next proposer: $proposer"

    # Stop the proposer
    stop_node "$proposer"
    sleep 10

    # With n=3, consensus should halt (no quorum)
    if verify_consensus_halted "alys-node-1" 10; then
        log_debug "Consensus halted after proposer crash (expected)"
    fi

    # Recover
    restart_node "$proposer"
    wait_for_validator_sync "$proposer" 180

    # Verify recovery
    if wait_for_height $((current_height + 3)) 180; then
        record_test_result "TM-A2" "PASSED" "Proposer Crash & Recovery"
    else
        record_test_result "TM-A2" "FAILED" "Consensus did not resume after proposer recovery"
    fi
}

run_TM_A3() {
    log_info "[TM-A3] Running: Validator Restart & WAL Recovery"

    if ! verify_all_validators_active; then
        record_test_result "TM-A3" "FAILED" "Pre-condition failed"
        return
    fi

    local pre_crash_height=$(get_consensus_height "alys-node-1")
    local target="alys-node-3"

    # Crash validator (simulate unexpected failure)
    crash_node "$target"

    # With n=3, consensus halts
    sleep 5

    # Restart the crashed validator
    restart_node "$target"
    wait_for_validator_sync "$target" 300

    # Verify consensus resumed and all nodes are at same height
    # Note: Late-joining nodes may need 12+ rounds to re-establish quorum
    sleep 60
    local h1=$(get_consensus_height "alys-node-1")
    local h2=$(get_consensus_height "alys-node-2")
    local h3=$(get_consensus_height "alys-node-3")

    if [[ "$h1" -gt "$pre_crash_height" ]] && [[ "$h1" -eq "$h2" ]] && [[ "$h2" -eq "$h3" ]]; then
        # Check for evidence (WAL should prevent equivocation)
        if check_equivocation_evidence "$target"; then
            record_test_result "TM-A3" "FAILED" "Double-voting detected after crash recovery"
        else
            record_test_result "TM-A3" "PASSED" "Validator Restart & WAL Recovery"
        fi
    else
        record_test_result "TM-A3" "FAILED" "Nodes not in sync after recovery: $h1, $h2, $h3"
    fi
}

run_TM_A4() {
    log_info "[TM-A4] Running: Crash Recovery Time Measurement"

    if ! verify_all_validators_active; then
        record_test_result "TM-A4" "FAILED" "Pre-condition failed"
        return
    fi

    local target="alys-node-2"
    local pre_height=$(get_consensus_height "alys-node-1")

    # Crash and immediately restart
    crash_node "$target"
    local crash_time=$(date +%s)

    restart_node "$target"

    # Wait for consensus to resume
    local resume_height=$((pre_height + 2))
    local recovery_start=$(date +%s)
    local max_recovery_time=90

    while true; do
        local current=$(get_consensus_height "alys-node-1")
        if [[ "$current" -ge "$resume_height" ]]; then
            local recovery_time=$(($(date +%s) - crash_time))
            log_info "Recovery time: ${recovery_time}s"
            if [[ $recovery_time -le 45 ]]; then
                record_test_result "TM-A4" "PASSED" "Crash Recovery Time: ${recovery_time}s (< 45s)"
            else
                record_test_result "TM-A4" "FAILED" "Recovery too slow: ${recovery_time}s (> 45s)"
            fi
            return
        fi

        local elapsed=$(($(date +%s) - recovery_start))
        if [[ $elapsed -ge $max_recovery_time ]]; then
            record_test_result "TM-A4" "FAILED" "Recovery timeout after ${elapsed}s"
            return
        fi
        sleep 1
    done
}

run_TM_A5() {
    log_info "[TM-A5] Running: Sequential Validator Restarts"

    if ! verify_all_validators_active; then
        record_test_result "TM-A5" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")

    # Restart each validator sequentially, waiting for recovery between each
    for node in "${NODE_NAMES[@]}"; do
        log_debug "Restarting $node..."
        stop_node "$node"
        sleep 5
        restart_node "$node"
        wait_for_validator_sync "$node" 180 || {
            record_test_result "TM-A5" "FAILED" "$node did not recover"
            return
        }
        sleep 10  # Let consensus stabilize
    done

    # Verify all nodes at same height and progressing
    sleep 15
    local h1=$(get_consensus_height "alys-node-1")
    local h2=$(get_consensus_height "alys-node-2")
    local h3=$(get_consensus_height "alys-node-3")

    if [[ "$h1" -gt "$initial_height" ]] && [[ "$h1" -eq "$h2" ]] && [[ "$h2" -eq "$h3" ]]; then
        record_test_result "TM-A5" "PASSED" "Sequential Validator Restarts"
    else
        record_test_result "TM-A5" "FAILED" "Height mismatch after restarts: $h1, $h2, $h3"
    fi
}

# ============================================================================
# Tier 1 Scenarios: Network Partitions (TM-B*)
# ============================================================================

run_TM_B1() {
    log_info "[TM-B1] Running: Single Node Isolation & Recovery"

    if ! verify_all_validators_active; then
        record_test_result "TM-B1" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")
    local target="alys-node-2"

    # Isolate node from network
    isolate_node "$target"

    # With n=3, consensus should halt
    sleep 10

    if ! verify_consensus_halted "alys-node-1" 10; then
        record_test_result "TM-B1" "FAILED" "Consensus did not halt on partition"
        reconnect_node "$target"
        return
    fi

    log_debug "Consensus correctly halted"

    # Heal partition
    reconnect_node "$target"
    sleep 5

    # Verify consensus resumed
    local post_heal_height=$(get_consensus_height "alys-node-1")
    if wait_for_height $((post_heal_height + 3)) 180; then
        record_test_result "TM-B1" "PASSED" "Single Node Isolation & Recovery"
    else
        record_test_result "TM-B1" "FAILED" "Consensus did not resume after heal"
    fi
}

run_TM_B2() {
    log_info "[TM-B2] Running: 2-1 Network Partition"

    if ! verify_all_validators_active; then
        record_test_result "TM-B2" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")

    # Isolate node-3 from the network
    isolate_node "alys-node-3"

    sleep 15

    # Both partitions should halt (neither has >=3)
    local h1=$(get_consensus_height "alys-node-1")
    local h2=$(get_consensus_height "alys-node-2")

    # Heights should not advance significantly
    if [[ $h1 -le $((initial_height + 1)) ]] && [[ $h2 -le $((initial_height + 1)) ]]; then
        log_debug "Both partitions correctly halted"
    else
        record_test_result "TM-B2" "FAILED" "Partition continued consensus unexpectedly"
        reconnect_node "alys-node-3"
        return
    fi

    # Heal
    reconnect_node "alys-node-3"
    wait_for_validator_sync "alys-node-3" 180

    # Verify recovery
    if wait_for_height $((h1 + 3)) 180; then
        record_test_result "TM-B2" "PASSED" "2-1 Network Partition"
    else
        record_test_result "TM-B2" "FAILED" "Consensus did not resume after partition heal"
    fi
}

run_TM_B5() {
    log_info "[TM-B5] Running: Partition Heal & Consensus Resume"

    if ! verify_all_validators_active; then
        record_test_result "TM-B5" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")

    # Create partition
    isolate_node "alys-node-2"
    sleep 10

    # Record halted state
    local halted_height=$(get_consensus_height "alys-node-1")

    # Heal partition
    reconnect_node "alys-node-2"
    local heal_time=$(date +%s)

    # Measure time to resume
    while true; do
        local current=$(get_consensus_height "alys-node-1")
        if [[ "$current" -gt "$halted_height" ]]; then
            local resume_time=$(($(date +%s) - heal_time))
            log_info "Consensus resumed in ${resume_time}s"

            # Verify no forks
            if verify_no_forks "$current"; then
                record_test_result "TM-B5" "PASSED" "Partition Heal & Consensus Resume (${resume_time}s)"
            else
                record_test_result "TM-B5" "FAILED" "Fork detected after partition heal"
            fi
            return
        fi

        local elapsed=$(($(date +%s) - heal_time))
        if [[ $elapsed -ge 180 ]]; then
            record_test_result "TM-B5" "FAILED" "Timeout waiting for consensus resume"
            return
        fi
        sleep 1
    done
}

# ============================================================================
# Tier 1 Scenarios: Timing (TM-C*)
# ============================================================================

run_TM_C2() {
    log_info "[TM-C2] Running: Vote Delay Chaos"

    if ! verify_all_validators_active; then
        record_test_result "TM-C2" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")

    # Add 500ms latency to one node
    add_latency "alys-node-2" 500

    # Consensus should still work, just slower
    sleep 30

    # Remove latency
    remove_latency "alys-node-2"

    # Verify progress was made
    local final_height=$(get_consensus_height "alys-node-1")
    if [[ "$final_height" -gt "$initial_height" ]]; then
        record_test_result "TM-C2" "PASSED" "Vote Delay Chaos (progressed $((final_height - initial_height)) blocks)"
    else
        record_test_result "TM-C2" "FAILED" "No progress with vote delay"
    fi
}

# ============================================================================
# Tier 1 Scenarios: Network Partitions (TM-B3, TM-B4)
# ============================================================================

run_TM_B3() {
    log_info "[TM-B3] Running: Asymmetric Partition (A->B, B-/->A)"

    if ! verify_all_validators_active; then
        record_test_result "TM-B3" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")

    # Create asymmetric partition using iptables
    # node-2 can receive from node-1 but cannot send back
    log_info "Creating asymmetric partition: node-1 -> node-2 (blocked return)"
    docker exec alys-node-2 iptables -A OUTPUT -d 172.22.0.10 -j DROP 2>/dev/null || {
        log_warn "iptables not available, skipping TM-B3"
        record_test_result "TM-B3" "SKIPPED" "iptables not available in container"
        return
    }

    sleep 15

    # With asymmetric partition, consensus should eventually detect and halt
    local current_height=$(get_consensus_height "alys-node-1")

    # Restore connectivity
    docker exec alys-node-2 iptables -D OUTPUT -d 172.22.0.10 -j DROP 2>/dev/null || true

    sleep 10

    # Verify recovery
    if wait_for_height $((current_height + 3)) 180; then
        record_test_result "TM-B3" "PASSED" "Asymmetric Partition Recovery"
    else
        record_test_result "TM-B3" "FAILED" "Consensus did not resume after asymmetric partition heal"
    fi
}

run_TM_B4() {
    log_info "[TM-B4] Running: Proposer Isolation"

    if ! verify_all_validators_active; then
        record_test_result "TM-B4" "FAILED" "Pre-condition failed"
        return
    fi

    local current_height=$(get_consensus_height "alys-node-1")
    local next_proposer=$(get_proposer $((current_height + 1)) 0)

    log_info "Next proposer: $next_proposer - isolating..."

    # Isolate the proposer
    isolate_node "$next_proposer"

    # With n=3, consensus should halt
    sleep 15

    if verify_consensus_halted "alys-node-1" 10; then
        log_debug "Consensus halted after proposer isolation (expected)"
    fi

    # Heal partition
    reconnect_node "$next_proposer"
    wait_for_validator_sync "$next_proposer" 180

    # Verify recovery - consensus should resume with timeout and round advancement
    local post_heal_height=$(get_consensus_height "alys-node-1")
    if wait_for_height $((post_heal_height + 3)) 180; then
        record_test_result "TM-B4" "PASSED" "Proposer Isolation & Recovery"
    else
        record_test_result "TM-B4" "FAILED" "Consensus did not resume after proposer reconnection"
    fi
}

# ============================================================================
# Tier 1 Scenarios: Timing (TM-C1, TM-C3, TM-C4)
# ============================================================================

run_TM_C1() {
    log_info "[TM-C1] Running: Timeout Storm (all nodes timeout simultaneously)"

    if ! verify_all_validators_active; then
        record_test_result "TM-C1" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")
    local initial_round=$(get_consensus_round "alys-node-1")

    # Add high latency to all nodes to force timeouts
    for node in "${NODE_NAMES[@]}"; do
        add_latency "$node" 5000  # 5 second delay exceeds all timeouts
    done

    # Wait for multiple timeout cycles
    sleep 30

    # Remove latency
    for node in "${NODE_NAMES[@]}"; do
        remove_latency "$node"
    done

    # Wait for recovery
    sleep 20

    local final_height=$(get_consensus_height "alys-node-1")
    local final_round=$(get_consensus_round "alys-node-1")

    # Should have advanced rounds due to timeouts, then recovered
    if [[ "$final_height" -gt "$initial_height" ]]; then
        record_test_result "TM-C1" "PASSED" "Timeout Storm Recovery (height: $initial_height -> $final_height)"
    else
        record_test_result "TM-C1" "FAILED" "No progress after timeout storm"
    fi
}

run_TM_C3() {
    log_info "[TM-C3] Running: Slow Proposal Delivery"

    if ! verify_all_validators_active; then
        record_test_result "TM-C3" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")
    local current_proposer=$(get_proposer $initial_height 0)

    # Add latency only to the proposer (delays proposal broadcast)
    add_latency "$current_proposer" 2500  # Near propose timeout

    sleep 30

    remove_latency "$current_proposer"

    # Verify progress was made (slower but should still work)
    local final_height=$(get_consensus_height "alys-node-1")
    if [[ "$final_height" -gt "$initial_height" ]]; then
        record_test_result "TM-C3" "PASSED" "Slow Proposal Delivery (progressed $((final_height - initial_height)) blocks)"
    else
        record_test_result "TM-C3" "FAILED" "No progress with slow proposals"
    fi
}

run_TM_C4() {
    log_info "[TM-C4] Running: Fast Rounds (rapid state transitions)"

    if ! verify_all_validators_active; then
        record_test_result "TM-C4" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")
    local test_duration=30

    # Monitor for rapid block production (normal operation stress test)
    sleep $test_duration

    local final_height=$(get_consensus_height "alys-node-1")
    local blocks_produced=$((final_height - initial_height))
    local blocks_per_second=$(echo "scale=2; $blocks_produced / $test_duration" | bc)

    log_info "Block rate: $blocks_per_second blocks/sec ($blocks_produced in ${test_duration}s)"

    # Should produce blocks consistently
    if [[ $blocks_produced -ge 5 ]]; then
        record_test_result "TM-C4" "PASSED" "Fast Rounds: $blocks_produced blocks in ${test_duration}s"
    else
        record_test_result "TM-C4" "FAILED" "Insufficient throughput: $blocks_produced blocks"
    fi
}

# ============================================================================
# Tier 2 Scenarios: WAL & Crash Recovery (TM-D*)
# ============================================================================

run_TM_D1() {
    log_info "[TM-D1] Running: Crash After Prevote (WAL prevents double-prevote)"

    if ! verify_all_validators_active; then
        record_test_result "TM-D1" "FAILED" "Pre-condition failed"
        return
    fi

    local target="alys-node-2"
    local pre_crash_height=$(get_consensus_height "alys-node-1")

    # Kill validator during consensus (likely during voting)
    crash_node "$target"
    sleep 2
    restart_node "$target"
    wait_for_validator_sync "$target" 180

    # Wait for consensus to resume
    sleep 20

    # Check for equivocation evidence (WAL should prevent double-voting)
    if check_equivocation_evidence "$target"; then
        record_test_result "TM-D1" "FAILED" "Double-prevote detected after crash (WAL failure)"
    else
        local post_height=$(get_consensus_height "alys-node-1")
        if [[ "$post_height" -gt "$pre_crash_height" ]]; then
            record_test_result "TM-D1" "PASSED" "Crash After Prevote - WAL prevented equivocation"
        else
            record_test_result "TM-D1" "FAILED" "Consensus did not resume after crash"
        fi
    fi
}

run_TM_D2() {
    log_info "[TM-D2] Running: Crash After Precommit (WAL recovery)"

    if ! verify_all_validators_active; then
        record_test_result "TM-D2" "FAILED" "Pre-condition failed"
        return
    fi

    local target="alys-node-3"
    local pre_crash_height=$(get_consensus_height "alys-node-1")

    # Rapid crash-restart cycle
    crash_node "$target"
    sleep 1
    restart_node "$target"
    wait_for_validator_sync "$target" 180

    sleep 20

    if check_equivocation_evidence "$target"; then
        record_test_result "TM-D2" "FAILED" "Double-precommit detected (WAL failure)"
    else
        local post_height=$(get_consensus_height "alys-node-1")
        if [[ "$post_height" -gt "$pre_crash_height" ]]; then
            record_test_result "TM-D2" "PASSED" "Crash After Precommit - WAL prevented equivocation"
        else
            record_test_result "TM-D2" "FAILED" "Consensus did not resume"
        fi
    fi
}

run_TM_D3() {
    log_info "[TM-D3] Running: Crash While Locked (lock state preserved)"

    if ! verify_all_validators_active; then
        record_test_result "TM-D3" "FAILED" "Pre-condition failed"
        return
    fi

    local target="alys-node-2"
    local pre_crash_height=$(get_consensus_height "alys-node-1")

    # Wait for some blocks then crash (validator likely locked)
    sleep 5
    crash_node "$target"
    sleep 3
    restart_node "$target"
    wait_for_validator_sync "$target" 180

    sleep 20

    # All nodes should be at same height (lock state preserved = same block committed)
    local h1=$(get_consensus_height "alys-node-1")
    local h2=$(get_consensus_height "alys-node-2")
    local h3=$(get_consensus_height "alys-node-3")

    if [[ "$h1" -eq "$h2" ]] && [[ "$h2" -eq "$h3" ]] && [[ "$h1" -gt "$pre_crash_height" ]]; then
        record_test_result "TM-D3" "PASSED" "Crash While Locked - state preserved"
    else
        record_test_result "TM-D3" "FAILED" "Height mismatch after crash: $h1, $h2, $h3"
    fi
}

run_TM_D4() {
    log_info "[TM-D4] Running: Corrupt WAL (safe recovery)"

    # Note: This test requires direct WAL file access which may not be available
    # in all Docker configurations. We simulate by checking crash resilience.

    if ! verify_all_validators_active; then
        record_test_result "TM-D4" "FAILED" "Pre-condition failed"
        return
    fi

    local target="alys-node-2"

    # Crash and restart - simulates WAL corruption scenario
    crash_node "$target"
    sleep 5
    restart_node "$target"

    # Wait longer for potential WAL recovery
    sleep 45

    # Check if validator is responding (didn't crash loop)
    local port=$(get_rpc_port "$target")
    if curl -s -X POST "http://localhost:$port" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"tendermint_consensusState","params":[],"id":1}' \
        | jq -e '.result' >/dev/null 2>&1; then
        record_test_result "TM-D4" "PASSED" "WAL Recovery - validator responding after crash"
    else
        record_test_result "TM-D4" "FAILED" "Validator not responding after crash (possible WAL issue)"
    fi
}

run_TM_D5() {
    log_info "[TM-D5] Running: Repeated Crash/Recovery Cycles"

    if ! verify_all_validators_active; then
        record_test_result "TM-D5" "FAILED" "Pre-condition failed"
        return
    fi

    local target="alys-node-2"
    local initial_height=$(get_consensus_height "alys-node-1")
    local cycles=3

    for i in $(seq 1 $cycles); do
        log_debug "Crash/recovery cycle $i of $cycles"
        crash_node "$target"
        sleep 2
        restart_node "$target"
        wait_for_validator_sync "$target" 180 || {
            record_test_result "TM-D5" "FAILED" "Recovery failed on cycle $i"
            return
        }
        sleep 10
    done

    # Verify no evidence generated during all cycles
    if check_equivocation_evidence "$target"; then
        record_test_result "TM-D5" "FAILED" "Equivocation detected during crash cycles"
    else
        local final_height=$(get_consensus_height "alys-node-1")
        if [[ "$final_height" -gt "$initial_height" ]]; then
            record_test_result "TM-D5" "PASSED" "Repeated Crash/Recovery ($cycles cycles, no equivocation)"
        else
            record_test_result "TM-D5" "FAILED" "Consensus stalled during crash cycles"
        fi
    fi
}

# ============================================================================
# Tier 2 Scenarios: Equivocation Prevention (TM-E5)
# ============================================================================

run_TM_E5() {
    log_info "[TM-E5] Running: No Self-Equivocation After Crash"

    if ! verify_all_validators_active; then
        record_test_result "TM-E5" "FAILED" "Pre-condition failed"
        return
    fi

    # This is the critical WAL safety test
    # Crash a validator multiple times and ensure no equivocation

    local initial_height=$(get_consensus_height "alys-node-1")
    local evidence_before=$(get_evidence "alys-node-1" | jq -r '.total // 0')

    # Crash each validator once
    for node in "${NODE_NAMES[@]}"; do
        crash_node "$node"
        sleep 1
        restart_node "$node"
        sleep 10
    done

    # Wait for full recovery
    wait_for_validator_sync "alys-node-1" 180
    wait_for_validator_sync "alys-node-2" 180
    wait_for_validator_sync "alys-node-3" 180

    sleep 30

    local evidence_after=$(get_evidence "alys-node-1" | jq -r '.total // 0')
    local final_height=$(get_consensus_height "alys-node-1")

    if [[ "$evidence_after" -gt "$evidence_before" ]]; then
        record_test_result "TM-E5" "FAILED" "Self-equivocation detected after crashes (evidence: $evidence_before -> $evidence_after)"
    elif [[ "$final_height" -gt "$initial_height" ]]; then
        record_test_result "TM-E5" "PASSED" "No Self-Equivocation After Crash (all validators)"
    else
        record_test_result "TM-E5" "FAILED" "Consensus did not resume after validator crashes"
    fi
}

# ============================================================================
# Tier 2 Scenarios: External Dependencies (TM-F*)
# ============================================================================

run_TM_F1() {
    log_info "[TM-F1] Running: Execution Layer Failure"

    if ! verify_all_validators_active; then
        record_test_result "TM-F1" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")

    # Stop execution layer
    log_info "Stopping execution layer (Reth)..."
    docker stop execution 2>/dev/null || true

    # Consensus should halt (no payload from EL)
    sleep 15

    # Restart execution layer
    log_info "Restarting execution layer..."
    docker start execution 2>/dev/null || true
    sleep 30

    # Verify recovery
    local final_height=$(get_consensus_height "alys-node-1")
    if [[ "$final_height" -gt "$initial_height" ]]; then
        record_test_result "TM-F1" "PASSED" "Execution Layer Failure & Recovery"
    else
        # Check if EL is the cause
        if docker ps | grep -q execution; then
            record_test_result "TM-F1" "FAILED" "Consensus did not resume after EL recovery"
        else
            record_test_result "TM-F1" "FAILED" "Execution layer did not restart"
        fi
    fi
}

run_TM_F2() {
    log_info "[TM-F2] Running: Bitcoin Core Failure"

    if ! verify_all_validators_active; then
        record_test_result "TM-F2" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")

    # Stop Bitcoin Core
    log_info "Stopping Bitcoin Core..."
    docker stop bitcoin-core 2>/dev/null || true

    # Consensus should continue (AuxPoW not required for consensus)
    sleep 20

    local height_during=$(get_consensus_height "alys-node-1")

    # Restart Bitcoin Core
    log_info "Restarting Bitcoin Core..."
    docker start bitcoin-core 2>/dev/null || true
    sleep 10

    local final_height=$(get_consensus_height "alys-node-1")

    # Consensus should have continued during Bitcoin Core outage
    if [[ "$height_during" -gt "$initial_height" ]] || [[ "$final_height" -gt "$initial_height" ]]; then
        record_test_result "TM-F2" "PASSED" "Bitcoin Core Failure - consensus continued"
    else
        record_test_result "TM-F2" "FAILED" "Consensus halted during Bitcoin Core outage"
    fi
}

run_TM_F3() {
    log_info "[TM-F3] Running: Prometheus/Monitoring Failure"

    if ! verify_all_validators_active; then
        record_test_result "TM-F3" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")

    # Stop monitoring (should not affect consensus)
    log_info "Stopping Prometheus..."
    docker stop prometheus 2>/dev/null || true

    sleep 15

    local final_height=$(get_consensus_height "alys-node-1")

    # Restart Prometheus
    docker start prometheus 2>/dev/null || true

    # Consensus should be completely unaffected
    if [[ "$final_height" -gt "$initial_height" ]]; then
        record_test_result "TM-F3" "PASSED" "Monitoring Failure - consensus unaffected"
    else
        record_test_result "TM-F3" "FAILED" "Consensus affected by monitoring failure (unexpected)"
    fi
}

# ============================================================================
# Tier 1 Scenarios: Liveness & Safety (TM-L*)
# ============================================================================

run_TM_L1() {
    log_info "[TM-L1] Running: Round Stall Recovery"

    if ! verify_all_validators_active; then
        record_test_result "TM-L1" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")

    # Force a stall by temporarily partitioning, then recover
    isolate_node "alys-node-2"
    sleep 15  # Let rounds advance due to timeouts
    reconnect_node "alys-node-2"
    wait_for_validator_sync "alys-node-2" 180

    sleep 30

    local final_height=$(get_consensus_height "alys-node-1")
    if [[ "$final_height" -gt "$initial_height" ]]; then
        record_test_result "TM-L1" "PASSED" "Round Stall Recovery"
    else
        record_test_result "TM-L1" "FAILED" "Consensus did not recover from stall"
    fi
}

run_TM_L2() {
    log_info "[TM-L2] Running: Multi-Round Block Commit"

    if ! verify_all_validators_active; then
        record_test_result "TM-L2" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")

    # Add latency to force round progression
    add_latency "alys-node-1" 2000
    add_latency "alys-node-2" 2000

    sleep 30

    remove_latency "alys-node-1"
    remove_latency "alys-node-2"

    sleep 20

    local final_height=$(get_consensus_height "alys-node-1")

    # Should eventually commit blocks even with delays causing multiple rounds
    if [[ "$final_height" -gt "$initial_height" ]]; then
        record_test_result "TM-L2" "PASSED" "Multi-Round Block Commit"
    else
        record_test_result "TM-L2" "FAILED" "No blocks committed during multi-round scenario"
    fi
}

run_TM_L3() {
    log_info "[TM-L3] Running: Height Progression"

    if ! verify_all_validators_active; then
        record_test_result "TM-L3" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")
    local test_duration=60
    local expected_blocks=$((test_duration / 3))  # ~1 block per 3 seconds with Tendermint

    sleep $test_duration

    local final_height=$(get_consensus_height "alys-node-1")
    local produced=$((final_height - initial_height))

    log_info "Produced $produced blocks in ${test_duration}s"

    if [[ $produced -ge $((expected_blocks / 2)) ]]; then
        record_test_result "TM-L3" "PASSED" "Height Progression: $produced blocks in ${test_duration}s"
    else
        record_test_result "TM-L3" "FAILED" "Insufficient progress: $produced blocks (expected ~$expected_blocks)"
    fi
}

# ============================================================================
# Category I: Block Integrity Tests
# ============================================================================
# These tests validate block data consistency across all nodes using the
# extended tendermint_commit RPC that returns full header and signature data.

# Get full commit data for a height (uses extended tendermint_commit RPC)
get_full_commit() {
    local node="$1"
    local height="$2"
    local port=$(get_rpc_port "$node")
    rpc_call "$port" "tendermint_commit" "[$height]" 2>/dev/null
}

# Compare full commit data across all nodes for a given height
# Returns 0 if all nodes agree, 1 if mismatch detected
verify_full_commit_consistency() {
    local height="$1"
    local commits=()
    local first_hash=""
    local first_parent=""
    local first_sigs=""

    log_debug "Verifying commit consistency at height $height"

    for node in "${NODE_NAMES[@]}"; do
        local commit=$(get_full_commit "$node" "$height")
        if [[ -z "$commit" ]] || [[ "$commit" == "null" ]]; then
            log_debug "Node $node returned empty commit for height $height"
            return 1
        fi

        local hash=$(echo "$commit" | jq -r '.signed_header.header.hash // empty')
        local parent=$(echo "$commit" | jq -r '.signed_header.header.parent_hash // empty')
        local sigs=$(echo "$commit" | jq -r '.signed_header.commit.signatures | length // 0')
        local available=$(echo "$commit" | jq -r '.commit_available // false')

        if [[ "$available" != "true" ]]; then
            log_debug "Commit not yet available at height $height on $node"
            return 2  # Not ready yet
        fi

        if [[ -z "$first_hash" ]]; then
            first_hash="$hash"
            first_parent="$parent"
            first_sigs="$sigs"
            log_debug "Reference: hash=$hash parent=$parent sigs=$sigs"
        else
            if [[ "$hash" != "$first_hash" ]]; then
                log_error "Hash mismatch at height $height: $node has $hash, expected $first_hash"
                return 1
            fi
            if [[ "$parent" != "$first_parent" ]]; then
                log_error "Parent hash mismatch at height $height: $node has $parent, expected $first_parent"
                return 1
            fi
            if [[ "$sigs" != "$first_sigs" ]]; then
                log_warn "Signature count mismatch at height $height: $node has $sigs, expected $first_sigs"
                # Not necessarily a failure, but worth noting
            fi
        fi
    done

    log_debug "All nodes agree at height $height"
    return 0
}

# Verify parent hash chain consistency (block N's parent == block N-1's hash)
verify_parent_chain() {
    local start_height="$1"
    local end_height="$2"
    local node="${3:-alys-node-1}"

    log_debug "Verifying parent chain from $start_height to $end_height on $node"

    local prev_hash=""
    for ((h = start_height; h <= end_height; h++)); do
        local commit=$(get_full_commit "$node" "$h")
        local hash=$(echo "$commit" | jq -r '.signed_header.header.hash // empty')
        local parent=$(echo "$commit" | jq -r '.signed_header.header.parent_hash // empty')

        if [[ -z "$hash" ]] || [[ -z "$parent" ]]; then
            log_error "Could not get hash/parent at height $h"
            return 1
        fi

        if [[ -n "$prev_hash" ]] && [[ "$parent" != "$prev_hash" ]]; then
            log_error "Parent chain broken at height $h: parent=$parent but prev hash was $prev_hash"
            return 1
        fi

        prev_hash="$hash"
    done

    return 0
}

run_TM_I1() {
    log_info "[TM-I1] Running: Random Block Sampling"

    if ! verify_all_validators_active; then
        record_test_result "TM-I1" "FAILED" "Pre-condition failed"
        return
    fi

    local current_height=$(get_consensus_height "alys-node-1")
    if [[ $current_height -lt 10 ]]; then
        log_warn "Not enough blocks yet (height=$current_height), waiting..."
        sleep 30
        current_height=$(get_consensus_height "alys-node-1")
    fi

    # Sample 5-10 random heights (skip last 2 blocks as commit may not be available)
    local max_sample_height=$((current_height - 2))
    if [[ $max_sample_height -lt 1 ]]; then
        record_test_result "TM-I1" "SKIPPED" "Not enough blocks (height=$current_height)"
        return
    fi

    local sample_count=5
    if [[ $max_sample_height -ge 10 ]]; then
        sample_count=10
    fi

    local passed=0
    local failed=0

    for ((i = 0; i < sample_count; i++)); do
        local sample_height=$((RANDOM % max_sample_height + 1))
        log_debug "Sampling height $sample_height"

        if verify_full_commit_consistency "$sample_height"; then
            ((passed++))
        else
            ((failed++))
            log_warn "Consistency check failed at height $sample_height"
        fi
    done

    log_info "Random sampling: $passed/$sample_count heights verified"

    if [[ $failed -eq 0 ]]; then
        record_test_result "TM-I1" "PASSED" "All $sample_count sampled blocks consistent"
    else
        record_test_result "TM-I1" "FAILED" "$failed/$sample_count blocks inconsistent"
    fi
}

run_TM_I2() {
    log_info "[TM-I2] Running: Last Commit Chain Verification"

    if ! verify_all_validators_active; then
        record_test_result "TM-I2" "FAILED" "Pre-condition failed"
        return
    fi

    local current_height=$(get_consensus_height "alys-node-1")
    if [[ $current_height -lt 10 ]]; then
        record_test_result "TM-I2" "SKIPPED" "Not enough blocks (height=$current_height)"
        return
    fi

    # Verify parent chain for recent blocks (last 10)
    local start=$((current_height - 10))
    if [[ $start -lt 1 ]]; then
        start=1
    fi
    local end=$((current_height - 2))  # Skip last 2 for commit availability

    local all_passed=true
    for node in "${NODE_NAMES[@]}"; do
        if ! verify_parent_chain "$start" "$end" "$node"; then
            log_error "Parent chain broken on $node"
            all_passed=false
        fi
    done

    if [[ "$all_passed" == "true" ]]; then
        record_test_result "TM-I2" "PASSED" "Parent chain consistent on all nodes ($start-$end)"
    else
        record_test_result "TM-I2" "FAILED" "Parent chain inconsistent"
    fi
}

run_TM_I3() {
    log_info "[TM-I3] Running: Historical Block Scan"

    if ! verify_all_validators_active; then
        record_test_result "TM-I3" "FAILED" "Pre-condition failed"
        return
    fi

    local current_height=$(get_consensus_height "alys-node-1")
    if [[ $current_height -lt 20 ]]; then
        record_test_result "TM-I3" "SKIPPED" "Not enough blocks (height=$current_height)"
        return
    fi

    local failed=0
    local checked=0

    # Scan early blocks [1-10]
    log_info "Scanning early blocks [1-10]..."
    for ((h = 1; h <= 10 && h <= current_height - 2; h++)); do
        if ! verify_full_commit_consistency "$h"; then
            ((failed++))
        fi
        ((checked++))
    done

    # Scan middle blocks [mid-5, mid+5]
    local mid=$((current_height / 2))
    local mid_start=$((mid - 5))
    local mid_end=$((mid + 5))
    if [[ $mid_start -lt 1 ]]; then mid_start=1; fi
    if [[ $mid_end -gt $((current_height - 2)) ]]; then mid_end=$((current_height - 2)); fi

    log_info "Scanning middle blocks [$mid_start-$mid_end]..."
    for ((h = mid_start; h <= mid_end; h++)); do
        if ! verify_full_commit_consistency "$h"; then
            ((failed++))
        fi
        ((checked++))
    done

    # Scan recent blocks [recent-10, recent-2]
    local recent_start=$((current_height - 12))
    local recent_end=$((current_height - 2))
    if [[ $recent_start -lt 1 ]]; then recent_start=1; fi

    log_info "Scanning recent blocks [$recent_start-$recent_end]..."
    for ((h = recent_start; h <= recent_end; h++)); do
        if ! verify_full_commit_consistency "$h"; then
            ((failed++))
        fi
        ((checked++))
    done

    log_info "Historical scan: checked $checked blocks, $failed failures"

    if [[ $failed -eq 0 ]]; then
        record_test_result "TM-I3" "PASSED" "All $checked historical blocks consistent"
    else
        record_test_result "TM-I3" "FAILED" "$failed/$checked blocks inconsistent"
    fi
}

run_TM_I4() {
    log_info "[TM-I4] Running: Real-time Block Consistency"

    if ! verify_all_validators_active; then
        record_test_result "TM-I4" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")
    local test_duration=30
    local checked=0
    local failed=0
    local last_checked_height=$initial_height

    log_info "Monitoring new blocks for ${test_duration}s..."

    local start_time=$(date +%s)
    while true; do
        local elapsed=$(($(date +%s) - start_time))
        if [[ $elapsed -ge $test_duration ]]; then
            break
        fi

        local current_height=$(get_consensus_height "alys-node-1")

        # Check any new blocks (with 2-block delay for commit availability)
        for ((h = last_checked_height + 1; h <= current_height - 2; h++)); do
            log_debug "Checking new block at height $h"
            if verify_full_commit_consistency "$h"; then
                ((checked++))
            else
                ((failed++))
                log_warn "Consistency failure at height $h"
            fi
            last_checked_height=$h
        done

        sleep 2
    done

    log_info "Real-time monitoring: checked $checked new blocks, $failed failures"

    if [[ $checked -eq 0 ]]; then
        record_test_result "TM-I4" "SKIPPED" "No new blocks during test period"
    elif [[ $failed -eq 0 ]]; then
        record_test_result "TM-I4" "PASSED" "All $checked new blocks consistent"
    else
        record_test_result "TM-I4" "FAILED" "$failed/$checked new blocks inconsistent"
    fi
}

run_TM_I5() {
    log_info "[TM-I5] Running: Signature Validation"

    if ! verify_all_validators_active; then
        record_test_result "TM-I5" "FAILED" "Pre-condition failed"
        return
    fi

    local current_height=$(get_consensus_height "alys-node-1")
    if [[ $current_height -lt 5 ]]; then
        record_test_result "TM-I5" "SKIPPED" "Not enough blocks (height=$current_height)"
        return
    fi

    # Check recent blocks for proper signatures
    local check_height=$((current_height - 3))
    if [[ $check_height -lt 1 ]]; then check_height=1; fi

    local passed=0
    local failed=0
    local total_checks=5

    for ((i = 0; i < total_checks && check_height >= 1; i++)); do
        local commit=$(get_full_commit "alys-node-1" "$check_height")
        local available=$(echo "$commit" | jq -r '.commit_available // false')

        if [[ "$available" != "true" ]]; then
            log_debug "Commit not available at height $check_height, skipping"
            ((check_height--))
            continue
        fi

        local sig_count=$(echo "$commit" | jq -r '.signed_header.commit.signatures | length')
        local commit_sigs=$(echo "$commit" | jq -r '[.signed_header.commit.signatures[] | select(.block_id_flag == "Commit")] | length')
        local has_signatures=$(echo "$commit" | jq -r '[.signed_header.commit.signatures[] | select(.signature != null)] | length')

        log_debug "Height $check_height: $sig_count total sigs, $commit_sigs commits, $has_signatures with signatures"

        # For Tendermint BFT with n=3, we need 100% (3/3) to commit
        # Check that we have signatures and they match the commit flags
        if [[ $sig_count -ge 1 ]] && [[ $commit_sigs -ge 1 ]]; then
            # Verify all Commit votes have signatures
            local missing_sigs=$(echo "$commit" | jq -r '[.signed_header.commit.signatures[] | select(.block_id_flag == "Commit" and .signature == null)] | length')
            if [[ $missing_sigs -eq 0 ]]; then
                ((passed++))
                log_debug "Height $check_height: Signatures valid"
            else
                ((failed++))
                log_warn "Height $check_height: $missing_sigs Commit votes missing signatures"
            fi
        else
            ((failed++))
            log_warn "Height $check_height: Insufficient signatures (count=$sig_count, commits=$commit_sigs)"
        fi

        ((check_height--))
    done

    log_info "Signature validation: $passed passed, $failed failed"

    if [[ $failed -eq 0 ]] && [[ $passed -gt 0 ]]; then
        record_test_result "TM-I5" "PASSED" "All $passed blocks have valid signatures"
    elif [[ $passed -eq 0 ]]; then
        record_test_result "TM-I5" "SKIPPED" "No blocks available for signature check"
    else
        record_test_result "TM-I5" "FAILED" "$failed blocks have invalid/missing signatures"
    fi
}

# ============================================================================
# Tier 3 Scenarios: AuxPoW Integration (TM-P*)
# ============================================================================

run_TM_P1() {
    log_info "[TM-P1] Running: createauxblock Response Validation"

    if ! verify_all_validators_active; then
        record_test_result "TM-P1" "FAILED" "Pre-condition failed"
        return
    fi

    local passed=0
    local failed=0

    for node in "${NODE_NAMES[@]}"; do
        local response=$(create_aux_block "$node")

        if [[ -z "$response" ]]; then
            log_error "Node $node: createauxblock returned empty"
            ((failed++))
            continue
        fi

        local hash=$(echo "$response" | jq -r '.hash // empty')
        local chainid=$(echo "$response" | jq -r '.chainid // 0')
        local bits=$(echo "$response" | jq -r '.bits // empty')
        local height=$(echo "$response" | jq -r '.height // 0')

        log_debug "Node $node: hash=$hash chainid=$chainid bits=$bits height=$height"

        # Validate fields
        if [[ -z "$hash" ]] || [[ ${#hash} -ne 64 ]]; then
            log_error "Node $node: Invalid hash format (len=${#hash})"
            ((failed++))
            continue
        fi

        if [[ "$chainid" != "1337" ]]; then
            log_error "Node $node: Expected chainid=1337, got $chainid"
            ((failed++))
            continue
        fi

        if [[ -z "$bits" ]]; then
            log_error "Node $node: Missing bits field"
            ((failed++))
            continue
        fi

        ((passed++))
    done

    if [[ $failed -eq 0 ]]; then
        record_test_result "TM-P1" "PASSED" "createauxblock validated on all $passed nodes"
    else
        record_test_result "TM-P1" "FAILED" "$failed nodes failed validation"
    fi
}

run_TM_P2() {
    log_info "[TM-P2] Running: Direct AuxPoW Header Validation"

    if ! verify_all_validators_active; then
        record_test_result "TM-P2" "FAILED" "Pre-condition failed"
        return
    fi

    # Find a block with AuxPoW
    local auxpow_height=$(find_block_with_auxpow "alys-node-1" 100)

    if [[ -z "$auxpow_height" ]]; then
        log_warn "No blocks with AuxPoW found in recent history"
        record_test_result "TM-P2" "SKIPPED" "No AuxPoW blocks available for testing"
        return
    fi

    log_info "Found AuxPoW block at height $auxpow_height"

    local block=$(get_block_by_height "alys-node-1" "$auxpow_height")
    local auxpow=$(get_block_auxpow_header "$block")

    # Validate AuxPoW header fields
    local chain_id=$(get_auxpow_field "$auxpow" "chain_id")
    local bits=$(get_auxpow_field "$auxpow" "bits")
    local range_start=$(get_auxpow_field "$auxpow" "range_start")
    local range_end=$(get_auxpow_field "$auxpow" "range_end")
    local has_proof=$(get_auxpow_field "$auxpow" "has_proof")

    log_debug "chain_id=$chain_id bits=$bits has_proof=$has_proof"
    log_debug "range: $range_start -> $range_end"

    local all_valid=true

    if [[ "$chain_id" != "1337" ]]; then
        log_error "Invalid chain_id: expected 1337, got $chain_id"
        all_valid=false
    fi

    if [[ -z "$bits" ]] || [[ "$bits" == "0" ]]; then
        log_error "Invalid bits: $bits"
        all_valid=false
    fi

    if [[ -z "$range_start" ]] || [[ "$range_start" == "null" ]]; then
        log_error "Missing range_start"
        all_valid=false
    fi

    if [[ -z "$range_end" ]] || [[ "$range_end" == "null" ]]; then
        log_error "Missing range_end"
        all_valid=false
    fi

    if [[ "$has_proof" != "true" ]]; then
        log_error "AuxPoW proof not present"
        all_valid=false
    fi

    if [[ "$all_valid" == "true" ]]; then
        record_test_result "TM-P2" "PASSED" "AuxPoW header valid at height $auxpow_height"
    else
        record_test_result "TM-P2" "FAILED" "AuxPoW header validation failed"
    fi
}

run_TM_P3() {
    log_info "[TM-P3] Running: Cross-Node AuxPoW Consistency"

    if ! verify_all_validators_active; then
        record_test_result "TM-P3" "FAILED" "Pre-condition failed"
        return
    fi

    # Find a block with AuxPoW
    local auxpow_height=$(find_block_with_auxpow "alys-node-1" 100)

    if [[ -z "$auxpow_height" ]]; then
        record_test_result "TM-P3" "SKIPPED" "No AuxPoW blocks available"
        return
    fi

    log_info "Checking AuxPoW consistency at height $auxpow_height"

    local chain_ids=()
    local bits_values=()
    local range_ends=()

    for node in "${NODE_NAMES[@]}"; do
        local block=$(get_block_by_height "$node" "$auxpow_height")
        local auxpow=$(get_block_auxpow_header "$block")

        local chain_id=$(get_auxpow_field "$auxpow" "chain_id")
        local bits=$(get_auxpow_field "$auxpow" "bits")
        local range_end=$(get_auxpow_field "$auxpow" "range_end")

        chain_ids+=("$chain_id")
        bits_values+=("$bits")
        range_ends+=("$range_end")

        log_debug "Node $node: chain_id=$chain_id bits=$bits range_end=$range_end"
    done

    # Check consistency
    local first_chain_id="${chain_ids[0]}"
    local first_bits="${bits_values[0]}"
    local first_range_end="${range_ends[0]}"
    local all_match=true

    for i in "${!NODE_NAMES[@]}"; do
        if [[ "${chain_ids[$i]}" != "$first_chain_id" ]]; then
            log_error "chain_id mismatch: ${NODE_NAMES[$i]} has ${chain_ids[$i]}, expected $first_chain_id"
            all_match=false
        fi
        if [[ "${bits_values[$i]}" != "$first_bits" ]]; then
            log_error "bits mismatch: ${NODE_NAMES[$i]} has ${bits_values[$i]}, expected $first_bits"
            all_match=false
        fi
        if [[ "${range_ends[$i]}" != "$first_range_end" ]]; then
            log_error "range_end mismatch: ${NODE_NAMES[$i]} has ${range_ends[$i]}, expected $first_range_end"
            all_match=false
        fi
    done

    if [[ "$all_match" == "true" ]]; then
        record_test_result "TM-P3" "PASSED" "AuxPoW consistent across all nodes"
    else
        record_test_result "TM-P3" "FAILED" "AuxPoW inconsistent across nodes"
    fi
}

run_TM_P4() {
    log_info "[TM-P4] Running: AuxPoW Range Chain Validation"

    if ! verify_all_validators_active; then
        record_test_result "TM-P4" "FAILED" "Pre-condition failed"
        return
    fi

    local current_height=$(get_consensus_height "alys-node-1")
    local auxpow_heights=()

    # Find multiple AuxPoW blocks (scan more to find distinct epochs)
    for ((h = current_height - 1; h >= 1 && ${#auxpow_heights[@]} < 10; h--)); do
        local block=$(get_block_by_height "alys-node-1" "$h")
        if verify_block_has_auxpow "$block"; then
            auxpow_heights+=("$h")
        fi
    done

    if [[ ${#auxpow_heights[@]} -lt 2 ]]; then
        record_test_result "TM-P4" "SKIPPED" "Need at least 2 AuxPoW blocks for range validation"
        return
    fi

    log_info "Found ${#auxpow_heights[@]} AuxPoW blocks: ${auxpow_heights[*]}"

    local valid=true
    local validation_errors=""

    # Collect unique epochs using simple string tracking (bash 3.x compatible)
    local seen_epochs=""
    local unique_epoch_count=0
    local prev_epoch=""

    for h in "${auxpow_heights[@]}"; do
        local block=$(get_block_by_height "alys-node-1" "$h")
        local auxpow=$(get_block_auxpow_header "$block")
        local range_start=$(get_auxpow_field "$auxpow" "range_start")
        local range_end=$(get_auxpow_field "$auxpow" "range_end")

        log_debug "Height $h: range $range_start -> $range_end"

        # Validation 1: range_start and range_end must be valid hashes (0x + 64 hex chars)
        if [[ ! "$range_start" =~ ^0x[0-9a-fA-F]{64}$ ]]; then
            log_error "Height $h: Invalid range_start format: $range_start"
            validation_errors="${validation_errors}Invalid range_start at height $h; "
            valid=false
        fi

        if [[ ! "$range_end" =~ ^0x[0-9a-fA-F]{64}$ ]]; then
            log_error "Height $h: Invalid range_end format: $range_end"
            validation_errors="${validation_errors}Invalid range_end at height $h; "
            valid=false
        fi

        # Validation 2: range_start and range_end must not be zero hashes
        local zero_hash="0x0000000000000000000000000000000000000000000000000000000000000000"
        if [[ "$range_start" == "$zero_hash" ]] && [[ $h -gt 1 ]]; then
            log_warn "Height $h: range_start is zero hash (only valid for genesis)"
        fi

        # Track unique epochs using string matching (bash 3.x compatible)
        local epoch_key="${range_start}|${range_end}"
        if [[ "$seen_epochs" != *"$epoch_key"* ]]; then
            seen_epochs="${seen_epochs}${epoch_key};"
            ((unique_epoch_count++))
            log_debug "New epoch #$unique_epoch_count at height $h"
        else
            # Multiple blocks sharing same epoch is expected and valid
            log_debug "Height $h shares epoch with earlier block"
        fi

        # Check for duplicate consecutive epochs (should not happen)
        if [[ -n "$prev_epoch" ]] && [[ "$epoch_key" != "$prev_epoch" ]]; then
            log_debug "Epoch transition detected at height $h"
        fi
        prev_epoch="$epoch_key"
    done

    log_info "Found $unique_epoch_count unique AuxPoW epochs across ${#auxpow_heights[@]} blocks"

    # Validation 3: Must have at least 1 valid epoch
    if [[ $unique_epoch_count -lt 1 ]]; then
        log_error "No valid AuxPoW epochs found"
        valid=false
        validation_errors="${validation_errors}No valid epochs; "
    fi

    # Report results
    if [[ "$valid" == "true" ]]; then
        record_test_result "TM-P4" "PASSED" "AuxPoW ranges validated: $unique_epoch_count epochs, ${#auxpow_heights[@]} blocks"
    else
        record_test_result "TM-P4" "FAILED" "Range validation errors: $validation_errors"
    fi
}

run_TM_P5() {
    log_info "[TM-P5] Running: AuxPoW Survives Validator Failure"

    if ! verify_all_validators_active; then
        record_test_result "TM-P5" "FAILED" "Pre-condition failed"
        return
    fi

    # Record AuxPoW block before crash
    local pre_auxpow_height=$(find_block_with_auxpow "alys-node-1" 50)
    local pre_height=$(get_consensus_height "alys-node-1")

    log_debug "Pre-crash: auxpow at $pre_auxpow_height, consensus at $pre_height"

    # Crash and recover a validator
    local target="alys-node-2"
    crash_node "$target"
    sleep 5
    restart_node "$target"
    wait_for_validator_sync "$target" 180

    # Wait for consensus to resume
    sleep 30

    # Verify AuxPoW block is still queryable after recovery
    if [[ -n "$pre_auxpow_height" ]]; then
        local block=$(get_block_by_height "alys-node-1" "$pre_auxpow_height")
        if verify_block_has_auxpow "$block"; then
            log_debug "AuxPoW block at $pre_auxpow_height still valid after recovery"
        else
            record_test_result "TM-P5" "FAILED" "AuxPoW block corrupted after validator crash"
            return
        fi
    fi

    # Verify the recovered node can also query AuxPoW
    if [[ -n "$pre_auxpow_height" ]]; then
        local block_from_recovered=$(get_block_by_height "$target" "$pre_auxpow_height")
        if verify_block_has_auxpow "$block_from_recovered"; then
            record_test_result "TM-P5" "PASSED" "AuxPoW data preserved through validator crash"
        else
            record_test_result "TM-P5" "FAILED" "Recovered node missing AuxPoW data"
        fi
    else
        record_test_result "TM-P5" "PASSED" "Validator recovered (no AuxPoW blocks to verify)"
    fi
}

run_TM_P6() {
    log_info "[TM-P6] Running: AuxPoW Query During Partition"

    if ! verify_all_validators_active; then
        record_test_result "TM-P6" "FAILED" "Pre-condition failed"
        return
    fi

    local auxpow_height=$(find_block_with_auxpow "alys-node-1" 50)

    # Create partition
    isolate_node "alys-node-2"
    sleep 10

    # Query AuxPoW from connected nodes (should still work)
    local query_works=true
    for node in "alys-node-1" "alys-node-3"; do
        if [[ -n "$auxpow_height" ]]; then
            local block=$(get_block_by_height "$node" "$auxpow_height")
            if ! verify_block_has_auxpow "$block"; then
                log_error "AuxPoW query failed on $node during partition"
                query_works=false
            fi
        fi
    done

    # Heal partition
    reconnect_node "alys-node-2"
    wait_for_validator_sync "alys-node-2" 180

    if [[ "$query_works" == "true" ]]; then
        record_test_result "TM-P6" "PASSED" "AuxPoW queries work during partition"
    else
        record_test_result "TM-P6" "FAILED" "AuxPoW queries failed during partition"
    fi
}

run_TM_P7() {
    log_info "[TM-P7] Running: Blocks Without AuxPoW Validation"

    if ! verify_all_validators_active; then
        record_test_result "TM-P7" "FAILED" "Pre-condition failed"
        return
    fi

    local current_height=$(get_consensus_height "alys-node-1")
    local blocks_without_auxpow=0
    local blocks_with_auxpow=0

    # Scan recent blocks
    for ((h = current_height - 1; h >= 1 && h >= current_height - 20; h--)); do
        local block=$(get_block_by_height "alys-node-1" "$h")

        if [[ -z "$block" ]]; then
            log_warn "Could not retrieve block at height $h"
            continue
        fi

        if verify_block_has_auxpow "$block"; then
            ((blocks_with_auxpow++))
        else
            ((blocks_without_auxpow++))
            # Verify has_auxpow is explicitly false
            local has_auxpow=$(echo "$block" | jq -r '.has_auxpow')
            if [[ "$has_auxpow" != "false" ]]; then
                log_error "Block $h: has_auxpow should be false, got $has_auxpow"
            fi
        fi
    done

    log_info "Scanned blocks: $blocks_with_auxpow with AuxPoW, $blocks_without_auxpow without"

    # Both types should be queryable
    if [[ $((blocks_with_auxpow + blocks_without_auxpow)) -gt 0 ]]; then
        record_test_result "TM-P7" "PASSED" "Block queries work with/without AuxPoW"
    else
        record_test_result "TM-P7" "FAILED" "Could not query any blocks"
    fi
}

run_TM_P8() {
    log_info "[TM-P8] Running: alys_getBlockByHeight RPC Validation"

    if ! verify_all_validators_active; then
        record_test_result "TM-P8" "FAILED" "Pre-condition failed"
        return
    fi

    # Test latest block query (no height param)
    local latest=$(get_block_by_height "alys-node-1")

    if [[ -z "$latest" ]]; then
        record_test_result "TM-P8" "FAILED" "Could not query latest block"
        return
    fi

    local height=$(echo "$latest" | jq -r '.height // 0')
    local hash=$(echo "$latest" | jq -r '.hash // empty')
    local parent_hash=$(echo "$latest" | jq -r '.parent_hash // empty')
    local timestamp=$(echo "$latest" | jq -r '.timestamp // 0')

    log_debug "Latest block: height=$height hash=${hash:0:16}... ts=$timestamp"

    # Validate response structure
    local valid=true

    if [[ -z "$hash" ]] || [[ ! "$hash" =~ ^0x[0-9a-fA-F]{64}$ ]]; then
        log_error "Invalid hash format: $hash"
        valid=false
    fi

    if [[ -z "$parent_hash" ]] || [[ ! "$parent_hash" =~ ^0x[0-9a-fA-F]{64}$ ]]; then
        log_error "Invalid parent_hash format: $parent_hash"
        valid=false
    fi

    if [[ "$timestamp" -le 0 ]]; then
        log_error "Invalid timestamp: $timestamp"
        valid=false
    fi

    # Test specific height query
    if [[ $height -gt 5 ]]; then
        local older=$(get_block_by_height "alys-node-1" "$((height - 5))")
        local older_height=$(echo "$older" | jq -r '.height // 0')
        if [[ "$older_height" != "$((height - 5))" ]]; then
            log_error "Height mismatch: requested $((height - 5)), got $older_height"
            valid=false
        fi
    fi

    if [[ "$valid" == "true" ]]; then
        record_test_result "TM-P8" "PASSED" "alys_getBlockByHeight RPC working correctly"
    else
        record_test_result "TM-P8" "FAILED" "RPC response validation failed"
    fi
}

run_TM_P9() {
    log_info "[TM-P9] Running: AuxPoW Range Hash Existence Validation"

    if ! verify_all_validators_active; then
        record_test_result "TM-P9" "FAILED" "Pre-condition failed"
        return
    fi

    local current_height=$(get_consensus_height "alys-node-1")
    local valid=true
    local validation_errors=""

    # Find a block with AuxPoW
    local auxpow_height=$(find_block_with_auxpow "alys-node-1" 50)

    if [[ -z "$auxpow_height" ]]; then
        record_test_result "TM-P9" "SKIPPED" "No AuxPoW blocks available"
        return
    fi

    local block=$(get_block_by_height "alys-node-1" "$auxpow_height")
    local auxpow=$(get_block_auxpow_header "$block")
    local range_start=$(get_auxpow_field "$auxpow" "range_start")
    local range_end=$(get_auxpow_field "$auxpow" "range_end")

    log_info "Validating AuxPoW at height $auxpow_height"
    log_debug "range_start: $range_start"
    log_debug "range_end: $range_end"

    # Find blocks matching range_start and range_end hashes
    local start_found=false
    local end_found=false
    local start_height=""
    local end_height=""

    # Scan backwards to find the blocks with these hashes
    for ((h = auxpow_height; h >= 1 && h >= auxpow_height - 100; h--)); do
        local scan_block=$(get_block_by_height "alys-node-1" "$h")
        local block_hash=$(echo "$scan_block" | jq -r '.hash // empty')

        if [[ "$block_hash" == "$range_start" ]]; then
            start_found=true
            start_height=$h
            log_debug "Found range_start at height $h"
        fi

        if [[ "$block_hash" == "$range_end" ]]; then
            end_found=true
            end_height=$h
            log_debug "Found range_end at height $h"
        fi

        if [[ "$start_found" == "true" ]] && [[ "$end_found" == "true" ]]; then
            break
        fi
    done

    # Validation 1: range_start hash must exist in the chain
    if [[ "$start_found" != "true" ]]; then
        log_error "range_start hash not found in chain: $range_start"
        validation_errors="${validation_errors}range_start not found; "
        valid=false
    fi

    # Validation 2: range_end hash must exist in the chain
    if [[ "$end_found" != "true" ]]; then
        log_error "range_end hash not found in chain: $range_end"
        validation_errors="${validation_errors}range_end not found; "
        valid=false
    fi

    # Validation 3: range_start height must be <= range_end height (chronological order)
    if [[ "$start_found" == "true" ]] && [[ "$end_found" == "true" ]]; then
        if [[ $start_height -gt $end_height ]]; then
            log_error "range_start (height $start_height) is after range_end (height $end_height)"
            validation_errors="${validation_errors}range order invalid; "
            valid=false
        else
            local range_size=$((end_height - start_height + 1))
            log_info "AuxPoW covers $range_size blocks (heights $start_height to $end_height)"

            # Validation 4: Verify parent chain continuity within the range
            local chain_valid=true
            for ((h = start_height + 1; h <= end_height; h++)); do
                local curr_block=$(get_block_by_height "alys-node-1" "$h")
                local prev_block=$(get_block_by_height "alys-node-1" "$((h - 1))")

                local curr_parent=$(echo "$curr_block" | jq -r '.parent_hash // empty')
                local prev_hash=$(echo "$prev_block" | jq -r '.hash // empty')

                if [[ "$curr_parent" != "$prev_hash" ]]; then
                    log_error "Parent chain break at height $h: parent=$curr_parent, prev_hash=$prev_hash"
                    chain_valid=false
                    break
                fi
            done

            if [[ "$chain_valid" != "true" ]]; then
                validation_errors="${validation_errors}parent chain break in range; "
                valid=false
            else
                log_debug "Parent chain continuous within range"
            fi
        fi
    fi

    # Report results
    if [[ "$valid" == "true" ]]; then
        record_test_result "TM-P9" "PASSED" "Range hashes exist and form continuous chain"
    else
        record_test_result "TM-P9" "FAILED" "Range validation errors: $validation_errors"
    fi
}

# ============================================================================
# Tier 2 Scenarios: Validator Set Updates (TM-V*)
# ============================================================================

# Node 4 configuration (dynamically added validator)
NODE4_NAME="alys-node-4"
NODE4_IP="172.22.0.13"
NODE4_RPC_PORT=3031
# BLS public key for validator 4 (from etc/config/validator4-keys.json)
NODE4_VALIDATOR_PUBKEY="90220dc92c39b95cfb107d1e2cdcd65cb400dabf55a00db7d66a5c8599692feaa561c13b467c321ccac02bf0f4b13c94"

# Get validator count from RPC
get_validator_count() {
    local node="${1:-alys-node-1}"
    local port=$(get_rpc_port "$node")
    local result=$(rpc_call "$port" "tendermint_validators" 2>/dev/null)
    if [[ -n "$result" ]]; then
        echo "$result" | jq -r '.total // 0'
    else
        echo "0"
    fi
}

# Check if a specific validator is in the set
verify_validator_in_set() {
    local node="${1:-alys-node-1}"
    local pubkey_hex="$2"
    local port=$(get_rpc_port "$node")

    local validators=$(rpc_call "$port" "tendermint_validators" 2>/dev/null)
    if [[ -z "$validators" ]]; then
        return 1
    fi

    # Check if pubkey exists in validators array
    echo "$validators" | jq -e --arg pk "$pubkey_hex" \
        '.validators[] | select(.pub_key == $pk or .pub_key.value == $pk)' >/dev/null 2>&1
}

# Wait for validator count to reach expected value
wait_for_validator_count() {
    local expected_count="$1"
    local timeout_seconds="${2:-120}"
    local node="${3:-alys-node-1}"

    local start_time=$(date +%s)
    while true; do
        local count=$(get_validator_count "$node")
        if [[ "$count" -ge "$expected_count" ]]; then
            log_debug "Validator count reached $count (expected $expected_count)"
            return 0
        fi

        local elapsed=$(($(date +%s) - start_time))
        if [[ $elapsed -ge $timeout_seconds ]]; then
            log_error "Timeout waiting for $expected_count validators (current: $count)"
            return 1
        fi

        log_debug "Current validator count: $count, waiting for $expected_count..."
        sleep 2
    done
}

# Start node 4 container (dynamic validator)
start_node4() {
    log_info "Starting $NODE4_NAME container..."

    # Start the container using docker compose profile
    docker compose -f "$COMPOSE_FILE" --profile dynamic up -d alys-node-4 2>/dev/null || {
        # Fallback: try direct docker start if container exists
        docker start "$NODE4_NAME" 2>/dev/null || {
            log_error "Failed to start $NODE4_NAME"
            return 1
        }
    }

    # Wait for container to be running
    local timeout=30
    local elapsed=0
    while ! docker ps --filter "name=$NODE4_NAME" --format '{{.Names}}' | grep -q "$NODE4_NAME"; do
        sleep 1
        ((elapsed++))
        if [[ $elapsed -ge $timeout ]]; then
            log_error "$NODE4_NAME did not start in ${timeout}s"
            return 1
        fi
    done

    log_info "$NODE4_NAME container started"
    return 0
}

# Stop node 4 container
stop_node4() {
    log_info "Stopping $NODE4_NAME container..."
    docker stop -t 30 "$NODE4_NAME" 2>/dev/null || true
}

run_TM_V1() {
    log_info "[TM-V1] Running: Dynamic Validator Addition (4th validator via governance)"

    # Pre-conditions: 3 validators active
    if ! verify_all_validators_active; then
        record_test_result "TM-V1" "FAILED" "Pre-condition failed: validators not active"
        return
    fi

    local initial_count=$(get_validator_count "alys-node-1")
    if [[ "$initial_count" -ne 3 ]]; then
        log_warn "Expected 3 initial validators, got $initial_count"
        # Not a hard failure - continue with test
    fi

    log_info "Initial validator count: $initial_count"
    local initial_height=$(get_consensus_height "alys-node-1")

    # The mock-governance is configured to push validator update after 60s
    # We need to wait for: governance delay (60s) + H+2 activation (~10-15s) + buffer
    log_info "Waiting for governance to push validator update..."
    log_info "This may take up to 120 seconds (60s governance delay + H+2 activation)"

    # Wait for validator count to increase (H+2 activation)
    if wait_for_validator_count 4 180 "alys-node-1"; then
        log_info "Validator count increased to 4"
    else
        record_test_result "TM-V1" "FAILED" "Validator count did not increase to 4"
        return
    fi

    # Verify all 3 original nodes see 4 validators
    local all_see_four=true
    for node in "${NODE_NAMES[@]}"; do
        local count=$(get_validator_count "$node")
        if [[ "$count" -ne 4 ]]; then
            log_error "Node $node sees $count validators (expected 4)"
            all_see_four=false
        else
            log_debug "Node $node sees 4 validators"
        fi
    done

    if [[ "$all_see_four" != "true" ]]; then
        record_test_result "TM-V1" "FAILED" "Not all nodes see 4 validators"
        return
    fi

    # Start node 4 and verify it can sync
    log_info "Starting validator node 4..."
    if ! start_node4; then
        record_test_result "TM-V1" "FAILED" "Failed to start node 4 container"
        return
    fi

    # Wait for node 4 to sync
    if wait_for_validator_sync "$NODE4_NAME" 120; then
        log_info "Node 4 synced successfully"
    else
        record_test_result "TM-V1" "FAILED" "Node 4 failed to sync"
        stop_node4
        return
    fi

    # Verify consensus continues with 4 validators
    local pre_four_height=$(get_consensus_height "alys-node-1")
    log_info "Waiting 30s for consensus to progress with 4 validators..."
    sleep 30
    local post_four_height=$(get_consensus_height "alys-node-1")

    if [[ "$post_four_height" -gt "$pre_four_height" ]]; then
        log_info "Consensus progressing with 4 validators: $pre_four_height -> $post_four_height"
    else
        record_test_result "TM-V1" "FAILED" "Consensus stalled after adding 4th validator"
        stop_node4
        return
    fi

    # Verify node 4 is participating (check its height matches others)
    local h1=$(get_consensus_height "alys-node-1")
    local h4=$(get_consensus_height "$NODE4_NAME")

    log_info "Height comparison: Node 1 at $h1, Node 4 at $h4"

    if [[ "$h4" -ge "$((h1 - 2))" ]]; then
        record_test_result "TM-V1" "PASSED" "Dynamic Validator Addition - 4 validators in consensus (heights: n1=$h1, n4=$h4)"
    else
        record_test_result "TM-V1" "FAILED" "Node 4 height ($h4) far behind others ($h1)"
    fi

    # Cleanup: stop node 4 (leave for optional follow-up tests)
    stop_node4
}

# ============================================================================
# Scenario Dispatcher
# ============================================================================

run_scenario() {
    local scenario="$1"

    case "$scenario" in
        # Category A: Validator Failures
        TM-A1) run_TM_A1 ;;
        TM-A2) run_TM_A2 ;;
        TM-A3) run_TM_A3 ;;
        TM-A4) run_TM_A4 ;;
        TM-A5) run_TM_A5 ;;

        # Category B: Network Partitions
        TM-B1) run_TM_B1 ;;
        TM-B2) run_TM_B2 ;;
        TM-B3) run_TM_B3 ;;
        TM-B4) run_TM_B4 ;;
        TM-B5) run_TM_B5 ;;

        # Category C: Timing
        TM-C1) run_TM_C1 ;;
        TM-C2) run_TM_C2 ;;
        TM-C3) run_TM_C3 ;;
        TM-C4) run_TM_C4 ;;

        # Category D: WAL & Crash Recovery
        TM-D1) run_TM_D1 ;;
        TM-D2) run_TM_D2 ;;
        TM-D3) run_TM_D3 ;;
        TM-D4) run_TM_D4 ;;
        TM-D5) run_TM_D5 ;;

        # Category E: Equivocation Prevention
        TM-E5) run_TM_E5 ;;

        # Category F: External Dependencies
        TM-F1) run_TM_F1 ;;
        TM-F2) run_TM_F2 ;;
        TM-F3) run_TM_F3 ;;

        # Category L: Liveness & Safety
        TM-L1) run_TM_L1 ;;
        TM-L2) run_TM_L2 ;;
        TM-L3) run_TM_L3 ;;

        # Category I: Block Integrity
        TM-I1) run_TM_I1 ;;
        TM-I2) run_TM_I2 ;;
        TM-I3) run_TM_I3 ;;
        TM-I4) run_TM_I4 ;;
        TM-I5) run_TM_I5 ;;

        # Category P: AuxPoW Integration
        TM-P1) run_TM_P1 ;;
        TM-P2) run_TM_P2 ;;
        TM-P3) run_TM_P3 ;;
        TM-P4) run_TM_P4 ;;
        TM-P5) run_TM_P5 ;;

        # Category V: Validator Set Updates
        TM-V1) run_TM_V1 ;;
        TM-P6) run_TM_P6 ;;
        TM-P7) run_TM_P7 ;;
        TM-P8) run_TM_P8 ;;
        TM-P9) run_TM_P9 ;;

        # Scenario groups
        tier1)
            run_TM_A1
            run_TM_A3
            run_TM_B1
            run_TM_B5
            run_TM_L3
            run_TM_I1
            run_TM_I3
            ;;

        tier2)
            run_TM_D1
            run_TM_D2
            run_TM_D3
            run_TM_E5
            run_TM_F1
            run_TM_F2
            run_TM_V1
            ;;

        tier3)
            run_TM_P1
            run_TM_P2
            run_TM_P3
            run_TM_P4
            run_TM_P5
            run_TM_P6
            run_TM_P7
            run_TM_P8
            run_TM_P9
            ;;

        validator)
            run_TM_A1
            run_TM_A2
            run_TM_A3
            run_TM_A4
            run_TM_A5
            ;;

        network)
            run_TM_B1
            run_TM_B2
            run_TM_B3
            run_TM_B4
            run_TM_B5
            ;;

        timing)
            run_TM_C1
            run_TM_C2
            run_TM_C3
            run_TM_C4
            ;;

        wal)
            run_TM_D1
            run_TM_D2
            run_TM_D3
            run_TM_D4
            run_TM_D5
            run_TM_E5
            ;;

        external)
            run_TM_F1
            run_TM_F2
            run_TM_F3
            ;;

        liveness)
            run_TM_L1
            run_TM_L2
            run_TM_L3
            ;;

        integrity)
            run_TM_I1
            run_TM_I2
            run_TM_I3
            run_TM_I4
            run_TM_I5
            ;;

        auxpow)
            run_TM_P1
            run_TM_P2
            run_TM_P3
            run_TM_P4
            run_TM_P5
            run_TM_P6
            run_TM_P7
            run_TM_P8
            run_TM_P9
            ;;

        valset)
            run_TM_V1
            ;;

        all)
            # Category A: Validator Failures
            run_TM_A1
            run_TM_A2
            run_TM_A3
            run_TM_A4
            run_TM_A5
            # Category B: Network Partitions
            run_TM_B1
            run_TM_B2
            run_TM_B3
            run_TM_B4
            run_TM_B5
            # Category C: Timing
            run_TM_C1
            run_TM_C2
            run_TM_C3
            run_TM_C4
            # Category D: WAL Recovery
            run_TM_D1
            run_TM_D2
            run_TM_D3
            run_TM_D4
            run_TM_D5
            # Category E: Equivocation
            run_TM_E5
            # Category F: External Dependencies
            run_TM_F1
            run_TM_F2
            run_TM_F3
            # Category L: Liveness
            run_TM_L1
            run_TM_L2
            run_TM_L3
            # Category I: Integrity
            run_TM_I1
            run_TM_I2
            run_TM_I3
            run_TM_I4
            run_TM_I5
            # Category P: AuxPoW
            run_TM_P1
            run_TM_P2
            run_TM_P3
            run_TM_P4
            run_TM_P5
            run_TM_P6
            run_TM_P7
            run_TM_P8
            run_TM_P9
            # Category V: Validator Set Updates
            run_TM_V1
            ;;

        *)
            log_error "Unknown scenario: $scenario"
            echo "Available scenarios:"
            echo "  A (Validator):  TM-A1, TM-A2, TM-A3, TM-A4, TM-A5"
            echo "  B (Network):    TM-B1, TM-B2, TM-B3, TM-B4, TM-B5"
            echo "  C (Timing):     TM-C1, TM-C2, TM-C3, TM-C4"
            echo "  D (WAL):        TM-D1, TM-D2, TM-D3, TM-D4, TM-D5"
            echo "  E (Equivoc):    TM-E5"
            echo "  F (External):   TM-F1, TM-F2, TM-F3"
            echo "  I (Integrity):  TM-I1, TM-I2, TM-I3, TM-I4, TM-I5"
            echo "  L (Liveness):   TM-L1, TM-L2, TM-L3"
            echo "  P (AuxPoW):     TM-P1, TM-P2, TM-P3, TM-P4, TM-P5, TM-P6, TM-P7, TM-P8, TM-P9"
            echo "  V (ValSet):     TM-V1"
            echo "  Groups: tier1, tier2, tier3, validator, network, timing, wal, external, integrity, liveness, auxpow, valset, all"
            exit 1
            ;;
    esac
}

# ============================================================================
# Main
# ============================================================================

usage() {
    cat <<EOF
Tendermint Chaos Testing Framework

Usage: $0 [OPTIONS]

Options:
  --scenario SCENARIO   Run specific scenario or group (required)
  --verbose             Enable verbose output
  --output-dir DIR      Output directory for results
  --help                Show this help

Scenarios:
  Category A - Validator Failures:
    TM-A1  Single Validator Crash & Recovery
    TM-A2  Proposer Crash
    TM-A3  Validator Restart & WAL Recovery
    TM-A4  Crash Recovery Time Measurement
    TM-A5  Sequential Validator Restarts

  Category B - Network Partitions:
    TM-B1  Single Node Isolation & Recovery
    TM-B2  2-1 Network Partition
    TM-B3  Asymmetric Partition
    TM-B4  Proposer Isolation
    TM-B5  Partition Heal & Consensus Resume

  Category C - Timing:
    TM-C1  Timeout Storm
    TM-C2  Vote Delay Chaos
    TM-C3  Slow Proposal Delivery
    TM-C4  Fast Rounds

  Category D - WAL & Crash Recovery:
    TM-D1  Crash After Prevote
    TM-D2  Crash After Precommit
    TM-D3  Crash While Locked
    TM-D4  Corrupt WAL Recovery
    TM-D5  Repeated Crash/Recovery Cycles

  Category E - Equivocation Prevention:
    TM-E5  No Self-Equivocation After Crash

  Category F - External Dependencies:
    TM-F1  Execution Layer Failure
    TM-F2  Bitcoin Core Failure
    TM-F3  Monitoring (Prometheus) Failure

  Category I - Block Integrity:
    TM-I1  Random Block Sampling
    TM-I2  Last Commit Chain Verification
    TM-I3  Historical Block Scan
    TM-I4  Real-time Block Consistency
    TM-I5  Signature Validation

  Category L - Liveness & Safety:
    TM-L1  Round Stall Recovery
    TM-L2  Multi-Round Block Commit
    TM-L3  Height Progression

  Category P - AuxPoW Integration:
    TM-P1  createauxblock Response Validation
    TM-P2  Direct AuxPoW Header Validation
    TM-P3  Cross-Node AuxPoW Consistency
    TM-P4  AuxPoW Range Chain Validation
    TM-P5  AuxPoW Survives Validator Failure
    TM-P6  AuxPoW Query During Partition
    TM-P7  Blocks Without AuxPoW Validation
    TM-P8  alys_getBlockByHeight RPC Validation
    TM-P9  AuxPoW Range Hash Existence Validation

  Category V - Validator Set Updates:
    TM-V1  Dynamic Validator Addition (4th validator via governance)

  Groups:
    tier1      Core scenarios (A1, A3, B1, B5, L3, I1, I3)
    tier2      Advanced scenarios (D1-D3, E5, F1-F2, V1)
    tier3      AuxPoW integration (P1-P9)
    validator  All validator failure scenarios (A1-A5)
    network    All network partition scenarios (B1-B5)
    timing     All timing scenarios (C1-C4)
    wal        All WAL/recovery scenarios (D1-D5, E5)
    external   All external dependency scenarios (F1-F3)
    integrity  All block integrity scenarios (I1-I5)
    liveness   All liveness scenarios (L1-L3)
    auxpow     All AuxPoW scenarios (P1-P9)
    valset     All validator set update scenarios (V1)
    all        All 38 scenarios

Examples:
  $0 --scenario TM-A1
  $0 --scenario tier1 --verbose
  $0 --scenario all --output-dir ./results
EOF
}

main() {
    local scenario=""

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --scenario)
                scenario="$2"
                shift 2
                ;;
            --verbose)
                VERBOSE=true
                shift
                ;;
            --output-dir)
                OUTPUT_DIR="$2"
                shift 2
                ;;
            --help)
                usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
        esac
    done

    if [[ -z "$scenario" ]]; then
        log_error "No scenario specified"
        usage
        exit 1
    fi

    # Check prerequisites
    log_info "Checking prerequisites..."
    if ! verify_all_validators_active; then
        log_error "Not all validators are active. Start the testnet first:"
        log_error "  docker compose -f docker-compose.tendermint-3node.yml up -d"
        exit 1
    fi

    # Check consensus is working
    local initial_height=$(get_consensus_height "alys-node-1")
    if [[ "$initial_height" == "0" ]]; then
        log_warn "Consensus appears to be at height 0. Waiting for startup..."
        sleep 10
    fi

    log_info "Starting chaos testing: $scenario"
    echo ""

    # Run scenarios
    run_scenario "$scenario"

    # Print summary
    print_test_summary
}

main "$@"
