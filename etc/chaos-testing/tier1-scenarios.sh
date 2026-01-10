#!/bin/bash
# Alys V2 Tier 1 Chaos Testing Scenarios
# Purpose: Structured test scenarios with blockchain state verification
# Usage: ./tier1-scenarios.sh [--scenario <1|2|3|all>] [--verbose]

set -euo pipefail

# ============================================================================
# Configuration
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
COMPOSE_FILE="$PROJECT_ROOT/etc/docker-compose.v2-regtest.yml"
LOG_DIR="$PROJECT_ROOT/logs/chaos-testing"
REPORT_DIR="$PROJECT_ROOT/reports/chaos-testing"

# RPC configuration (dynamic port calculation)
V2_RPC_BASE_PORT=3001
V2_RPC_PORT_INCREMENT=10

# Node discovery (populated by discover_nodes)
NODE_COUNT=0
NODE_NAMES=()
SPECIFIED_NODE_COUNT=0

# Timing configuration
BLOCK_INTERVAL=4              # Aura slot duration in seconds
SYNC_TIMEOUT=120              # Max seconds to wait for sync
STARTUP_TIMEOUT=90            # Max seconds for node startup
PARTITION_BLOCKS=15           # Number of blocks to let accumulate during partition
RECOVERY_WAIT=30              # Seconds to wait for recovery after reconnection

# Test state
TEST_ID="tier1-$(date +%Y%m%d-%H%M%S)"
VERBOSE=false
PASSED_TESTS=0
FAILED_TESTS=0
SKIPPED_TESTS=0

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# ============================================================================
# Logging Functions
# ============================================================================

log() {
    echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1"
}

log_verbose() {
    if [ "$VERBOSE" = true ]; then
        echo -e "${CYAN}[$(date '+%H:%M:%S')] [VERBOSE]${NC} $1"
    fi
}

log_success() {
    echo -e "${GREEN}[$(date '+%H:%M:%S')] ✓${NC} $1"
}

log_error() {
    echo -e "${RED}[$(date '+%H:%M:%S')] ✗${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[$(date '+%H:%M:%S')] ⚠${NC} $1"
}

log_step() {
    echo -e "${BOLD}[$(date '+%H:%M:%S')] →${NC} $1"
}

print_header() {
    local title=$1
    echo ""
    echo -e "${BOLD}================================================================================${NC}"
    echo -e "${BOLD}  $title${NC}"
    echo -e "${BOLD}================================================================================${NC}"
    echo ""
}

print_result() {
    local name=$1
    local status=$2
    local duration=$3

    if [ "$status" = "PASSED" ]; then
        echo -e "${GREEN}${BOLD}RESULT: PASSED${NC} ($duration seconds)"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    elif [ "$status" = "FAILED" ]; then
        echo -e "${RED}${BOLD}RESULT: FAILED${NC} ($duration seconds)"
        FAILED_TESTS=$((FAILED_TESTS + 1))
    else
        echo -e "${YELLOW}${BOLD}RESULT: SKIPPED${NC}"
        SKIPPED_TESTS=$((SKIPPED_TESTS + 1))
    fi
    echo ""
}

# ============================================================================
# Node Discovery and Selection Functions
# ============================================================================

# Get RPC URL for a node by number (1, 2, 3, etc.)
get_rpc_url() {
    local node_num=$1
    local port=$((V2_RPC_BASE_PORT + V2_RPC_PORT_INCREMENT * (node_num - 1)))
    echo "http://localhost:$port"
}

