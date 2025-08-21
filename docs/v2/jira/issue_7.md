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

- [ ] Create ALYS-007-1: Design ChainActor message protocol with comprehensive message definitions [https://marathondh.atlassian.net/browse/AN-393]
- [ ] Create ALYS-007-2: Implement ChainActor core structure with consensus integration [https://marathondh.atlassian.net/browse/AN-394]
- [ ] Create ALYS-007-3: Implement block production logic with timing constraints [https://marathondh.atlassian.net/browse/AN-395]
- [ ] Create ALYS-007-4: Implement block import and validation pipeline [https://marathondh.atlassian.net/browse/AN-396]
- [ ] Create ALYS-007-5: Implement chain state management and reorganization [https://marathondh.atlassian.net/browse/AN-397]
- [ ] Create ALYS-007-6: Implement finalization logic with AuxPoW integration [https://marathondh.atlassian.net/browse/AN-398]
- [ ] Create ALYS-007-7: Create migration adapter for gradual legacy transition [https://marathondh.atlassian.net/browse/AN-399]
- [ ] Create ALYS-007-8: Implement comprehensive test suite (unit, integration, performance) [https://marathondh.atlassian.net/browse/AN-401]
- [ ] Create ALYS-007-9: Integration with actor supervision system [https://marathondh.atlassian.net/browse/AN-402]
- [ ] Create ALYS-007-10: Performance benchmarking and optimization [https://marathondh.atlassian.net/browse/AN-403]

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