# ALYS-010: Implement SyncActor with Improved Sync Algorithm

## Issue Type
Task

## Description

Implement the SyncActor to replace the problematic sync implementation with a robust, actor-based solution. This includes parallel block validation, intelligent peer selection, checkpoint-based recovery, and the ability to produce blocks when 99.5% synced.

## Acceptance Criteria

- [ ] SyncActor replaces current sync implementation
- [ ] Parallel block validation implemented
- [ ] Smart peer selection based on performance
- [ ] Checkpoint system for recovery
- [ ] Block production enabled at 99.5% sync
- [ ] Adaptive batch sizing based on network conditions
- [ ] Recovery from network partitions
- [ ] Sync speed improved by >2x
- [ ] Comprehensive metrics and monitoring

## Technical Details

### Implementation Steps

1. **Define SyncActor Messages and State**
```rust
// src/actors/sync/messages.rs

use actix::prelude::*;

#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct StartSync {
    pub from_height: Option<u64>,
    pub target_height: Option<u64>,
    pub checkpoint: Option<BlockCheckpoint>,
}

#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct PauseSync;

#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct ResumeSync;

#[derive(Message)]
#[rtype(result = "Result<SyncStatus, SyncError>")]
pub struct GetSyncStatus;

#[derive(Message)]
#[rtype(result = "Result<bool, SyncError>")]
pub struct CanProduceBlocks;

#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct ProcessBlockBatch {
    pub blocks: Vec<SignedConsensusBlock>,
    pub from_peer: PeerId,
}

#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct PeerDiscovered {
    pub peer_id: PeerId,
    pub reported_height: u64,
    pub protocol_version: String,
}

#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct PeerDisconnected {
    pub peer_id: PeerId,
    pub reason: String,
}

#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct CreateCheckpoint;

#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct RecoverFromCheckpoint {
    pub checkpoint: BlockCheckpoint,
}

#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub state: SyncState,
    pub current_height: u64,
    pub target_height: u64,
    pub blocks_per_second: f64,
    pub peers_connected: usize,
    pub estimated_completion: Option<Duration>,
    pub can_produce_blocks: bool,
}

#[derive(Debug, Clone)]
pub enum SyncState {
    Idle,
    Discovering { started_at: Instant, attempts: u32 },
    DownloadingHeaders { start: u64, current: u64, target: u64 },
    DownloadingBlocks { start: u64, current: u64, target: u64, batch_size: usize },
    CatchingUp { blocks_behind: u64, sync_speed: f64 },
    Synced { last_check: Instant },
    Failed { reason: String, last_good_height: u64, recovery_attempts: u32 },
}
```

2. **Implement SyncActor Core**
```rust
// src/actors/sync/mod.rs

use actix::prelude::*;
use std::collections::{HashMap, VecDeque};

pub struct SyncActor {
    // State machine
    state: SyncState,
    sync_progress: SyncProgress,
    
    // Peer management
    peer_manager: Addr<PeerManagerActor>,
    active_peers: HashMap<PeerId, PeerSyncInfo>,
    
    // Block processing
    block_processor: Addr<BlockProcessorActor>,
    block_buffer: BlockBuffer,
    
    // Chain interaction
    chain_actor: Addr<ChainActor>,
    
    // Checkpointing
    checkpoint_manager: CheckpointManager,
    
    // Configuration
    config: SyncConfig,
    
    // Metrics
    metrics: SyncMetrics,
    start_time: Instant,
}

#[derive(Clone)]
pub struct SyncConfig {
    pub checkpoint_interval: u64,
    pub max_checkpoints: usize,
    pub batch_size_min: usize,
    pub batch_size_max: usize,
    pub parallel_downloads: usize,
    pub validation_workers: usize,
    pub production_threshold: f64, // 0.995 = 99.5%
    pub peer_score_threshold: f64,
    pub request_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct SyncProgress {
    pub genesis_height: u64,
    pub current_height: u64,
    pub target_height: u64,
    pub highest_peer_height: u64,
    pub blocks_processed: u64,
    pub blocks_failed: u64,
    pub blocks_per_second: f64,
    pub last_checkpoint: Option<BlockCheckpoint>,
    pub active_downloads: usize,
}

#[derive(Debug, Clone)]
pub struct PeerSyncInfo {
    pub peer_id: PeerId,
    pub reported_height: u64,
    pub last_response: Instant,
    pub blocks_served: u64,
    pub average_latency: Duration,
    pub error_count: u32,
    pub score: f64,
}

struct BlockBuffer {
    buffer: VecDeque<(u64, SignedConsensusBlock)>,
    max_size: usize,
    pending_validation: HashMap<Hash256, ValidationStatus>,
}

impl SyncActor {
    pub fn new(
        config: SyncConfig,
        peer_manager: Addr<PeerManagerActor>,
        block_processor: Addr<BlockProcessorActor>,
        chain_actor: Addr<ChainActor>,
    ) -> Self {
        Self {
            state: SyncState::Idle,
            sync_progress: SyncProgress::default(),
            peer_manager,
            active_peers: HashMap::new(),
            block_processor,
            block_buffer: BlockBuffer::new(10000),
            chain_actor,
            checkpoint_manager: CheckpointManager::new(
                config.checkpoint_interval,
                config.max_checkpoints,
            ),
            config,
            metrics: SyncMetrics::new(),
            start_time: Instant::now(),
        }
    }
    
    fn can_produce_blocks(&self) -> bool {
        match &self.state {
            SyncState::Synced { .. } => true,
            SyncState::CatchingUp { blocks_behind, .. } => {
                // Allow production when very close to synced
                let progress = self.sync_progress.current_height as f64 
                    / self.sync_progress.target_height as f64;
                progress >= self.config.production_threshold && *blocks_behind <= 10
            }
            _ => false,
        }
    }
}

impl Actor for SyncActor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("SyncActor started");
        
        // Start sync progress monitor
        ctx.run_interval(Duration::from_secs(5), |act, _| {
            act.update_sync_metrics();
            
            // Update global metrics
            SYNC_CURRENT_HEIGHT.set(act.sync_progress.current_height as i64);
            SYNC_TARGET_HEIGHT.set(act.sync_progress.target_height as i64);
            SYNC_BLOCKS_PER_SECOND.set(act.sync_progress.blocks_per_second);
            
            let state_num = match act.state {
                SyncState::Idle => 0,
                SyncState::Discovering { .. } => 1,
                SyncState::DownloadingHeaders { .. } => 2,
                SyncState::DownloadingBlocks { .. } => 3,
                SyncState::CatchingUp { .. } => 4,
                SyncState::Synced { .. } => 5,
                SyncState::Failed { .. } => 6,
            };
            SYNC_STATE.set(state_num);
        });
        
        // Start checkpoint creator
        ctx.run_interval(Duration::from_secs(30), |act, ctx| {
            if act.should_create_checkpoint() {
                ctx.spawn(
                    async move {
                        if let Err(e) = act.create_checkpoint().await {
                            warn!("Failed to create checkpoint: {}", e);
                        }
                    }
                    .into_actor(act)
                );
            }
        });
    }
}

impl Handler<StartSync> for SyncActor {
    type Result = ResponseActFuture<Self, Result<(), SyncError>>;
    
    fn handle(&mut self, msg: StartSync, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            info!("Starting sync from height {:?} to {:?}", 
                msg.from_height, msg.target_height);
            
            // Try to recover from checkpoint if available
            let start_height = if let Some(checkpoint) = msg.checkpoint {
                info!("Recovering from checkpoint at height {}", checkpoint.height);
                self.recover_from_checkpoint(checkpoint).await?;
                checkpoint.height
            } else if let Some(checkpoint) = self.checkpoint_manager.find_latest() {
                info!("Found checkpoint at height {}", checkpoint.height);
                self.recover_from_checkpoint(checkpoint).await?;
                checkpoint.height
            } else {
                msg.from_height.unwrap_or(0)
            };
            
            // Get target height from peers if not specified
            let target_height = if let Some(height) = msg.target_height {
                height
            } else {
                self.get_network_height().await?
            };
            
            self.sync_progress.current_height = start_height;
            self.sync_progress.target_height = target_height;
            
            // Start sync state machine
            self.state = SyncState::Discovering {
                started_at: Instant::now(),
                attempts: 0,
            };
            
            self.run_sync_loop().await
        }.into_actor(self))
    }
}

impl SyncActor {
    async fn run_sync_loop(&mut self) -> Result<(), SyncError> {
        loop {
            match self.state.clone() {
                SyncState::Discovering { started_at, attempts } => {
                    if attempts > 30 {
                        self.state = SyncState::Failed {
                            reason: "No peers found".to_string(),
                            last_good_height: self.sync_progress.current_height,
                            recovery_attempts: 0,
                        };
                        continue;
                    }
                    
                    // Request peers from peer manager
                    let peers = self.peer_manager
                        .send(GetAvailablePeers)
                        .await??;
                    
                    if peers.len() >= self.config.parallel_downloads {
                        self.transition_to_downloading(peers).await?;
                    } else {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        self.state = SyncState::Discovering {
                            started_at,
                            attempts: attempts + 1,
                        };
                    }
                }
                
                SyncState::DownloadingHeaders { .. } => {
                    self.download_and_validate_headers().await?;
                }
                
                SyncState::DownloadingBlocks { .. } => {
                    self.download_and_process_blocks().await?;
                }
                
                SyncState::CatchingUp { blocks_behind, .. } => {
                    if blocks_behind == 0 {
                        self.state = SyncState::Synced {
                            last_check: Instant::now(),
                        };
                        info!("🎉 Sync complete!");
                        break;
                    }
                    
                    self.catch_up_recent_blocks().await?;
                }
                
                SyncState::Synced { .. } => {
                    // Sync complete
                    break;
                }
                
                SyncState::Failed { recovery_attempts, .. } => {
                    if recovery_attempts < 5 {
                        self.attempt_recovery().await?;
                    } else {
                        return Err(SyncError::MaxRecoveryAttemptsExceeded);
                    }
                }
                
                SyncState::Idle => {
                    // Waiting for start command
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
        
        Ok(())
    }
    
    async fn download_and_process_blocks(&mut self) -> Result<(), SyncError> {
        if let SyncState::DownloadingBlocks {
            current,
            target,
            mut batch_size,
            ..
        } = &mut self.state.clone() {
            // Get optimal batch size based on network conditions
            batch_size = self.calculate_optimal_batch_size().await?;
            
            // Select best peers for download
            let peers = self.select_best_peers(self.config.parallel_downloads).await?;
            
            // Create parallel download tasks
            let mut download_futures = Vec::new();
            
            for (i, peer) in peers.iter().enumerate() {
                let start_height = current + (i as u64 * batch_size as u64);
                if start_height >= target {
                    break;
                }
                
                let count = ((target - start_height).min(batch_size as u64)) as usize;
                
                let future = self.download_block_range(
                    peer.clone(),
                    start_height,
                    count,
                );
                
                download_futures.push(future);
            }
            
            // Execute downloads in parallel
            let download_results = futures::future::join_all(download_futures).await;
            
            // Process downloaded blocks
            for result in download_results {
                match result {
                    Ok(blocks) => {
                        // Send to block processor for parallel validation
                        let processed = self.block_processor
                            .send(ProcessBlockBatch { blocks: blocks.clone() })
                            .await??;
                        
                        // Update progress
                        self.sync_progress.current_height += processed.processed as u64;
                        self.sync_progress.blocks_processed += processed.processed as u64;
                        self.sync_progress.blocks_failed += processed.failed as u64;
                        
                        // Import validated blocks to chain
                        for block in processed.validated_blocks {
                            self.chain_actor
                                .send(ImportBlock { block, broadcast: false })
                                .await??;
                        }
                        
                        // Create checkpoint if needed
                        if self.sync_progress.current_height % self.config.checkpoint_interval == 0 {
                            self.create_checkpoint().await?;
                        }
                    }
                    Err(e) => {
                        warn!("Block download failed: {}", e);
                        // Peer scoring will handle bad peers
                    }
                }
            }
            
            // Update state
            if self.sync_progress.current_height >= target - 10 {
                self.state = SyncState::CatchingUp {
                    blocks_behind: target - self.sync_progress.current_height,
                    sync_speed: self.sync_progress.blocks_per_second,
                };
            } else {
                self.state = SyncState::DownloadingBlocks {
                    start: self.sync_progress.genesis_height,
                    current: self.sync_progress.current_height,
                    target,
                    batch_size,
                };
            }
        }
        
        Ok(())
    }
    
    async fn calculate_optimal_batch_size(&self) -> Result<usize, SyncError> {
        // Get network metrics
        let avg_latency = self.calculate_average_peer_latency();
        let avg_bandwidth = self.estimate_bandwidth();
        let peer_count = self.active_peers.len();
        
        // Adaptive batch size calculation
        let base_size = 128;
        let latency_factor = (100.0 / avg_latency.as_millis() as f64)
            .max(0.5)
            .min(4.0);
        let bandwidth_factor = (avg_bandwidth / 10.0)
            .max(1.0)
            .min(8.0);
        let peer_factor = (peer_count as f64 / 5.0)
            .max(0.5)
            .min(2.0);
        
        let optimal_size = (base_size as f64 * latency_factor * bandwidth_factor * peer_factor) as usize;
        
        Ok(optimal_size.max(self.config.batch_size_min).min(self.config.batch_size_max))
    }
    
    async fn select_best_peers(&self, count: usize) -> Result<Vec<PeerId>, SyncError> {
        let mut scored_peers: Vec<_> = self.active_peers
            .values()
            .filter(|peer| peer.score > self.config.peer_score_threshold)
            .map(|peer| (peer.peer_id.clone(), peer.score))
            .collect();
        
        scored_peers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        Ok(scored_peers
            .into_iter()
            .take(count)
            .map(|(id, _)| id)
            .collect())
    }
    
    async fn create_checkpoint(&mut self) -> Result<(), SyncError> {
        let current_block = self.chain_actor
            .send(GetBlock { height: self.sync_progress.current_height })
            .await??;
        
        let checkpoint = BlockCheckpoint {
            height: self.sync_progress.current_height,
            hash: current_block.hash(),
            parent_hash: current_block.parent_hash,
            state_root: current_block.state_root,
            timestamp: Utc::now(),
            sync_progress: self.sync_progress.clone(),
            verified: true,
        };
        
        self.checkpoint_manager.create(checkpoint.clone()).await?;
        self.sync_progress.last_checkpoint = Some(checkpoint);
        
        self.metrics.checkpoints_created.inc();
        
        info!("Created checkpoint at height {}", self.sync_progress.current_height);
        
        Ok(())
    }
    
    async fn recover_from_checkpoint(&mut self, checkpoint: BlockCheckpoint) -> Result<(), SyncError> {
        info!("Recovering from checkpoint at height {}", checkpoint.height);
        
        // Restore sync progress
        self.sync_progress = checkpoint.sync_progress;
        
        // Verify checkpoint block exists in chain
        let block_exists = self.chain_actor
            .send(HasBlock { hash: checkpoint.hash })
            .await??;
        
        if !block_exists {
            // Need to sync from before checkpoint
            self.sync_progress.current_height = checkpoint.height.saturating_sub(100);
            warn!("Checkpoint block not found, starting from height {}", 
                self.sync_progress.current_height);
        }
        
        Ok(())
    }
}
```

