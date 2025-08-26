# ALYS-007: Implement ChainActor

## Issue Type
Task

## Priority
Critical

## Story Points
8

## Sprint
Migration Sprint 2

## Component
Core Architecture

## Labels
`migration`, `phase-1`, `actor-system`, `chain`, `consensus`

## Description

Implement the ChainActor that will replace the monolithic Chain struct with a message-driven actor. This actor will handle consensus operations, block production, and chain state management using the actor model, eliminating shared mutable state issues.

## Subtasks

- [X] Create ALYS-007-1: Design ChainActor message protocol with comprehensive message definitions [https://marathondh.atlassian.net/browse/AN-393]
- [X] Create ALYS-007-2: Implement ChainActor core structure with consensus integration [https://marathondh.atlassian.net/browse/AN-394]
- [X] Create ALYS-007-3: Implement block production logic with timing constraints [https://marathondh.atlassian.net/browse/AN-395]
- [X] Create ALYS-007-4: Implement block import and validation pipeline [https://marathondh.atlassian.net/browse/AN-396]
- [X] Create ALYS-007-5: Implement chain state management and reorganization [https://marathondh.atlassian.net/browse/AN-397]
- [X] Create ALYS-007-6: Implement finalization logic with AuxPoW integration [https://marathondh.atlassian.net/browse/AN-398]
- [X] Create ALYS-007-7: Create migration adapter for gradual legacy transition [https://marathondh.atlassian.net/browse/AN-399]
- [X] Create ALYS-007-8: Implement comprehensive test suite (unit, integration, performance) [https://marathondh.atlassian.net/browse/AN-401]
- [X] Create ALYS-007-9: Integration with actor supervision system [https://marathondh.atlassian.net/browse/AN-402]
- [X] Create ALYS-007-10: Performance benchmarking and optimization [https://marathondh.atlassian.net/browse/AN-403]

## Acceptance Criteria
- [ ] ChainActor implements all Chain functionality
- [ ] Message protocol defined for all chain operations
- [ ] State isolation - no Arc<RwLock<>> usage
- [ ] Integration with EngineActor for execution
- [ ] Integration with BridgeActor for peg operations
- [ ] Backward compatibility via adapter pattern
- [ ] No consensus disruption during migration
- [ ] Performance equal or better than current implementation
- [ ] Comprehensive error handling and recovery

## Technical Details

### Implementation Steps

1. **Define ChainActor Messages**
```rust
// src/actors/chain/messages.rs

use actix::prelude::*;
use crate::types::*;

/// Messages handled by ChainActor
#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct ImportBlock {
    pub block: SignedConsensusBlock,
    pub broadcast: bool,
}

#[derive(Message)]
#[rtype(result = "Result<SignedConsensusBlock, ChainError>")]
pub struct ProduceBlock {
    pub slot: u64,
    pub timestamp: Duration,
}

#[derive(Message)]
#[rtype(result = "Result<Vec<SignedConsensusBlock>, ChainError>")]
pub struct GetBlocksByRange {
    pub start_height: u64,
    pub count: usize,
}

#[derive(Message)]
#[rtype(result = "Result<ChainStatus, ChainError>")]
pub struct GetChainStatus;

#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct UpdateFederation {
    pub version: u32,
    pub members: Vec<FederationMember>,
    pub threshold: usize,
}

#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct FinalizeBlocks {
    pub pow_header: AuxPowHeader,
    pub target_height: u64,
}

#[derive(Message)]
#[rtype(result = "Result<bool, ChainError>")]
pub struct ValidateBlock {
    pub block: SignedConsensusBlock,
}

#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct ReorgChain {
    pub new_head: Hash256,
    pub blocks: Vec<SignedConsensusBlock>,
}

/// Responses from ChainActor
#[derive(Debug, Clone)]
pub struct ChainStatus {
    pub head_height: u64,
    pub head_hash: Hash256,
    pub finalized_height: Option<u64>,
    pub finalized_hash: Option<Hash256>,
    pub sync_status: SyncStatus,
    pub pending_pow: Option<AuxPowHeader>,
    pub federation_version: u32,
}

#[derive(Debug, Clone)]
pub enum SyncStatus {
    Syncing { current: u64, target: u64 },
    Synced,
    Failed(String),
}
```

2. **Implement ChainActor Core**
```rust
// src/actors/chain/mod.rs

use actix::prelude::*;
use std::collections::VecDeque;

pub struct ChainActor {
    // Consensus components
    aura: AuraConsensus,
    auxpow: Option<AuxPowMiner>,
    federation: Federation,
    
    // Chain state (owned by actor, no sharing)
    head: ConsensusBlock,
    finalized: Option<ConsensusBlock>,
    pending_pow: Option<AuxPowHeader>,
    block_buffer: VecDeque<SignedConsensusBlock>,
    
    // Child actors
    engine_actor: Addr<EngineActor>,
    bridge_actor: Addr<BridgeActor>,
    storage_actor: Addr<StorageActor>,
    network_actor: Addr<NetworkActor>,
    
    // Configuration
    config: ChainConfig,
    
    // Metrics
    metrics: ChainMetrics,
}

impl ChainActor {
    pub fn new(
        config: ChainConfig,
        engine_actor: Addr<EngineActor>,
        bridge_actor: Addr<BridgeActor>,
        storage_actor: Addr<StorageActor>,
        network_actor: Addr<NetworkActor>,
    ) -> Result<Self> {
        // Load initial state from storage
        let head = storage_actor.send(GetHead).await??;
        let finalized = storage_actor.send(GetFinalized).await??;
        
        // Initialize consensus components
        let aura = AuraConsensus::new(config.aura_config.clone())?;
        let auxpow = config.auxpow_config.as_ref()
            .map(|cfg| AuxPowMiner::new(cfg.clone()))
            .transpose()?;
        let federation = Federation::new(config.federation_config.clone())?;
        
        Ok(Self {
            aura,
            auxpow,
            federation,
            head,
            finalized,
            pending_pow: None,
            block_buffer: VecDeque::with_capacity(100),
            engine_actor,
            bridge_actor,
            storage_actor,
            network_actor,
            config,
            metrics: ChainMetrics::new(),
        })
    }
}

impl Actor for ChainActor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("ChainActor started");
        
        // Start block production timer
        ctx.run_interval(self.config.slot_duration, |act, ctx| {
            let slot = act.calculate_current_slot();
            let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            
            ctx.spawn(
                async move {
                    act.try_produce_block(slot, timestamp).await
                }
                .into_actor(act)
                .map(|result, _, _| {
                    if let Err(e) = result {
                        error!("Block production failed: {}", e);
                    }
                })
            );
        });
        
        // Start finalization checker
        ctx.run_interval(Duration::from_secs(10), |act, ctx| {
            ctx.spawn(
                async move {
                    act.check_finalization().await
                }
                .into_actor(act)
            );
        });
    }
    
    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        info!("ChainActor stopping");
        Running::Stop
    }
}

impl Handler<ProduceBlock> for ChainActor {
    type Result = ResponseActFuture<Self, Result<SignedConsensusBlock, ChainError>>;
    
    fn handle(&mut self, msg: ProduceBlock, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            // Check if we should produce this slot
            if !self.aura.should_produce(msg.slot, &self.config.authority_key) {
                return Err(ChainError::NotOurSlot);
            }
            
            // Check if already produced for this slot
            if self.already_produced_slot(msg.slot) {
                return Err(ChainError::SlotAlreadyProduced);
            }
            
            self.metrics.block_production_attempts.inc();
            let start = Instant::now();
            
            // Step 1: Collect pending peg-ins as withdrawals
            let pending_pegins = self.bridge_actor
                .send(GetPendingPegins)
                .await??;
            
            let withdrawals = pending_pegins
                .into_iter()
                .map(|pegin| Withdrawal {
                    index: pegin.index,
                    validator_index: 0, // Not used
                    address: pegin.evm_address,
                    amount: pegin.amount_wei,
                })
                .collect();
            
            // Step 2: Build execution payload
            let payload = self.engine_actor
                .send(BuildBlock {
                    timestamp: msg.timestamp,
                    parent: Some(self.head.execution_payload.block_hash),
                    withdrawals,
                })
                .await??;
            
            // Step 3: Create consensus block
            let consensus_block = ConsensusBlock {
                slot: msg.slot,
                parent_hash: self.head.hash(),
                execution_payload: payload,
                timestamp: msg.timestamp,
                producer: self.config.authority_key.public(),
            };
            
            // Step 4: Sign block with Aura
            let signature = self.aura.sign_block(&consensus_block)?;
            
            let signed_block = SignedConsensusBlock {
                message: consensus_block,
                signature,
            };
            
            // Step 5: Import our own block
            self.import_block_internal(signed_block.clone(), true).await?;
            
            // Step 6: Broadcast to network
            self.network_actor
                .send(BroadcastBlock(signed_block.clone()))
                .await?;
            
            self.metrics.blocks_produced.inc();
            self.metrics.block_production_time.observe(start.elapsed().as_secs_f64());
            
            info!("Produced block at slot {} height {}", msg.slot, self.head.height());
            
            Ok(signed_block)
        }.into_actor(self))
    }
}

impl Handler<ImportBlock> for ChainActor {
    type Result = ResponseActFuture<Self, Result<(), ChainError>>;
    
    fn handle(&mut self, msg: ImportBlock, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            self.import_block_internal(msg.block, msg.broadcast).await
        }.into_actor(self))
    }
}

impl ChainActor {
    async fn import_block_internal(
        &mut self,
        block: SignedConsensusBlock,
        broadcast: bool,
    ) -> Result<(), ChainError> {
        let start = Instant::now();
        
        // Step 1: Validate block
        self.validate_block(&block).await?;
        
        // Step 2: Check if extends current head
        if block.message.parent_hash != self.head.hash() {
            // Potential reorg or future block
            if block.message.height() > self.head.height() + 1 {
                // Future block, buffer it
                self.block_buffer.push_back(block);
                return Ok(());
            } else {
                // Potential reorg
                return self.handle_potential_reorg(block).await;
            }
        }
        
        // Step 3: Execute block in execution layer
        self.engine_actor
            .send(CommitBlock {
                payload: block.message.execution_payload.clone(),
            })
            .await??;
        
        // Step 4: Update chain state
        self.head = block.message.clone();
        
        // Step 5: Persist to storage
        self.storage_actor
            .send(StoreBlock {
                block: block.clone(),
                update_head: true,
            })
            .await??;
        
        // Step 6: Process buffered blocks
        self.process_buffered_blocks().await?;
        
        // Step 7: Broadcast if needed
        if broadcast {
            self.network_actor
                .send(BroadcastBlock(block.clone()))
                .await?;
        }
        
        self.metrics.blocks_imported.inc();
        self.metrics.block_import_time.observe(start.elapsed().as_secs_f64());
        
        info!("Imported block at height {}", block.message.height());
        
        Ok(())
    }
    
    async fn validate_block(&self, block: &SignedConsensusBlock) -> Result<(), ChainError> {
        // Validate structure
        if block.message.slot == 0 {
            return Err(ChainError::InvalidSlot);
        }
        
        // Validate signature
        let expected_producer = self.aura.get_slot_producer(block.message.slot)?;
        if block.message.producer != expected_producer {
            return Err(ChainError::WrongProducer);
        }
        
        if !self.aura.verify_signature(block)? {
            return Err(ChainError::InvalidSignature);
        }
        
        // Validate execution payload
        self.engine_actor
            .send(ValidatePayload {
                payload: block.message.execution_payload.clone(),
            })
            .await??;
        
        Ok(())
    }
    
    async fn check_finalization(&mut self) -> Result<(), ChainError> {
        // Check if we have pending PoW
        if let Some(pow_header) = &self.pending_pow {
            let pow_height = pow_header.height;
            
            // Check if PoW confirms our current head
            if self.head.height() >= pow_height {
                info!("Finalizing blocks up to height {} with PoW", pow_height);
                
                // Update finalized block
                self.finalized = Some(self.head.clone());
                
                // Notify engine of finalization
                self.engine_actor
                    .send(FinalizeBlock {
                        block_hash: self.head.execution_payload.block_hash,
                    })
                    .await?;
                
                // Clear pending PoW
                self.pending_pow = None;
                
                self.metrics.blocks_finalized.inc();
            }
        }
        
        // Check if we need to halt due to no PoW
        if let Some(finalized) = &self.finalized {
            let blocks_since_finalized = self.head.height() - finalized.height();
            if blocks_since_finalized > self.config.max_blocks_without_pow {
                warn!("No PoW for {} blocks, halting block production", blocks_since_finalized);
                // Set flag to prevent block production
                // This would be handled by the actor system
            }
        }
        
        Ok(())
    }
}
```

3. **Implement State Management**
```rust
// src/actors/chain/state.rs

impl ChainActor {
    /// Get current chain state without locks
    pub fn get_chain_state(&self) -> ChainState {
        ChainState {
            head: self.head.clone(),
            finalized: self.finalized.clone(),
            height: self.head.height(),
            federation_version: self.federation.version(),
        }
    }
    
    /// Handle chain reorganization
    async fn handle_potential_reorg(
        &mut self,
        new_block: SignedConsensusBlock,
    ) -> Result<(), ChainError> {
        info!("Potential reorg detected at height {}", new_block.message.height());
        
        // Find common ancestor
        let common_ancestor = self.find_common_ancestor(&new_block).await?;
        
        // Calculate reorg depth
        let reorg_depth = self.head.height() - common_ancestor.height();
        
        if reorg_depth > self.config.max_reorg_depth {
            return Err(ChainError::ReorgTooDeep);
        }
        
        // Get the new chain
        let new_chain = self.get_chain_from_ancestor(&new_block, &common_ancestor).await?;
        
        // Validate new chain is heavier
        if !self.is_heavier_chain(&new_chain) {
            return Err(ChainError::NotHeavierChain);
        }
        
        // Revert current chain
        self.revert_to_height(common_ancestor.height()).await?;
        
        // Apply new chain
        for block in new_chain {
            self.import_block_internal(block, false).await?;
        }
        
        self.metrics.reorgs.inc();
        self.metrics.reorg_depth.observe(reorg_depth as f64);
        
        info!("Reorg complete, new head at height {}", self.head.height());
        
        Ok(())
    }
    
    async fn revert_to_height(&mut self, height: u64) -> Result<(), ChainError> {
        while self.head.height() > height {
            // Notify engine to revert
            self.engine_actor
                .send(RevertBlock {
                    block_hash: self.head.execution_payload.block_hash,
                })
                .await??;
            
            // Load parent block
            let parent_hash = self.head.parent_hash;
            let parent = self.storage_actor
                .send(GetBlock { hash: parent_hash })
                .await??
                .ok_or(ChainError::ParentNotFound)?;
            
            self.head = parent.message;
        }
        
        Ok(())
    }
}
```

4. **Create Migration Adapter**
```rust
// src/actors/chain/adapter.rs

use crate::chain::Chain as LegacyChain;

/// Adapter to migrate from legacy Chain to ChainActor
pub struct ChainMigrationAdapter {
    legacy_chain: Option<Arc<RwLock<LegacyChain>>>,
    chain_actor: Option<Addr<ChainActor>>,
    feature_flags: Arc<FeatureFlagManager>,
    migration_state: MigrationState,
}

#[derive(Debug, Clone)]
enum MigrationState {
    LegacyOnly,
    Parallel,      // Run both, compare results
    ActorPrimary,  // Actor primary, legacy backup
    ActorOnly,
}

impl ChainMigrationAdapter {
    pub async fn import_block(&self, block: SignedConsensusBlock) -> Result<()> {
        match self.migration_state {
            MigrationState::LegacyOnly => {
                self.legacy_chain.as_ref().unwrap()
                    .write().await
                    .import_block(block).await
            }
            MigrationState::Parallel => {
                // Run both in parallel
                let legacy_future = self.legacy_chain.as_ref().unwrap()
                    .write()
                    .then(|mut chain| async move {
                        chain.import_block(block.clone()).await
                    });
                
                let actor_future = self.chain_actor.as_ref().unwrap()
                    .send(ImportBlock { block: block.clone(), broadcast: false });
                
                let (legacy_result, actor_result) = tokio::join!(legacy_future, actor_future);
                
                // Compare results
                match (&legacy_result, &actor_result) {
                    (Ok(_), Ok(_)) => {
                        self.metrics.parallel_success.inc();
                    }
                    (Ok(_), Err(e)) => {
                        warn!("Actor import failed while legacy succeeded: {}", e);
                        self.metrics.actor_only_failures.inc();
                    }
                    (Err(e), Ok(_)) => {
                        warn!("Legacy import failed while actor succeeded: {}", e);
                        self.metrics.legacy_only_failures.inc();
                    }
                    (Err(e1), Err(e2)) => {
                        error!("Both imports failed: legacy={}, actor={}", e1, e2);
                        self.metrics.both_failures.inc();
                    }
                }
                
                // Return legacy result during parallel phase
                legacy_result
            }
            MigrationState::ActorPrimary => {
                // Try actor first
                match self.chain_actor.as_ref().unwrap()
                    .send(ImportBlock { block: block.clone(), broadcast: false })
                    .await
                {
                    Ok(result) => result,
                    Err(e) => {
                        warn!("Actor import failed, falling back to legacy: {}", e);
                        self.legacy_chain.as_ref().unwrap()
                            .write().await
                            .import_block(block).await
                    }
                }
            }
            MigrationState::ActorOnly => {
                self.chain_actor.as_ref().unwrap()
                    .send(ImportBlock { block, broadcast: false })
                    .await?
            }
        }
    }
}
```

## Testing Plan

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[actix::test]
    async fn test_block_production() {
        let chain_actor = create_test_chain_actor().await;
        
        let block = chain_actor.send(ProduceBlock {
            slot: 1,
            timestamp: Duration::from_secs(1000),
        }).await.unwrap().unwrap();
        
        assert_eq!(block.message.slot, 1);
        assert!(chain_actor.send(GetChainStatus).await.unwrap().unwrap().head_height == 1);
    }
    
    #[actix::test]
    async fn test_block_import() {
        let chain_actor = create_test_chain_actor().await;
        let block = create_test_block(1);
        
        chain_actor.send(ImportBlock {
            block: block.clone(),
            broadcast: false,
        }).await.unwrap().unwrap();
        
        let status = chain_actor.send(GetChainStatus).await.unwrap().unwrap();
        assert_eq!(status.head_hash, block.message.hash());
    }
    
    #[actix::test]
    async fn test_reorg_handling() {
        let chain_actor = create_test_chain_actor().await;
        
        // Build initial chain
        let blocks_a = create_chain_branch("a", 5);
        for block in &blocks_a {
            chain_actor.send(ImportBlock {
                block: block.clone(),
                broadcast: false,
            }).await.unwrap().unwrap();
        }
        
        // Create competing branch (heavier)
        let blocks_b = create_heavier_chain_branch("b", 4);
        
        // Import competing branch - should trigger reorg
        for block in &blocks_b {
            chain_actor.send(ImportBlock {
                block: block.clone(),
                broadcast: false,
            }).await.unwrap().unwrap();
        }
        
        // Verify reorg happened
        let status = chain_actor.send(GetChainStatus).await.unwrap().unwrap();
        assert_eq!(status.head_hash, blocks_b.last().unwrap().message.hash());
    }
}
```

### Integration Tests
1. Test interaction with EngineActor
2. Test interaction with BridgeActor
3. Test parallel migration mode
4. Test graceful transition from legacy

### Performance Tests
```rust
#[bench]
fn bench_block_import(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let chain_actor = runtime.block_on(create_test_chain_actor());
    
    let blocks: Vec<_> = (0..1000)
        .map(|i| create_test_block(i))
        .collect();
    
    b.iter(|| {
        runtime.block_on(async {
            for block in &blocks {
                chain_actor.send(ImportBlock {
                    block: block.clone(),
                    broadcast: false,
                }).await.unwrap().unwrap();
            }
        })
    });
}
```

## Dependencies

### Blockers
- ALYS-006: Actor supervisor must be implemented first

### Blocked By
None

### Related Issues
- ALYS-008: EngineActor (execution layer)
- ALYS-009: BridgeActor (peg operations)
- ALYS-010: StorageActor (persistence)
- ALYS-011: NetworkActor (P2P)

## Definition of Done

- [ ] ChainActor fully implemented
- [ ] All chain operations migrated
- [ ] Message protocol documented
- [ ] Migration adapter tested
- [ ] No consensus disruption during switch
- [ ] Performance benchmarks pass
- [ ] Integration tests pass
- [ ] Documentation updated
- [ ] Code review completed

## Notes

- Add support for checkpoint sync

## Next Steps

### Work Completed Analysis (70% Complete)

**Completed Components (✓):**
- Message protocol design with comprehensive message types (95% complete)
- Core actor structure with consensus integration (80% complete) 
- Block production logic with timing constraints (85% complete)
- Block import and validation pipeline (75% complete)
- Chain state management architecture (70% complete)

**Detailed Work Analysis:**
1. **Message Protocol (95%)** - All message types defined including ImportBlock, ProduceBlock, GetBlocksByRange, GetChainStatus, UpdateFederation, FinalizeBlocks, ValidateBlock, ReorgChain with proper response types
2. **Actor Structure (80%)** - Core ChainActor struct defined with owned state, child actor addresses, consensus components, and metrics
3. **Block Production (85%)** - Complete ProduceBlock handler with peg-in collection, execution payload building, consensus block creation, signing, and network broadcast
4. **Block Import (75%)** - ImportBlock handler with validation, reorg handling, execution layer integration, and state updates
5. **State Management (70%)** - Chain state ownership, finalization checking, and reorganization logic

### Remaining Work Analysis

**Missing Critical Components:**
- Finalization logic with AuxPoW integration (30% complete)
- Chain state reorganization implementation (40% complete) 
- Migration adapter for gradual legacy transition (25% complete)
- Comprehensive test suite (20% complete)
- Actor supervision system integration (10% complete)
- Performance benchmarking and optimization (0% complete)

### Detailed Next Step Plans

#### Priority 1: Complete Core ChainActor Implementation

**Plan:** Implement missing finalization logic, complete reorganization handling, and add proper error recovery mechanisms.

**Implementation 1: Enhanced Finalization System**
```rust
// src/actors/chain/finalization.rs
use actix::prelude::*;
use std::collections::HashMap;
use crate::types::*;

#[derive(Debug, Clone)]
pub struct FinalizationManager {
    pending_finalizations: HashMap<u64, AuxPowHeader>,
    finalization_queue: VecDeque<FinalizationEntry>,
    last_finalized_height: u64,
    config: FinalizationConfig,
}

#[derive(Debug, Clone)]
pub struct FinalizationEntry {
    pub height: u64,
    pub block_hash: Hash256,
    pub pow_header: AuxPowHeader,
    pub received_at: Instant,
}

#[derive(Debug, Clone)]
pub struct FinalizationConfig {
    pub max_pending_finalizations: usize,
    pub finalization_timeout: Duration,
    pub min_confirmations: u32,
    pub max_finalization_lag: u64,
}

impl FinalizationManager {
    pub fn new(config: FinalizationConfig) -> Self {
        Self {
            pending_finalizations: HashMap::new(),
            finalization_queue: VecDeque::new(),
            last_finalized_height: 0,
            config,
        }
    }

    pub fn add_pow_header(&mut self, pow_header: AuxPowHeader) -> Result<(), ChainError> {
        let height = pow_header.height;
        
        // Validate PoW header
        if !self.validate_pow_header(&pow_header)? {
            return Err(ChainError::InvalidPowHeader);
        }

        // Check if already have finalization for this height
        if self.pending_finalizations.contains_key(&height) {
            return Err(ChainError::DuplicateFinalization);
        }

        // Add to pending
        self.pending_finalizations.insert(height, pow_header.clone());
        
        // Add to queue for processing
        self.finalization_queue.push_back(FinalizationEntry {
            height,
            block_hash: pow_header.block_hash,
            pow_header,
            received_at: Instant::now(),
        });

        // Clean up old entries
        self.cleanup_expired_entries();
        
        Ok(())
    }

    pub fn process_finalization_queue(
        &mut self,
        current_head_height: u64,
    ) -> Vec<FinalizationEntry> {
        let mut ready_for_finalization = Vec::new();
        
        while let Some(entry) = self.finalization_queue.front() {
            // Check if we can finalize this height
            if entry.height <= current_head_height && 
               entry.height > self.last_finalized_height {
                
                // Check confirmations
                let confirmations = current_head_height - entry.height;
                if confirmations >= self.config.min_confirmations as u64 {
                    ready_for_finalization.push(self.finalization_queue.pop_front().unwrap());
                    self.last_finalized_height = entry.height;
                } else {
                    break; // Wait for more confirmations
                }
            } else if entry.height > current_head_height {
                break; // Future block, wait
            } else {
                // Old block, remove
                self.finalization_queue.pop_front();
                self.pending_finalizations.remove(&entry.height);
            }
        }
        
        ready_for_finalization
    }

    fn validate_pow_header(&self, pow_header: &AuxPowHeader) -> Result<bool, ChainError> {
        // Validate PoW difficulty
        if pow_header.difficulty < self.config.min_difficulty {
            return Ok(false);
        }

        // Validate merkle path
        if !pow_header.validate_merkle_path()? {
            return Ok(false);
        }

        // Validate parent block hash
        if pow_header.parent_block_hash.is_zero() {
            return Ok(false);
        }

        Ok(true)
    }

    fn cleanup_expired_entries(&mut self) {
        let now = Instant::now();
        
        self.finalization_queue.retain(|entry| {
            let expired = now.duration_since(entry.received_at) > self.config.finalization_timeout;
            if expired {
                self.pending_finalizations.remove(&entry.height);
            }
            !expired
        });
    }
}

// Enhanced ChainActor with finalization
impl ChainActor {
    pub async fn handle_auxpow_header(&mut self, pow_header: AuxPowHeader) -> Result<(), ChainError> {
        info!("Received AuxPoW header for height {}", pow_header.height);
        
        // Add to finalization manager
        self.finalization_manager.add_pow_header(pow_header.clone())?;
        
        // Process any ready finalizations
        let ready_finalizations = self.finalization_manager
            .process_finalization_queue(self.head.height());
        
        for finalization in ready_finalizations {
            self.finalize_blocks_up_to(finalization.height, finalization.pow_header).await?;
        }
        
        self.metrics.pow_headers_received.inc();
        Ok(())
    }

    async fn finalize_blocks_up_to(
        &mut self,
        target_height: u64,
        pow_header: AuxPowHeader,
    ) -> Result<(), ChainError> {
        info!("Finalizing blocks up to height {}", target_height);
        
        // Get all blocks from last finalized to target
        let finalized_height = self.finalized.as_ref().map(|b| b.height()).unwrap_or(0);
        
        if target_height <= finalized_height {
            return Ok(()); // Already finalized
        }

        // Get blocks to finalize
        let blocks_to_finalize = self.storage_actor
            .send(GetBlockRange {
                start_height: finalized_height + 1,
                end_height: target_height,
            })
            .await??;

        // Validate finalization
        for block in &blocks_to_finalize {
            if !self.validate_finalization_eligibility(block, &pow_header)? {
                return Err(ChainError::InvalidFinalization);
            }
        }

        // Update finalized state
        if let Some(final_block) = blocks_to_finalize.last() {
            self.finalized = Some(final_block.message.clone());
            
            // Notify engine of finalization
            self.engine_actor
                .send(FinalizeBlocks {
                    blocks: blocks_to_finalize.clone(),
                    pow_proof: pow_header,
                })
                .await??;

            // Notify bridge of finalized state
            self.bridge_actor
                .send(UpdateFinalizedState {
                    finalized_height: target_height,
                    finalized_hash: final_block.message.hash(),
                })
                .await?;

            // Update metrics
            self.metrics.blocks_finalized.inc_by(blocks_to_finalize.len() as u64);
            self.metrics.finalized_height.set(target_height as i64);

            info!("Finalized {} blocks, new finalized height: {}", 
                  blocks_to_finalize.len(), target_height);
        }

        Ok(())
    }

    fn validate_finalization_eligibility(
        &self,
        block: &SignedConsensusBlock,
        pow_header: &AuxPowHeader,
    ) -> Result<bool, ChainError> {
        // Check block is in our chain
        if !self.is_block_in_canonical_chain(block)? {
            return Ok(false);
        }

        // Check PoW commits to this block's bundle
        let bundle_hash = self.calculate_bundle_hash_for_height(block.message.height())?;
        if pow_header.committed_bundle_hash != bundle_hash {
            return Ok(false);
        }

        // Check timing constraints
        let block_time = block.message.timestamp;
        let pow_time = pow_header.timestamp;
        
        if pow_time < block_time {
            return Ok(false); // PoW can't be before block
        }

        if pow_time.duration_since(block_time) > Duration::from_secs(3600) {
            return Ok(false); // PoW too late (1 hour max)
        }

        Ok(true)
    }
}

// Message for receiving AuxPoW headers
#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct SubmitAuxPowHeader {
    pub pow_header: AuxPowHeader,
}

impl Handler<SubmitAuxPowHeader> for ChainActor {
    type Result = ResponseActFuture<Self, Result<(), ChainError>>;
    
    fn handle(&mut self, msg: SubmitAuxPowHeader, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            self.handle_auxpow_header(msg.pow_header).await
        }.into_actor(self))
    }
}

// Enhanced reorganization with finalization constraints
impl ChainActor {
    async fn handle_potential_reorg_with_finalization(
        &mut self,
        new_block: SignedConsensusBlock,
    ) -> Result<(), ChainError> {
        let finalized_height = self.finalized.as_ref().map(|b| b.height()).unwrap_or(0);
        
        // Cannot reorg past finalized blocks
        if new_block.message.height() <= finalized_height {
            return Err(ChainError::ReorgPastFinalized);
        }

        // Find common ancestor
        let common_ancestor = self.find_common_ancestor(&new_block).await?;
        
        // Check reorg doesn't affect finalized blocks
        if common_ancestor.height() < finalized_height {
            return Err(ChainError::ReorgWouldAffectFinalized);
        }

        // Continue with normal reorg logic
        self.handle_potential_reorg(new_block).await
    }
}
```

**Implementation 2: Advanced Chain State Management**
```rust
// src/actors/chain/state_manager.rs
use actix::prelude::*;
use std::collections::{HashMap, BTreeMap, VecDeque};

#[derive(Debug)]
pub struct ChainStateManager {
    // State trees for different heights
    state_at_height: BTreeMap<u64, ChainSnapshot>,
    // Pending blocks not yet in main chain
    orphan_pool: HashMap<Hash256, SignedConsensusBlock>,
    // Block index for fast lookups
    block_index: HashMap<Hash256, BlockMetadata>,
    // Chain metrics
    chain_metrics: ChainStateMetrics,
    // Configuration
    config: StateManagerConfig,
}

#[derive(Debug, Clone)]
pub struct ChainSnapshot {
    pub block: ConsensusBlock,
    pub state_root: Hash256,
    pub execution_state: ExecutionState,
    pub federation_state: FederationState,
    pub finalization_status: FinalizationStatus,
}

#[derive(Debug, Clone)]
pub struct BlockMetadata {
    pub height: u64,
    pub parent: Hash256,
    pub children: Vec<Hash256>,
    pub difficulty: U256,
    pub timestamp: Duration,
    pub is_finalized: bool,
    pub is_canonical: bool,
}

#[derive(Debug, Clone)]
pub enum FinalizationStatus {
    Unfinalized,
    PendingFinalization(AuxPowHeader),
    Finalized(AuxPowHeader),
}

#[derive(Debug)]
pub struct StateManagerConfig {
    pub max_orphan_blocks: usize,
    pub state_cache_size: usize,
    pub max_reorg_depth: u64,
    pub snapshot_interval: u64,
}

impl ChainStateManager {
    pub fn new(config: StateManagerConfig, genesis: ConsensusBlock) -> Self {
        let mut state_manager = Self {
            state_at_height: BTreeMap::new(),
            orphan_pool: HashMap::new(),
            block_index: HashMap::new(),
            chain_metrics: ChainStateMetrics::new(),
            config,
        };

        // Initialize with genesis
        let genesis_snapshot = ChainSnapshot {
            block: genesis.clone(),
            state_root: genesis.execution_payload.state_root,
            execution_state: ExecutionState::default(),
            federation_state: FederationState::default(),
            finalization_status: FinalizationStatus::Finalized(AuxPowHeader::genesis()),
        };

        state_manager.state_at_height.insert(0, genesis_snapshot);
        state_manager.block_index.insert(genesis.hash(), BlockMetadata {
            height: 0,
            parent: Hash256::zero(),
            children: vec![],
            difficulty: U256::zero(),
            timestamp: genesis.timestamp,
            is_finalized: true,
            is_canonical: true,
        });

        state_manager
    }

    pub fn add_block(&mut self, block: SignedConsensusBlock) -> Result<AddBlockResult, ChainError> {
        let block_hash = block.message.hash();
        let parent_hash = block.message.parent_hash;

        // Check if we already have this block
        if self.block_index.contains_key(&block_hash) {
            return Ok(AddBlockResult::AlreadyExists);
        }

        // Check if parent exists
        if let Some(parent_metadata) = self.block_index.get_mut(&parent_hash) {
            // Parent exists, add to chain
            parent_metadata.children.push(block_hash);
            
            let height = parent_metadata.height + 1;
            
            // Add block metadata
            self.block_index.insert(block_hash, BlockMetadata {
                height,
                parent: parent_hash,
                children: vec![],
                difficulty: block.message.difficulty,
                timestamp: block.message.timestamp,
                is_finalized: false,
                is_canonical: self.is_extending_canonical_chain(&parent_hash),
            });

            // Create state snapshot
            let snapshot = self.create_snapshot_from_parent(&block, parent_hash)?;
            self.state_at_height.insert(height, snapshot);

            // Update chain tip if canonical
            if self.is_extending_canonical_chain(&parent_hash) {
                self.update_canonical_chain(block_hash, height)?;
                Ok(AddBlockResult::ExtendedChain)
            } else {
                Ok(AddBlockResult::CreatedFork)
            }
        } else {
            // Parent doesn't exist, add to orphan pool
            if self.orphan_pool.len() >= self.config.max_orphan_blocks {
                // Remove oldest orphan
                if let Some((oldest_hash, _)) = self.orphan_pool.iter().next() {
                    let oldest_hash = *oldest_hash;
                    self.orphan_pool.remove(&oldest_hash);
                }
            }
            
            self.orphan_pool.insert(block_hash, block);
            Ok(AddBlockResult::Orphaned)
        }
    }

    fn create_snapshot_from_parent(
        &self,
        block: &SignedConsensusBlock,
        parent_hash: Hash256,
    ) -> Result<ChainSnapshot, ChainError> {
        // Get parent snapshot
        let parent_metadata = self.block_index.get(&parent_hash)
            .ok_or(ChainError::ParentNotFound)?;
        
        let parent_snapshot = self.state_at_height.get(&parent_metadata.height)
            .ok_or(ChainError::ParentStateNotFound)?;

        // Apply block transitions
        let new_execution_state = self.apply_execution_transitions(
            &parent_snapshot.execution_state,
            &block.message.execution_payload,
        )?;

        let new_federation_state = self.apply_federation_transitions(
            &parent_snapshot.federation_state,
            &block.message,
        )?;

        Ok(ChainSnapshot {
            block: block.message.clone(),
            state_root: block.message.execution_payload.state_root,
            execution_state: new_execution_state,
            federation_state: new_federation_state,
            finalization_status: FinalizationStatus::Unfinalized,
        })
    }

    pub fn reorganize_to_block(
        &mut self,
        target_block_hash: Hash256,
    ) -> Result<ReorgResult, ChainError> {
        let target_metadata = self.block_index.get(&target_block_hash)
            .ok_or(ChainError::BlockNotFound)?;

        let current_tip = self.get_canonical_tip()?;
        
        // Find common ancestor
        let common_ancestor = self.find_common_ancestor(
            target_block_hash,
            current_tip.block.hash(),
        )?;

        let reorg_depth = current_tip.block.height() - common_ancestor.height;
        if reorg_depth > self.config.max_reorg_depth {
            return Err(ChainError::ReorgTooDeep);
        }

        // Check finalization constraints
        if common_ancestor.finalization_status != FinalizationStatus::Unfinalized {
            return Err(ChainError::ReorgPastFinalized);
        }

        // Build new canonical chain
        let new_chain = self.build_chain_to_block(target_block_hash, common_ancestor.block.hash())?;
        
        // Update canonical flags
        self.update_canonical_flags(&new_chain)?;

        // Update state snapshots
        self.rebuild_state_from_ancestor(&common_ancestor, &new_chain)?;

        self.chain_metrics.reorgs.inc();
        self.chain_metrics.reorg_depth.observe(reorg_depth as f64);

        Ok(ReorgResult {
            old_tip: current_tip.block.hash(),
            new_tip: target_block_hash,
            reorg_depth,
            blocks_reverted: reorg_depth,
            blocks_applied: new_chain.len() as u64,
        })
    }

    pub fn finalize_up_to_height(&mut self, height: u64, pow_header: AuxPowHeader) -> Result<(), ChainError> {
        // Find all blocks up to height in canonical chain
        let mut blocks_to_finalize = vec![];
        
        for (h, snapshot) in self.state_at_height.range(..=height) {
            if let Some(metadata) = self.block_index.get(&snapshot.block.hash()) {
                if metadata.is_canonical && !metadata.is_finalized {
                    blocks_to_finalize.push(*h);
                }
            }
        }

        // Mark blocks as finalized
        for h in blocks_to_finalize {
            if let Some(snapshot) = self.state_at_height.get_mut(&h) {
                snapshot.finalization_status = FinalizationStatus::Finalized(pow_header.clone());
                
                if let Some(metadata) = self.block_index.get_mut(&snapshot.block.hash()) {
                    metadata.is_finalized = true;
                }
            }
        }

        // Prune old non-canonical branches
        self.prune_non_canonical_branches(height)?;

        self.chain_metrics.finalized_height.set(height as i64);
        
        Ok(())
    }

    fn prune_non_canonical_branches(&mut self, finalized_height: u64) -> Result<(), ChainError> {
        let blocks_to_remove: Vec<Hash256> = self.block_index
            .iter()
            .filter(|(_, metadata)| {
                metadata.height <= finalized_height && !metadata.is_canonical
            })
            .map(|(hash, _)| *hash)
            .collect();

        for hash in blocks_to_remove {
            self.block_index.remove(&hash);
            // Also remove from height index if present
            if let Some(metadata) = self.block_index.get(&hash) {
                self.state_at_height.remove(&metadata.height);
            }
        }

        // Cleanup orphan pool of old blocks
        let orphans_to_remove: Vec<Hash256> = self.orphan_pool
            .iter()
            .filter(|(_, block)| block.message.height() <= finalized_height)
            .map(|(hash, _)| *hash)
            .collect();

        for hash in orphans_to_remove {
            self.orphan_pool.remove(&hash);
        }

        Ok(())
    }

    pub fn process_orphan_blocks(&mut self) -> Result<Vec<ProcessedBlock>, ChainError> {
        let mut processed = Vec::new();
        let mut retry_queue = VecDeque::new();

        // Move all orphans to retry queue
        for (hash, block) in self.orphan_pool.drain() {
            retry_queue.push_back((hash, block));
        }

        // Process retry queue until no progress
        let mut made_progress = true;
        while made_progress && !retry_queue.is_empty() {
            made_progress = false;
            let queue_size = retry_queue.len();
            
            for _ in 0..queue_size {
                if let Some((hash, block)) = retry_queue.pop_front() {
                    match self.add_block(block.clone()) {
                        Ok(AddBlockResult::ExtendedChain) | Ok(AddBlockResult::CreatedFork) => {
                            processed.push(ProcessedBlock {
                                hash,
                                result: ProcessBlockResult::Accepted,
                            });
                            made_progress = true;
                        }
                        Ok(AddBlockResult::Orphaned) => {
                            retry_queue.push_back((hash, block));
                        }
                        Ok(AddBlockResult::AlreadyExists) => {
                            // Skip, already processed
                            made_progress = true;
                        }
                        Err(e) => {
                            processed.push(ProcessedBlock {
                                hash,
                                result: ProcessBlockResult::Rejected(e),
                            });
                        }
                    }
                }
            }
        }

        // Put unprocessed blocks back in orphan pool
        for (hash, block) in retry_queue {
            self.orphan_pool.insert(hash, block);
        }

        Ok(processed)
    }
}

#[derive(Debug)]
pub enum AddBlockResult {
    ExtendedChain,
    CreatedFork,
    Orphaned,
    AlreadyExists,
}

#[derive(Debug)]
pub struct ReorgResult {
    pub old_tip: Hash256,
    pub new_tip: Hash256,
    pub reorg_depth: u64,
    pub blocks_reverted: u64,
    pub blocks_applied: u64,
}

#[derive(Debug)]
pub struct ProcessedBlock {
    pub hash: Hash256,
    pub result: ProcessBlockResult,
}

#[derive(Debug)]
pub enum ProcessBlockResult {
    Accepted,
    Rejected(ChainError),
}
```

**Implementation 3: Production Migration System**
```rust
// src/actors/chain/migration.rs
use actix::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug)]
pub struct ChainMigrationController {
    // Migration state
    current_phase: MigrationPhase,
    phase_start_time: Instant,
    
    // Legacy chain
    legacy_chain: Option<Arc<RwLock<crate::chain::Chain>>>,
    
    // New actor
    chain_actor: Option<Addr<ChainActor>>,
    
    // Migration metrics
    metrics: MigrationMetrics,
    
    // Feature flags
    feature_flags: Arc<dyn FeatureFlagProvider>,
    
    // Configuration
    config: MigrationConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MigrationPhase {
    LegacyOnly,
    ShadowMode,        // Actor runs in background, results compared
    CanaryMode,        // Small % of operations use actor
    ParallelMode,      // Both systems active, results compared
    ActorPrimary,      // Actor primary, legacy fallback
    ActorOnly,
    Rollback,          // Emergency rollback to legacy
}

#[derive(Debug)]
pub struct MigrationConfig {
    pub shadow_mode_duration: Duration,
    pub canary_percentage: f64,
    pub parallel_mode_duration: Duration,
    pub primary_mode_duration: Duration,
    pub success_threshold: f64,
    pub error_threshold: f64,
    pub performance_threshold: f64,
}

#[derive(Debug)]
pub struct MigrationMetrics {
    // Operation counts
    pub legacy_operations: AtomicU64,
    pub actor_operations: AtomicU64,
    pub parallel_operations: AtomicU64,
    
    // Success rates
    pub legacy_success_rate: AtomicU64,
    pub actor_success_rate: AtomicU64,
    
    // Performance metrics
    pub legacy_avg_latency: AtomicU64,
    pub actor_avg_latency: AtomicU64,
    
    // Error metrics
    pub legacy_errors: AtomicU64,
    pub actor_errors: AtomicU64,
    pub comparison_mismatches: AtomicU64,
}

impl ChainMigrationController {
    pub fn new(
        legacy_chain: Arc<RwLock<crate::chain::Chain>>,
        config: MigrationConfig,
        feature_flags: Arc<dyn FeatureFlagProvider>,
    ) -> Self {
        Self {
            current_phase: MigrationPhase::LegacyOnly,
            phase_start_time: Instant::now(),
            legacy_chain: Some(legacy_chain),
            chain_actor: None,
            metrics: MigrationMetrics::new(),
            feature_flags,
            config,
        }
    }

    pub async fn initialize_actor(&mut self, chain_actor: Addr<ChainActor>) -> Result<(), MigrationError> {
        // Sync actor with current legacy state
        let legacy_state = {
            let legacy = self.legacy_chain.as_ref().unwrap().read().await;
            ChainState {
                head: legacy.head().clone(),
                finalized: legacy.finalized().cloned(),
                height: legacy.height(),
                federation_version: legacy.federation_version(),
            }
        };

        // Initialize actor with legacy state
        chain_actor.send(InitializeFromLegacy {
            state: legacy_state,
        }).await??;

        self.chain_actor = Some(chain_actor);
        Ok(())
    }

    pub async fn advance_migration_phase(&mut self) -> Result<MigrationPhase, MigrationError> {
        let phase_duration = self.phase_start_time.elapsed();
        let current_metrics = self.calculate_current_metrics().await?;

        let next_phase = match self.current_phase {
            MigrationPhase::LegacyOnly => {
                // Check if actor is ready
                if self.chain_actor.is_some() {
                    MigrationPhase::ShadowMode
                } else {
                    return Err(MigrationError::ActorNotReady);
                }
            }
            
            MigrationPhase::ShadowMode => {
                if phase_duration >= self.config.shadow_mode_duration {
                    // Check shadow mode success metrics
                    if current_metrics.actor_success_rate >= self.config.success_threshold &&
                       current_metrics.comparison_accuracy >= 0.95 {
                        MigrationPhase::CanaryMode
                    } else {
                        return Err(MigrationError::ShadowModeFailed);
                    }
                } else {
                    return Ok(self.current_phase.clone());
                }
            }
            
            MigrationPhase::CanaryMode => {
                // Gradually increase canary percentage
                let canary_progress = phase_duration.as_secs_f64() / 300.0; // 5 minutes
                let target_percentage = (canary_progress * self.config.canary_percentage).min(self.config.canary_percentage);
                
                if canary_progress >= 1.0 && 
                   current_metrics.actor_success_rate >= self.config.success_threshold {
                    MigrationPhase::ParallelMode
                } else if current_metrics.actor_error_rate > self.config.error_threshold {
                    MigrationPhase::Rollback
                } else {
                    return Ok(self.current_phase.clone());
                }
            }
            
            MigrationPhase::ParallelMode => {
                if phase_duration >= self.config.parallel_mode_duration {
                    if current_metrics.actor_success_rate >= self.config.success_threshold &&
                       current_metrics.performance_ratio >= self.config.performance_threshold {
                        MigrationPhase::ActorPrimary
                    } else {
                        MigrationPhase::Rollback
                    }
                } else {
                    return Ok(self.current_phase.clone());
                }
            }
            
            MigrationPhase::ActorPrimary => {
                if phase_duration >= self.config.primary_mode_duration {
                    if current_metrics.actor_success_rate >= self.config.success_threshold {
                        MigrationPhase::ActorOnly
                    } else {
                        MigrationPhase::Rollback
                    }
                } else {
                    return Ok(self.current_phase.clone());
                }
            }
            
            MigrationPhase::ActorOnly => {
                // Migration complete
                return Ok(self.current_phase.clone());
            }
            
            MigrationPhase::Rollback => {
                // Stay in rollback mode
                return Ok(self.current_phase.clone());
            }
        };

        // Perform phase transition
        self.transition_to_phase(next_phase.clone()).await?;
        
        Ok(next_phase)
    }

    async fn transition_to_phase(&mut self, new_phase: MigrationPhase) -> Result<(), MigrationError> {
        info!("Transitioning from {:?} to {:?}", self.current_phase, new_phase);
        
        match (&self.current_phase, &new_phase) {
            (MigrationPhase::LegacyOnly, MigrationPhase::ShadowMode) => {
                // Start shadow mode - actor runs but results not used
                self.start_shadow_mode().await?;
            }
            
            (MigrationPhase::ShadowMode, MigrationPhase::CanaryMode) => {
                // Start canary mode - small percentage uses actor
                self.start_canary_mode().await?;
            }
            
            (MigrationPhase::CanaryMode, MigrationPhase::ParallelMode) => {
                // Start parallel mode - both systems used equally
                self.start_parallel_mode().await?;
            }
            
            (MigrationPhase::ParallelMode, MigrationPhase::ActorPrimary) => {
                // Actor becomes primary
                self.start_actor_primary_mode().await?;
            }
            
            (MigrationPhase::ActorPrimary, MigrationPhase::ActorOnly) => {
                // Complete migration
                self.complete_migration().await?;
            }
            
            (_, MigrationPhase::Rollback) => {
                // Emergency rollback
                self.perform_rollback().await?;
            }
            
            _ => {
                return Err(MigrationError::InvalidTransition);
            }
        }

        self.current_phase = new_phase;
        self.phase_start_time = Instant::now();
        
        Ok(())
    }

    async fn start_shadow_mode(&mut self) -> Result<(), MigrationError> {
        // Configure actor to run in shadow mode
        if let Some(actor) = &self.chain_actor {
            actor.send(ConfigureShadowMode {
                enabled: true,
            }).await??;
        }
        
        info!("Shadow mode started");
        Ok(())
    }

    async fn complete_migration(&mut self) -> Result<(), MigrationError> {
        // Drop legacy chain
        self.legacy_chain = None;
        
        // Notify actor that migration is complete
        if let Some(actor) = &self.chain_actor {
            actor.send(MigrationComplete).await??;
        }
        
        info!("Chain actor migration completed successfully");
        Ok(())
    }

    pub async fn import_block(&self, block: SignedConsensusBlock) -> Result<(), ChainError> {
        match self.current_phase {
            MigrationPhase::LegacyOnly => {
                self.import_block_legacy_only(block).await
            }
            
            MigrationPhase::ShadowMode => {
                self.import_block_shadow_mode(block).await
            }
            
            MigrationPhase::CanaryMode => {
                self.import_block_canary_mode(block).await
            }
            
            MigrationPhase::ParallelMode => {
                self.import_block_parallel_mode(block).await
            }
            
            MigrationPhase::ActorPrimary => {
                self.import_block_actor_primary(block).await
            }
            
            MigrationPhase::ActorOnly => {
                self.import_block_actor_only(block).await
            }
            
            MigrationPhase::Rollback => {
                self.import_block_legacy_only(block).await
            }
        }
    }

    async fn import_block_shadow_mode(&self, block: SignedConsensusBlock) -> Result<(), ChainError> {
        // Legacy import (primary)
        let legacy_result = {
            let mut legacy = self.legacy_chain.as_ref().unwrap().write().await;
            legacy.import_block(block.clone()).await
        };

        // Actor import (shadow)
        if let Some(actor) = &self.chain_actor {
            let _shadow_result = actor.send(ImportBlock {
                block: block.clone(),
                broadcast: false,
            }).await;
            
            // Compare results but don't fail on mismatch in shadow mode
            // Just log for analysis
        }

        self.metrics.legacy_operations.fetch_add(1, Ordering::Relaxed);
        
        legacy_result
    }

    async fn import_block_canary_mode(&self, block: SignedConsensusBlock) -> Result<(), ChainError> {
        // Determine if this operation should use actor (canary)
        let use_actor = self.should_use_actor_canary();
        
        if use_actor {
            self.metrics.actor_operations.fetch_add(1, Ordering::Relaxed);
            
            match self.chain_actor.as_ref().unwrap().send(ImportBlock {
                block: block.clone(),
                broadcast: true,
            }).await {
                Ok(Ok(())) => {
                    self.metrics.actor_success_rate.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
                Ok(Err(e)) | Err(_) => {
                    self.metrics.actor_errors.fetch_add(1, Ordering::Relaxed);
                    
                    // Fallback to legacy
                    warn!("Actor import failed in canary mode, falling back to legacy");
                    let mut legacy = self.legacy_chain.as_ref().unwrap().write().await;
                    legacy.import_block(block).await
                }
            }
        } else {
            self.metrics.legacy_operations.fetch_add(1, Ordering::Relaxed);
            
            let mut legacy = self.legacy_chain.as_ref().unwrap().write().await;
            let result = legacy.import_block(block).await;
            
            if result.is_ok() {
                self.metrics.legacy_success_rate.fetch_add(1, Ordering::Relaxed);
            } else {
                self.metrics.legacy_errors.fetch_add(1, Ordering::Relaxed);
            }
            
            result
        }
    }

    fn should_use_actor_canary(&self) -> bool {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let roll: f64 = rng.gen();
        
        let phase_progress = self.phase_start_time.elapsed().as_secs_f64() / 300.0; // 5 minutes
        let current_percentage = (phase_progress * self.config.canary_percentage).min(self.config.canary_percentage);
        
        roll < current_percentage / 100.0
    }
}

// Messages for migration control
#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct InitializeFromLegacy {
    pub state: ChainState,
}

#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct ConfigureShadowMode {
    pub enabled: bool,
}

#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct MigrationComplete;

impl Handler<InitializeFromLegacy> for ChainActor {
    type Result = Result<(), ChainError>;
    
    fn handle(&mut self, msg: InitializeFromLegacy, _: &mut Context<Self>) -> Self::Result {
        info!("Initializing ChainActor from legacy state at height {}", msg.state.height);
        
        self.head = msg.state.head;
        self.finalized = msg.state.finalized;
        
        // Load any missing state from storage
        // This would involve syncing with the storage actor
        
        Ok(())
    }
}
```

#### Priority 2: Comprehensive Testing and Integration

**Plan:** Create extensive test suites covering unit tests, integration tests, and performance benchmarks.

**Comprehensive Test Implementation:**
```rust
// tests/integration/chain_actor_tests.rs
use actix::prelude::*;
use crate::actors::chain::*;

#[tokio::test]
async fn test_chain_actor_full_lifecycle() {
    let system = ActorSystem::new("test").unwrap();
    
    // Setup test environment
    let (engine_actor, bridge_actor, storage_actor, network_actor) = create_test_actors().await;
    
    // Create chain actor
    let chain_actor = ChainActor::new(
        test_config(),
        engine_actor,
        bridge_actor, 
        storage_actor,
        network_actor,
    ).unwrap().start();
    
    // Test block production
    let block1 = chain_actor.send(ProduceBlock {
        slot: 1,
        timestamp: Duration::from_secs(1000),
    }).await.unwrap().unwrap();
    
    assert_eq!(block1.message.slot, 1);
    
    // Test block import
    let test_block = create_test_block(2, block1.message.hash());
    chain_actor.send(ImportBlock {
        block: test_block.clone(),
        broadcast: false,
    }).await.unwrap().unwrap();
    
    // Test chain status
    let status = chain_actor.send(GetChainStatus).await.unwrap().unwrap();
    assert_eq!(status.head_height, 2);
    assert_eq!(status.head_hash, test_block.message.hash());
    
    // Test finalization
    let pow_header = create_test_auxpow_header(2);
    chain_actor.send(SubmitAuxPowHeader {
        pow_header,
    }).await.unwrap().unwrap();
    
    // Verify finalization
    let final_status = chain_actor.send(GetChainStatus).await.unwrap().unwrap();
    assert_eq!(final_status.finalized_height, Some(2));
}

#[tokio::test]
async fn test_chain_reorganization() {
    let system = ActorSystem::new("test").unwrap();
    let chain_actor = create_test_chain_actor().await;
    
    // Build initial chain A (height 1-5)
    let mut chain_a = Vec::new();
    let mut parent_hash = Hash256::zero();
    
    for i in 1..=5 {
        let block = create_test_block(i, parent_hash);
        parent_hash = block.message.hash();
        chain_a.push(block.clone());
        
        chain_actor.send(ImportBlock {
            block,
            broadcast: false,
        }).await.unwrap().unwrap();
    }
    
    // Verify initial state
    let status = chain_actor.send(GetChainStatus).await.unwrap().unwrap();
    assert_eq!(status.head_height, 5);
    assert_eq!(status.head_hash, chain_a[4].message.hash());
    
    // Create competing chain B (height 1-6, heavier)
    let mut chain_b = Vec::new();
    parent_hash = Hash256::zero();
    
    for i in 1..=6 {
        let mut block = create_test_block(i, parent_hash);
        if i > 1 {
            // Make chain B heavier
            block.message.difficulty = chain_a[0].message.difficulty + U256::from(100);
        }
        parent_hash = block.message.hash();
        chain_b.push(block);
    }
    
    // Import competing chain (should trigger reorg)
    for block in &chain_b {
        chain_actor.send(ImportBlock {
            block: block.clone(),
            broadcast: false,
        }).await.unwrap().unwrap();
    }
    
    // Verify reorg happened
    let final_status = chain_actor.send(GetChainStatus).await.unwrap().unwrap();
    assert_eq!(final_status.head_height, 6);
    assert_eq!(final_status.head_hash, chain_b[5].message.hash());
}

#[tokio::test]
async fn test_migration_adapter() {
    let legacy_chain = Arc::new(RwLock::new(create_test_legacy_chain()));
    let feature_flags = Arc::new(TestFeatureFlagManager::new());
    
    let mut adapter = ChainMigrationController::new(
        legacy_chain.clone(),
        test_migration_config(),
        feature_flags,
    );
    
    // Test legacy-only mode
    let block1 = create_test_block(1, Hash256::zero());
    adapter.import_block(block1.clone()).await.unwrap();
    
    // Initialize actor
    let chain_actor = create_test_chain_actor().await;
    adapter.initialize_actor(chain_actor).await.unwrap();
    
    // Advance to shadow mode
    adapter.advance_migration_phase().await.unwrap();
    assert_eq!(adapter.current_phase, MigrationPhase::ShadowMode);
    
    // Test shadow mode operation
    let block2 = create_test_block(2, block1.message.hash());
    adapter.import_block(block2).await.unwrap();
    
    // Both legacy and actor should have the block
    let legacy_height = legacy_chain.read().await.height();
    assert_eq!(legacy_height, 2);
}

// Performance tests
mod bench {
    use super::*;
    use criterion::{criterion_group, criterion_main, Criterion};
    
    fn bench_block_import(c: &mut Criterion) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let chain_actor = rt.block_on(create_test_chain_actor());
        
        let blocks: Vec<_> = (1..=1000)
            .map(|i| create_test_block(i, Hash256::random()))
            .collect();
        
        c.bench_function("chain_actor_block_import", |b| {
            b.iter(|| {
                rt.block_on(async {
                    for block in &blocks {
                        chain_actor.send(ImportBlock {
                            block: block.clone(),
                            broadcast: false,
                        }).await.unwrap().unwrap();
                    }
                })
            })
        });
    }
    
    fn bench_block_production(c: &mut Criterion) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let chain_actor = rt.block_on(create_test_chain_actor());
        
        c.bench_function("chain_actor_block_production", |b| {
            b.iter(|| {
                rt.block_on(async {
                    chain_actor.send(ProduceBlock {
                        slot: rand::random(),
                        timestamp: Duration::from_secs(rand::random::<u64>() % 10000),
                    }).await.unwrap().unwrap();
                })
            })
        });
    }
    
    criterion_group!(benches, bench_block_import, bench_block_production);
    criterion_main!(benches);
}
```

### Detailed Test Plan

**Unit Tests (150 tests):**
1. Message handling tests (30 tests)
2. State management tests (40 tests) 
3. Block validation tests (25 tests)
4. Finalization logic tests (20 tests)
5. Reorganization tests (20 tests)
6. Migration adapter tests (15 tests)

**Integration Tests (75 tests):**
1. Actor communication tests (25 tests)
2. End-to-end block lifecycle (20 tests)
3. Migration workflow tests (15 tests)
4. Error recovery tests (15 tests)

**Performance Tests (25 tests):**
1. Block import throughput (5 tests)
2. Memory usage optimization (10 tests)
3. Actor message latency (10 tests)

### Implementation Timeline

**Week 1-2: Core Implementation**
- Complete finalization system with AuxPoW integration
- Implement advanced state management with reorganization
- Create production migration controller

**Week 3: Testing and Integration**
- Develop comprehensive test suite
- Integration with existing actor system
- Performance optimization and benchmarking

**Week 4: Migration and Validation**
- Test migration adapter in staging
- Validate against legacy system
- Performance and stability testing

### Success Metrics

**Functional Metrics:**
- 100% test coverage for core chain operations
- Zero consensus disruptions during migration
- All acceptance criteria met

**Performance Metrics:**
- Block import time ≤ 50ms (95th percentile)
- Memory usage reduction of 30% vs legacy
- Actor message latency ≤ 1ms median

**Operational Metrics:**
- Migration success rate > 99.9%
- Zero finalization failures
- Successful rollback capability within 30 seconds

### Risk Mitigation

**Technical Risks:**
- **State synchronization issues**: Comprehensive state validation and checksums
- **Actor supervision failures**: Circuit breaker patterns and automatic restarts
- **Migration data loss**: Parallel validation and rollback capabilities

**Operational Risks:**
- **Performance degradation**: Extensive benchmarking and gradual rollout
- **Consensus disruption**: Feature flag controls and immediate rollback
- **Integration failures**: Isolated testing environments and staged deployment