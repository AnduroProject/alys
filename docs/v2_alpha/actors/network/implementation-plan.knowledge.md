# NetworkActor V2 Implementation Plan - Systematic Porting & Simplification

## Executive Summary

The NetworkActor system is a **massive, sophisticated P2P networking solution** with **26,125+ lines of code** across multiple actors. Unlike the StorageActor (5,000 lines), this system requires careful architectural decisions to maintain functionality while achieving significant simplification.

### **V1 System Complexity Analysis**

**Current Architecture:**
- **NetworkActor**: Core P2P networking with libp2p integration
- **PeerActor**: Peer management and discovery (2,655 lines)
- **SyncActor**: Blockchain synchronization logic (13,333 lines)
- **NetworkSupervisor**: Fault-tolerant supervision and health monitoring
- **Complex Protocol Stack**: Gossipsub, Kademlia DHT, mDNS, Request-Response
- **Custom Dependencies**: Heavy `actor_system` crate usage
- **Comprehensive Testing**: 14 test files with chaos/performance testing

**Identified Simplification Opportunities:**
1. **Actor Consolidation**: Merge overlapping functionality between actors
2. **Protocol Reduction**: Eliminate unused or redundant P2P protocols
3. **Supervisor Simplification**: Reduce fault tolerance complexity
4. **Message Protocol Streamlining**: Consolidate similar message types
5. **Dependency Cleanup**: Remove `actor_system` and simplify dependencies

---

## Phase 1: Architecture Decision & Consolidation Strategy

### 1.1 Actor Consolidation Analysis

**Current Multi-Actor System:**
```rust
// V1 - Complex multi-actor architecture
NetworkSupervisor
├── NetworkActor (libp2p management)
├── PeerActor (peer discovery/management)
└── SyncActor (blockchain synchronization)
```

**V2 Simplified Architecture (Two-Actor System):**
```rust
// V2 - Two-Actor System (Selected Design)
NetworkActor (P2P protocols)
├── peer_manager: PeerManager
├── gossip_handler: GossipHandler
└── protocol_handler: ProtocolHandler

SyncActor (blockchain sync only)
├── sync_state: SyncState
├── block_requests: BlockRequestManager
└── peer_coordination: PeerCoordinator
```

**Selected: Two-Actor System**
- **Justification**: Clear separation of concerns - networking vs. blockchain logic
- **Benefits**: Easier to reason about, independent scaling, cleaner testing
- **Trade-off**: Some inter-actor communication but eliminates supervision complexity
- **No NetworkSupervisor**: Actors manage their own lifecycle

### 1.2 Protocol Stack Simplification

**Current V1 Protocols:**
```rust
// V1 - Full libp2p protocol stack
- Gossipsub (message broadcasting)
- Kademlia DHT (peer discovery)
- mDNS (local discovery)
- Request-Response (direct peer queries)
- Transport: TCP + QUIC
- Security: Noise protocol
- Multiplexing: Yamux
```

**V2 Simplified Stack:**
```rust
// V2 - Essential protocols only
- Gossipsub (core message broadcasting)
- Request-Response (direct peer queries)
- mDNS (local network discovery - REQUIRED from V1)
- Bootstrap Discovery (simple peer list based)
- Transport: TCP only
- Security: Noise protocol (keep)
- Multiplexing: Yamux (keep)
```

**Protocols Removed:**
- **Kademlia DHT**: Removed - use bootstrap-based discovery + mDNS
- **QUIC Transport**: Removed - TCP sufficient for initial implementation

**Protocols Preserved from V1:**
- **mDNS**: **REQUIRED** - used in V1 for local network discovery

---

## Phase 2: Dependency Cleanup & Foundation

### 2.1 Remove Custom Actor System Dependencies

**From V1:**
```rust
use actor_system::{AlysActor, LifecycleAware, ActorResult, ActorError};
use actor_system::blockchain::{BlockchainAwareActor, BlockchainTimingConstraints};
use actor_system::supervision::{RestartStrategy, SupervisorStrategy};
```

**To V2:**
```rust
// Use standard Actix patterns only
use actix::prelude::*;
use tokio::time::{Duration, Instant};
use anyhow::{Result, Error}; // Replace ActorResult/ActorError
```

