# Revised Systematic Plan for Porting ChainActor to V2

Based on your clarification and following the successful patterns from StorageActor and NetworkActor V2, this is a comprehensive plan for porting the ChainActor from V1 to V2 while significantly simplifying its implementation.

## Architecture Clarification

**V1 System Issues:**
- ChainActor V1: Overly complex actor with 15+ modules, custom supervision, over-engineered metrics
- chain.rs V1: Monolithic ~2000 line file with shared mutable state, complex RwLock patterns
- Complex dependency: Custom `actor_system` crate + extensive supervision + complex configuration

**V2 System Goals:**
- Pure Actix (no `actor_system` crate) + essential blockchain operations
- Replace both V1 ChainActor complexity AND monolithic chain.rs
- Simplified but complete blockchain functionality
- Standard Actix actor patterns following StorageActor/NetworkActor V2 approach

## Phase 1: Dependency Cleanup & Foundation

### 1.1 Remove Custom Actor System Dependencies
**From V1:**
```rust
use actor_system::{Actor as AlysActor, ActorMetrics, AlysActorMessage, ActorError, SupervisionConfig, FederationConfig};
```

**To V2:**
```rust
// Use standard Actix patterns only
use actix::prelude::*;
```

### 1.2 Dependencies to Keep/Add
- **Keep:** `actix`, `lighthouse_wrapper`, `eyre`, `bitcoin`, `bridge`, `ethereum_types`
- **Keep:** All blockchain-related dependencies (Engine, Storage, Aura, Bridge)
- **Remove:** `actor_system` references
- **Add to V2:** Any missing blockchain dependencies from chain.rs

### 1.3 Core Blockchain Operations (No Changes to Logic)
- **Keep:** Block production logic from chain.rs
- **Keep:** Block validation and consensus logic
- **Keep:** AuxPoW processing and finalization
- **Keep:** Peg-in/peg-out operations
- **Keep:** Fee distribution and miner rewards
- **Simplify:** Remove circuit breaker complexity, simplify sync status

## Phase 2: Pure Actix Actor Implementation

### 2.1 Actor Structure (Massive Simplification)
```rust
// V1 (complex approach with 15+ modules)
pub struct ChainActor {
    config: ChainActorConfig,           // Complex config with supervision
    chain_state: LocalChainState,       // Complex state management
    pending_blocks: HashMap<Hash256, PendingBlockInfo>,
    federation: FederationState,        // Over-engineered federation
    auxpow_state: AuxPowState,          // Complex AuxPoW state
    subscribers: HashMap<Uuid, BlockSubscriber>,
    metrics: ChainActorMetrics,         // Over-engineered metrics
    actor_addresses: ActorAddresses,    // Complex actor coordination
    validation_cache: ValidationCache,  // Complex validation caching
    health_monitor: ActorHealthMonitor, // Over-engineered health monitoring
    // ... 15+ additional complex fields
}

// V2 (simplified approach - core blockchain functionality)
pub struct ChainActor {
    // Core blockchain state (derived from chain.rs)
    engine: Engine,
    aura: Aura,
    head: Option<BlockRef>,
    sync_status: SyncStatus,

    // Essential AuxPoW and consensus
    queued_pow: Option<AuxPowHeader>,
    max_blocks_without_pow: u64,
    federation: Vec<Address>,

    // Peg operations (simplified from chain.rs)
    bridge: Bridge,
    queued_pegins: BTreeMap<Txid, PegInInfo>,
    bitcoin_wallet: BitcoinWallet,
    bitcoin_signature_collector: BitcoinSignatureCollector,
    maybe_bitcoin_signer: Option<BitcoinSigner>,

    // Essential configuration
    is_validator: bool,
    retarget_params: BitcoinConsensusParams,
    block_hash_cache: Option<BlockHashCache>,

    // Actor integration
    storage_actor: Option<Addr<StorageActor>>,
    network_actor: Option<Addr<NetworkActor>>,

    // Simple metrics
    metrics: ChainMetrics,
}
```

### 2.2 Remove Custom Actor System Integration
**Changes needed:**
- Remove `ActorMetrics` → Use direct prometheus metrics like StorageActor
- Remove `AlysActorMessage` → Use standard Actix `Message` trait
- Remove `ActorError` → Use `ChainError` directly
- Remove complex supervision → Use simple actor lifecycle
- Remove over-engineered health monitoring → Use basic health checks

## Phase 3: Message System (Simplified but Complete)