3. **Implement Parallel Block Processor**
```rust
// src/actors/sync/processor.rs

use actix::prelude::*;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct BlockProcessorActor {
    workers: Vec<Addr<ValidationWorker>>,
    validation_queue: VecDeque<ValidationTask>,
    execution_queue: VecDeque<ExecutionTask>,
    results: HashMap<Hash256, ValidationResult>,
    config: ProcessorConfig,
}

pub struct ValidationWorker {
    id: usize,
    aura: Arc<AuraConsensus>,
    federation: Arc<Federation>,
}

#[derive(Message)]
#[rtype(result = "Result<ProcessingResult, SyncError>")]
pub struct ProcessBlockBatch {
    pub blocks: Vec<SignedConsensusBlock>,
}

#[derive(Debug, Clone)]
pub struct ProcessingResult {
    pub processed: usize,
    pub failed: usize,
    pub validated_blocks: Vec<SignedConsensusBlock>,
}

impl BlockProcessorActor {
    pub fn new(config: ProcessorConfig) -> Self {
        let workers = (0..config.worker_count)
            .map(|id| {
                ValidationWorker::new(id, config.aura.clone(), config.federation.clone())
                    .start()
            })
            .collect();
        
        Self {
            workers,
            validation_queue: VecDeque::new(),
            execution_queue: VecDeque::new(),
            results: HashMap::new(),
            config,
        }
    }
}

impl Handler<ProcessBlockBatch> for BlockProcessorActor {
    type Result = ResponseActFuture<Self, Result<ProcessingResult, SyncError>>;
    
    fn handle(&mut self, msg: ProcessBlockBatch, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            let start = Instant::now();
            
            // Stage 1: Parallel signature validation
            let validation_futures: Vec<_> = msg.blocks
                .iter()
                .enumerate()
                .map(|(i, block)| {
                    let worker = &self.workers[i % self.workers.len()];
                    worker.send(ValidateBlock(block.clone()))
                })
                .collect();
            
            let validation_results = futures::future::join_all(validation_futures).await;
            
            // Stage 2: Parallel parent verification
            let mut valid_blocks = Vec::new();
            let mut failed_count = 0;
            
            for (block, result) in msg.blocks.iter().zip(validation_results) {
                match result {
                    Ok(Ok(valid)) if valid => {
                        valid_blocks.push(block.clone());
                    }
                    _ => {
                        failed_count += 1;
                        self.metrics.validation_failures.inc();
                    }
                }
            }
            
            // Stage 3: Order blocks by height for sequential import
            valid_blocks.sort_by_key(|b| b.message.height());
            
            self.metrics.blocks_validated.add(valid_blocks.len() as i64);
            self.metrics.validation_time.observe(start.elapsed().as_secs_f64());
            
            Ok(ProcessingResult {
                processed: valid_blocks.len(),
                failed: failed_count,
                validated_blocks: valid_blocks,
            })
        }.into_actor(self))
    }
}

impl ValidationWorker {
    async fn validate_block(&self, block: &SignedConsensusBlock) -> Result<bool, SyncError> {
        // Validate block structure
        if block.message.slot == 0 {
            return Ok(false);
        }
        
        // Validate signature
        let expected_producer = self.aura.get_slot_producer(block.message.slot)?;
        if block.message.producer != expected_producer {
            return Ok(false);
        }
        
        if !self.aura.verify_signature(block)? {
            return Ok(false);
        }
        
        // Additional validation...
        
        Ok(true)
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
    async fn test_sync_from_genesis() {
        let sync_actor = create_test_sync_actor().await;
        
        sync_actor.send(StartSync {
            from_height: Some(0),
            target_height: Some(1000),
            checkpoint: None,
        }).await.unwrap().unwrap();
        
        // Wait for completion
        tokio::time::sleep(Duration::from_secs(10)).await;
        
        let status = sync_actor.send(GetSyncStatus).await.unwrap().unwrap();
        assert_eq!(status.current_height, 1000);
        assert!(matches!(status.state, SyncState::Synced { .. }));
    }
    
    #[actix::test]
    async fn test_checkpoint_recovery() {
        let sync_actor = create_test_sync_actor().await;
        
        // Create checkpoint at height 500
        let checkpoint = create_test_checkpoint(500);
        
        sync_actor.send(StartSync {
            from_height: None,
            target_height: Some(1000),
            checkpoint: Some(checkpoint),
        }).await.unwrap().unwrap();
        
        // Should start from checkpoint
        let status = sync_actor.send(GetSyncStatus).await.unwrap().unwrap();
        assert!(status.current_height >= 500);
    }
    
    #[actix::test]
    async fn test_parallel_download() {
        let sync_actor = create_test_sync_actor().await;
        
        // Measure time with parallel downloads
        let start = Instant::now();
        
        sync_actor.send(StartSync {
            from_height: Some(0),
            target_height: Some(1000),
            checkpoint: None,
        }).await.unwrap().unwrap();
        
        let parallel_time = start.elapsed();
        
        // Should be significantly faster than sequential
        assert!(parallel_time < Duration::from_secs(5));
    }
    
    #[actix::test]
    async fn test_can_produce_blocks() {
        let sync_actor = create_test_sync_actor().await;
        
        // Start sync
        sync_actor.send(StartSync {
            from_height: Some(0),
            target_height: Some(1000),
            checkpoint: None,
        }).await.unwrap().unwrap();
        
        // Check production capability at different sync levels
        for height in [0, 500, 990, 995, 1000] {
            // Simulate sync progress
            set_sync_height(&sync_actor, height).await;
            
            let can_produce = sync_actor.send(CanProduceBlocks)
                .await.unwrap().unwrap();
            
            if height >= 995 {
                assert!(can_produce, "Should produce at {}% sync", height * 100 / 1000);
            } else {
                assert!(!can_produce, "Should not produce at {}% sync", height * 100 / 1000);
            }
        }
    }
}
```

### Integration Tests
1. Test with real network conditions
2. Test network partition recovery
3. Test peer disconnection handling
4. Test checkpoint creation and recovery
5. Test with slow/malicious peers

### Performance Tests
```rust
#[bench]
fn bench_parallel_validation(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let processor = runtime.block_on(create_test_processor());
    
    let blocks = (0..1000)
        .map(|i| create_test_block(i))
        .collect();
    
    b.iter(|| {
        runtime.block_on(async {
            processor.send(ProcessBlockBatch { blocks: blocks.clone() })
                .await.unwrap().unwrap()
        })
    });
}
```

## Subtasks

### Phase 1: Foundation and Core Architecture (2 days)

#### ALYS-010-1: Design SyncActor Message Protocol and State Machine
**Priority**: Highest  
**Effort**: 4 hours  
**Dependencies**: ALYS-006 (Actor supervisor)

**Implementation Steps**:
1. **Test-First Design**:
   - Write failing tests for message handling (`test_sync_messages.rs`)
   - Define expected behavior for each message type
   - Test state transitions and validation

2. **Core Implementation**:
   - Create `messages.rs` with comprehensive message types
   - Implement `SyncState` enum with detailed state tracking
   - Design `SyncStatus` and `SyncProgress` structures
   - Add message validation and error handling

3. **Acceptance Criteria**:
   - [ ] All message types defined with proper Actix Message derive
   - [ ] State machine transitions tested and documented
   - [ ] Message validation prevents invalid state changes
   - [ ] Error types cover all failure scenarios
   - [ ] Unit tests achieve >95% coverage

#### ALYS-010-2: Implement SyncActor Core Structure and Lifecycle
**Priority**: High  
**Effort**: 6 hours  
**Dependencies**: ALYS-010-1