# Discover running alys-node containers
discover_nodes() {
    log "Discovering running nodes..."

    # Find all running alys-node-* containers, sorted by number
    mapfile -t NODE_NAMES < <(docker ps --format '{{.Names}}' | grep -E '^alys-node-[0-9]+$' | sort -V)
    NODE_COUNT=${#NODE_NAMES[@]}

    if [ "$NODE_COUNT" -eq 0 ]; then
        log_error "No running alys-node containers found"
        return 1
    fi

    # If user specified a node count, validate it
    if [ "$SPECIFIED_NODE_COUNT" -gt 0 ]; then
        if [ "$NODE_COUNT" -lt "$SPECIFIED_NODE_COUNT" ]; then
            log_error "Requested $SPECIFIED_NODE_COUNT nodes but only $NODE_COUNT running"
            return 1
        fi
        # Trim to specified count
        NODE_NAMES=("${NODE_NAMES[@]:0:$SPECIFIED_NODE_COUNT}")
        NODE_COUNT=$SPECIFIED_NODE_COUNT
    fi

    log_success "Discovered $NODE_COUNT nodes: ${NODE_NAMES[*]}"
    return 0
}

# Get a random node from the discovered nodes
# Usage: get_random_node [exclude_node]
get_random_node() {
    local exclude=${1:-}
    local available=()

    for node in "${NODE_NAMES[@]}"; do
        if [ -z "$exclude" ] || [ "$node" != "$exclude" ]; then
            available+=("$node")
        fi
    done

    if [ ${#available[@]} -eq 0 ]; then
        log_error "No available nodes to select"
        return 1
    fi

    echo "${available[$((RANDOM % ${#available[@]}))]}"
}

# Get all nodes except the specified one
# Usage: get_other_nodes <exclude_node>
get_other_nodes() {
    local exclude=$1
    local others=()

    for node in "${NODE_NAMES[@]}"; do
        if [ "$node" != "$exclude" ]; then
            others+=("$node")
        fi
    done

    echo "${others[@]}"
}

# Extract node number from container name (alys-node-2 -> 2)
get_node_number() {
    local node=$1

    if [[ "$node" =~ alys-node-([0-9]+) ]]; then
        echo "${BASH_REMATCH[1]}"
    elif [[ "$node" =~ ^node([0-9]+)$ ]]; then
        echo "${BASH_REMATCH[1]}"
    elif [[ "$node" =~ ^[0-9]+$ ]]; then
        echo "$node"
    else
        echo "1"  # Default fallback
    fi
}

# Normalize node name to full container name
normalize_node_name() {
    local node=$1

    if [[ "$node" =~ ^alys-node-[0-9]+$ ]]; then
        echo "$node"
    elif [[ "$node" =~ ^node([0-9]+)$ ]]; then
        echo "alys-node-${BASH_REMATCH[1]}"
    elif [[ "$node" =~ ^[0-9]+$ ]]; then
        echo "alys-node-$node"
    else
        echo "$node"
    fi
}

# ============================================================================
# RPC Functions for V2 Chain State
# ============================================================================

# Get block height from V2 RPC (falls back to log parsing if RPC unavailable)
get_block_height() {
    local node=$1
    local node_num
    local rpc_url

    # Normalize node name and get node number
    node=$(normalize_node_name "$node")
    node_num=$(get_node_number "$node")
    rpc_url=$(get_rpc_url "$node_num")

    # Try V2 RPC first
    local height=$(curl -s --connect-timeout 2 "$rpc_url/v2/chain/status" 2>/dev/null | jq -r '.height // empty' 2>/dev/null || echo "")

    if [ -n "$height" ] && [ "$height" != "null" ]; then
        echo "$height"
        return 0
    fi

    # Fallback: Parse logs for latest block height
    height=$(docker logs --tail=500 "$node" 2>&1 | \
        grep -oE "block_number=[0-9]+" | \
        tail -1 | \
        grep -oE "[0-9]+" || echo "0")

    echo "${height:-0}"
}

# Wait for node to reach a specific height
wait_for_height() {
    local node=$1
    local target_height=$2
    local timeout=${3:-$SYNC_TIMEOUT}
    local start_time=$(date +%s)

    log_verbose "Waiting for $node to reach height $target_height (timeout: ${timeout}s)"

    while true; do
        local current_height=$(get_block_height "$node")
        local elapsed=$(($(date +%s) - start_time))

        log_verbose "  $node height: $current_height (target: $target_height, elapsed: ${elapsed}s)"

        if [ "$current_height" -ge "$target_height" ]; then
            log_success "$node reached height $current_height (target was $target_height)"
            return 0
        fi

        if [ $elapsed -ge $timeout ]; then
            log_error "$node failed to reach height $target_height (stuck at $current_height) after ${timeout}s"
            return 1
        fi

        sleep 2
    done
}

# Wait for sync completion by watching logs
wait_for_sync_complete() {
    local node=$1
    local timeout=${2:-$SYNC_TIMEOUT}
    local start_time=$(date +%s)

    log_verbose "Waiting for $node sync to complete (timeout: ${timeout}s)"

    while true; do
        local elapsed=$(($(date +%s) - start_time))

        # Check for sync completion message
        if docker logs --since "${timeout}s" "$node" 2>&1 | grep -q "Sync completed successfully"; then
            log_success "$node sync completed"
            return 0
        fi

        # Also check if node is producing blocks (indicates sync is done)
        if docker logs --since "10s" "$node" 2>&1 | grep -q "Block produced successfully"; then
            log_success "$node is producing blocks (sync complete)"
            return 0
        fi

        if [ $elapsed -ge $timeout ]; then
            log_warning "$node sync completion not detected within ${timeout}s (may still be working)"
            return 0  # Don't fail, rely on height checks
        fi

        sleep 3
    done
}

# Check if node is producing blocks
is_producing_blocks() {
    local node=$1
    local window=${2:-20}  # Check last N seconds

    docker logs --since "${window}s" "$node" 2>&1 | grep -q "Block produced successfully"
}

# Check if node is importing blocks
is_importing_blocks() {
    local node=$1
    local window=${2:-20}

    docker logs --since "${window}s" "$node" 2>&1 | grep -q "Network block imported successfully"
}

# ============================================================================
# Docker Operations
# ============================================================================

# Check if container is running
is_container_running() {
    local container=$1
    local status=$(docker inspect -f '{{.State.Status}}' "$container" 2>/dev/null || echo "not_found")
    [ "$status" = "running" ]
}

# Check if the compose project has at least one running container.
# Prefer Compose-native filtering; fall back to a best-effort text check for older versions.
is_compose_project_running() {
    local ids=""

    # Compose v2 supports filtering by status; this avoids brittle parsing of the ps table.
    if ids="$(docker compose -f "$COMPOSE_FILE" ps --status running -q 2>/dev/null)"; then
        [ -n "$ids" ]
        return $?
    fi

    # Fallback: older compose versions may not support --status
    docker compose -f "$COMPOSE_FILE" ps 2>/dev/null | grep -Eqi '(^|[[:space:]])(Up|running)([[:space:]]|$)'
}

# Wait for container to be running
wait_for_container() {
    local container=$1
    local timeout=${2:-60}
    local start_time=$(date +%s)

    while true; do
        if is_container_running "$container"; then
            return 0
        fi

        local elapsed=$(($(date +%s) - start_time))
        if [ $elapsed -ge $timeout ]; then
            return 1
        fi

        sleep 1
    done
}

# Disconnect node from network (network partition)
disconnect_node() {
    local node=$1
    log_step "Disconnecting $node from network..."

    # Method 1: Docker network disconnect
    docker network disconnect alys-regtest "$node" 2>/dev/null || true

    log_success "$node disconnected from network"
}

# Reconnect node to network
reconnect_node() {
    local node=$1
    log_step "Reconnecting $node to network..."

    # Reconnect to Docker network
    docker network connect alys-regtest "$node" 2>/dev/null || true

    log_success "$node reconnected to network"
}

# Stop node container
stop_node() {
    local node=$1
    log_step "Stopping $node..."

    docker compose -f "$COMPOSE_FILE" stop "$node" 2>/dev/null

    log_success "$node stopped"
}

# Start node container
start_node() {
    local node=$1
    log_step "Starting $node..."

    docker compose -f "$COMPOSE_FILE" start "$node" 2>/dev/null

    # Wait for container to be running
    if wait_for_container "$node" 60; then
        log_success "$node started"
        return 0
    else
        log_error "$node failed to start"
        return 1
    fi
}

# ============================================================================
# Chaos Injection Functions
# ============================================================================

# Inject network latency using tc netem
# Usage: inject_network_latency <node> <delay_ms> [jitter_ms]
inject_network_latency() {
    local node=$1
    local delay_ms=${2:-500}
    local jitter_ms=${3:-50}

    log_step "Injecting ${delay_ms}ms latency (±${jitter_ms}ms) on $node..."

    # Try tc first (preferred), fall back to iptables delay simulation
    if docker exec "$node" tc qdisc add dev eth0 root netem delay "${delay_ms}ms" "${jitter_ms}ms" 2>/dev/null; then
        log_success "Latency injection active on $node"
        return 0
    else
        log_warning "tc not available, latency injection may be limited"
        return 1
    fi
}

# Remove network latency
remove_network_latency() {
    local node=$1

    log_step "Removing network latency from $node..."
    docker exec "$node" tc qdisc del dev eth0 root 2>/dev/null || true
    log_success "Latency removed from $node"
}

# Inject packet loss using tc netem
# Usage: inject_packet_loss <node> <loss_percent>
inject_packet_loss() {
    local node=$1
    local loss_percent=${2:-10}

    log_step "Injecting ${loss_percent}% packet loss on $node..."

    if docker exec "$node" tc qdisc add dev eth0 root netem loss "${loss_percent}%" 2>/dev/null; then
        log_success "Packet loss injection active on $node"
        return 0
    else
        log_warning "tc not available, packet loss injection may be limited"
        return 1
    fi
}

# Remove packet loss
remove_packet_loss() {
    local node=$1

    log_step "Removing packet loss from $node..."
    docker exec "$node" tc qdisc del dev eth0 root 2>/dev/null || true
    log_success "Packet loss removed from $node"
}

# Inject memory pressure on a node
# Usage: inject_memory_pressure <node> <megabytes>
inject_memory_pressure() {
    local node=$1
    local megabytes=${2:-256}

    log_step "Injecting ${megabytes}MB memory pressure on $node..."

    # Use dd to allocate memory (runs in background)
    docker exec -d "$node" sh -c "dd if=/dev/zero of=/dev/shm/memstress bs=1M count=$megabytes 2>/dev/null" || true
    log_success "Memory pressure active on $node"
}

# Remove memory pressure
remove_memory_pressure() {
    local node=$1

    log_step "Removing memory pressure from $node..."
    docker exec "$node" sh -c "rm -f /dev/shm/memstress" 2>/dev/null || true
    log_success "Memory pressure removed from $node"
}

# Inject disk I/O stress on a node
# Usage: inject_disk_stress <node> <num_processes>
inject_disk_stress() {
    local node=$1
    local num_procs=${2:-5}

    log_step "Injecting disk I/O stress ($num_procs processes) on $node..."

    docker exec -d "$node" sh -c "for i in \$(seq 1 $num_procs); do dd if=/dev/zero of=/tmp/diskstress\$i bs=1M count=100 conv=fdatasync 2>/dev/null & done" 2>/dev/null || true
    log_success "Disk stress active on $node"
}

# Remove disk I/O stress
remove_disk_stress() {
    local node=$1

    log_step "Removing disk stress from $node..."
    docker exec "$node" sh -c "pkill -f 'dd if=/dev/zero of=/tmp/diskstress' 2>/dev/null; rm -f /tmp/diskstress* 2>/dev/null" || true
    log_success "Disk stress removed from $node"
}

# Stop execution layer container
stop_execution_layer() {
    log_step "Stopping execution layer..."
    docker compose -f "$COMPOSE_FILE" stop execution 2>/dev/null
    log_success "Execution layer stopped"
}

# Start execution layer container
start_execution_layer() {
    log_step "Starting execution layer..."
    docker compose -f "$COMPOSE_FILE" start execution 2>/dev/null

    if wait_for_container "execution" 60; then
        log_success "Execution layer started"
        return 0
    else
        log_error "Execution layer failed to start"
        return 1
    fi
}

# Stop Bitcoin Core container
stop_bitcoin_core() {
    log_step "Stopping Bitcoin Core..."
    docker compose -f "$COMPOSE_FILE" stop bitcoin-core 2>/dev/null
    log_success "Bitcoin Core stopped"
}

# Start Bitcoin Core container
start_bitcoin_core() {
    log_step "Starting Bitcoin Core..."
    docker compose -f "$COMPOSE_FILE" start bitcoin-core 2>/dev/null

    if wait_for_container "bitcoin-core" 60; then
        log_success "Bitcoin Core started"
        return 0
    else
        log_error "Bitcoin Core failed to start"
        return 1
    fi
}

# ============================================================================
# Metrics Collection
# ============================================================================

# Collect a snapshot of system metrics
collect_metrics_snapshot() {
    local output_file=${1:-}

    log "Collecting metrics snapshot..."

    local metrics=""
    metrics+="Timestamp: $(date '+%Y-%m-%d %H:%M:%S')\n\n"

    # Container resource usage
    metrics+="=== Container Resource Usage ===\n"
    metrics+="$(docker stats --no-stream --format 'table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.NetIO}}\t{{.BlockIO}}' 2>/dev/null || echo "N/A")\n\n"

    # Node heights
    metrics+="=== Node Heights ===\n"
    for node in "${NODE_NAMES[@]}"; do
        local height=$(get_block_height "$node")
        metrics+="  $node: $height\n"
    done
    metrics+="\n"

    # Disk usage
    metrics+="=== Disk Usage ===\n"
    for node in "${NODE_NAMES[@]}"; do
        local node_num=$(get_node_number "$node")
        local db_size=$(du -sh "$PROJECT_ROOT/data/node${node_num}/db" 2>/dev/null | cut -f1 || echo "N/A")
        metrics+="  $node DB: $db_size\n"
    done
    metrics+="  Execution: $(du -sh "$PROJECT_ROOT/data/execution/data" 2>/dev/null | cut -f1 || echo "N/A")\n"
    metrics+="\n"

    # Network connectivity
    metrics+="=== Network Connectivity ===\n"
    for node in "${NODE_NAMES[@]}"; do
        local ping_result="OK"
        if ! docker exec "$node" ping -c 1 -W 1 172.20.0.10 >/dev/null 2>&1; then
            ping_result="FAILED"
        fi
        metrics+="  $node: $ping_result\n"
    done

    if [ -n "$output_file" ]; then
        echo -e "$metrics" >> "$output_file"
    else
        echo -e "$metrics"
    fi
}

# ============================================================================
# Pre-flight Checks
# ============================================================================

check_prerequisites() {
    log "Checking prerequisites..."

    log_verbose "SCRIPT_DIR=${SCRIPT_DIR}"
    log_verbose "PROJECT_ROOT=${PROJECT_ROOT}"
    log_verbose "COMPOSE_FILE=${COMPOSE_FILE}"

    # Check Docker Compose environment
    if ! is_compose_project_running; then
        log_error "Docker Compose environment is not running"
        log "Please start it first:"
        log "  cd $PROJECT_ROOT/etc && docker compose -f docker-compose.v2-regtest.yml up -d"
        exit 1
    fi

    # Discover running alys-node containers
    if ! discover_nodes; then
        exit 1
    fi

    # Require at least 2 nodes for meaningful chaos testing
    if [ "$NODE_COUNT" -lt 2 ]; then
        log_error "At least 2 nodes required for chaos testing (found: $NODE_COUNT)"
        log "Please start additional nodes:"
        log "  docker compose -f $COMPOSE_FILE up -d alys-node-1 alys-node-2"
        exit 1
    fi

    # Check infrastructure containers
    local infra_containers=("execution" "bitcoin-core")
    for container in "${infra_containers[@]}"; do
        if ! is_container_running "$container"; then
            log_error "Infrastructure container $container is not running"
            exit 1
        fi
    done

    # Check jq is available
    if ! command -v jq &> /dev/null; then
        log_warning "jq not found - some features may be limited"
    fi

    # Create directories
    mkdir -p "$LOG_DIR"
    mkdir -p "$REPORT_DIR"

    log_success "Prerequisites check passed ($NODE_COUNT nodes, infrastructure OK)"
}

# Wait for system to be stable and producing blocks
wait_for_stable_system() {
    local timeout=${1:-120}
    log "Waiting for system to stabilize ($NODE_COUNT nodes)..."

    local start_time=$(date +%s)
    local last_check_time=$start_time

    # Track heights and progress times for all nodes using associative arrays
    declare -A last_heights
    declare -A last_progress_times

    for node in "${NODE_NAMES[@]}"; do
        local h=$(get_block_height "$node" | tr -cd '0-9')
        last_heights["$node"]=${h:-0}
        last_progress_times["$node"]=$start_time
    done

    while true; do
        local now=$(date +%s)
        local elapsed=$((now - start_time))
        local check_delta=$((now - last_check_time))
        last_check_time=$now

        # Check each node's height and activity
        local all_active=true
        local heights_str=""
        local progress_str=""

        for node in "${NODE_NAMES[@]}"; do
            local height=$(get_block_height "$node" | tr -cd '0-9')
            height=${height:-0}

            # Update progress if height increased
            if [ "$height" -gt "${last_heights[$node]}" ]; then
                last_progress_times["$node"]=$now
                last_heights["$node"]=$height
            fi

            # Check if node is active (height increase or log activity)
            local node_active=false
            if [ $((now - ${last_progress_times[$node]})) -le 30 ]; then
                node_active=true
            elif is_producing_blocks "$node" 30 || is_importing_blocks "$node" 30; then
                node_active=true
            fi

            if [ "$node_active" = false ]; then
                all_active=false
            fi

            # Build status strings
            local node_num=$(get_node_number "$node")
            heights_str+="n${node_num}=$height "
            progress_str+="n${node_num}=$((now - ${last_progress_times[$node]}))s "
        done

        if [ "$all_active" = true ]; then
            log_success "System is stable - all $NODE_COUNT nodes active (heights: ${heights_str% })"
            return 0
        fi

        if [ $elapsed -ge $timeout ]; then
            log_warning "System may not be fully stable after ${timeout}s, proceeding anyway"
            return 0
        fi

        log_verbose "Waiting for block activity... (${elapsed}s elapsed, heights: ${heights_str% }, progress: ${progress_str% })"

        # Keep the polling cadence roughly aligned to BLOCK_INTERVAL, but cap at 5s.
        local sleep_s=5
        if [ "$BLOCK_INTERVAL" -gt 0 ] && [ "$BLOCK_INTERVAL" -lt 5 ]; then
            sleep_s="$BLOCK_INTERVAL"
        fi
        # If the loop body itself took time, reduce sleep a bit to avoid drifting too far.
        if [ "$check_delta" -gt 0 ] && [ "$check_delta" -lt "$sleep_s" ]; then
            sleep_s=$((sleep_s - check_delta))
            [ "$sleep_s" -lt 1 ] && sleep_s=1
        fi
        sleep "$sleep_s"
    done
}

# ============================================================================
# SCENARIO 1: Network Partition (Random node falls behind, re-syncs)
# ============================================================================

scenario_1_network_partition() {
    print_header "SCENARIO 1: Network Partition Recovery"

    # Select random target node for partition
    local target_node=$(get_random_node)
    local target_num=$(get_node_number "$target_node")
    local other_nodes=($(get_other_nodes "$target_node"))
    local reference_node="${other_nodes[0]}"  # Use first other node as reference

    echo "Description: Disable $target_node networking so it falls behind, then"
    echo "             re-enable networking and verify it re-syncs to other nodes"
    echo ""
    echo "Target node: $target_node"
    echo "Reference node: $reference_node"
    echo "Other nodes: ${other_nodes[*]}"
    echo ""

    local start_time=$(date +%s)
    local test_passed=true

    # Step 1: Record initial state for all nodes
    log_step "Step 1: Recording initial state..."
    declare -A initial_heights
    local all_zero=true

    for node in "${NODE_NAMES[@]}"; do
        initial_heights["$node"]=$(get_block_height "$node")
        log "  $node height: ${initial_heights[$node]}"
        if [ "${initial_heights[$node]}" -gt 0 ]; then
            all_zero=false
        fi
    done

    if [ "$all_zero" = true ]; then
        log_warning "All nodes at height 0 - waiting for some blocks first..."
        sleep $((BLOCK_INTERVAL * 5))
        for node in "${NODE_NAMES[@]}"; do
            initial_heights["$node"]=$(get_block_height "$node")
            log "  $node height: ${initial_heights[$node]}"
        done
    fi

    # Step 2: Disconnect target node
    log_step "Step 2: Disconnecting $target_node from network..."
    disconnect_node "$target_node"

    # Step 3: Wait for blocks to accumulate on other nodes
    local wait_time=$((BLOCK_INTERVAL * PARTITION_BLOCKS))
    log_step "Step 3: Waiting ${wait_time}s for other nodes to produce ~${PARTITION_BLOCKS} blocks..."
    sleep $wait_time

    local ref_height_during=$(get_block_height "$reference_node")
    local target_height_during=$(get_block_height "$target_node")

    log "  $reference_node height (producing): $ref_height_during"
    log "  $target_node height (partitioned): $target_height_during"

    local blocks_produced=$((ref_height_during - initial_heights[$reference_node]))
    log "  Blocks produced during partition: $blocks_produced"

    if [ $blocks_produced -lt 5 ]; then
        log_warning "Only $blocks_produced blocks produced during partition (expected ~$PARTITION_BLOCKS)"
    fi

    # Step 4: Verify target fell behind
    log_step "Step 4: Verifying $target_node fell behind..."
    local height_gap=$((ref_height_during - target_height_during))

    if [ $height_gap -gt 0 ]; then
        log_success "$target_node is $height_gap blocks behind $reference_node"
    else
        log_warning "$target_node did not fall behind as expected (gap: $height_gap)"
    fi

    # Step 5: Reconnect target node
    log_step "Step 5: Reconnecting $target_node to network..."
    reconnect_node "$target_node"

    # Step 6: Wait for re-sync
    log_step "Step 6: Waiting for $target_node to re-sync (timeout: ${SYNC_TIMEOUT}s)..."

    local sync_target=$ref_height_during

    if ! wait_for_height "$target_node" "$sync_target" "$SYNC_TIMEOUT"; then
        log_error "$target_node failed to sync to height $sync_target"
        test_passed=false
    fi

    # Step 7: Verify all nodes are at same height
    log_step "Step 7: Verifying chain consistency across all $NODE_COUNT nodes..."
    sleep 10  # Allow a few more blocks

    declare -A final_heights
    local max_height=0
    local min_height=999999999

    for node in "${NODE_NAMES[@]}"; do
        final_heights["$node"]=$(get_block_height "$node")
        log "  $node final height: ${final_heights[$node]}"
        [ "${final_heights[$node]}" -gt "$max_height" ] && max_height="${final_heights[$node]}"
        [ "${final_heights[$node]}" -lt "$min_height" ] && min_height="${final_heights[$node]}"
    done

    local final_gap=$((max_height - min_height))
    if [ $final_gap -le 2 ]; then  # Allow 2 block difference
        log_success "All nodes in sync (max gap: $final_gap blocks)"
    else
        log_error "Nodes not in sync (max gap: $final_gap blocks)"
        test_passed=false
    fi

    # Step 8: Verify block production resumed on all nodes
    log_step "Step 8: Verifying block production resumed on all nodes..."
    sleep $((BLOCK_INTERVAL * 3))

    for node in "${NODE_NAMES[@]}"; do
        if is_producing_blocks "$node" 20 || is_importing_blocks "$node" 20; then
            log_success "$node is active"
        else
            log_error "$node is not active"
            test_passed=false
        fi
    done

    # Calculate duration and print result
    local duration=$(($(date +%s) - start_time))

    echo ""
    if [ "$test_passed" = true ]; then
        print_result "Network Partition Recovery" "PASSED" "$duration"
    else
        print_result "Network Partition Recovery" "FAILED" "$duration"
    fi

    return $([ "$test_passed" = true ] && echo 0 || echo 1)
}

# ============================================================================
# SCENARIO 2: Node Restart (Random node stops/starts, syncs)
# ============================================================================

scenario_2_node_restart() {
    print_header "SCENARIO 2: Node Restart Recovery"

    # Select random target node for restart
    local target_node=$(get_random_node)
    local target_num=$(get_node_number "$target_node")
    local other_nodes=($(get_other_nodes "$target_node"))
    local reference_node="${other_nodes[0]}"

    echo "Description: Bring down $target_node completely, let other nodes produce blocks,"
    echo "             then bring $target_node back up and verify it syncs"
    echo ""
    echo "Target node: $target_node (will be stopped/restarted)"
    echo "Reference node: $reference_node"
    echo "Other nodes: ${other_nodes[*]}"
    echo ""

    local start_time=$(date +%s)
    local test_passed=true

    # Step 1: Record initial state for all nodes
    log_step "Step 1: Recording initial state..."
    declare -A initial_heights

    for node in "${NODE_NAMES[@]}"; do
        initial_heights["$node"]=$(get_block_height "$node")
        log "  $node height: ${initial_heights[$node]}"
    done

    # Step 2: Stop target node
    log_step "Step 2: Stopping $target_node container..."
    stop_node "$target_node"

    # Verify it's stopped
    if is_container_running "$target_node"; then
        log_error "$target_node failed to stop"
        test_passed=false
    else
        log_success "$target_node container stopped"
    fi

    # Step 3: Wait for blocks to accumulate on other nodes
    local wait_time=$((BLOCK_INTERVAL * PARTITION_BLOCKS))
    log_step "Step 3: Waiting ${wait_time}s for other nodes to produce blocks while $target_node is down..."
    sleep $wait_time

    local ref_height_while_down=$(get_block_height "$reference_node")
    local blocks_produced=$((ref_height_while_down - initial_heights[$reference_node]))

    log "  $reference_node height: $ref_height_while_down (+$blocks_produced blocks)"

    # Step 4: Start target node
    log_step "Step 4: Starting $target_node container..."
    if ! start_node "$target_node"; then
        log_error "Failed to start $target_node"
        test_passed=false
    fi

    # Wait for it to initialize
    log "  Waiting for $target_node to initialize..."
    sleep 15

    # Step 5: Wait for sync
    log_step "Step 5: Waiting for $target_node to sync (timeout: ${SYNC_TIMEOUT}s)..."

    local sync_target=$ref_height_while_down

    if ! wait_for_height "$target_node" "$sync_target" "$SYNC_TIMEOUT"; then
        log_error "$target_node failed to sync to height $sync_target"
        test_passed=false
    fi

    # Step 6: Verify chain consistency across all nodes
    log_step "Step 6: Verifying chain consistency across all $NODE_COUNT nodes..."
    sleep 10

    declare -A final_heights
    local max_height=0
    local min_height=999999999

    for node in "${NODE_NAMES[@]}"; do
        final_heights["$node"]=$(get_block_height "$node")
        log "  $node final height: ${final_heights[$node]}"
        [ "${final_heights[$node]}" -gt "$max_height" ] && max_height="${final_heights[$node]}"
        [ "${final_heights[$node]}" -lt "$min_height" ] && min_height="${final_heights[$node]}"
    done

    local final_gap=$((max_height - min_height))
    if [ $final_gap -le 2 ]; then
        log_success "All nodes in sync (max gap: $final_gap blocks)"
    else
        log_error "Nodes not in sync (max gap: $final_gap blocks)"
        test_passed=false
    fi

    # Step 7: Verify block production resumed on all nodes
    log_step "Step 7: Verifying block production resumed on all nodes..."
    sleep $((BLOCK_INTERVAL * 3))

    for node in "${NODE_NAMES[@]}"; do
        if is_producing_blocks "$node" 20 || is_importing_blocks "$node" 20; then
            log_success "$node is active"
        else
            log_error "$node is not active"
            test_passed=false
        fi
    done

    # Calculate duration and print result
    local duration=$(($(date +%s) - start_time))

    echo ""
    if [ "$test_passed" = true ]; then
        print_result "Node Restart Recovery" "PASSED" "$duration"
    else
        print_result "Node Restart Recovery" "FAILED" "$duration"
    fi

    return $([ "$test_passed" = true ] && echo 0 || echo 1)
}

# ============================================================================
# SCENARIO 3: Leader Failover (Random node down, others continue, node syncs)
# ============================================================================

scenario_3_leader_failover() {
    print_header "SCENARIO 3: Leader Failover"

    # Select random node as "leader" to take down
    local leader_node=$(get_random_node)
    local leader_num=$(get_node_number "$leader_node")
    local remaining_nodes=($(get_other_nodes "$leader_node"))
    local reference_node="${remaining_nodes[0]}"

    echo "Description: Bring down $leader_node (simulated leader failure),"
    echo "             verify remaining nodes continue producing blocks,"
    echo "             then bring $leader_node back up and verify it syncs"
    echo ""
    echo "Leader node (will fail): $leader_node"
    echo "Remaining nodes: ${remaining_nodes[*]}"
    echo ""

    local start_time=$(date +%s)
    local test_passed=true

    # Step 1: Record initial state for all nodes
    log_step "Step 1: Recording initial state..."
    declare -A initial_heights

    for node in "${NODE_NAMES[@]}"; do
        initial_heights["$node"]=$(get_block_height "$node")
        log "  $node height: ${initial_heights[$node]}"
    done

    # Step 2: Stop the leader node
    log_step "Step 2: Stopping $leader_node (simulating leader failure)..."
    stop_node "$leader_node"

    if is_container_running "$leader_node"; then
        log_error "$leader_node failed to stop"
        test_passed=false
    else
        log_success "$leader_node container stopped"
    fi

    # Step 3: Wait and verify remaining nodes continue producing
    local wait_time=$((BLOCK_INTERVAL * PARTITION_BLOCKS))
    log_step "Step 3: Waiting ${wait_time}s - verifying remaining ${#remaining_nodes[@]} nodes continue producing..."
    sleep $wait_time

    local ref_height_solo=$(get_block_height "$reference_node")
    local blocks_produced_solo=$((ref_height_solo - initial_heights[$reference_node]))

    log "  $reference_node height: $ref_height_solo (+$blocks_produced_solo blocks)"

    # With N validators in Aura, remaining nodes produce (N-1)/N of expected blocks
    local expected_blocks=$((PARTITION_BLOCKS * (NODE_COUNT - 1) / NODE_COUNT))
    if [ $blocks_produced_solo -ge $expected_blocks ]; then
        log_success "Remaining nodes produced $blocks_produced_solo blocks while $leader_node was down"
    else
        log_warning "Remaining nodes only produced $blocks_produced_solo blocks (expected ~$expected_blocks)"
    fi

    # Step 4: Verify remaining nodes are actively producing
    log_step "Step 4: Verifying remaining nodes are actively producing blocks..."
    for node in "${remaining_nodes[@]}"; do
        if is_producing_blocks "$node" 30; then
            log_success "$node is producing blocks"
        else
            log_verbose "$node may not be producing (could be expected if not its slot)"
        fi
    done

    # Step 5: Restart the leader node
    log_step "Step 5: Starting $leader_node..."
    if ! start_node "$leader_node"; then
        log_error "Failed to start $leader_node"
        test_passed=false
    fi

    # Wait for initialization
    log "  Waiting for $leader_node to initialize..."
    sleep 15

    # Step 6: Wait for leader to sync
    log_step "Step 6: Waiting for $leader_node to sync to remaining nodes' height..."

    local sync_target=$ref_height_solo

    if ! wait_for_height "$leader_node" "$sync_target" "$SYNC_TIMEOUT"; then
        log_error "$leader_node failed to sync to height $sync_target"
        test_passed=false
    fi

    # Step 7: Verify chain consistency across all nodes
    log_step "Step 7: Verifying chain consistency across all $NODE_COUNT nodes..."
    sleep 10

    declare -A final_heights
    local max_height=0
    local min_height=999999999

    for node in "${NODE_NAMES[@]}"; do
        final_heights["$node"]=$(get_block_height "$node")
        log "  $node final height: ${final_heights[$node]}"
        [ "${final_heights[$node]}" -gt "$max_height" ] && max_height="${final_heights[$node]}"
        [ "${final_heights[$node]}" -lt "$min_height" ] && min_height="${final_heights[$node]}"
    done

    local final_gap=$((max_height - min_height))
    if [ $final_gap -le 2 ]; then
        log_success "All nodes in sync (max gap: $final_gap blocks)"
    else
        log_error "Nodes not in sync (max gap: $final_gap blocks)"
        test_passed=false
    fi

    # Step 8: Verify block production resumed on all nodes
    log_step "Step 8: Verifying block production resumed on all nodes..."
    sleep $((BLOCK_INTERVAL * 4))

    for node in "${NODE_NAMES[@]}"; do
        if is_producing_blocks "$node" 20 || is_importing_blocks "$node" 20; then
            log_success "$node is active"
        else
            log_error "$node is not active"
            test_passed=false
        fi
    done

    # Calculate duration and print result
    local duration=$(($(date +%s) - start_time))

    echo ""
    if [ "$test_passed" = true ]; then
        print_result "Leader Failover" "PASSED" "$duration"
    else
        print_result "Leader Failover" "FAILED" "$duration"
    fi

    return $([ "$test_passed" = true ] && echo 0 || echo 1)
}

# ============================================================================
# SCENARIO 4: Network Latency (All nodes affected by latency)
# ============================================================================

scenario_4_network_latency() {
    print_header "SCENARIO 4: Network Latency Resilience"

    local delay_ms=500
    local jitter_ms=50

    echo "Description: Inject ${delay_ms}ms network latency (±${jitter_ms}ms) on all nodes,"
    echo "             verify block production continues, then remove latency"
    echo ""
    echo "Target: All $NODE_COUNT nodes"
    echo ""

    local start_time=$(date +%s)
    local test_passed=true

    # Step 1: Record initial state
    log_step "Step 1: Recording initial state..."
    declare -A initial_heights
    for node in "${NODE_NAMES[@]}"; do
        initial_heights["$node"]=$(get_block_height "$node")
        log "  $node height: ${initial_heights[$node]}"
    done

    # Step 2: Inject latency on all nodes
    log_step "Step 2: Injecting ${delay_ms}ms latency on all nodes..."
    for node in "${NODE_NAMES[@]}"; do
        inject_network_latency "$node" "$delay_ms" "$jitter_ms" || true
    done

    # Step 3: Wait and verify block production continues
    local wait_time=$((BLOCK_INTERVAL * PARTITION_BLOCKS))
    log_step "Step 3: Waiting ${wait_time}s to observe block production under latency..."
    sleep $wait_time

    declare -A latency_heights
    local blocks_during_latency=0
    for node in "${NODE_NAMES[@]}"; do
        latency_heights["$node"]=$(get_block_height "$node")
        local node_blocks=$((latency_heights[$node] - initial_heights[$node]))
        blocks_during_latency=$((blocks_during_latency + node_blocks))
        log "  $node height: ${latency_heights[$node]} (+$node_blocks)"
    done

    # Average blocks per node
    local avg_blocks=$((blocks_during_latency / NODE_COUNT))
    if [ $avg_blocks -ge $((PARTITION_BLOCKS / 2)) ]; then
        log_success "Block production continued under latency (avg $avg_blocks blocks/node)"
    else
        log_warning "Block production may be degraded under latency (avg $avg_blocks blocks/node)"
    fi

    # Step 4: Remove latency from all nodes
    log_step "Step 4: Removing latency from all nodes..."
    for node in "${NODE_NAMES[@]}"; do
        remove_network_latency "$node"
    done

    # Step 5: Verify recovery and chain consistency
    log_step "Step 5: Verifying chain consistency after latency removal..."
    sleep $((BLOCK_INTERVAL * 5))

    declare -A final_heights
    local max_height=0
    local min_height=999999999

    for node in "${NODE_NAMES[@]}"; do
        final_heights["$node"]=$(get_block_height "$node")
        log "  $node final height: ${final_heights[$node]}"
        [ "${final_heights[$node]}" -gt "$max_height" ] && max_height="${final_heights[$node]}"
        [ "${final_heights[$node]}" -lt "$min_height" ] && min_height="${final_heights[$node]}"
    done

    local final_gap=$((max_height - min_height))
    if [ $final_gap -le 2 ]; then
        log_success "All nodes in sync (max gap: $final_gap blocks)"
    else
        log_error "Nodes not in sync (max gap: $final_gap blocks)"
        test_passed=false
    fi

    # Calculate duration and print result
    local duration=$(($(date +%s) - start_time))

    echo ""
    if [ "$test_passed" = true ]; then
        print_result "Network Latency Resilience" "PASSED" "$duration"
    else
        print_result "Network Latency Resilience" "FAILED" "$duration"
    fi

    return $([ "$test_passed" = true ] && echo 0 || echo 1)
}

# ============================================================================
# SCENARIO 5: Packet Loss (Random node with packet loss)
# ============================================================================

scenario_5_packet_loss() {
    print_header "SCENARIO 5: Packet Loss Resilience"

    local target_node=$(get_random_node)
    local loss_percent=10

    echo "Description: Inject ${loss_percent}% packet loss on $target_node,"
    echo "             verify system continues operating, then remove packet loss"
    echo ""
    echo "Target node: $target_node"
    echo ""

    local start_time=$(date +%s)
    local test_passed=true

    # Step 1: Record initial state
    log_step "Step 1: Recording initial state..."
    declare -A initial_heights
    for node in "${NODE_NAMES[@]}"; do
        initial_heights["$node"]=$(get_block_height "$node")
        log "  $node height: ${initial_heights[$node]}"
    done

    # Step 2: Inject packet loss
    log_step "Step 2: Injecting ${loss_percent}% packet loss on $target_node..."
    inject_packet_loss "$target_node" "$loss_percent" || true

    # Step 3: Wait and observe
    local wait_time=$((BLOCK_INTERVAL * PARTITION_BLOCKS))
    log_step "Step 3: Waiting ${wait_time}s to observe system behavior under packet loss..."
    sleep $wait_time

    declare -A loss_heights
    for node in "${NODE_NAMES[@]}"; do
        loss_heights["$node"]=$(get_block_height "$node")
        local node_blocks=$((loss_heights[$node] - initial_heights[$node]))
        log "  $node height: ${loss_heights[$node]} (+$node_blocks)"
    done

    # Step 4: Remove packet loss
    log_step "Step 4: Removing packet loss from $target_node..."
    remove_packet_loss "$target_node"

    # Step 5: Verify recovery
    log_step "Step 5: Verifying chain consistency after packet loss removal..."
    sleep $((BLOCK_INTERVAL * 5))

    declare -A final_heights
    local max_height=0
    local min_height=999999999

    for node in "${NODE_NAMES[@]}"; do
        final_heights["$node"]=$(get_block_height "$node")
        log "  $node final height: ${final_heights[$node]}"
        [ "${final_heights[$node]}" -gt "$max_height" ] && max_height="${final_heights[$node]}"
        [ "${final_heights[$node]}" -lt "$min_height" ] && min_height="${final_heights[$node]}"
    done

    local final_gap=$((max_height - min_height))
    if [ $final_gap -le 2 ]; then
        log_success "All nodes in sync (max gap: $final_gap blocks)"
    else
        log_error "Nodes not in sync (max gap: $final_gap blocks)"
        test_passed=false
    fi

    local duration=$(($(date +%s) - start_time))

    echo ""
    if [ "$test_passed" = true ]; then
        print_result "Packet Loss Resilience" "PASSED" "$duration"
    else
        print_result "Packet Loss Resilience" "FAILED" "$duration"
    fi

    return $([ "$test_passed" = true ] && echo 0 || echo 1)
}

# ============================================================================
# SCENARIO 6: Memory Pressure (Random node under memory stress)
# ============================================================================

scenario_6_memory_pressure() {
    print_header "SCENARIO 6: Memory Pressure Resilience"

    local target_node=$(get_random_node)
    local memory_mb=256

    echo "Description: Apply ${memory_mb}MB memory pressure on $target_node,"
    echo "             verify node continues operating, then release pressure"
    echo ""
    echo "Target node: $target_node"
    echo ""

    local start_time=$(date +%s)
    local test_passed=true

    # Step 1: Record initial state
    log_step "Step 1: Recording initial state..."
    declare -A initial_heights
    for node in "${NODE_NAMES[@]}"; do
        initial_heights["$node"]=$(get_block_height "$node")
        log "  $node height: ${initial_heights[$node]}"
    done

    # Step 2: Inject memory pressure
    log_step "Step 2: Applying ${memory_mb}MB memory pressure on $target_node..."
    inject_memory_pressure "$target_node" "$memory_mb"

    # Step 3: Wait and observe
    local wait_time=$((BLOCK_INTERVAL * PARTITION_BLOCKS))
    log_step "Step 3: Waiting ${wait_time}s to observe system behavior under memory pressure..."
    sleep $wait_time

    # Verify target node is still running
    if is_container_running "$target_node"; then
        log_success "$target_node still running under memory pressure"
    else
        log_error "$target_node crashed under memory pressure"
        test_passed=false
    fi

    declare -A pressure_heights
    for node in "${NODE_NAMES[@]}"; do
        pressure_heights["$node"]=$(get_block_height "$node")
        local node_blocks=$((pressure_heights[$node] - initial_heights[$node]))
        log "  $node height: ${pressure_heights[$node]} (+$node_blocks)"
    done

    # Step 4: Remove memory pressure
    log_step "Step 4: Releasing memory pressure from $target_node..."
    remove_memory_pressure "$target_node"

    # Step 5: Verify recovery
    log_step "Step 5: Verifying chain consistency after memory pressure release..."
    sleep $((BLOCK_INTERVAL * 5))

    declare -A final_heights
    local max_height=0
    local min_height=999999999

    for node in "${NODE_NAMES[@]}"; do
        final_heights["$node"]=$(get_block_height "$node")
        log "  $node final height: ${final_heights[$node]}"
        [ "${final_heights[$node]}" -gt "$max_height" ] && max_height="${final_heights[$node]}"
        [ "${final_heights[$node]}" -lt "$min_height" ] && min_height="${final_heights[$node]}"
    done

    local final_gap=$((max_height - min_height))
    if [ $final_gap -le 2 ]; then
        log_success "All nodes in sync (max gap: $final_gap blocks)"
    else
        log_error "Nodes not in sync (max gap: $final_gap blocks)"
        test_passed=false
    fi

    local duration=$(($(date +%s) - start_time))

    echo ""
    if [ "$test_passed" = true ]; then
        print_result "Memory Pressure Resilience" "PASSED" "$duration"
    else
        print_result "Memory Pressure Resilience" "FAILED" "$duration"
    fi

    return $([ "$test_passed" = true ] && echo 0 || echo 1)
}

# ============================================================================
# SCENARIO 7: Disk I/O Stress (Random node under disk stress)
# ============================================================================

scenario_7_disk_stress() {
    print_header "SCENARIO 7: Disk I/O Stress Resilience"

    local target_node=$(get_random_node)
    local num_procs=5

    echo "Description: Apply disk I/O stress ($num_procs processes) on $target_node,"
    echo "             verify node continues operating, then stop stress"
    echo ""
    echo "Target node: $target_node"
    echo ""

    local start_time=$(date +%s)
    local test_passed=true

    # Step 1: Record initial state
    log_step "Step 1: Recording initial state..."
    declare -A initial_heights
    for node in "${NODE_NAMES[@]}"; do
        initial_heights["$node"]=$(get_block_height "$node")
        log "  $node height: ${initial_heights[$node]}"
    done

    # Step 2: Inject disk stress
    log_step "Step 2: Applying disk I/O stress on $target_node..."
    inject_disk_stress "$target_node" "$num_procs"

    # Step 3: Wait and observe
    local wait_time=$((BLOCK_INTERVAL * PARTITION_BLOCKS))
    log_step "Step 3: Waiting ${wait_time}s to observe system behavior under disk stress..."
    sleep $wait_time

    # Verify target node is still running
    if is_container_running "$target_node"; then
        log_success "$target_node still running under disk stress"
    else
        log_error "$target_node crashed under disk stress"
        test_passed=false
    fi

    declare -A stress_heights
    for node in "${NODE_NAMES[@]}"; do
        stress_heights["$node"]=$(get_block_height "$node")
        local node_blocks=$((stress_heights[$node] - initial_heights[$node]))
        log "  $node height: ${stress_heights[$node]} (+$node_blocks)"
    done

    # Step 4: Remove disk stress
    log_step "Step 4: Removing disk stress from $target_node..."
    remove_disk_stress "$target_node"

    # Step 5: Verify recovery
    log_step "Step 5: Verifying chain consistency after disk stress removal..."
    sleep $((BLOCK_INTERVAL * 5))

    declare -A final_heights
    local max_height=0
    local min_height=999999999

    for node in "${NODE_NAMES[@]}"; do
        final_heights["$node"]=$(get_block_height "$node")
        log "  $node final height: ${final_heights[$node]}"
        [ "${final_heights[$node]}" -gt "$max_height" ] && max_height="${final_heights[$node]}"
        [ "${final_heights[$node]}" -lt "$min_height" ] && min_height="${final_heights[$node]}"
    done

    local final_gap=$((max_height - min_height))
    if [ $final_gap -le 2 ]; then
        log_success "All nodes in sync (max gap: $final_gap blocks)"
    else
        log_error "Nodes not in sync (max gap: $final_gap blocks)"
        test_passed=false
    fi

    local duration=$(($(date +%s) - start_time))

    echo ""
    if [ "$test_passed" = true ]; then
        print_result "Disk I/O Stress Resilience" "PASSED" "$duration"
    else
        print_result "Disk I/O Stress Resilience" "FAILED" "$duration"
    fi

    return $([ "$test_passed" = true ] && echo 0 || echo 1)
}

# ============================================================================
# SCENARIO 8: Execution Layer Failure
# ============================================================================

scenario_8_execution_failure() {
    print_header "SCENARIO 8: Execution Layer Failure Recovery"

    echo "Description: Stop the execution layer, observe node behavior,"
    echo "             then restart and verify recovery"
    echo ""

    local start_time=$(date +%s)
    local test_passed=true

    # Step 1: Record initial state
    log_step "Step 1: Recording initial state..."
    declare -A initial_heights
    for node in "${NODE_NAMES[@]}"; do
        initial_heights["$node"]=$(get_block_height "$node")
        log "  $node height: ${initial_heights[$node]}"
    done

    # Step 2: Stop execution layer
    log_step "Step 2: Stopping execution layer..."
    stop_execution_layer

    # Verify it's stopped
    if is_container_running "execution"; then
        log_error "Execution layer failed to stop"
        test_passed=false
    else
        log_success "Execution layer stopped"
    fi

    # Step 3: Wait and observe node behavior
    local wait_time=$((BLOCK_INTERVAL * 5))
    log_step "Step 3: Waiting ${wait_time}s to observe node behavior without execution layer..."
    sleep $wait_time

    # Nodes should still be running (but may not produce blocks)
    local nodes_running=0
    for node in "${NODE_NAMES[@]}"; do
        if is_container_running "$node"; then
            nodes_running=$((nodes_running + 1))
        fi
    done
    log "  $nodes_running/$NODE_COUNT nodes still running"

    # Step 4: Restart execution layer
    log_step "Step 4: Restarting execution layer..."
    if ! start_execution_layer; then
        log_error "Failed to restart execution layer"
        test_passed=false
    fi

    # Wait for execution layer to fully initialize
    log "  Waiting for execution layer to initialize..."
    sleep 15

    # Step 5: Verify block production resumes
    log_step "Step 5: Verifying block production resumes..."
    sleep $((BLOCK_INTERVAL * PARTITION_BLOCKS))

    declare -A final_heights
    local max_height=0
    local min_height=999999999
    local block_production_resumed=false

    for node in "${NODE_NAMES[@]}"; do
        final_heights["$node"]=$(get_block_height "$node")
        local node_blocks=$((final_heights[$node] - initial_heights[$node]))
        log "  $node final height: ${final_heights[$node]} (+$node_blocks)"
        [ "${final_heights[$node]}" -gt "$max_height" ] && max_height="${final_heights[$node]}"
        [ "${final_heights[$node]}" -lt "$min_height" ] && min_height="${final_heights[$node]}"
        if [ $node_blocks -gt 0 ]; then
            block_production_resumed=true
        fi
    done

    if [ "$block_production_resumed" = true ]; then
        log_success "Block production resumed after execution layer recovery"
    else
        log_error "Block production did not resume"
        test_passed=false
    fi

    local final_gap=$((max_height - min_height))
    if [ $final_gap -le 2 ]; then
        log_success "All nodes in sync (max gap: $final_gap blocks)"
    else
        log_error "Nodes not in sync (max gap: $final_gap blocks)"
        test_passed=false
    fi

    local duration=$(($(date +%s) - start_time))

    echo ""
    if [ "$test_passed" = true ]; then
        print_result "Execution Layer Failure Recovery" "PASSED" "$duration"
    else
        print_result "Execution Layer Failure Recovery" "FAILED" "$duration"
    fi

    return $([ "$test_passed" = true ] && echo 0 || echo 1)
}

# ============================================================================
# SCENARIO 9: Bitcoin Core Failure
# ============================================================================

scenario_9_bitcoin_failure() {
    print_header "SCENARIO 9: Bitcoin Core Failure Recovery"

    echo "Description: Stop Bitcoin Core, observe node behavior,"
    echo "             then restart and verify recovery"
    echo ""

    local start_time=$(date +%s)
    local test_passed=true

    # Step 1: Record initial state
    log_step "Step 1: Recording initial state..."
    declare -A initial_heights
    for node in "${NODE_NAMES[@]}"; do
        initial_heights["$node"]=$(get_block_height "$node")
        log "  $node height: ${initial_heights[$node]}"
    done

    # Step 2: Stop Bitcoin Core
    log_step "Step 2: Stopping Bitcoin Core..."
    stop_bitcoin_core

    # Verify it's stopped
    if is_container_running "bitcoin-core"; then
        log_error "Bitcoin Core failed to stop"
        test_passed=false
    else
        log_success "Bitcoin Core stopped"
    fi

    # Step 3: Wait and observe node behavior
    local wait_time=$((BLOCK_INTERVAL * 5))
    log_step "Step 3: Waiting ${wait_time}s to observe node behavior without Bitcoin Core..."
    sleep $wait_time

    # Nodes should still be running
    local nodes_running=0
    for node in "${NODE_NAMES[@]}"; do
        if is_container_running "$node"; then
            nodes_running=$((nodes_running + 1))
        fi
    done
    log "  $nodes_running/$NODE_COUNT nodes still running"

    # Step 4: Restart Bitcoin Core
    log_step "Step 4: Restarting Bitcoin Core..."
    if ! start_bitcoin_core; then
        log_error "Failed to restart Bitcoin Core"
        test_passed=false
    fi

    # Wait for Bitcoin Core to fully initialize
    log "  Waiting for Bitcoin Core to initialize..."
    sleep 15

    # Step 5: Verify block production resumes
    log_step "Step 5: Verifying block production resumes..."
    sleep $((BLOCK_INTERVAL * PARTITION_BLOCKS))

    declare -A final_heights
    local max_height=0
    local min_height=999999999
    local block_production_resumed=false

    for node in "${NODE_NAMES[@]}"; do
        final_heights["$node"]=$(get_block_height "$node")
        local node_blocks=$((final_heights[$node] - initial_heights[$node]))
        log "  $node final height: ${final_heights[$node]} (+$node_blocks)"
        [ "${final_heights[$node]}" -gt "$max_height" ] && max_height="${final_heights[$node]}"
        [ "${final_heights[$node]}" -lt "$min_height" ] && min_height="${final_heights[$node]}"
        if [ $node_blocks -gt 0 ]; then
            block_production_resumed=true
        fi
    done

    if [ "$block_production_resumed" = true ]; then
        log_success "Block production resumed after Bitcoin Core recovery"
    else
        log_error "Block production did not resume"
        test_passed=false
    fi

    local final_gap=$((max_height - min_height))
    if [ $final_gap -le 2 ]; then
        log_success "All nodes in sync (max gap: $final_gap blocks)"
    else
        log_error "Nodes not in sync (max gap: $final_gap blocks)"
        test_passed=false
    fi

    local duration=$(($(date +%s) - start_time))

    echo ""
    if [ "$test_passed" = true ]; then
        print_result "Bitcoin Core Failure Recovery" "PASSED" "$duration"
    else
        print_result "Bitcoin Core Failure Recovery" "FAILED" "$duration"
    fi

    return $([ "$test_passed" = true ] && echo 0 || echo 1)
}

# ============================================================================
# Stress Test Mode
# ============================================================================

# Available chaos types for stress testing
CHAOS_TYPES=("partition" "restart" "latency" "packet_loss" "memory" "disk")

# Run a random chaos event
run_random_chaos() {
    local chaos_type="${CHAOS_TYPES[$((RANDOM % ${#CHAOS_TYPES[@]}))]}"
    local target_node=$(get_random_node)
    local duration=${1:-30}

    log "Injecting chaos: $chaos_type on $target_node for ${duration}s..."

    case $chaos_type in
        partition)
            disconnect_node "$target_node"
            sleep "$duration"
            reconnect_node "$target_node"
            ;;
        restart)
            stop_node "$target_node"
            sleep "$duration"
            start_node "$target_node"
            ;;
        latency)
            inject_network_latency "$target_node" 500 50 || true
            sleep "$duration"
            remove_network_latency "$target_node"
            ;;
        packet_loss)
            inject_packet_loss "$target_node" 10 || true
            sleep "$duration"
            remove_packet_loss "$target_node"
            ;;
        memory)
            inject_memory_pressure "$target_node" 256
            sleep "$duration"
            remove_memory_pressure "$target_node"
            ;;
        disk)
            inject_disk_stress "$target_node" 5
            sleep "$duration"
            remove_disk_stress "$target_node"
            ;;
    esac

    log_success "Chaos event completed: $chaos_type on $target_node"
}