### 2.2 Dependencies to Keep/Add/Remove

**Keep (Core Networking):**
```toml
# Essential P2P networking
libp2p = { version = "0.52", features = ["gossipsub", "noise", "tcp", "yamux"] }
actix = "0.13"
tokio = { version = "1.0", features = ["full"] }
tracing = "0.1"
serde = { version = "1.0", features = ["derive"] }
```

**Add to V2:**
```toml
# Add to V2's Cargo.toml
libp2p = "0.52"  # Not currently in V2
anyhow = "1.0"   # Error handling
thiserror = "1.0" # Custom error types
```

**Remove (Eliminate Complexity):**
```toml
# Remove from V1 dependencies
# - actor_system (custom crate)
# - Complex supervision frameworks
# - Unused libp2p features (kademlia, mdns, quic)
```

### 2.3 Simplified Configuration

**V1 Configuration (Complex):**
```rust
// V1 - Multiple config structures
pub struct NetworkConfig { /* 10+ fields */ }
pub struct GossipConfig { /* 8+ fields */ }
pub struct DiscoveryConfig { /* 6+ fields */ }
pub struct TransportConfig { /* 5+ fields */ }
pub struct FederationNetworkConfig { /* 4+ fields */ }
```

**V2 Simplified Configuration:**
```rust
// V2 - Consolidated config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    // Essential networking
    pub listen_addresses: Vec<String>,
    pub bootstrap_peers: Vec<String>,
    pub max_connections: usize,

    // Simplified gossip
    pub gossip_topics: Vec<String>,
    pub message_size_limit: usize,

    // Basic discovery
    pub discovery_interval: Duration,
    pub connection_timeout: Duration,
}
```

---

## Phase 3: Core Actor Implementation

### 3.1 Simplified NetworkActor Structure

```rust
// V2 NetworkActor - Consolidated implementation
#[derive(Debug)]
pub struct NetworkActor {
    // Core networking
    swarm: Option<Swarm<NetworkBehaviour>>,
    local_peer_id: PeerId,
    config: NetworkConfig,

    // Consolidated managers (replacing separate actors)
    peer_manager: PeerManager,
    sync_manager: SyncManager,

    // Metrics and state
    metrics: NetworkMetrics,
    active_peers: HashMap<PeerId, PeerInfo>,
    pending_requests: HashMap<RequestId, PendingRequest>,

    // Shutdown coordination
    shutdown_requested: bool,
}

impl Actor for NetworkActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("V2 NetworkActor started");
        self.initialize_networking(ctx);
    }
}
```

### 3.2 Embedded Manager Components

```rust
// Peer management (simplified from PeerActor)
#[derive(Debug)]
struct PeerManager {
    connected_peers: HashMap<PeerId, PeerInfo>,
    peer_reputation: HashMap<PeerId, f64>,
    discovery_state: DiscoveryState,
}

impl PeerManager {
    fn handle_new_peer(&mut self, peer_id: PeerId, addr: Multiaddr) -> Result<()> {
        // Simplified peer handling logic
    }

    fn update_peer_reputation(&mut self, peer_id: &PeerId, delta: f64) {
        // Basic reputation system
    }
}

// Sync management (simplified from SyncActor)
#[derive(Debug)]
struct SyncManager {
    sync_state: SyncState,
    block_requests: HashMap<RequestId, BlockRequest>,
    sync_peers: Vec<PeerId>,
}

impl SyncManager {
    fn handle_new_block(&mut self, block: Block, peer_id: PeerId) -> Result<()> {
        // Core block sync logic only
    }

    fn request_blocks(&mut self, range: BlockRange) -> Result<RequestId> {
        // Simplified block requesting
    }
}
```

---

## Phase 4: Message System Simplification

### 4.1 Consolidated Message Types

**V1 Message Complexity:**
- `network_messages.rs` (15+ message types)
- `peer_messages.rs` (12+ message types)
- `sync_messages.rs` (20+ message types)