### 3.1 Essential Message Types (Reduce from 25+ to ~10)
**Core Operations:**
- `ProduceBlock` - Block production for validators
- `ImportBlock` - Block import from network/sync
- `ProcessAuxPow` - AuxPoW processing and finalization
- `ProcessPegins` - Peg-in operations
- `ProcessPegouts` - Peg-out operations
- `GetChainStatus` - Chain status queries
- `GetBlockByHeight` / `GetBlockByHash` - Block retrieval for RPC
- `BroadcastBlock` - Block broadcasting via NetworkActor

**Remove Complex Messages:**
- Complex subscription systems
- Over-engineered metrics messages
- Complex federation update messages
- Detailed validation messages with multiple levels
- Complex reorganization messages

### 3.2 Simplified Message Handlers
**Pattern to follow (like StorageActor):**
```rust
// V1 pattern - complex async handlers with supervision
impl Handler<ImportBlock> for ChainActor {
    type Result = ResponseActFuture<Self, Result<ImportBlockResult, ChainError>>;

    fn handle(&mut self, msg: ImportBlock, _: &mut Context<Self>) -> Self::Result {
        // Complex async processing with supervision callbacks
    }
}

// V2 pattern - simple handlers following StorageActor approach
impl Handler<ImportBlock> for ChainActor {
    type Result = ResponseFuture<Result<(), ChainError>>;

    fn handle(&mut self, msg: ImportBlock, _: &mut Context<Self>) -> Self::Result {
        // Clone components for async operation like StorageActor
        let engine = self.engine.clone();
        let storage_actor = self.storage_actor.clone();

        Box::pin(async move {
            // Simple async block processing
            // Core logic from chain.rs but actor-based
        })
    }
}
```

## Phase 4: Component Porting (Direct Migration from chain.rs)

### 4.1 Block Production Logic (`produce_block` from chain.rs)
- **Port:** Complete `produce_block` method from chain.rs:437-692
- **Integration:** Convert to `ProduceBlock` message handler
- **Simplification:** Remove complex rollback logic, simplify payload building
- **Keep:** Fee collection, peg-in processing, AuxPoW integration

### 4.2 Block Import and Processing (`process_block` from chain.rs)
- **Port:** Complete `process_block` method from chain.rs:923-1124
- **Integration:** Convert to `ImportBlock` message handler
- **Keep:** All validation logic, consensus checks, AuxPoW verification
- **Simplify:** Remove complex supervision patterns

### 4.3 AuxPoW Processing (`check_pow`, finalization logic)
- **Port:** AuxPoW validation from chain.rs:1293-1380
- **Port:** Finalization logic from chain.rs:1815-1841
- **Integration:** Convert to `ProcessAuxPow` message handler
- **Keep:** All Bitcoin merged mining logic intact

### 4.4 Peg Operations (`fill_pegins`, `create_pegout_payments`)
- **Port:** Peg-in processing from chain.rs:252-382
- **Port:** Peg-out creation from chain.rs:882-911
- **Integration:** Convert to `ProcessPegins` and `ProcessPegouts` handlers
- **Keep:** All Bridge integration and UTXO management

### 4.5 Chain State Management
- **Port:** Head tracking, sync status from chain.rs
- **Simplify:** Remove complex RwLock patterns, use actor state
- **Keep:** Block candidates, queued operations

## Phase 5: Handler Implementation Strategy

### 5.1 Core Message Handlers (Essential Blockchain Operations)
**Pattern to follow:**
```rust
// ProduceBlock handler - port chain.rs:437-692
impl Handler<ProduceBlock> for ChainActor {
    type Result = ResponseFuture<Result<SignedConsensusBlock, ChainError>>;
    // Port complete block production logic
}

// ImportBlock handler - port chain.rs:923-1124
impl Handler<ImportBlock> for ChainActor {
    type Result = ResponseFuture<Result<(), ChainError>>;
    // Port complete block validation and import logic
}

// ProcessAuxPow handler - port chain.rs:1293-1380 + finalization
impl Handler<ProcessAuxPow> for ChainActor {
    type Result = ResponseFuture<Result<bool, ChainError>>;
    // Port AuxPoW validation and finalization
}
```

### 5.2 Actor Integration (Following NetworkActor V2 Pattern)
**StorageActor Integration:**
```rust
// Store block via StorageActor (like SyncActor does)
if let Some(ref storage_actor) = self.storage_actor {
    let store_msg = StorageMessage::StoreBlock { block, canonical: true };
    storage_actor.send(store_msg).await?;
}
```