# Run stress test mode with continuous chaos injection
run_stress_test() {
    local duration=${1:-300}     # Total duration in seconds
    local failure_rate=${2:-0.3} # Probability of chaos per interval (0.0-1.0)
    local interval=${3:-30}      # Check interval in seconds

    print_header "Stress Test Mode"
    echo "Duration: ${duration}s"
    echo "Failure Rate: ${failure_rate} (probability per ${interval}s interval)"
    echo "Nodes: $NODE_COUNT (${NODE_NAMES[*]})"
    echo ""

    local start_time=$(date +%s)
    local end_time=$((start_time + duration))
    local chaos_events=0
    local metrics_file="$LOG_DIR/${TEST_ID}-stress-metrics.log"

    log "Starting stress test..."
    echo "Stress Test Started: $(date)" > "$metrics_file"

    while [ $(date +%s) -lt $end_time ]; do
        local now=$(date +%s)
        local elapsed=$((now - start_time))
        local remaining=$((end_time - now))

        log_verbose "Stress test: ${elapsed}s elapsed, ${remaining}s remaining"

        # Collect metrics periodically
        collect_metrics_snapshot "$metrics_file"

        # Random chaos injection based on failure rate
        # Use $RANDOM (0-32767) to simulate probability
        local threshold=$((32767 * ${failure_rate%.*}${failure_rate#*.} / 100))
        if [ $((RANDOM)) -lt $threshold ]; then
            chaos_events=$((chaos_events + 1))
            run_random_chaos "$interval" &
        fi

        sleep "$interval"
    done

    # Wait for any background chaos events to complete
    wait

    log_success "Stress test completed"
    log "  Duration: ${duration}s"
    log "  Chaos events injected: $chaos_events"
    log "  Metrics file: $metrics_file"

    # Final system check
    log_step "Final system health check..."
    sleep 10

    local all_healthy=true
    for node in "${NODE_NAMES[@]}"; do
        if is_container_running "$node"; then
            log_success "$node is running"
        else
            log_error "$node is not running"
            all_healthy=false
        fi
    done

    if [ "$all_healthy" = true ]; then
        print_result "Stress Test" "PASSED" "$duration"
    else
        print_result "Stress Test" "FAILED" "$duration"
    fi
}

# ============================================================================
# Interactive Mode
# ============================================================================

show_interactive_menu() {
    clear
    echo -e "${BLUE}╔════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║         Alys V2 Interactive Chaos Testing Console              ║${NC}"
    echo -e "${BLUE}╚════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${CYAN}Test ID:${NC} $TEST_ID"
    echo -e "${CYAN}Nodes:${NC} $NODE_COUNT (${NODE_NAMES[*]})"
    echo ""

    # Show current system status
    echo -e "${YELLOW}Current System Status:${NC}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    for node in "${NODE_NAMES[@]}"; do
        local status=$(docker inspect -f '{{.State.Status}}' "$node" 2>/dev/null || echo "not_found")
        local height=$(get_block_height "$node")
        if [ "$status" = "running" ]; then
            echo -e "  ${GREEN}●${NC} $node: $status (height: $height)"
        else
            echo -e "  ${RED}●${NC} $node: $status"
        fi
    done
    for container in execution bitcoin-core; do
        local status=$(docker inspect -f '{{.State.Status}}' "$container" 2>/dev/null || echo "not_found")
        if [ "$status" = "running" ]; then
            echo -e "  ${GREEN}●${NC} $container: $status"
        else
            echo -e "  ${RED}●${NC} $container: $status"
        fi
    done
    echo ""

    cat <<EOF
${CYAN}Chaos Scenarios:${NC}
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

${GREEN}Network Chaos:${NC}
  1) Network partition (disconnect random node)
  2) Network latency (add 500ms delay)
  3) Packet loss (10% packet drop)

${GREEN}Node Chaos:${NC}
  4) Stop random node (and restart)
  5) Memory pressure (256MB allocation)
  6) Disk I/O stress

