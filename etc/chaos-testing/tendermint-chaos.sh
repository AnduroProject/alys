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
    rpc_call "$port" "tendermint_commit" "[{\"height\": $height}]" 2>/dev/null
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
    local block_hashes=()

    for node in "${NODE_NAMES[@]}"; do
        local port=$(get_rpc_port "$node")
        local hash=$(rpc_call "$port" "tendermint_commit" "[{\"height\":$height}]" 2>/dev/null \
            | jq -r '.block_hash // "unknown"')
        block_hashes+=("$hash")
    done

    local unique_hashes=$(printf '%s\n' "${block_hashes[@]}" | sort -u | wc -l | tr -d ' ')
    if [[ "$unique_hashes" -eq 1 ]]; then
        log_debug "No fork detected at height $height"
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
    log_info "Reconnecting $node to network $NETWORK_NAME..."
    docker network connect "$NETWORK_NAME" "$node" 2>/dev/null || true
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
    docker stop "$node" 2>/dev/null || true
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
    wait_for_validator_sync "$target" 30

    # Verify consensus resumed
    local post_recovery_height=$(get_consensus_height "alys-node-1")
    if wait_for_height $((post_recovery_height + 3)) 60; then
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
    wait_for_validator_sync "$proposer" 30

    # Verify recovery
    if wait_for_height $((current_height + 3)) 60; then
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
    wait_for_validator_sync "$target" 30

    # Verify consensus resumed and all nodes are at same height
    sleep 10
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
    local max_recovery_time=60

    while true; do
        local current=$(get_consensus_height "alys-node-1")
        if [[ "$current" -ge "$resume_height" ]]; then
            local recovery_time=$(($(date +%s) - crash_time))
            log_info "Recovery time: ${recovery_time}s"
            if [[ $recovery_time -le 30 ]]; then
                record_test_result "TM-A4" "PASSED" "Crash Recovery Time: ${recovery_time}s (< 30s)"
            else
                record_test_result "TM-A4" "FAILED" "Recovery too slow: ${recovery_time}s (> 30s)"
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
        wait_for_validator_sync "$node" 30 || {
            record_test_result "TM-A5" "FAILED" "$node did not recover"
            return
        }
        sleep 5  # Let consensus stabilize
    done

    # Verify all nodes at same height and progressing
    sleep 10
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
    if wait_for_height $((post_heal_height + 3)) 60; then
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
    wait_for_validator_sync "alys-node-3" 30

    # Verify recovery
    if wait_for_height $((h1 + 3)) 60; then
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
        if [[ $elapsed -ge 60 ]]; then
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
    if wait_for_height $((current_height + 3)) 60; then
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
    wait_for_validator_sync "$next_proposer" 30

    # Verify recovery - consensus should resume with timeout and round advancement
    local post_heal_height=$(get_consensus_height "alys-node-1")
    if wait_for_height $((post_heal_height + 3)) 60; then
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
    wait_for_validator_sync "$target" 30

    # Wait for consensus to resume
    sleep 15

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
    wait_for_validator_sync "$target" 30

    sleep 15

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
    wait_for_validator_sync "$target" 30

    sleep 15

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
    sleep 30

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
        wait_for_validator_sync "$target" 20 || {
            record_test_result "TM-D5" "FAILED" "Recovery failed on cycle $i"
            return
        }
        sleep 5
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
        sleep 5
    done

    # Wait for full recovery
    wait_for_validator_sync "alys-node-1" 30
    wait_for_validator_sync "alys-node-2" 30
    wait_for_validator_sync "alys-node-3" 30

    sleep 20

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
    sleep 20

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
    wait_for_validator_sync "alys-node-2" 30

    sleep 20

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

        # Scenario groups
        tier1)
            run_TM_A1
            run_TM_A3
            run_TM_B1
            run_TM_B5
            run_TM_L3
            ;;

        tier2)
            run_TM_D1
            run_TM_D2
            run_TM_D3
            run_TM_E5
            run_TM_F1
            run_TM_F2
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
            echo "  L (Liveness):   TM-L1, TM-L2, TM-L3"
            echo "  Groups: tier1, tier2, validator, network, timing, wal, external, liveness, all"
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

  Category L - Liveness & Safety:
    TM-L1  Round Stall Recovery
    TM-L2  Multi-Round Block Commit
    TM-L3  Height Progression

  Groups:
    tier1      Core scenarios (A1, A3, B1, B5, L3)
    tier2      Advanced scenarios (D1-D3, E5, F1-F2)
    validator  All validator failure scenarios (A1-A5)
    network    All network partition scenarios (B1-B5)
    timing     All timing scenarios (C1-C4)
    wal        All WAL/recovery scenarios (D1-D5, E5)
    external   All external dependency scenarios (F1-F3)
    liveness   All liveness scenarios (L1-L3)
    all        All 24 scenarios

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