**NetworkActor Integration:**
```rust
// Broadcast block via NetworkActor
if let Some(ref network_actor) = self.network_actor {
    let broadcast_msg = NetworkMessage::BroadcastBlock { block_data, priority: true };
    network_actor.send(broadcast_msg).await?;
}
```

## Phase 6: File Structure

### 6.1 Directory Structure in `/app/src/actors_v2/chain/`
```
chain/
├── mod.rs              # Module exports
├── actor.rs            # Main ChainActor (simplified from V1 + chain.rs logic)
├── messages.rs         # Essential message types (10 vs 25+)
├── handlers.rs         # All message handlers (consolidated)
├── config.rs           # Simplified configuration
├── metrics.rs          # Basic metrics (not over-engineered)
├── state.rs            # Chain state management
└── error.rs            # Error types
```

### 6.2 Integration with V2 Cargo.toml
**Add dependencies:**
```toml
# Add blockchain-specific dependencies from chain.rs
bitcoin = "0.31"
bridge = { path = "../bridge" }  # If needed
lighthouse_wrapper = { path = "../lighthouse_wrapper" }
ethereum_types = "0.14"
eyre = "0.6"
```

## Phase 7: Testing (Direct Port and Simplify)

### 7.1 Test Migration Strategy
- **Port:** Essential tests from V1 ChainActor
- **Port:** Chain logic tests from chain.rs
- **Remove:** Over-engineered supervision tests
- **Remove:** Complex metrics tests
- **Keep:** Block production, validation, AuxPoW, peg operation tests
- **Follow:** StorageActor V2 testing patterns

### 7.2 Test Structure
```
testing/
├── unit/
│   ├── chain_actor_tests.rs     # Core actor functionality
│   ├── block_production_tests.rs # Block production logic
│   ├── auxpow_tests.rs          # AuxPoW processing
│   └── peg_operation_tests.rs   # Peg operations
└── integration/
    ├── chain_coordination_tests.rs # Actor coordination
    └── blockchain_workflow_tests.rs # End-to-end blockchain operations
```

## Implementation Strategy

### Priority 1: Core Actor Foundation (Straightforward)
1. Create basic ChainActor structure following StorageActor V2 pattern
2. Port essential blockchain state from chain.rs
3. Remove V1 ChainActor complexity and `actor_system` dependencies
4. Set up basic message system with ~10 essential messages

### Priority 2: Core Blockchain Logic Port (Direct Migration)
1. Port block production logic from chain.rs:437-692 to `ProduceBlock` handler
2. Port block import logic from chain.rs:923-1124 to `ImportBlock` handler
3. Port AuxPoW processing from chain.rs:1293-1380 to `ProcessAuxPow` handler
4. Port peg operations from chain.rs:252-382 and 882-911

### Priority 3: Actor Integration (Standard)
1. Integrate with StorageActor V2 for block storage
2. Integrate with NetworkActor V2 for block broadcasting
3. Add basic RPC endpoints for chain queries
4. Add essential metrics (not over-engineered)

### Priority 4: Testing and Validation (Following V2 Patterns)
1. Port essential tests from both V1 ChainActor and chain.rs
2. Follow StorageActor V2 testing patterns
3. Create integration tests for actor coordination
4. Validate end-to-end blockchain workflows

## Key Insight

This is primarily a **logic consolidation and simplification task**. We're taking:
- **Complex V1 ChainActor** (over-engineered, 15+ modules, custom supervision)
- **Monolithic chain.rs** (shared mutable state, ~2000 lines, complex async patterns)

And creating:
- **Simple ChainActor V2** (essential blockchain operations, standard Actix patterns)
- **Clean actor integration** (StorageActor + NetworkActor coordination)
- **Maintained functionality** (all essential blockchain logic preserved)

**Estimated Effort:** Medium complexity - requires understanding blockchain logic from chain.rs and simplifying V1 ChainActor complexity, but follows established V2 patterns from StorageActor and NetworkActor implementations.

**Success Criteria:**
1. ChainActor V2 handles all essential blockchain operations
2. Clean integration with StorageActor V2 and NetworkActor V2
3. Maintains AuxPoW, peg operations, and consensus functionality
4. Follows standard Actix patterns without custom `actor_system`
5. Significantly simpler than V1 while preserving essential features