${GREEN}Infrastructure Chaos:${NC}
  7) Execution layer failure
  8) Bitcoin Core failure

${GREEN}Observation:${NC}
  s) Show system status
  m) Collect metrics snapshot
  l) View recent logs (Node 1)

${GREEN}Test Scenarios:${NC}
  t1) Run Scenario 1 (Network Partition)
  t2) Run Scenario 2 (Node Restart)
  t3) Run Scenario 3 (Leader Failover)
  ta) Run all scenarios

${GREEN}Session:${NC}
  r) Generate report
  q) Quit

${CYAN}Enter choice:${NC}
EOF
}

run_interactive_mode() {
    print_header "Interactive Chaos Testing Mode"

    while true; do
        show_interactive_menu
        read -r choice

        case $choice in
            1)
                local node=$(get_random_node)
                echo -e "\n${RED}Disconnecting $node from network...${NC}"
                disconnect_node "$node"
                echo -e "${YELLOW}Press Enter to reconnect...${NC}"
                read -r
                reconnect_node "$node"
                ;;
            2)
                local node=$(get_random_node)
                echo -e "\n${RED}Injecting 500ms latency on $node...${NC}"
                inject_network_latency "$node" 500 50 || true
                echo -e "${YELLOW}Press Enter to remove latency...${NC}"
                read -r
                remove_network_latency "$node"
                ;;
            3)
                local node=$(get_random_node)
                echo -e "\n${RED}Injecting 10% packet loss on $node...${NC}"
                inject_packet_loss "$node" 10 || true
                echo -e "${YELLOW}Press Enter to remove packet loss...${NC}"
                read -r
                remove_packet_loss "$node"
                ;;
            4)
                local node=$(get_random_node)
                echo -e "\n${RED}Stopping $node...${NC}"
                stop_node "$node"
                echo -e "${YELLOW}Press Enter to restart...${NC}"
                read -r
                start_node "$node"
                ;;
            5)
                local node=$(get_random_node)
                echo -e "\n${RED}Applying memory pressure on $node...${NC}"
                inject_memory_pressure "$node" 256
                echo -e "${YELLOW}Press Enter to release...${NC}"
                read -r
                remove_memory_pressure "$node"
                ;;
            6)
                local node=$(get_random_node)
                echo -e "\n${RED}Applying disk stress on $node...${NC}"
                inject_disk_stress "$node" 5
                echo -e "${YELLOW}Press Enter to stop...${NC}"
                read -r
                remove_disk_stress "$node"
                ;;
            7)
                echo -e "\n${RED}Stopping execution layer...${NC}"
                stop_execution_layer
                echo -e "${YELLOW}Press Enter to restart...${NC}"
                read -r
                start_execution_layer
                ;;
            8)
                echo -e "\n${RED}Stopping Bitcoin Core...${NC}"
                stop_bitcoin_core
                echo -e "${YELLOW}Press Enter to restart...${NC}"
                read -r
                start_bitcoin_core
                ;;
            s)
                echo ""
                # Status is shown in menu, just pause
                echo "Press Enter to continue..."
                read -r
                ;;
            m)
                echo ""
                collect_metrics_snapshot
                echo "Press Enter to continue..."
                read -r
                ;;
            l)
                echo -e "\n${CYAN}Recent logs from alys-node-1:${NC}"
                docker logs --tail=30 alys-node-1 2>&1 | sed 's/^/  /'
                echo ""
                echo "Press Enter to continue..."
                read -r
                ;;
            t1)
                scenario_1_network_partition
                echo "Press Enter to continue..."
                read -r
                ;;
            t2)
                scenario_2_node_restart
                echo "Press Enter to continue..."
                read -r
                ;;
            t3)
                scenario_3_leader_failover
                echo "Press Enter to continue..."
                read -r
                ;;
            ta)
                scenario_1_network_partition || true
                sleep 5
                scenario_2_node_restart || true
                sleep 5
                scenario_3_leader_failover || true
                echo "Press Enter to continue..."
                read -r
                ;;
            r)
                generate_report
                echo "Press Enter to continue..."
                read -r
                ;;
            q|Q)
                echo -e "\n${CYAN}Generate report before exiting? (y/n)${NC}"
                read -r gen_report
                if [[ $gen_report =~ ^[Yy]$ ]]; then
                    generate_report
                fi
                echo -e "${GREEN}Exiting interactive mode.${NC}"
                return 0
                ;;
            *)
                echo -e "${RED}Invalid choice${NC}"
                sleep 1
                ;;
        esac
    done
}