**Implementation Steps**:
1. **TDD Approach**:
   - Write tests for actor lifecycle (`test_sync_lifecycle.rs`)
   - Test actor startup, shutdown, and restart scenarios
   - Mock external dependencies (ChainActor, PeerManager)

2. **Core Implementation**:
   - Implement `SyncActor` struct with all required fields
   - Add actor lifecycle methods (`started`, `stopped`)
   - Create periodic tasks (metrics, checkpoints)
   - Implement basic message handlers

3. **Acceptance Criteria**:
   - [ ] Actor starts and stops cleanly
   - [ ] Periodic tasks execute correctly
   - [ ] External actor addresses properly managed
   - [ ] Memory usage remains bounded
   - [ ] Integration tests with actor system pass

#### ALYS-010-3: Implement Configuration and Metrics System
**Priority**: Medium  
**Effort**: 4 hours  
**Dependencies**: ALYS-010-2

**Implementation Steps**:
1. **Configuration Design**:
   - Write tests for configuration validation
   - Test different environment configs (dev, test, prod)
   - Validate configuration parameter ranges

2. **Metrics Implementation**:
   - Create comprehensive Prometheus metrics
   - Test metric collection and updates
   - Implement metric aggregation logic

3. **Acceptance Criteria**:
   - [ ] Configuration validation prevents invalid settings
   - [ ] All metrics properly registered with Prometheus
   - [ ] Metric values accurately reflect sync state
   - [ ] Configuration hot-reloading supported
   - [ ] Performance impact of metrics < 1%

### Phase 2: Peer Management and Network Layer (1.5 days)

#### ALYS-010-4: Implement Intelligent Peer Selection and Scoring
**Priority**: High  
**Effort**: 5 hours  
**Dependencies**: ALYS-010-2

**Implementation Steps**:
1. **Test-Driven Design**:
   - Write tests for peer scoring algorithms (`test_peer_scoring.rs`)
   - Test peer selection under various network conditions
   - Mock different peer behaviors (fast, slow, malicious)

2. **Implementation**:
   - Create `PeerSyncInfo` with comprehensive scoring
   - Implement adaptive peer selection algorithms
   - Add peer performance tracking
   - Implement peer blacklisting and recovery

3. **Acceptance Criteria**:
   - [ ] Peer scores accurately reflect performance
   - [ ] Best peers selected for critical operations
   - [ ] Malicious peers quickly identified and excluded
   - [ ] Peer selection adapts to changing conditions
   - [ ] Property-based tests verify scoring invariants

#### ALYS-010-5: Implement Adaptive Batch Size Calculation
**Priority**: Medium  
**Effort**: 3 hours  
**Dependencies**: ALYS-010-4

**Implementation Steps**:
1. **Algorithm Testing**:
   - Test batch size adaptation under different network conditions
   - Verify optimal batch sizes for various scenarios
   - Test edge cases (very slow/fast networks)

2. **Implementation**:
   - Create network condition assessment methods
   - Implement adaptive batch size algorithm
   - Add bandwidth and latency estimation
   - Implement batch size bounds checking

3. **Acceptance Criteria**:
   - [ ] Batch size adapts to network conditions
   - [ ] Performance improves with optimal batch sizes
   - [ ] Batch size stays within configured bounds
   - [ ] Algorithm handles edge cases gracefully
   - [ ] Benchmarks show >20% improvement in throughput

### Phase 3: Block Processing and Validation (1.5 days)

#### ALYS-010-6: Implement Parallel Block Validation System
**Priority**: Highest  
**Effort**: 6 hours  
**Dependencies**: ALYS-007 (ChainActor), ALYS-010-2

**Implementation Steps**:
1. **Parallel Architecture Design**:
   - Write tests for parallel validation (`test_parallel_validation.rs`)
   - Test validation worker pool management
   - Verify parallel processing maintains order

2. **Implementation**:
   - Create `BlockProcessorActor` with worker pool
   - Implement `ValidationWorker` actors
   - Add parallel validation pipeline
   - Implement result aggregation and ordering

3. **Acceptance Criteria**:
   - [ ] Validation scales with CPU cores
   - [ ] Block order preserved during parallel processing
   - [ ] Validation errors properly handled and reported
   - [ ] Worker failures don't block entire pipeline
   - [ ] Performance tests show >3x speedup with 4+ cores

#### ALYS-010-7: Implement Block Download and Processing Pipeline
**Priority**: High  
**Effort**: 5 hours  
**Dependencies**: ALYS-010-6

**Implementation Steps**:
1. **Pipeline Testing**:
   - Write integration tests for download pipeline
   - Test error handling and retry mechanisms
   - Mock various network failure scenarios

2. **Implementation**:
   - Create parallel block download system
   - Implement download coordination and scheduling
   - Add progress tracking and reporting
   - Implement error recovery and peer fallback

3. **Acceptance Criteria**:
   - [ ] Multiple peers used simultaneously for downloads
   - [ ] Failed downloads automatically retried with different peers
   - [ ] Download progress accurately tracked and reported
   - [ ] Pipeline handles peer disconnections gracefully
   - [ ] Stress tests handle 1000+ concurrent block requests

### Phase 4: Checkpoint System (1 day)

#### ALYS-010-8: Implement Checkpoint Creation and Management
**Priority**: High  
**Effort**: 4 hours  
**Dependencies**: ALYS-013 (StorageActor), ALYS-010-2

**Implementation Steps**:
1. **Checkpoint Design**:
   - Write tests for checkpoint creation and validation
   - Test checkpoint recovery scenarios
   - Verify checkpoint data integrity

2. **Implementation**:
   - Create `CheckpointManager` with storage integration
   - Implement periodic checkpoint creation
   - Add checkpoint verification and validation
   - Implement checkpoint cleanup and pruning

3. **Acceptance Criteria**:
   - [ ] Checkpoints created at regular intervals
   - [ ] Checkpoint data includes all necessary state
   - [ ] Old checkpoints automatically pruned
   - [ ] Checkpoint corruption detected and handled
   - [ ] Recovery from checkpoint faster than full sync

#### ALYS-010-9: Implement Checkpoint Recovery System
**Priority**: High  
**Effort**: 4 hours  
**Dependencies**: ALYS-010-8

**Implementation Steps**:
1. **Recovery Testing**:
   - Test recovery from various checkpoint states
   - Test recovery failure scenarios
   - Verify sync continues correctly after recovery

2. **Implementation**:
   - Implement checkpoint discovery and loading
   - Add checkpoint verification before recovery
   - Create fallback mechanisms for corrupted checkpoints
   - Implement progress tracking during recovery

3. **Acceptance Criteria**:
   - [ ] Automatic recovery from latest valid checkpoint
   - [ ] Recovery handles corrupted checkpoints gracefully
   - [ ] Progress tracking continues seamlessly after recovery
   - [ ] Recovery time proportional to blocks since checkpoint
   - [ ] Integration tests verify end-to-end recovery

### Phase 5: Advanced Features and Optimization (1 day)

#### ALYS-010-10: Implement 99.5% Sync Threshold for Block Production
**Priority**: Critical  
**Effort**: 3 hours  
**Dependencies**: ALYS-010-7, ALYS-007 (ChainActor)

**Implementation Steps**:
1. **Threshold Logic Testing**:
   - Write tests for production threshold calculation
   - Test edge cases around threshold boundary
   - Verify integration with block production system

2. **Implementation**:
   - Add sync progress calculation methods
   - Implement production eligibility checks
   - Create threshold monitoring and alerting
   - Add safety mechanisms to prevent premature production

3. **Acceptance Criteria**:
   - [ ] Block production enabled exactly at 99.5% sync
   - [ ] Threshold calculation accounts for network height changes
   - [ ] Safety mechanisms prevent production during sync issues
   - [ ] Monitoring alerts when threshold crossed
   - [ ] End-to-end tests verify production starts correctly

#### ALYS-010-11: Implement Network Partition Recovery
**Priority**: Medium  
**Effort**: 4 hours  
**Dependencies**: ALYS-010-4, ALYS-010-8

**Implementation Steps**:
1. **Partition Simulation**:
   - Write chaos engineering tests for network partitions
   - Test recovery from various partition scenarios
   - Simulate slow/intermittent network conditions

2. **Implementation**:
   - Add network condition detection
   - Implement adaptive retry mechanisms
   - Create partition recovery strategies
   - Add network health monitoring

3. **Acceptance Criteria**:
   - [ ] Automatic detection of network partitions
   - [ ] Recovery strategies adapt to partition type
   - [ ] Sync continues when network connectivity restored
   - [ ] No data corruption during partition events
   - [ ] Chaos engineering tests pass consistently

#### ALYS-010-12: Performance Optimization and Benchmarking
**Priority**: Medium  
**Effort**: 3 hours  
**Dependencies**: All previous subtasks

**Implementation Steps**:
1. **Performance Testing**:
   - Create comprehensive benchmark suite
   - Measure sync performance under various conditions
   - Compare performance to baseline implementation

2. **Optimization**:
   - Profile and optimize critical paths
   - Implement memory and CPU optimizations
   - Add performance monitoring and alerting

3. **Acceptance Criteria**:
   - [ ] Sync speed improved by >2x compared to baseline
   - [ ] Memory usage remains bounded during large syncs
   - [ ] CPU utilization efficiently distributed across cores
   - [ ] Benchmark results consistently meet performance targets
   - [ ] Performance regression tests integrated into CI

### Phase 6: Integration and Documentation (0.5 days)

#### ALYS-010-13: Integration Testing and System Validation
**Priority**: High  
**Effort**: 3 hours  
**Dependencies**: All implementation subtasks

**Implementation Steps**:
1. **End-to-End Testing**:
   - Create comprehensive integration test suite
   - Test interaction with all dependent actors
   - Validate system behavior under realistic conditions

2. **System Validation**:
   - Run extended sync tests with real network data
   - Validate all acceptance criteria
   - Perform security and stability testing

3. **Acceptance Criteria**:
   - [ ] All integration tests pass consistently
   - [ ] System handles realistic workloads
   - [ ] No memory leaks or resource exhaustion
   - [ ] All original acceptance criteria validated
   - [ ] Performance targets achieved in production environment

#### ALYS-010-14: Documentation and Knowledge Transfer
**Priority**: Medium  
**Effort**: 2 hours  
**Dependencies**: ALYS-010-13

**Implementation Steps**:
1. **Technical Documentation**:
   - Create architecture documentation
   - Document configuration options and tuning
   - Add troubleshooting guides

2. **Knowledge Transfer**:
   - Conduct code review sessions
   - Create operational runbooks
   - Update system architecture documentation

3. **Acceptance Criteria**:
   - [ ] Complete API documentation
   - [ ] Architecture diagrams updated
   - [ ] Configuration guide complete
   - [ ] Troubleshooting guide available
   - [ ] Team knowledge transfer sessions completed

### Testing Strategy by Phase

**Unit Testing**: Each subtask includes comprehensive unit tests with >90% coverage
**Integration Testing**: Cross-actor communication and workflow testing
**Performance Testing**: Benchmarks and performance regression prevention
**Chaos Engineering**: Network partition, peer failure, and resource exhaustion testing
**Property Testing**: Invariant verification using PropTest generators

### Quality Gates

1. **Code Review**: All code reviewed by senior team members
2. **Testing**: All tests pass with >90% coverage before merge
3. **Performance**: Benchmarks meet >2x improvement target
4. **Documentation**: Architecture and API docs complete
5. **Security**: Security review for network-facing components

## Dependencies

### Blockers
- ALYS-006: Actor supervisor
- ALYS-007: ChainActor for block import

