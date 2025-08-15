# ALYS-010: Implement SyncActor with Improved Sync Algorithm

## Issue Type
Task

## Priority
Critical

## Story Points
10

## Sprint
Migration Sprint 3

## Component
Sync System

## Labels
`migration`, `phase-2`, `sync`, `actor-system`, `performance`

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
- Add support for light client sync
- Implement state sync for even faster sync
- Consider pruning old checkpoints

## Time Tracking

- Estimated: 6 days
- Actual: _To be filled_