# ============================================================================
# Report Generation
# ============================================================================

generate_report() {
    local report_file="$REPORT_DIR/${TEST_ID}-report.md"

    log "Generating test report..."

    # Build dynamic node status table
    local node_status_table=""
    for node in "${NODE_NAMES[@]}"; do
        local status=$(docker inspect -f '{{.State.Status}}' "$node" 2>/dev/null || echo "N/A")
        node_status_table+="| $node | $status |
"
    done

    cat > "$report_file" <<EOF
# Tier 1 Chaos Testing Report

**Test ID:** $TEST_ID
**Date:** $(date '+%Y-%m-%d %H:%M:%S')
**Node Count:** $NODE_COUNT
**Nodes:** ${NODE_NAMES[*]}

---

## Summary

| Metric | Value |
|--------|-------|
| Passed Tests | $PASSED_TESTS |
| Failed Tests | $FAILED_TESTS |
| Skipped Tests | $SKIPPED_TESTS |
| Total Tests | $((PASSED_TESTS + FAILED_TESTS + SKIPPED_TESTS)) |

---

## Test Results

### Scenario 1: Network Partition Recovery
Tests that a randomly selected node can recover and re-sync after being disconnected from the network.

### Scenario 2: Node Restart Recovery
Tests that a randomly selected node can sync from disk and network after a complete container restart.

### Scenario 3: Leader Failover
Tests that remaining nodes continue block production when a randomly selected node fails, and the failed node can sync on return.

---

## System State After Tests

### Alys Nodes
| Container | Status |
|-----------|--------|
${node_status_table}
### Infrastructure
| Container | Status |
|-----------|--------|
| execution | $(docker inspect -f '{{.State.Status}}' execution 2>/dev/null || echo "N/A") |
| bitcoin-core | $(docker inspect -f '{{.State.Status}}' bitcoin-core 2>/dev/null || echo "N/A") |

---

**Report Generated:** $(date '+%Y-%m-%d %H:%M:%S')
EOF

    log_success "Report generated: $report_file"
}