**V2 Simplified Messages:**
```rust
// Core network operations
#[derive(Debug, Message)]
#[rtype(result = "Result<NetworkResponse>")]
pub enum NetworkMessage {
    // Essential networking
    StartNetwork { listen_addrs: Vec<String>, bootstrap_peers: Vec<String> },
    StopNetwork { graceful: bool },
    GetNetworkStatus,

    // Essential broadcasting
    BroadcastBlock { block_data: Vec<u8>, priority: bool },
    BroadcastTransaction { tx_data: Vec<u8> },

    // Essential sync
    RequestBlocks { start_height: u64, count: u32 },
    HandleNewBlock { block: Block, peer_id: PeerId },

    // Peer management
    ConnectToPeer { peer_addr: String },
    DisconnectPeer { peer_id: PeerId },

    // System
    GetMetrics,
}

#[derive(Debug)]
pub enum NetworkResponse {
    Started,
    Stopped,
    Status(NetworkStatus),
    Broadcasted { message_id: String },
    BlocksRequested { request_id: String },
    Connected { peer_id: PeerId },
    Metrics(NetworkMetrics),
}
```

### 4.2 Remove Complex Message Routing

**V1 Complex Routing:**
```rust
// V1 - Complex inter-actor message routing
NetworkSupervisor -> NetworkActor -> PeerActor -> SyncActor
```

**V2 Direct Handling:**
```rust
// V2 - Direct message handling in single actor
impl Handler<NetworkMessage> for NetworkActor {
    type Result = ResponseActFuture<Self, Result<NetworkResponse>>;

    fn handle(&mut self, msg: NetworkMessage, _ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            NetworkMessage::BroadcastBlock { block_data, priority } => {
                // Direct handling - no actor message passing
                self.handle_broadcast_block(block_data, priority)
            }
            NetworkMessage::RequestBlocks { start_height, count } => {
                // Sync logic directly in actor
                self.sync_manager.request_blocks(start_height, count)
            }
            // ... other cases
        }
    }
}
```

---

## Phase 5: Protocol Implementation

### 5.1 Simplified Network Behaviour

```rust
// V2 - Streamlined libp2p behaviour
#[derive(NetworkBehaviour)]
pub struct NetworkBehaviour {
    gossipsub: Gossipsub,
    request_response: RequestResponse<BlockCodec>,
    identify: Identify,
    mdns: Mdns, // REQUIRED - preserved from V1
    // Remove: kademlia only
}

impl NetworkBehaviour {
    pub fn new(local_key: &Keypair, config: &NetworkConfig) -> Result<Self> {
        // Simplified behaviour setup
        let gossipsub_config = GossipsubConfigBuilder::default()
            .max_transmit_size(config.message_size_limit)
            .build()
            .map_err(|e| anyhow!("Gossipsub config error: {}", e))?;

        let gossipsub = Gossipsub::new(
            MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        ).map_err(|e| anyhow!("Gossipsub creation error: {}", e))?;

        // Simplified request-response
        let request_response = RequestResponse::new(
            BlockCodec(),
            iter::once((BlockProtocol(), ProtocolSupport::Full)),
            RequestResponseConfig::default(),
        );

        let identify = Identify::new(IdentifyConfig::new(
            "/alys/1.0.0".to_string(),
            local_key.public(),
        ));

        Ok(Self {
            gossipsub,
            request_response,
            identify,
        })
    }
}
```

### 5.2 Essential Protocol Handlers

```rust
impl NetworkActor {
    fn handle_behaviour_event(&mut self, event: NetworkBehaviourEvent) -> Result<()> {
        match event {
            // Gossipsub - keep essential functionality
            NetworkBehaviourEvent::Gossipsub(GossipsubEvent::Message {
                propagation_source: peer_id,
                message_id: id,
                message,
            }) => {
                self.handle_gossip_message(peer_id, id, message)
            }

            // Request-Response - simplified
            NetworkBehaviourEvent::RequestResponse(RequestResponseEvent::Message { .. }) => {
                self.handle_request_response_message(message)
            }

            // Identify - basic peer identification
            NetworkBehaviourEvent::Identify(IdentifyEvent::Received { peer_id, info }) => {
                self.peer_manager.handle_peer_identified(peer_id, info)
            }

            _ => Ok(()), // Ignore other events
        }
    }
}
```