### Blocked By
None

### Related Issues
- ALYS-011: PeerManagerActor
- ALYS-012: NetworkActor  
- ALYS-013: StorageActor for checkpoints

## Definition of Done

- [ ] SyncActor fully implemented
- [ ] Parallel validation working
- [ ] Checkpoint system operational
- [ ] 99.5% sync threshold for production
- [ ] Network partition recovery tested
- [ ] Performance improved >2x
- [ ] All tests passing
- [ ] Documentation complete
- [ ] Code review completed

## Notes

- Consider implementing snap sync for faster initial sync
- Consider adding support for light client sync
- Consider implementing state sync for even faster sync
- Consider pruning old checkpoints

## Next Steps

### Work Completed Analysis (80% Complete)

**Completed Components (✓):**
- Message protocol design with comprehensive sync operations (95% complete)
- Core SyncActor structure with state machine implementation (85% complete)
- Parallel block validation system with worker pools (80% complete)
- Block processing pipeline with download coordination (85% complete)
- Checkpoint system architecture with creation and recovery (75% complete)
- Advanced features including 99.5% sync threshold logic (70% complete)

**Detailed Work Analysis:**
1. **Message Protocol (95%)** - All message types defined including StartSync, PauseSync, ResumeSync, GetSyncStatus, CanProduceBlocks, ProcessBlockBatch, PeerDiscovered, PeerDisconnected, CreateCheckpoint, RecoverFromCheckpoint with proper state management
2. **Actor Structure (85%)** - Complete SyncActor with state machine, peer management, block processing, chain interaction, checkpoint management, configuration, and metrics
3. **Block Validation (80%)** - BlockProcessorActor with parallel validation workers, processing pipeline, and result aggregation
4. **Block Processing (85%)** - Parallel download system, batch processing, adaptive sizing, and peer selection algorithms
5. **Checkpoint System (75%)** - CheckpointManager with creation, recovery, validation, and pruning capabilities
6. **Advanced Features (70%)** - 99.5% sync threshold, network partition recovery, and performance optimizations

### Remaining Work Analysis

**Missing Critical Components:**
- Production error handling and resilience patterns for network failures (35% complete)
- Advanced peer management with reputation scoring and adaptive selection (40% complete)
- Comprehensive monitoring and alerting system (30% complete)
- Network partition detection and recovery mechanisms (25% complete)
- Performance optimization and memory management (20% complete)
- Integration testing with real network conditions (15% complete)

### Detailed Next Step Plans

#### Priority 1: Complete Production-Ready SyncActor

**Plan:** Implement comprehensive error handling, advanced peer management, and robust network partition recovery for the SyncActor.

**Implementation 1: Advanced Error Handling and Network Resilience**
```rust
// src/actors/sync/error_handling.rs
use actix::prelude::*;
use std::time::{Duration, Instant};
use std::collections::HashMap;

#[derive(Debug)]
pub struct SyncErrorHandler {
    // Error recovery strategies
    recovery_strategies: HashMap<SyncErrorType, RecoveryStrategy>,
    // Network condition monitoring
    network_monitor: NetworkMonitor,
    // Circuit breakers for external services
    circuit_breakers: HashMap<String, CircuitBreaker>,
    // Retry policies
    retry_policies: HashMap<OperationType, RetryPolicy>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum SyncErrorType {
    // Network errors
    NetworkPartition,
    PeerTimeout,
    ConnectionLost,
    HighLatency,
    
    // Block errors
    InvalidBlock,
    ValidationFailure,
    DownloadFailure,
    ProcessingFailure,
    
    // State errors
    CheckpointCorrupted,
    StateInconsistency,
    ChainReorganization,
    
    // Resource errors
    OutOfMemory,
    StorageFailure,
    CapacityExceeded,
}

#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    Retry { max_attempts: u32, backoff: Duration },
    Fallback { alternative_action: String },
    Checkpoint { restore_from: u64 },
    Reset { full_restart: bool },
    Escalate { to_supervisor: bool },
}

#[derive(Debug)]
pub struct NetworkMonitor {
    // Network health metrics
    latency_samples: VecDeque<Duration>,
    bandwidth_samples: VecDeque<f64>,
    packet_loss_rate: f64,
    partition_detected: bool,
    last_successful_operation: Instant,
    
    // Peer connectivity
    connected_peers: HashSet<PeerId>,
    failed_peers: HashSet<PeerId>,
    peer_health_scores: HashMap<PeerId, f64>,
}

impl SyncErrorHandler {
    pub fn new() -> Self {
        let mut recovery_strategies = HashMap::new();
        
        // Network partition recovery
        recovery_strategies.insert(SyncErrorType::NetworkPartition, RecoveryStrategy::Checkpoint {
            restore_from: 0, // Will be calculated dynamically
        });
        
        // Peer timeout recovery
        recovery_strategies.insert(SyncErrorType::PeerTimeout, RecoveryStrategy::Fallback {
            alternative_action: "switch_to_backup_peers".to_string(),
        });
        
        // Block validation failure recovery
        recovery_strategies.insert(SyncErrorType::ValidationFailure, RecoveryStrategy::Retry {
            max_attempts: 3,
            backoff: Duration::from_secs(5),
        });
        
        // Storage failure recovery
        recovery_strategies.insert(SyncErrorType::StorageFailure, RecoveryStrategy::Reset {
            full_restart: true,
        });
        
        Self {
            recovery_strategies,
            network_monitor: NetworkMonitor::new(),
            circuit_breakers: HashMap::new(),
            retry_policies: HashMap::new(),
        }
    }

    pub async fn handle_sync_error(
        &mut self,
        error: SyncError,
        context: &str,
    ) -> Result<RecoveryAction, SyncError> {
        let error_type = self.classify_error(&error);
        
        // Update network monitor
        self.network_monitor.record_error(&error_type);
        
        // Check circuit breakers
        if let Some(cb) = self.circuit_breakers.get_mut(context) {
            if cb.is_open() {
                return Ok(RecoveryAction::WaitForRecovery(Duration::from_secs(30)));
            }
            cb.record_failure();
        }

        // Get recovery strategy
        let strategy = self.recovery_strategies.get(&error_type)
            .cloned()
            .unwrap_or(RecoveryStrategy::Escalate { to_supervisor: true });

        match strategy {
            RecoveryStrategy::Retry { max_attempts, backoff } => {
                self.execute_retry_recovery(error_type, max_attempts, backoff).await
            }
            
            RecoveryStrategy::Fallback { alternative_action } => {
                self.execute_fallback_recovery(alternative_action).await
            }
            
            RecoveryStrategy::Checkpoint { restore_from } => {
                let checkpoint_height = if restore_from == 0 {
                    self.calculate_safe_checkpoint_height().await?
                } else {
                    restore_from
                };
                Ok(RecoveryAction::RestoreFromCheckpoint(checkpoint_height))
            }
            
            RecoveryStrategy::Reset { full_restart } => {
                if full_restart {
                    Ok(RecoveryAction::FullRestart)
                } else {
                    Ok(RecoveryAction::SoftReset)
                }
            }
            
            RecoveryStrategy::Escalate { to_supervisor } => {
                if to_supervisor {
                    Ok(RecoveryAction::EscalateToSupervisor(error))
                } else {
                    Ok(RecoveryAction::ManualIntervention)
                }
            }
        }
    }

    async fn execute_retry_recovery(
        &mut self,
        error_type: SyncErrorType,
        max_attempts: u32,
        backoff: Duration,
    ) -> Result<RecoveryAction, SyncError> {
        // Implement exponential backoff with jitter
        let jitter = Duration::from_millis(rand::random::<u64>() % 1000);
        let delay = backoff + jitter;
        
        Ok(RecoveryAction::RetryAfterDelay {
            delay,
            max_attempts,
            error_type,
        })
    }

    async fn execute_fallback_recovery(
        &mut self,
        alternative_action: String,
    ) -> Result<RecoveryAction, SyncError> {
        match alternative_action.as_str() {
            "switch_to_backup_peers" => {
                let backup_peers = self.select_backup_peers().await?;
                Ok(RecoveryAction::SwitchToPeers(backup_peers))
            }
            
            "reduce_batch_size" => {
                Ok(RecoveryAction::AdjustBatchSize(0.5)) // Reduce by 50%
            }
            
            "increase_timeout" => {
                Ok(RecoveryAction::AdjustTimeout(Duration::from_secs(60)))
            }
            
            _ => {
                warn!("Unknown fallback action: {}", alternative_action);
                Ok(RecoveryAction::ManualIntervention)
            }
        }
    }

    fn classify_error(&self, error: &SyncError) -> SyncErrorType {
        match error {
            SyncError::NetworkTimeout => {
                if self.network_monitor.is_partition_detected() {
                    SyncErrorType::NetworkPartition
                } else {
                    SyncErrorType::PeerTimeout
                }
            }
            
            SyncError::BlockValidationFailed(_) => SyncErrorType::ValidationFailure,
            SyncError::BlockDownloadFailed(_) => SyncErrorType::DownloadFailure,
            SyncError::CheckpointCorrupted => SyncErrorType::CheckpointCorrupted,
            SyncError::StorageError(_) => SyncErrorType::StorageFailure,
            SyncError::OutOfMemory => SyncErrorType::OutOfMemory,
            
            _ => SyncErrorType::ProcessingFailure,
        }
    }

    async fn select_backup_peers(&self) -> Result<Vec<PeerId>, SyncError> {
        // Select peers with highest health scores that aren't in failed set
        let mut healthy_peers: Vec<_> = self.network_monitor.peer_health_scores
            .iter()
            .filter(|(peer_id, score)| {
                **score > 0.7 && // High health score
                !self.network_monitor.failed_peers.contains(peer_id) &&
                self.network_monitor.connected_peers.contains(peer_id)
            })
            .map(|(peer_id, score)| (peer_id.clone(), *score))
            .collect();

        healthy_peers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        Ok(healthy_peers.into_iter()
            .take(5) // Max 5 backup peers
            .map(|(peer_id, _)| peer_id)
            .collect())
    }

    async fn calculate_safe_checkpoint_height(&self) -> Result<u64, SyncError> {
        // Find the most recent checkpoint that's guaranteed to be safe
        // This should be a checkpoint that's well behind the current tip
        // to avoid potential reorganizations
        
        let current_height = self.network_monitor.get_current_sync_height().await?;
        let safety_margin = 100; // 100 blocks safety margin
        
        Ok(current_height.saturating_sub(safety_margin))
    }
}

impl NetworkMonitor {
    pub fn new() -> Self {
        Self {
            latency_samples: VecDeque::with_capacity(100),
            bandwidth_samples: VecDeque::with_capacity(100),
            packet_loss_rate: 0.0,
            partition_detected: false,
            last_successful_operation: Instant::now(),
            connected_peers: HashSet::new(),
            failed_peers: HashSet::new(),
            peer_health_scores: HashMap::new(),
        }
    }

    pub fn record_latency(&mut self, latency: Duration) {
        if self.latency_samples.len() >= 100 {
            self.latency_samples.pop_front();
        }
        self.latency_samples.push_back(latency);
        
        // Detect high latency conditions
        let avg_latency = self.average_latency();
        if avg_latency > Duration::from_secs(5) {
            warn!("High latency detected: {:?}", avg_latency);
        }
    }

    pub fn record_peer_response(&mut self, peer_id: PeerId, success: bool, latency: Duration) {
        if success {
            self.connected_peers.insert(peer_id.clone());
            self.failed_peers.remove(&peer_id);
            self.last_successful_operation = Instant::now();
            
            // Update peer health score
            let current_score = self.peer_health_scores.get(&peer_id).unwrap_or(&0.5);
            let new_score = (current_score * 0.9 + 0.1).min(1.0); // Increase score
            self.peer_health_scores.insert(peer_id, new_score);
            
            self.record_latency(latency);
        } else {
            self.failed_peers.insert(peer_id.clone());
            
            // Decrease peer health score
            let current_score = self.peer_health_scores.get(&peer_id).unwrap_or(&0.5);
            let new_score = (current_score * 0.9).max(0.0); // Decrease score
            self.peer_health_scores.insert(peer_id, new_score);
        }
        
        // Update partition detection
        self.update_partition_detection();
    }

    fn update_partition_detection(&mut self) {
        let time_since_success = self.last_successful_operation.elapsed();
        let failed_ratio = self.failed_peers.len() as f64 / 
            (self.connected_peers.len() + self.failed_peers.len()) as f64;
        
        // Detect partition if:
        // 1. No successful operations for >60 seconds
        // 2. More than 70% of peers have failed
        // 3. Average latency is extremely high
        
        let partition_indicators = [
            time_since_success > Duration::from_secs(60),
            failed_ratio > 0.7,
            self.average_latency() > Duration::from_secs(10),
        ];
        
        let partition_score = partition_indicators.iter()
            .map(|&indicator| if indicator { 1.0 } else { 0.0 })
            .sum::<f64>() / partition_indicators.len() as f64;
        
        self.partition_detected = partition_score > 0.6; // 60% confidence threshold
        
        if self.partition_detected {
            warn!("Network partition detected! Score: {:.2}", partition_score);
        }
    }

    pub fn is_partition_detected(&self) -> bool {
        self.partition_detected
    }

    pub fn average_latency(&self) -> Duration {
        if self.latency_samples.is_empty() {
            Duration::from_millis(100) // Default assumption
        } else {
            let total: u64 = self.latency_samples.iter().map(|d| d.as_millis() as u64).sum();
            Duration::from_millis(total / self.latency_samples.len() as u64)
        }
    }

    pub fn record_error(&mut self, error_type: &SyncErrorType) {
        match error_type {
            SyncErrorType::NetworkPartition => {
                self.partition_detected = true;
            }
            SyncErrorType::PeerTimeout | SyncErrorType::ConnectionLost => {
                // These will be handled by record_peer_response
            }
            _ => {
                // Other errors don't directly affect network monitoring
            }
        }
    }

    pub async fn get_current_sync_height(&self) -> Result<u64, SyncError> {
        // This would query the current sync state
        // For now, return a placeholder
        Ok(1000) // Would be implemented with actual sync state query
    }
}

#[derive(Debug, Clone)]
pub enum RecoveryAction {
    RetryAfterDelay {
        delay: Duration,
        max_attempts: u32,
        error_type: SyncErrorType,
    },
    SwitchToPeers(Vec<PeerId>),
    AdjustBatchSize(f64), // Multiplier
    AdjustTimeout(Duration),
    RestoreFromCheckpoint(u64),
    WaitForRecovery(Duration),
    FullRestart,
    SoftReset,
    EscalateToSupervisor(SyncError),
    ManualIntervention,
}

// Enhanced SyncActor with error handling
impl SyncActor {
    pub async fn handle_error_with_recovery(
        &mut self,
        error: SyncError,
        context: &str,
    ) -> Result<(), SyncError> {
        let recovery_action = self.error_handler.handle_sync_error(error.clone(), context).await?;
        
        match recovery_action {
            RecoveryAction::RetryAfterDelay { delay, max_attempts, error_type } => {
                info!("Retrying operation after {:?} (max {} attempts)", delay, max_attempts);
                tokio::time::sleep(delay).await;
                // The actual retry would be handled by the calling code
                Ok(())
            }
            
            RecoveryAction::SwitchToPeers(new_peers) => {
                info!("Switching to backup peers: {} peers", new_peers.len());
                self.switch_to_peers(new_peers).await
            }
            
            RecoveryAction::AdjustBatchSize(multiplier) => {
                let old_size = self.current_batch_size;
                self.current_batch_size = ((old_size as f64) * multiplier).max(1.0) as usize;
                info!("Adjusted batch size from {} to {}", old_size, self.current_batch_size);
                Ok(())
            }
            
            RecoveryAction::RestoreFromCheckpoint(height) => {
                info!("Restoring from checkpoint at height {}", height);
                self.restore_from_checkpoint_height(height).await
            }
            
            RecoveryAction::FullRestart => {
                warn!("Performing full sync restart due to unrecoverable error");
                self.restart_sync().await
            }
            
            RecoveryAction::EscalateToSupervisor(error) => {
                error!("Escalating error to supervisor: {:?}", error);
                // This would send a message to the supervisor actor
                Err(error)
            }
            
            _ => {
                warn!("Recovery action not fully implemented: {:?}", recovery_action);
                Ok(())
            }
        }
    }

    async fn switch_to_peers(&mut self, new_peers: Vec<PeerId>) -> Result<(), SyncError> {
        // Clear current peer assignments
        for peer_info in self.active_peers.values_mut() {
            peer_info.score *= 0.5; // Reduce score of current peers
        }
        
        // Add new peers with high initial scores
        for peer_id in new_peers {
            self.active_peers.insert(peer_id.clone(), PeerSyncInfo {
                peer_id: peer_id.clone(),
                reported_height: 0, // Will be updated when peer responds
                last_response: Instant::now(),
                blocks_served: 0,
                average_latency: Duration::from_millis(100),
                error_count: 0,
                score: 0.8, // High initial score for backup peers
            });
        }
        
        Ok(())
    }

    async fn restore_from_checkpoint_height(&mut self, height: u64) -> Result<(), SyncError> {
        // Find checkpoint at or before the specified height
        let checkpoint = self.checkpoint_manager.find_at_height(height).await?
            .ok_or(SyncError::CheckpointNotFound)?;
        
        // Reset sync state
        self.sync_progress.current_height = checkpoint.height;
        self.sync_progress.target_height = self.get_network_height().await?;
        
        // Clear any in-progress operations
        self.block_buffer.clear();
        
        // Restart sync from checkpoint
        self.state = SyncState::DownloadingBlocks {
            start: checkpoint.height,
            current: checkpoint.height,
            target: self.sync_progress.target_height,
            batch_size: self.config.batch_size_min,
        };
        
        info!("Restored sync from checkpoint at height {}", checkpoint.height);
        Ok(())
    }

    async fn restart_sync(&mut self) -> Result<(), SyncError> {
        warn!("Performing full sync restart");
        
        // Reset all state
        self.sync_progress = SyncProgress::default();
        self.active_peers.clear();
        self.block_buffer.clear();
        
        // Find latest checkpoint
        if let Some(checkpoint) = self.checkpoint_manager.find_latest() {
            self.sync_progress.current_height = checkpoint.height;
        } else {
            self.sync_progress.current_height = 0;
        }
        
        // Get target height
        self.sync_progress.target_height = self.get_network_height().await?;
        
        // Start fresh discovery
        self.state = SyncState::Discovering {
            started_at: Instant::now(),
            attempts: 0,
        };
        
        info!("Sync restarted from height {}", self.sync_progress.current_height);
        Ok(())
    }
}
```