# ============================================================================
# CLI Interface
# ============================================================================

show_usage() {
    cat <<EOF
Alys V2 Chaos Testing Framework

Usage: $0 [OPTIONS]

Modes:
    --mode scenario          Run structured test scenarios (default)
    --mode stress            Run continuous stress testing with random chaos
    --mode interactive       Interactive menu-driven chaos injection

Scenario Options:
    --scenario <N|all>       Run specific scenario(s) (default: all)
    --nodes <N>              Limit to first N nodes (default: auto-detect)
    --verbose                Enable verbose output

Stress Test Options:
    --duration <seconds>     Total stress test duration (default: 300)
    --failure-rate <0.0-1.0> Probability of chaos per interval (default: 0.3)

Scenarios:
    Core Scenarios (Tier 1):
      1  Network Partition     - Random node partitioned, then re-syncs
      2  Node Restart          - Random node stops/restarts and syncs
      3  Leader Failover       - Random node down, others continue

    Network Scenarios:
      4  Network Latency       - All nodes with 500ms latency
      5  Packet Loss           - Random node with 10% packet loss

    Resource Scenarios:
      6  Memory Pressure       - Random node with memory stress
      7  Disk I/O Stress       - Random node with disk stress

    Infrastructure Scenarios:
      8  Execution Failure     - Execution layer stops and restarts
      9  Bitcoin Failure       - Bitcoin Core stops and restarts

    Composite:
      all                      - Run all scenarios (1-9) in sequence
      tier1                    - Run core scenarios (1-3) only

Node Selection:
    Nodes are auto-discovered from running alys-node-* containers.
    Each scenario randomly selects a target node for disruption.
    Use --nodes to limit testing to a subset of available nodes.

Examples:
    # Run all scenarios with all available nodes
    $0 --scenario all

    # Run core tier 1 scenarios only
    $0 --scenario tier1

    # Run network partition test with verbose output
    $0 --scenario 1 --verbose

    # Run stress test for 10 minutes with 20% chaos probability
    $0 --mode stress --duration 600 --failure-rate 0.2

    # Interactive chaos injection mode
    $0 --mode interactive

    # Run execution failure scenario with 3 nodes
    $0 --scenario 8 --nodes 3
EOF
}