---

## Phase 6: File Structure & Organization

### 6.1 V2 Directory Structure

```
app/src/actors_v2/network/
├── mod.rs                  # Module exports and core types
├── network_actor.rs        # NetworkActor implementation (P2P protocols)
├── sync_actor.rs          # SyncActor implementation (blockchain sync)
├── config.rs              # Network and Sync configs
├── behaviour.rs           # libp2p NetworkBehaviour
├── messages.rs            # NetworkMessage and SyncMessage types
├── metrics.rs             # Network and sync metrics
├── managers/              # Component managers
│   ├── mod.rs
│   ├── peer_manager.rs    # Peer connection management
│   ├── gossip_handler.rs  # Gossip message handling
│   └── block_request_manager.rs # Block request coordination
├── protocols/             # Protocol implementations
│   ├── mod.rs
│   ├── gossip.rs         # Gossipsub handling (TCP only)
│   └── request_response.rs # Direct peer requests (TCP only)
├── handlers/              # Message handlers by actor
│   ├── mod.rs
│   ├── network_handlers.rs # NetworkActor message handlers
│   └── sync_handlers.rs    # SyncActor message handlers
└── testing/               # Testing infrastructure
    ├── mod.rs
    ├── network_harness.rs  # NetworkActor test harness
    ├── sync_harness.rs     # SyncActor test harness
    ├── unit/
    │   ├── network_tests.rs
    │   └── sync_tests.rs
    ├── integration/
    │   └── network_sync_integration.rs
    └── chaos/
```

### 6.2 Key Files Mapping

**V1 to V2 Migration:**
```rust
// V1 Multiple Files -> V2 Two-Actor System
alys/network/network/actor.rs        -> actors_v2/network/network_actor.rs
alys/network/peer/actor.rs           -> actors_v2/network/managers/peer_manager.rs
alys/network/sync/actor.rs           -> actors_v2/network/sync_actor.rs
alys/network/supervisor.rs           -> [REMOVED - no supervision]
alys/network/messages/*.rs           -> actors_v2/network/messages.rs (split NetworkMessage/SyncMessage)
alys/network/network/protocols/*.rs  -> actors_v2/network/protocols/ (simplified)
alys/network/transport/              -> [REMOVED - TCP only via libp2p]
```

---

## Phase 7: Implementation Strategy

### 7.1 Priority 1: Two-Actor Foundation (Week 1-2)

**Step 1.1: Create Basic Structure**
```bash
# Create directory structure
mkdir -p app/src/actors_v2/network/{managers,protocols,handlers,testing}

# Create core files for two-actor system
touch app/src/actors_v2/network/{mod.rs,network_actor.rs,sync_actor.rs,config.rs,messages.rs}
touch app/src/actors_v2/network/managers/{mod.rs,peer_manager.rs,gossip_handler.rs,block_request_manager.rs}
```

**Step 1.2: Port and Split Core Logic**
1. Copy V1 `network/actor.rs` to V2 `network_actor.rs`
2. Extract sync logic from V1 `sync/actor.rs` to V2 `sync_actor.rs`
3. Remove `actor_system` imports and dependencies from both actors
4. Remove NetworkSupervisor - actors manage own lifecycle

**Step 1.3: Implement Split Message System**
1. Create separate `NetworkMessage` and `SyncMessage` enums
2. Implement inter-actor communication (NetworkActor ↔ SyncActor)
3. Remove complex supervision and routing logic

### 7.2 Priority 2: Actor-Specific Components (Week 3-4)

**Step 2.1: Implement NetworkActor Components**
1. **PeerManager**: Extract peer logic from V1 PeerActor (2,655 lines)
   - Remove Kademlia DHT complexity
   - Implement bootstrap-based peer discovery
   - Keep essential peer reputation and connection management
   - Target ~500-800 lines (70% reduction)

2. **GossipHandler**: Simplify gossip message processing
   - Remove complex topic management
   - Focus on block/transaction broadcasting
   - Target ~200-300 lines