**Implementation 2: Advanced Peer Management and Reputation System**
```rust
// src/actors/sync/peer_manager.rs
use actix::prelude::*;
use std::collections::{HashMap, BTreeMap};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct AdvancedPeerManager {
    // Peer reputation system
    peer_reputations: HashMap<PeerId, PeerReputation>,
    
    // Performance tracking
    peer_performance: HashMap<PeerId, PeerPerformance>,
    
    // Peer selection strategies
    selection_strategy: PeerSelectionStrategy,
    
    // Bandwidth management
    bandwidth_allocator: BandwidthAllocator,
    
    // Connection management
    connection_manager: ConnectionManager,
    
    // Configuration
    config: PeerManagerConfig,
}

#[derive(Debug, Clone)]
pub struct PeerReputation {
    pub peer_id: PeerId,
    pub trust_score: f64,          // 0.0 - 1.0
    pub reliability_score: f64,    // 0.0 - 1.0
    pub performance_score: f64,    // 0.0 - 1.0
    pub behavior_score: f64,       // 0.0 - 1.0
    pub overall_score: f64,        // Weighted average
    pub last_updated: Instant,
    pub interactions: u64,
    pub blacklisted: bool,
    pub blacklist_until: Option<Instant>,
}

#[derive(Debug, Clone)]
pub struct PeerPerformance {
    pub peer_id: PeerId,
    pub average_latency: Duration,
    pub bandwidth_estimate: f64,   // MB/s
    pub success_rate: f64,         // 0.0 - 1.0
    pub blocks_served: u64,
    pub bytes_transferred: u64,
    pub error_count: u32,
    pub consecutive_failures: u32,
    pub last_response: Instant,
    pub response_time_history: VecDeque<Duration>,
}

#[derive(Debug, Clone)]
pub enum PeerSelectionStrategy {
    HighestReputation,
    PerformanceBased,
    Diversified { max_per_region: usize },
    Adaptive { learning_rate: f64 },
    LoadBalanced { target_utilization: f64 },
}

#[derive(Debug)]
pub struct BandwidthAllocator {
    total_bandwidth: f64,
    peer_allocations: HashMap<PeerId, f64>,
    allocation_strategy: AllocationStrategy,
    utilization_tracker: UtilizationTracker,
}

#[derive(Debug)]
pub enum AllocationStrategy {
    EqualShare,
    PerformanceBased,
    PriorityBased { priority_levels: Vec<f64> },
    Dynamic { adjustment_factor: f64 },
}

impl AdvancedPeerManager {
    pub fn new(config: PeerManagerConfig) -> Self {
        Self {
            peer_reputations: HashMap::new(),
            peer_performance: HashMap::new(),
            selection_strategy: PeerSelectionStrategy::Adaptive { learning_rate: 0.1 },
            bandwidth_allocator: BandwidthAllocator::new(config.total_bandwidth_mb),
            connection_manager: ConnectionManager::new(config.max_connections),
            config,
        }
    }

    pub async fn select_optimal_peers(
        &mut self,
        count: usize,
        operation_type: OperationType,
    ) -> Result<Vec<PeerId>, PeerManagerError> {
        // Update peer scores before selection
        self.update_all_peer_scores().await?;
        
        match &self.selection_strategy {
            PeerSelectionStrategy::HighestReputation => {
                self.select_by_reputation(count).await
            }
            
            PeerSelectionStrategy::PerformanceBased => {
                self.select_by_performance(count, operation_type).await
            }
            
            PeerSelectionStrategy::Diversified { max_per_region } => {
                self.select_diversified(count, *max_per_region).await
            }
            
            PeerSelectionStrategy::Adaptive { learning_rate } => {
                self.select_adaptive(count, *learning_rate, operation_type).await
            }
            
            PeerSelectionStrategy::LoadBalanced { target_utilization } => {
                self.select_load_balanced(count, *target_utilization).await
            }
        }
    }

    async fn select_adaptive(
        &mut self,
        count: usize,
        learning_rate: f64,
        operation_type: OperationType,
    ) -> Result<Vec<PeerId>, PeerManagerError> {
        // Adaptive selection uses reinforcement learning principles
        // to continuously improve peer selection based on outcomes
        
        let mut candidates: Vec<_> = self.peer_reputations
            .values()
            .filter(|rep| !rep.blacklisted && self.is_peer_available(&rep.peer_id))
            .collect();

        // Sort by adaptive score (combination of historical performance and exploration)
        candidates.sort_by(|a, b| {
            let score_a = self.calculate_adaptive_score(a, learning_rate, operation_type);
            let score_b = self.calculate_adaptive_score(b, learning_rate, operation_type);
            score_b.partial_cmp(&score_a).unwrap()
        });

        let selected: Vec<PeerId> = candidates
            .into_iter()
            .take(count)
            .map(|rep| rep.peer_id.clone())
            .collect();

        // Update selection history for learning
        for peer_id in &selected {
            self.record_peer_selection(peer_id.clone(), operation_type);
        }

        Ok(selected)
    }

    fn calculate_adaptive_score(
        &self,
        reputation: &PeerReputation,
        learning_rate: f64,
        operation_type: OperationType,
    ) -> f64 {
        // Exploitation: Use known performance
        let exploitation_score = reputation.overall_score;
        
        // Exploration: Encourage trying less-tested peers
        let exploration_bonus = if reputation.interactions < 10 {
            0.1 / (reputation.interactions as f64 + 1.0) // Higher bonus for fewer interactions
        } else {
            0.0
        };
        
        // Operation-specific weighting
        let operation_weight = self.get_operation_weight(&reputation.peer_id, operation_type);
        
        // Recency factor: Prefer recently responsive peers
        let recency_factor = {
            let time_since_update = reputation.last_updated.elapsed().as_secs() as f64;
            (-time_since_update / 3600.0).exp() // Exponential decay over 1 hour
        };
        
        // Combined adaptive score
        let base_score = exploitation_score * operation_weight * recency_factor;
        let final_score = base_score + (exploration_bonus * learning_rate);
        
        final_score.min(1.0).max(0.0)
    }

    fn get_operation_weight(&self, peer_id: &PeerId, operation_type: OperationType) -> f64 {
        if let Some(performance) = self.peer_performance.get(peer_id) {
            match operation_type {
                OperationType::HeaderDownload => {
                    // Prioritize low latency for headers
                    if performance.average_latency < Duration::from_millis(100) {
                        1.2
                    } else if performance.average_latency < Duration::from_millis(500) {
                        1.0
                    } else {
                        0.7
                    }
                }
                
                OperationType::BlockDownload => {
                    // Prioritize high bandwidth for blocks
                    if performance.bandwidth_estimate > 10.0 {
                        1.2
                    } else if performance.bandwidth_estimate > 5.0 {
                        1.0
                    } else {
                        0.8
                    }
                }
                
                OperationType::StateSync => {
                    // Prioritize reliability for state sync
                    if performance.success_rate > 0.95 {
                        1.3
                    } else if performance.success_rate > 0.9 {
                        1.0
                    } else {
                        0.6
                    }
                }
                
                _ => 1.0, // Default weight
            }
        } else {
            0.8 // Unknown performance, slightly lower weight
        }
    }

    pub async fn update_peer_performance(
        &mut self,
        peer_id: PeerId,
        operation_result: OperationResult,
    ) -> Result<(), PeerManagerError> {
        let performance = self.peer_performance.entry(peer_id.clone())
            .or_insert_with(|| PeerPerformance::new(peer_id.clone()));

        match operation_result {
            OperationResult::Success { latency, bytes_transferred } => {
                performance.last_response = Instant::now();
                performance.consecutive_failures = 0;
                
                // Update latency (exponential moving average)
                let alpha = 0.1;
                performance.average_latency = Duration::from_millis(
                    ((1.0 - alpha) * performance.average_latency.as_millis() as f64 +
                     alpha * latency.as_millis() as f64) as u64
                );
                
                // Update bandwidth estimate
                if let Some(duration) = latency.checked_sub(Duration::from_millis(10)) {
                    let bandwidth = bytes_transferred as f64 / duration.as_secs_f64() / 1_000_000.0;
                    performance.bandwidth_estimate = (1.0 - alpha) * performance.bandwidth_estimate + alpha * bandwidth;
                }
                
                // Update success rate
                let total_ops = performance.blocks_served + performance.error_count as u64;
                if total_ops > 0 {
                    performance.success_rate = performance.blocks_served as f64 / total_ops as f64;
                }
                
                performance.blocks_served += 1;
                performance.bytes_transferred += bytes_transferred;
                
                // Add to response time history
                if performance.response_time_history.len() >= 100 {
                    performance.response_time_history.pop_front();
                }
                performance.response_time_history.push_back(latency);
            }
            
            OperationResult::Failure { error_type, .. } => {
                performance.error_count += 1;
                performance.consecutive_failures += 1;
                
                // Update success rate
                let total_ops = performance.blocks_served + performance.error_count as u64;
                if total_ops > 0 {
                    performance.success_rate = performance.blocks_served as f64 / total_ops as f64;
                }
                
                // Check if peer should be temporarily blacklisted
                if performance.consecutive_failures >= 5 {
                    self.temporarily_blacklist_peer(peer_id.clone(), Duration::from_secs(300)).await?;
                }
            }
        }

        // Update reputation based on performance
        self.update_peer_reputation(peer_id).await?;
        
        Ok(())
    }

    async fn update_peer_reputation(&mut self, peer_id: PeerId) -> Result<(), PeerManagerError> {
        let performance = self.peer_performance.get(&peer_id)
            .ok_or(PeerManagerError::PeerNotFound)?;

        let reputation = self.peer_reputations.entry(peer_id.clone())
            .or_insert_with(|| PeerReputation::new(peer_id.clone()));

        // Update individual score components
        reputation.reliability_score = performance.success_rate;
        
        reputation.performance_score = {
            // Normalize latency score (lower is better)
            let latency_score = if performance.average_latency < Duration::from_millis(50) {
                1.0
            } else if performance.average_latency < Duration::from_millis(200) {
                0.8
            } else if performance.average_latency < Duration::from_millis(500) {
                0.6
            } else {
                0.3
            };
            
            // Normalize bandwidth score
            let bandwidth_score = (performance.bandwidth_estimate / 20.0).min(1.0);
            
            (latency_score + bandwidth_score) / 2.0
        };
        
        reputation.behavior_score = {
            // Penalize consecutive failures
            let failure_penalty = (performance.consecutive_failures as f64 * 0.1).min(0.5);
            (1.0 - failure_penalty).max(0.0)
        };
        
        // Calculate overall score (weighted average)
        reputation.overall_score = 
            reputation.trust_score * 0.25 +
            reputation.reliability_score * 0.35 +
            reputation.performance_score * 0.25 +
            reputation.behavior_score * 0.15;
        
        reputation.last_updated = Instant::now();
        reputation.interactions += 1;
        
        Ok(())
    }

    async fn temporarily_blacklist_peer(
        &mut self,
        peer_id: PeerId,
        duration: Duration,
    ) -> Result<(), PeerManagerError> {
        if let Some(reputation) = self.peer_reputations.get_mut(&peer_id) {
            reputation.blacklisted = true;
            reputation.blacklist_until = Some(Instant::now() + duration);
            
            warn!("Temporarily blacklisted peer {} for {:?}", peer_id, duration);
        }
        
        Ok(())
    }

    pub async fn cleanup_blacklisted_peers(&mut self) -> Result<(), PeerManagerError> {
        let now = Instant::now();
        let mut to_unblacklist = Vec::new();
        
        for (peer_id, reputation) in &self.peer_reputations {
            if reputation.blacklisted {
                if let Some(blacklist_until) = reputation.blacklist_until {
                    if now >= blacklist_until {
                        to_unblacklist.push(peer_id.clone());
                    }
                }
            }
        }
        
        for peer_id in to_unblacklist {
            if let Some(reputation) = self.peer_reputations.get_mut(&peer_id) {
                reputation.blacklisted = false;
                reputation.blacklist_until = None;
                info!("Removed blacklist for peer {}", peer_id);
            }
        }
        
        Ok(())
    }

    fn is_peer_available(&self, peer_id: &PeerId) -> bool {
        if let Some(reputation) = self.peer_reputations.get(peer_id) {
            !reputation.blacklisted
        } else {
            true // Unknown peers are considered available
        }
    }

    fn record_peer_selection(&mut self, peer_id: PeerId, operation_type: OperationType) {
        // This would be used for reinforcement learning
        // Record the selection for later evaluation of outcomes
    }
}

#[derive(Debug, Clone)]
pub enum OperationType {
    HeaderDownload,
    BlockDownload,
    StateSync,
    PeerDiscovery,
}

#[derive(Debug, Clone)]
pub enum OperationResult {
    Success {
        latency: Duration,
        bytes_transferred: u64,
    },
    Failure {
        error_type: String,
        latency: Option<Duration>,
    },
}

impl PeerReputation {
    pub fn new(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            trust_score: 0.5,        // Start with neutral trust
            reliability_score: 0.5,  // Start with neutral reliability
            performance_score: 0.5,  // Start with neutral performance
            behavior_score: 1.0,     // Start with good behavior assumption
            overall_score: 0.6,      // Slightly above neutral to encourage initial use
            last_updated: Instant::now(),
            interactions: 0,
            blacklisted: false,
            blacklist_until: None,
        }
    }
}

impl PeerPerformance {
    pub fn new(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            average_latency: Duration::from_millis(200), // Conservative initial estimate
            bandwidth_estimate: 1.0, // 1 MB/s conservative initial estimate
            success_rate: 1.0, // Start optimistic
            blocks_served: 0,
            bytes_transferred: 0,
            error_count: 0,
            consecutive_failures: 0,
            last_response: Instant::now(),
            response_time_history: VecDeque::new(),
        }
    }
}

#[derive(Debug)]
pub enum PeerManagerError {
    PeerNotFound,
    NoAvailablePeers,
    BandwidthExceeded,
    ConfigurationError(String),
}
```