# ============================================================================
# Main Entry Point
# ============================================================================

main() {
    local mode="scenario"
    local scenario="all"
    local stress_duration=300
    local stress_failure_rate="0.3"

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --mode)
                mode="$2"
                if [[ ! "$mode" =~ ^(scenario|stress|interactive)$ ]]; then
                    log_error "--mode must be one of: scenario, stress, interactive"
                    exit 1
                fi
                shift 2
                ;;
            --scenario)
                scenario="$2"
                shift 2
                ;;
            --nodes)
                SPECIFIED_NODE_COUNT="$2"
                if ! [[ "$SPECIFIED_NODE_COUNT" =~ ^[0-9]+$ ]] || [ "$SPECIFIED_NODE_COUNT" -lt 2 ]; then
                    log_error "--nodes must be a number >= 2"
                    exit 1
                fi
                shift 2
                ;;
            --duration)
                stress_duration="$2"
                if ! [[ "$stress_duration" =~ ^[0-9]+$ ]]; then
                    log_error "--duration must be a positive integer (seconds)"
                    exit 1
                fi
                shift 2
                ;;
            --failure-rate)
                stress_failure_rate="$2"
                shift 2
                ;;
            --verbose)
                VERBOSE=true
                shift
                ;;
            --help)
                show_usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                show_usage
                exit 1
                ;;
        esac
    done

    # Print header
    print_header "Alys V2 Chaos Testing Framework"
    echo "Test ID: $TEST_ID"
    echo "Mode: $mode"
    if [ "$mode" = "scenario" ]; then
        echo "Scenario: $scenario"
    elif [ "$mode" = "stress" ]; then
        echo "Duration: ${stress_duration}s"
        echo "Failure Rate: $stress_failure_rate"
    fi
    echo "Verbose: $VERBOSE"
    if [ "$SPECIFIED_NODE_COUNT" -gt 0 ]; then
        echo "Requested nodes: $SPECIFIED_NODE_COUNT"
    else
        echo "Nodes: auto-detect"
    fi
    echo ""

    # Pre-flight checks
    check_prerequisites

    # Run based on mode
    case $mode in
        interactive)
            run_interactive_mode
            exit 0
            ;;
        stress)
            wait_for_stable_system
            run_stress_test "$stress_duration" "$stress_failure_rate"
            generate_report
            ;;
        scenario)
            # Wait for system to stabilize
            wait_for_stable_system

            # Run scenarios
            case $scenario in
                1)
                    scenario_1_network_partition
                    ;;
                2)
                    scenario_2_node_restart
                    ;;
                3)
                    scenario_3_leader_failover
                    ;;
                4)
                    scenario_4_network_latency
                    ;;
                5)
                    scenario_5_packet_loss
                    ;;
                6)
                    scenario_6_memory_pressure
                    ;;
                7)
                    scenario_7_disk_stress
                    ;;
                8)
                    scenario_8_execution_failure
                    ;;
                9)
                    scenario_9_bitcoin_failure
                    ;;
                tier1)
                    log "Running Tier 1 scenarios (1-3)..."
                    echo ""

                    scenario_1_network_partition || true
                    sleep 10

                    scenario_2_node_restart || true
                    sleep 10

                    scenario_3_leader_failover || true
                    ;;
                all)
                    log "Running all scenarios (1-9)..."
                    echo ""

                    scenario_1_network_partition || true
                    sleep 10

                    scenario_2_node_restart || true
                    sleep 10

                    scenario_3_leader_failover || true
                    sleep 10

                    scenario_4_network_latency || true
                    sleep 10

                    scenario_5_packet_loss || true
                    sleep 10

                    scenario_6_memory_pressure || true
                    sleep 10

                    scenario_7_disk_stress || true
                    sleep 10

                    scenario_8_execution_failure || true
                    sleep 10

                    scenario_9_bitcoin_failure || true
                    ;;
                *)
                    log_error "Unknown scenario: $scenario"
                    show_usage
                    exit 1
                    ;;
            esac

            # Generate report
            generate_report
            ;;
    esac

    # Print final summary
    print_header "Final Summary"
    echo -e "  ${CYAN}Nodes:${NC}   $NODE_COUNT (${NODE_NAMES[*]})"
    echo -e "  ${GREEN}Passed:${NC}  $PASSED_TESTS"
    echo -e "  ${RED}Failed:${NC}  $FAILED_TESTS"
    echo -e "  ${YELLOW}Skipped:${NC} $SKIPPED_TESTS"
    echo ""

    if [ $FAILED_TESTS -eq 0 ]; then
        echo -e "${GREEN}${BOLD}All tests passed!${NC}"
        exit 0
    else
        echo -e "${RED}${BOLD}Some tests failed.${NC}"
        exit 1
    fi
}

main "$@"