**Step 2.2: Implement SyncActor Core Logic**
1. Extract and simplify sync logic from V1 SyncActor (13,333 lines)
2. Remove complex state machine (simplify to linear sync states)
3. Implement coordination with NetworkActor for block requests
4. Keep essential block validation and storage integration
5. Target ~2,000-3,000 lines (80% reduction)

### 7.3 Priority 3: Simplified Protocol Integration (Week 5)

**Step 3.1: Streamlined libp2p Behaviour**
1. Port V1 `behaviour.rs` with selective protocol reductions:
   - **Remove**: Kademlia DHT (use bootstrap peers + mDNS instead)
   - **KEEP**: mDNS (REQUIRED - used in V1 for local network discovery)
   - **Remove**: QUIC transport (TCP only)
   - **Keep**: Gossipsub, Request-Response, Identify, mDNS
2. Simplify gossipsub to essential topics only
3. Implement basic request-response for block sync between actors
4. Preserve mDNS for local peer discovery (essential V1 functionality)

**Step 3.2: Protocol Event Processing**
1. Implement simplified event handling in NetworkActor
2. Remove complex supervision and fault tolerance
3. Add inter-actor message passing (Network ↔ Sync coordination)

### 7.4 Priority 4: Actor Integration & Testing (Week 6)

**Step 4.1: Two-Actor System Integration**
1. Update V2's Cargo.toml with simplified libp2p dependencies:
   ```toml
   libp2p = { version = "0.52", features = ["gossipsub", "noise", "tcp", "yamux", "identify", "request-response", "mdns"] }
   # Remove: kademlia, quic features only
   # Keep: mdns (REQUIRED from V1)
   ```
2. Implement NetworkActor ↔ SyncActor coordination
3. Integrate both actors with existing V2 StorageActor
4. Add network RPC endpoints for external communication

**Step 4.2: Two-Actor Testing**
1. Create separate test harnesses for NetworkActor and SyncActor
2. Port essential unit tests from V1 (split by actor responsibility)
3. Create integration tests for actor coordination
4. Basic performance testing for simplified protocol stack

---

## Phase 8: Testing Strategy (Based on StorageActor Framework)

### 8.1 Adapt StorageActor Testing Framework

**Reference Files:**
- `@docs/v2_alpha/actors/storage/testing-guide.knowledge.md`
- `app/src/actors_v2/testing/` (base infrastructure)

**NetworkActor Testing Structure:**
```
app/src/actors_v2/testing/network/
├── mod.rs                  # NetworkTestHarness
├── fixtures.rs            # Test data generation
├── unit/
│   └── mod.rs             # 15 unit tests
├── integration/
│   └── mod.rs             # 10 integration tests
├── property/
│   └── mod.rs             # 8 property tests
└── chaos/
    └── mod.rs             # 6 chaos tests
```

### 8.2 NetworkActor-Specific Tests

**Unit Tests (60% - ~15 tests):**
- Peer connection/disconnection
- Message broadcasting
- Block sync request handling
- Protocol event processing
- Manager component isolation

**Integration Tests (25% - ~10 tests):**
- End-to-end block sync
- Multi-peer gossip propagation
- Network recovery scenarios
- Inter-actor communication

**Property Tests (10% - ~8 tests):**
- Network partition tolerance
- Message delivery guarantees
- Peer discovery consistency
- Sync state invariants

**Chaos Tests (5% - ~6 tests):**
- Network partitions and healing
- High peer churn scenarios
- Message loss and recovery
- Performance under load

### 8.3 Test Harness Implementation

```rust
// NetworkTestHarness - simplified from V1's complex test setup
pub struct NetworkTestHarness {
    actor: Arc<RwLock<NetworkActor>>,
    temp_dir: TempDir,
    config: NetworkConfig,
    test_peers: HashMap<PeerId, TestPeer>,
}

#[async_trait]
impl ActorTestHarness for NetworkTestHarness {
    type Actor = NetworkActor;
    type Config = NetworkConfig;
    type Message = NetworkMessage;
    type Error = NetworkTestError;

    async fn new() -> Result<Self, Self::Error> {
        // Create isolated test network environment
    }

    async fn send_message(&mut self, message: Self::Message) -> Result<(), Self::Error> {
        // Test message handling with proper async patterns
    }
}
```