**Implementation 3: Comprehensive Monitoring and Performance Optimization**
```rust
// src/actors/sync/monitoring.rs
use prometheus::{Counter, Histogram, Gauge, IntGauge};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct SyncMonitoringSystem {
    // Core sync metrics
    pub sync_metrics: SyncMetrics,
    
    // Performance monitoring
    pub performance_tracker: PerformanceTracker,
    
    // Resource monitoring
    pub resource_monitor: ResourceMonitor,
    
    // Alerting system
    pub alert_manager: AlertManager,
    
    // Health checker
    pub health_checker: HealthChecker,
}

#[derive(Debug)]
pub struct SyncMetrics {
    // Sync progress metrics
    pub sync_current_height: IntGauge,
    pub sync_target_height: IntGauge,
    pub sync_blocks_per_second: Gauge,
    pub sync_state: IntGauge,
    pub sync_progress_percentage: Gauge,
    
    // Download metrics
    pub blocks_downloaded: Counter,
    pub blocks_validated: Counter,
    pub blocks_failed: Counter,
    pub download_latency: Histogram,
    pub validation_latency: Histogram,
    
    // Peer metrics
    pub connected_peers: IntGauge,
    pub active_downloads: IntGauge,
    pub peer_scores: Gauge,
    pub peer_timeouts: Counter,
    
    // Checkpoint metrics
    pub checkpoints_created: Counter,
    pub checkpoint_recovery_time: Histogram,
    
    // Error metrics
    pub sync_errors: prometheus::CounterVec,
    pub recovery_attempts: prometheus::CounterVec,
    
    // Network metrics
    pub network_bandwidth_usage: Gauge,
    pub network_latency: Histogram,
    pub partition_detected: IntGauge,
}

#[derive(Debug)]
pub struct PerformanceTracker {
    // Performance measurements
    sync_start_time: Instant,
    last_measurement: Instant,
    blocks_at_last_measurement: u64,
    
    // Performance history
    throughput_history: Vec<ThroughputSample>,
    latency_history: Vec<LatencySample>,
    
    // Performance targets
    target_throughput: f64, // blocks per second
    target_latency: Duration,
    
    // Optimization recommendations
    optimization_engine: OptimizationEngine,
}

#[derive(Debug, Clone)]
pub struct ThroughputSample {
    pub timestamp: Instant,
    pub blocks_per_second: f64,
    pub peers_active: usize,
    pub batch_size: usize,
    pub network_conditions: NetworkConditions,
}

#[derive(Debug, Clone)]
pub struct LatencySample {
    pub timestamp: Instant,
    pub operation_type: String,
    pub latency: Duration,
    pub peer_id: Option<PeerId>,
    pub success: bool,
}

#[derive(Debug, Clone)]
pub struct NetworkConditions {
    pub average_latency: Duration,
    pub bandwidth_estimate: f64,
    pub packet_loss: f64,
    pub jitter: Duration,
}

impl SyncMonitoringSystem {
    pub fn new() -> Self {
        let sync_metrics = SyncMetrics::new();
        
        Self {
            sync_metrics,
            performance_tracker: PerformanceTracker::new(),
            resource_monitor: ResourceMonitor::new(),
            alert_manager: AlertManager::new(),
            health_checker: HealthChecker::new(),
        }
    }

    pub async fn update_sync_progress(
        &mut self,
        current_height: u64,
        target_height: u64,
        state: &SyncState,
    ) -> Result<(), MonitoringError> {
        // Update basic metrics
        self.sync_metrics.sync_current_height.set(current_height as i64);
        self.sync_metrics.sync_target_height.set(target_height as i64);
        
        // Calculate progress percentage
        let progress = if target_height > 0 {
            (current_height as f64 / target_height as f64) * 100.0
        } else {
            0.0
        };
        self.sync_metrics.sync_progress_percentage.set(progress);
        
        // Update state metric
        let state_value = match state {
            SyncState::Idle => 0,
            SyncState::Discovering { .. } => 1,
            SyncState::DownloadingHeaders { .. } => 2,
            SyncState::DownloadingBlocks { .. } => 3,
            SyncState::CatchingUp { .. } => 4,
            SyncState::Synced { .. } => 5,
            SyncState::Failed { .. } => 6,
        };
        self.sync_metrics.sync_state.set(state_value);
        
        // Update performance tracker
        self.performance_tracker.update_progress(current_height).await?;
        
        // Check for performance issues
        self.analyze_performance_trends().await?;
        
        // Update resource utilization
        self.resource_monitor.update().await?;
        
        // Check health status
        self.health_checker.check_sync_health(current_height, target_height, state).await?;
        
        Ok(())
    }

    async fn analyze_performance_trends(&mut self) -> Result<(), MonitoringError> {
        let current_throughput = self.performance_tracker.calculate_current_throughput();
        
        // Record throughput sample
        self.performance_tracker.throughput_history.push(ThroughputSample {
            timestamp: Instant::now(),
            blocks_per_second: current_throughput,
            peers_active: self.get_active_peer_count(),
            batch_size: self.get_current_batch_size(),
            network_conditions: self.get_network_conditions().await?,
        });
        
        // Limit history size
        if self.performance_tracker.throughput_history.len() > 1000 {
            self.performance_tracker.throughput_history.drain(0..100);
        }
        
        // Generate optimization recommendations
        let recommendations = self.performance_tracker.optimization_engine
            .analyze_and_recommend(&self.performance_tracker.throughput_history).await?;
        
        // Apply automatic optimizations if enabled
        for recommendation in recommendations {
            if recommendation.auto_apply {
                info!("Auto-applying optimization: {}", recommendation.description);
                self.apply_optimization(recommendation).await?;
            } else {
                info!("Manual optimization recommended: {}", recommendation.description);
            }
        }
        
        Ok(())
    }

    async fn apply_optimization(&mut self, recommendation: OptimizationRecommendation) -> Result<(), MonitoringError> {
        match recommendation.optimization_type {
            OptimizationType::IncreaseBatchSize { new_size } => {
                info!("Increasing batch size to {}", new_size);
                // This would send a message to the sync actor to adjust batch size
            }
            
            OptimizationType::AdjustParallelism { new_worker_count } => {
                info!("Adjusting parallelism to {} workers", new_worker_count);
                // This would reconfigure the parallel validation workers
            }
            
            OptimizationType::ChangePeerSelection { strategy } => {
                info!("Changing peer selection strategy to {:?}", strategy);
                // This would update the peer selection algorithm
            }
            
            OptimizationType::AdjustTimeout { new_timeout } => {
                info!("Adjusting timeout to {:?}", new_timeout);
                // This would update request timeouts
            }
        }
        
        Ok(())
    }

    pub async fn record_operation_latency(
        &mut self,
        operation_type: &str,
        latency: Duration,
        peer_id: Option<PeerId>,
        success: bool,
    ) -> Result<(), MonitoringError> {
        // Record in Prometheus metrics
        self.sync_metrics.download_latency.observe(latency.as_secs_f64());
        
        // Record in performance tracker
        self.performance_tracker.latency_history.push(LatencySample {
            timestamp: Instant::now(),
            operation_type: operation_type.to_string(),
            latency,
            peer_id,
            success,
        });
        
        // Limit history size
        if self.performance_tracker.latency_history.len() > 5000 {
            self.performance_tracker.latency_history.drain(0..500);
        }
        
        // Check for latency alerts
        if latency > Duration::from_secs(10) {
            self.alert_manager.trigger_alert(Alert {
                level: AlertLevel::Warning,
                message: format!("High latency detected: {:?} for {}", latency, operation_type),
                timestamp: Instant::now(),
                metadata: AlertMetadata {
                    operation_type: Some(operation_type.to_string()),
                    latency: Some(latency),
                    peer_id,
                },
            }).await?;
        }
        
        Ok(())
    }

    async fn get_network_conditions(&self) -> Result<NetworkConditions, MonitoringError> {
        // Calculate average latency from recent samples
        let recent_latencies: Vec<Duration> = self.performance_tracker.latency_history
            .iter()
            .filter(|sample| sample.timestamp.elapsed() < Duration::from_secs(60))
            .map(|sample| sample.latency)
            .collect();
        
        let average_latency = if recent_latencies.is_empty() {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(
                recent_latencies.iter().map(|d| d.as_millis()).sum::<u128>() as u64
                / recent_latencies.len() as u64
            )
        };
        
        // Estimate bandwidth from recent throughput
        let bandwidth_estimate = self.performance_tracker.throughput_history
            .iter()
            .filter(|sample| sample.timestamp.elapsed() < Duration::from_secs(60))
            .map(|sample| sample.blocks_per_second * 2.0) // Assume 2MB average block size
            .fold(0.0, |acc, x| acc + x) / 60.0; // Average over 1 minute
        
        // Calculate jitter (standard deviation of latency)
        let jitter = if recent_latencies.len() > 1 {
            let mean = average_latency.as_millis() as f64;
            let variance = recent_latencies.iter()
                .map(|d| (d.as_millis() as f64 - mean).powi(2))
                .sum::<f64>() / recent_latencies.len() as f64;
            Duration::from_millis(variance.sqrt() as u64)
        } else {
            Duration::from_millis(0)
        };
        
        Ok(NetworkConditions {
            average_latency,
            bandwidth_estimate,
            packet_loss: 0.0, // Would be calculated from actual network stats
            jitter,
        })
    }

    fn get_active_peer_count(&self) -> usize {
        // This would query the actual peer manager
        5 // Placeholder
    }

    fn get_current_batch_size(&self) -> usize {
        // This would query the current sync configuration
        128 // Placeholder
    }
}

#[derive(Debug)]
pub struct OptimizationEngine {
    learning_history: Vec<OptimizationAttempt>,
    performance_model: PerformanceModel,
}

#[derive(Debug, Clone)]
pub struct OptimizationRecommendation {
    pub optimization_type: OptimizationType,
    pub confidence: f64,
    pub expected_improvement: f64,
    pub description: String,
    pub auto_apply: bool,
}

#[derive(Debug, Clone)]
pub enum OptimizationType {
    IncreaseBatchSize { new_size: usize },
    AdjustParallelism { new_worker_count: usize },
    ChangePeerSelection { strategy: String },
    AdjustTimeout { new_timeout: Duration },
}

#[derive(Debug, Clone)]
pub struct OptimizationAttempt {
    pub timestamp: Instant,
    pub optimization_type: OptimizationType,
    pub before_performance: f64,
    pub after_performance: f64,
    pub success: bool,
}

impl OptimizationEngine {
    pub async fn analyze_and_recommend(
        &mut self,
        throughput_history: &[ThroughputSample],
    ) -> Result<Vec<OptimizationRecommendation>, MonitoringError> {
        let mut recommendations = Vec::new();
        
        if throughput_history.is_empty() {
            return Ok(recommendations);
        }
        
        let recent_samples: Vec<&ThroughputSample> = throughput_history
            .iter()
            .filter(|sample| sample.timestamp.elapsed() < Duration::from_secs(300))
            .collect();
        
        if recent_samples.is_empty() {
            return Ok(recommendations);
        }
        
        let current_throughput = recent_samples.iter()
            .map(|sample| sample.blocks_per_second)
            .sum::<f64>() / recent_samples.len() as f64;
        
        let target_throughput = 50.0; // blocks per second
        
        if current_throughput < target_throughput * 0.8 {
            // Performance is below 80% of target, recommend optimizations
            
            // Analyze batch size impact
            if let Some(batch_recommendation) = self.analyze_batch_size_impact(&recent_samples) {
                recommendations.push(batch_recommendation);
            }
            
            // Analyze parallelism impact
            if let Some(parallelism_recommendation) = self.analyze_parallelism_impact(&recent_samples) {
                recommendations.push(parallelism_recommendation);
            }
            
            // Analyze network conditions
            if let Some(network_recommendation) = self.analyze_network_impact(&recent_samples) {
                recommendations.push(network_recommendation);
            }
        }
        
        Ok(recommendations)
    }

    fn analyze_batch_size_impact(&self, samples: &[&ThroughputSample]) -> Option<OptimizationRecommendation> {
        // Analyze correlation between batch size and throughput
        let mut batch_size_performance: HashMap<usize, Vec<f64>> = HashMap::new();
        
        for sample in samples {
            batch_size_performance
                .entry(sample.batch_size)
                .or_insert_with(Vec::new)
                .push(sample.blocks_per_second);
        }
        
        // Find optimal batch size
        let mut best_batch_size = 128;
        let mut best_performance = 0.0;
        
        for (batch_size, performances) in batch_size_performance {
            let avg_performance = performances.iter().sum::<f64>() / performances.len() as f64;
            if avg_performance > best_performance {
                best_performance = avg_performance;
                best_batch_size = batch_size;
            }
        }
        
        // Current average batch size
        let current_avg_batch = samples.iter().map(|s| s.batch_size).sum::<usize>() / samples.len();
        
        if best_batch_size > current_avg_batch && best_performance > 0.0 {
            Some(OptimizationRecommendation {
                optimization_type: OptimizationType::IncreaseBatchSize { 
                    new_size: best_batch_size 
                },
                confidence: 0.8,
                expected_improvement: (best_performance / samples.iter()
                    .map(|s| s.blocks_per_second)
                    .sum::<f64>() / samples.len() as f64) - 1.0,
                description: format!("Increase batch size from {} to {} for better throughput", 
                                   current_avg_batch, best_batch_size),
                auto_apply: true,
            })
        } else {
            None
        }
    }

    fn analyze_parallelism_impact(&self, samples: &[&ThroughputSample]) -> Option<OptimizationRecommendation> {
        // Analyze correlation between number of active peers and throughput
        let peer_throughput: Vec<(usize, f64)> = samples.iter()
            .map(|sample| (sample.peers_active, sample.blocks_per_second))
            .collect();
        
        // Simple analysis: if throughput increases with more peers, recommend more parallelism
        let avg_throughput_by_peers: HashMap<usize, f64> = {
            let mut groups: HashMap<usize, Vec<f64>> = HashMap::new();
            for (peers, throughput) in peer_throughput {
                groups.entry(peers).or_insert_with(Vec::new).push(throughput);
            }
            groups.into_iter()
                .map(|(peers, throughputs)| {
                    (peers, throughputs.iter().sum::<f64>() / throughputs.len() as f64)
                })
                .collect()
        };
        
        if let Some((&max_peers, &max_throughput)) = avg_throughput_by_peers.iter()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap()) {
            
            let current_avg_peers = samples.iter().map(|s| s.peers_active).sum::<usize>() / samples.len();
            
            if max_peers > current_avg_peers && max_throughput > 0.0 {
                return Some(OptimizationRecommendation {
                    optimization_type: OptimizationType::AdjustParallelism { 
                        new_worker_count: max_peers 
                    },
                    confidence: 0.7,
                    expected_improvement: (max_throughput / samples.iter()
                        .map(|s| s.blocks_per_second)
                        .sum::<f64>() / samples.len() as f64) - 1.0,
                    description: format!("Increase parallelism from {} to {} workers", 
                                       current_avg_peers, max_peers),
                    auto_apply: false, // More conservative for parallelism changes
                });
            }
        }
        
        None
    }

    fn analyze_network_impact(&self, samples: &[&ThroughputSample]) -> Option<OptimizationRecommendation> {
        // Analyze if network conditions are limiting performance
        let high_latency_samples = samples.iter()
            .filter(|sample| sample.network_conditions.average_latency > Duration::from_secs(1))
            .count();
        
        if high_latency_samples as f64 / samples.len() as f64 > 0.5 {
            Some(OptimizationRecommendation {
                optimization_type: OptimizationType::AdjustTimeout { 
                    new_timeout: Duration::from_secs(30)
                },
                confidence: 0.9,
                expected_improvement: 0.2,
                description: "Increase timeout due to high network latency".to_string(),
                auto_apply: true,
            })
        } else {
            None
        }
    }
}

impl SyncMetrics {
    pub fn new() -> Self {
        Self {
            sync_current_height: IntGauge::new(
                "sync_current_height",
                "Current sync height"
            ).expect("Failed to create sync_current_height gauge"),
            
            sync_target_height: IntGauge::new(
                "sync_target_height", 
                "Target sync height"
            ).expect("Failed to create sync_target_height gauge"),
            
            sync_blocks_per_second: Gauge::new(
                "sync_blocks_per_second",
                "Current sync speed in blocks per second"
            ).expect("Failed to create sync_blocks_per_second gauge"),
            
            sync_state: IntGauge::new(
                "sync_state",
                "Current sync state (0=idle, 1=discovering, 2=headers, 3=blocks, 4=catching_up, 5=synced, 6=failed)"
            ).expect("Failed to create sync_state gauge"),
            
            sync_progress_percentage: Gauge::new(
                "sync_progress_percentage",
                "Sync progress as percentage"
            ).expect("Failed to create sync_progress_percentage gauge"),
            
            blocks_downloaded: Counter::new(
                "sync_blocks_downloaded_total",
                "Total blocks downloaded during sync"
            ).expect("Failed to create blocks_downloaded counter"),
            
            blocks_validated: Counter::new(
                "sync_blocks_validated_total",
                "Total blocks validated during sync"
            ).expect("Failed to create blocks_validated counter"),
            
            blocks_failed: Counter::new(
                "sync_blocks_failed_total",
                "Total blocks that failed validation"
            ).expect("Failed to create blocks_failed counter"),
            
            download_latency: Histogram::with_opts(
                prometheus::HistogramOpts::new(
                    "sync_download_latency_seconds",
                    "Latency of block download operations"
                ).buckets(vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0])
            ).expect("Failed to create download_latency histogram"),
            
            validation_latency: Histogram::with_opts(
                prometheus::HistogramOpts::new(
                    "sync_validation_latency_seconds",
                    "Latency of block validation operations"
                ).buckets(vec![0.01, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0])
            ).expect("Failed to create validation_latency histogram"),
            
            connected_peers: IntGauge::new(
                "sync_connected_peers",
                "Number of connected peers for sync"
            ).expect("Failed to create connected_peers gauge"),
            
            active_downloads: IntGauge::new(
                "sync_active_downloads",
                "Number of active block downloads"
            ).expect("Failed to create active_downloads gauge"),
            
            peer_scores: Gauge::new(
                "sync_peer_average_score",
                "Average score of connected peers"
            ).expect("Failed to create peer_scores gauge"),
            
            peer_timeouts: Counter::new(
                "sync_peer_timeouts_total",
                "Total number of peer timeouts during sync"
            ).expect("Failed to create peer_timeouts counter"),
            
            checkpoints_created: Counter::new(
                "sync_checkpoints_created_total",
                "Total number of checkpoints created"
            ).expect("Failed to create checkpoints_created counter"),
            
            checkpoint_recovery_time: Histogram::with_opts(
                prometheus::HistogramOpts::new(
                    "sync_checkpoint_recovery_seconds",
                    "Time taken to recover from checkpoint"
                ).buckets(vec![1.0, 5.0, 10.0, 30.0, 60.0, 300.0])
            ).expect("Failed to create checkpoint_recovery_time histogram"),
            
            sync_errors: prometheus::CounterVec::new(
                prometheus::Opts::new(
                    "sync_errors_total",
                    "Total sync errors by type"
                ),
                &["error_type"]
            ).expect("Failed to create sync_errors counter"),
            
            recovery_attempts: prometheus::CounterVec::new(
                prometheus::Opts::new(
                    "sync_recovery_attempts_total", 
                    "Total recovery attempts by type"
                ),
                &["recovery_type"]
            ).expect("Failed to create recovery_attempts counter"),
            
            network_bandwidth_usage: Gauge::new(
                "sync_network_bandwidth_mbps",
                "Current network bandwidth usage in MB/s"
            ).expect("Failed to create network_bandwidth_usage gauge"),
            
            network_latency: Histogram::with_opts(
                prometheus::HistogramOpts::new(
                    "sync_network_latency_seconds",
                    "Network latency to peers"
                ).buckets(vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0])
            ).expect("Failed to create network_latency histogram"),
            
            partition_detected: IntGauge::new(
                "sync_partition_detected",
                "Whether network partition is detected (1=yes, 0=no)"
            ).expect("Failed to create partition_detected gauge"),
        }
    }

    pub fn register_all(&self) -> Result<(), prometheus::Error> {
        prometheus::register(Box::new(self.sync_current_height.clone()))?;
        prometheus::register(Box::new(self.sync_target_height.clone()))?;
        prometheus::register(Box::new(self.sync_blocks_per_second.clone()))?;
        prometheus::register(Box::new(self.sync_state.clone()))?;
        prometheus::register(Box::new(self.sync_progress_percentage.clone()))?;
        prometheus::register(Box::new(self.blocks_downloaded.clone()))?;
        prometheus::register(Box::new(self.blocks_validated.clone()))?;
        prometheus::register(Box::new(self.blocks_failed.clone()))?;
        prometheus::register(Box::new(self.download_latency.clone()))?;
        prometheus::register(Box::new(self.validation_latency.clone()))?;
        prometheus::register(Box::new(self.connected_peers.clone()))?;
        prometheus::register(Box::new(self.active_downloads.clone()))?;
        prometheus::register(Box::new(self.peer_scores.clone()))?;
        prometheus::register(Box::new(self.peer_timeouts.clone()))?;
        prometheus::register(Box::new(self.checkpoints_created.clone()))?;
        prometheus::register(Box::new(self.checkpoint_recovery_time.clone()))?;
        prometheus::register(Box::new(self.sync_errors.clone()))?;
        prometheus::register(Box::new(self.recovery_attempts.clone()))?;
        prometheus::register(Box::new(self.network_bandwidth_usage.clone()))?;
        prometheus::register(Box::new(self.network_latency.clone()))?;
        prometheus::register(Box::new(self.partition_detected.clone()))?;
        
        Ok(())
    }
}

#[derive(Debug)]
pub enum MonitoringError {
    MetricUpdateFailed(String),
    ResourceQueryFailed(String),
    AlertSystemFailed(String),
}

// Additional monitoring components would be implemented here...
```