---

## Phase 9: Simplification Validation & Metrics

### 9.1 Complexity Reduction Goals

**Target Metrics:**
```rust
// V1 Baseline -> V2 Target (Updated with mDNS requirement)
Total Lines:        26,125 -> 8,000-12,000    (60-70% reduction)
Number of Actors:   4 -> 2                    (50% reduction)
Message Types:      47+ -> 15-20              (60% reduction)
Config Complexity:  5 structs -> 2 structs    (60% reduction)
Protocol Complexity: 7 protocols -> 4         (43% reduction - includes mDNS)
```

**Maintainability Improvements:**
- Single actor vs. multi-actor coordination
- Direct message handling vs. complex routing
- Simplified supervision vs. fault-tolerant supervision
- Consolidated configuration vs. multiple config files

### 9.2 Functionality Preservation Checklist

**Core Networking (Must Keep):**
- ✅ Block broadcasting via gossipsub
- ✅ Transaction propagation
- ✅ Peer discovery and connection management (including mDNS)
- ✅ Local network discovery via mDNS (REQUIRED from V1)
- ✅ Basic block synchronization
- ✅ Network metrics and monitoring

**Advanced Features (Evaluate):**
- 🔍 Complex sync state machines (simplify)
- 🔍 Advanced peer reputation (basic version)
- 🔍 Kademlia DHT (remove - use bootstrap + mDNS)
- ✅ mDNS discovery (REQUIRED - preserve from V1)
- 🔍 Sophisticated fault tolerance (simplify)

**Federation Features (Keep if Used):**
- ✅ Federation-aware message routing (if needed)
- ✅ Priority message handling
- ✅ Cross-chain communication (if used)

---

## Phase 10: Risk Mitigation & Rollback Plan

### 10.1 Implementation Risks

**High Risk:**
- **Sync Logic Complexity**: SyncActor has 13,333 lines - risk of missing critical logic
- **P2P Protocol Compatibility**: Removing protocols may break network participation
- **Performance Degradation**: Single actor may have different performance characteristics

**Medium Risk:**
- **Message Loss**: Simplified message handling may miss edge cases
- **Peer Discovery**: Removing Kademlia may impact peer finding
- **Testing Coverage**: Complex V1 system may have uncovered test scenarios

### 10.2 Mitigation Strategies

**Progressive Implementation:**
1. **Parallel Development**: Keep V1 running while building V2
2. **Feature Flags**: Implement toggles for simplified vs. full functionality
3. **Extensive Testing**: Focus heavily on integration and property tests
4. **Performance Monitoring**: Benchmark V2 against V1 throughout development

**Rollback Contingencies:**
1. **Hybrid Approach**: Fall back to 2-actor system (Network + Sync) if single actor fails
2. **Protocol Restoration**: Re-add removed protocols if network participation drops
3. **V1 Fallback**: Maintain ability to revert to V1 NetworkActor if critical issues arise

---

## Implementation Timeline

**Week 1-2**: Core Actor Foundation
**Week 3-4**: Manager Components Implementation
**Week 5**: Protocol Integration & Simplification
**Week 6**: System Integration & Basic Testing
**Week 7-8**: Comprehensive Testing & Performance Validation
**Week 9**: Documentation & Knowledge Transfer
**Week 10**: Production Deployment & Monitoring

**Total Estimated Effort**: 10 weeks (vs. StorageActor's 2-3 weeks)

---

## Success Criteria

1. **Functionality**: All essential P2P networking features preserved
2. **Simplification**: 60-70% code reduction while maintaining core capabilities
3. **Performance**: No significant performance degradation vs. V1
4. **Maintainability**: Single actor architecture with clear component separation
5. **Testing**: Comprehensive test suite adapted from StorageActor framework
6. **Documentation**: Complete onboarding guide following StorageActor pattern

**This NetworkActor port represents a major architectural simplification while preserving essential P2P networking capabilities. The key insight is consolidating the multi-actor system into a two-actor architecture with embedded managers - dramatically reducing complexity while maintaining functionality including essential mDNS local discovery from V1.**