#### Priority 2: Integration and Performance Optimization

**Plan:** Complete integration testing, performance benchmarking, and final optimization.

### Detailed Test Plan

**Unit Tests (250 tests):**
1. Message handling tests (50 tests)
2. State machine transition tests (40 tests)
3. Peer management and selection tests (45 tests)
4. Block processing and validation tests (50 tests)
5. Checkpoint system tests (35 tests)
6. Error handling and recovery tests (30 tests)

**Integration Tests (150 tests):**
1. Full sync workflow tests (40 tests)
2. Network partition recovery tests (25 tests)
3. Peer failure handling tests (25 tests)
4. Performance regression tests (30 tests)
5. Resource utilization tests (30 tests)

**Performance Tests (75 benchmarks):**
1. Sync speed benchmarks (20 benchmarks)
2. Memory usage optimization (15 benchmarks)
3. CPU utilization efficiency (15 benchmarks)
4. Network bandwidth optimization (15 benchmarks)
5. Concurrent operation benchmarks (10 benchmarks)

### Implementation Timeline

**Week 1-2: Error Handling and Resilience**
- Complete advanced error handling with recovery strategies
- Implement network partition detection and recovery
- Add comprehensive circuit breaker patterns

**Week 3: Peer Management and Optimization**
- Complete advanced peer reputation system
- Implement adaptive peer selection algorithms
- Add bandwidth allocation and load balancing

**Week 4: Monitoring and Performance**
- Complete comprehensive monitoring system
- Implement performance optimization engine
- Add automated tuning and alerting

### Success Metrics

**Functional Metrics:**
- 100% test coverage for sync operations
- All acceptance criteria satisfied
- Zero data corruption during sync operations

**Performance Metrics:**
- Sync speed improved by >2x compared to baseline
- 99.5% sync threshold for block production working correctly
- Memory usage ≤ 512MB during full sync
- Network bandwidth utilization >80%

**Operational Metrics:**
- 99.9% sync operation success rate
- Network partition recovery within 60 seconds
- Checkpoint recovery time ≤ 30 seconds
- Zero manual interventions required during normal operation

### Risk Mitigation

**Technical Risks:**
- **Network partition handling**: Comprehensive partition detection and multiple recovery strategies
- **Peer selection failures**: Reputation-based scoring with fallback mechanisms
- **Performance degradation**: Continuous monitoring with automated optimization

**Operational Risks:**
- **Sync stalling**: Multiple recovery mechanisms and escalation procedures
- **Resource exhaustion**: Resource monitoring with automatic throttling
- **State corruption**: Checkpoint validation and recovery capabilities