# Alys Node Syncing: Comprehensive Analysis and Improvement Strategy

## Executive Summary

Alys node syncing has been historically plagued with issues that prevent nodes from producing blocks until fully synchronized. This knowledge graph provides a comprehensive analysis of current syncing problems and proposes architectural improvements using actor patterns, better testing strategies, and resilience mechanisms.

## Current Syncing Architecture Problems

### 1. Monolithic Sync State Management

```rust
// Current problematic pattern in chain.rs
pub struct Chain {
    sync_status: RwLock<SyncStatus>,  // Binary state: InProgress or Synced
    head: RwLock<Option<BlockRef>>,
    peers: RwLock<HashSet<PeerId>>,
    // ... many other fields
}

enum SyncStatus {
    InProgress,
    Synced,
}
```

**Problems:**
1. **Binary sync state** - No granularity about sync progress
2. **No partial sync support** - Can't produce blocks even if nearly synced
3. **Shared mutable state** - RwLock contention during sync
4. **No sync metrics** - Hard to diagnose sync issues
5. **All-or-nothing approach** - Single failure can halt entire sync

### 2. Sync Process Issues

```rust
// Current sync implementation (chain.rs:2182-2365)
pub async fn sync(self: Arc<Self>) {
    *self.sync_status.write().await = SyncStatus::InProgress;
    
    // Phase 1: Wait for peers (blocking)
    let peer_id = loop {
        let peers = self.peers.read().await;
        if let Some(selected_peer) = peers.iter().choose(&mut rand::thread_rng()) {
            break selected_peer;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    
    // Phase 2: Sync blocks in batches of 1024
    loop {
        let request = BlocksByRangeRequest {
            start_height: head + 1,
            count: 1024,  // Fixed batch size
        };
        
        // Single point of failure - if RPC fails, sync stops
        let mut receive_stream = self
            .send_blocks_by_range_with_peer_fallback(request, 3)
            .await?;
        
        // Process blocks sequentially
        while let Some(block) = receive_stream.recv().await {
            match self.process_block(block).await {
                Err(e) => {
                    // Rollback on any error
                    self.rollback_head(head.saturating_sub(1)).await;
                    return;  // Exit sync completely!
                }
            }
        }
    }
    
    *self.sync_status.write().await = SyncStatus::Synced;
}
```

**Critical Issues:**
1. **No checkpointing** - Sync restarts from genesis on failure
2. **Sequential processing** - Can't parallelize validation
3. **Fixed batch size** - Not adaptive to network conditions
4. **No partial progress** - Can't produce blocks while catching up
5. **Poor error handling** - Single error stops entire sync
6. **No sync recovery** - Manual intervention needed after failure

### 3. Block Production Blocking

```rust
// Block production prevented during sync (chain.rs:437)
pub async fn produce_block(&self) -> Result<(), Error> {
    if !self.sync_status.read().await.is_synced() {
        CHAIN_BLOCK_PRODUCTION_TOTALS
            .with_label_values(&["attempted", "not_synced"])
            .inc();
        return Err(Error::NotSynced);  // Can't produce blocks!
    }
    // ... rest of block production
}
```

**Problems:**
1. **Complete blocking** - Even if 99.9% synced, can't produce
2. **No "optimistic" mode** - Could produce on recent blocks
3. **No sync estimation** - Don't know when production will resume

## Proposed Actor-Based Sync Architecture

### 1. SyncActor Design

```rust
/// Dedicated actor for managing synchronization
pub struct SyncActor {
    // Sync state machine
    state: SyncState,
    
    // Progress tracking
    sync_progress: SyncProgress,
    
    // Peer management
    peer_manager: Addr<PeerManagerActor>,
    
    // Block processing
    block_processor: Addr<BlockProcessorActor>,
    
    // Chain actor for updates
    chain_actor: Addr<ChainActor>,
    
    // Checkpointing
    checkpoint_manager: CheckpointManager,
    
    // Metrics
    metrics: SyncMetrics,
}

/// Granular sync state with recovery information
#[derive(Debug, Clone)]
pub enum SyncState {
    /// Initial state, discovering peers
    Discovering {
        started_at: Instant,
        attempts: u32,
    },
    
    /// Downloading headers for validation
    DownloadingHeaders {
        start_height: u64,
        target_height: u64,
        current_height: u64,
        peer: PeerId,
    },
    
    /// Downloading and processing blocks
    DownloadingBlocks {
        start_height: u64,
        target_height: u64,
        current_height: u64,
        batch_size: usize,
        peers: Vec<PeerId>,
    },
    
    /// Catching up recent blocks (can produce)
    CatchingUp {
        blocks_behind: u64,
        sync_speed: f64,  // blocks per second
        estimated_time: Duration,
    },
    
    /// Fully synced
    Synced {
        last_check: Instant,
        peer_height: u64,
    },
    
    /// Sync failed, attempting recovery
    Failed {
        reason: String,
        last_good_height: u64,
        recovery_attempts: u32,
        next_retry: Instant,
    },
}

/// Detailed progress tracking
#[derive(Debug, Clone)]
pub struct SyncProgress {
    // Heights
    pub genesis_height: u64,
    pub current_height: u64,
    pub target_height: u64,
    pub highest_peer_height: u64,
    
    // Performance
    pub blocks_processed: u64,
    pub blocks_failed: u64,
    pub sync_start_time: Instant,
    pub blocks_per_second: f64,
    
    // Checkpoints
    pub last_checkpoint: Option<BlockCheckpoint>,
    pub checkpoint_frequency: u64,  // Every N blocks
    
    // Network
    pub active_peers: usize,
    pub total_peers: usize,
    pub peer_scores: HashMap<PeerId, PeerScore>,
}

/// Sync-related messages
#[derive(Message)]
#[rtype(result = "Result<()>")]
pub enum SyncMessage {
    /// Start syncing from a specific height
    StartSync {
        from_height: Option<u64>,
        target_height: Option<u64>,
    },
    
    /// Pause sync (e.g., for maintenance)
    PauseSync,
    
    /// Resume sync after pause
    ResumeSync,
    
    /// Handle new peer discovered
    PeerDiscovered {
        peer_id: PeerId,
        reported_height: u64,
    },
    
    /// Handle peer disconnection
    PeerDisconnected {
        peer_id: PeerId,
        reason: String,
    },
    
    /// Process batch of blocks
    ProcessBlockBatch {
        blocks: Vec<SignedConsensusBlock>,
        from_peer: PeerId,
    },
    
    /// Checkpoint current progress
    CreateCheckpoint,
    
    /// Recover from checkpoint
    RecoverFromCheckpoint {
        checkpoint: BlockCheckpoint,
    },
    
    /// Get current sync status
    GetSyncStatus,
    
    /// Check if we can produce blocks
    CanProduceBlocks,
}
```

### 2. Parallel Block Processing

```rust
/// Actor for parallel block validation and processing
pub struct BlockProcessorActor {
    // Worker pool for parallel validation
    workers: Vec<Addr<BlockValidatorWorker>>,
    
    // Processing pipeline
    validation_queue: VecDeque<BlockValidationTask>,
    execution_queue: VecDeque<BlockExecutionTask>,
    commit_queue: VecDeque<BlockCommitTask>,
    
    // State tracking
    processing_state: HashMap<Hash256, BlockProcessingState>,
    
    // Dependencies
    engine_actor: Addr<EngineActor>,
    storage_actor: Addr<StorageActor>,
}

/// Worker for parallel block validation
pub struct BlockValidatorWorker {
    id: usize,
    aura: Arc<Aura>,
    federation: Arc<Federation>,
}

impl BlockProcessorActor {
    /// Process blocks in parallel pipeline
    pub async fn process_block_batch(
        &mut self,
        blocks: Vec<SignedConsensusBlock>,
    ) -> Result<ProcessingResult> {
        // Stage 1: Parallel signature validation
        let validation_futures = blocks
            .iter()
            .map(|block| {
                let worker = self.get_next_worker();
                worker.send(ValidateBlock(block.clone()))
            })
            .collect::<Vec<_>>();
        
        let validation_results = futures::future::join_all(validation_futures).await;
        
        // Stage 2: Parallel parent verification
        let parent_checks = validation_results
            .iter()
            .filter_map(|r| r.as_ref().ok())
            .map(|block| self.verify_parent_exists(block))
            .collect::<Vec<_>>();
        
        let parent_results = futures::future::join_all(parent_checks).await;
        
        // Stage 3: Sequential execution (required for state consistency)
        let mut executed_blocks = Vec::new();
        for (block, parent_ok) in blocks.iter().zip(parent_results) {
            if parent_ok {
                match self.execute_block(block).await {
                    Ok(result) => executed_blocks.push(result),
                    Err(e) => {
                        // Don't fail entire batch - mark for retry
                        self.mark_for_retry(block, e);
                    }
                }
            }
        }
        
        // Stage 4: Batch commit to storage
        self.storage_actor
            .send(BatchCommitBlocks(executed_blocks))
            .await??;
        
        Ok(ProcessingResult {
            processed: executed_blocks.len(),
            failed: blocks.len() - executed_blocks.len(),
        })
    }
}
```

### 3. Smart Peer Management

```rust
/// Actor for intelligent peer selection and management
pub struct PeerManagerActor {
    // Peer tracking
    peers: HashMap<PeerId, PeerInfo>,
    
    // Performance metrics
    peer_metrics: HashMap<PeerId, PeerMetrics>,
    
    // Selection strategy
    selection_strategy: PeerSelectionStrategy,
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub reported_height: u64,
    pub connected_at: Instant,
    pub last_response: Instant,
    pub protocol_version: String,
    pub location: Option<GeoLocation>,  // For proximity-based selection
}

#[derive(Debug, Default)]
pub struct PeerMetrics {
    pub blocks_served: u64,
    pub average_latency: Duration,
    pub error_rate: f64,
    pub bandwidth: f64,  // MB/s
    pub reliability_score: f64,  // 0.0 to 1.0
}

#[derive(Debug, Clone)]
pub enum PeerSelectionStrategy {
    /// Fastest response time
    LowestLatency,
    
    /// Highest reliability score
    MostReliable,
    
    /// Round-robin for load distribution
    RoundRobin,
    
    /// Weighted by multiple factors
    Weighted {
        latency_weight: f64,
        reliability_weight: f64,
        bandwidth_weight: f64,
    },
    
    /// Geographic proximity (reduce latency)
    ProximityBased,
}

impl PeerManagerActor {
    /// Select best peers for sync based on strategy
    pub fn select_sync_peers(&self, count: usize) -> Vec<PeerId> {
        let mut scored_peers: Vec<(PeerId, f64)> = self.peers
            .iter()
            .filter_map(|(id, info)| {
                let metrics = self.peer_metrics.get(id)?;
                let score = self.calculate_peer_score(info, metrics);
                Some((*id, score))
            })
            .collect();
        
        scored_peers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        scored_peers
            .into_iter()
            .take(count)
            .map(|(id, _)| id)
            .collect()
    }
    
    /// Adaptive batch size based on network conditions
    pub fn calculate_optimal_batch_size(&self) -> usize {
        let avg_bandwidth = self.calculate_average_bandwidth();
        let avg_latency = self.calculate_average_latency();
        let peer_count = self.peers.len();
        
        // Adaptive formula
        let base_size = 128;
        let bandwidth_factor = (avg_bandwidth / 10.0).min(8.0).max(1.0);
        let latency_factor = (100.0 / avg_latency.as_millis() as f64).min(4.0).max(0.5);
        let peer_factor = (peer_count as f64 / 5.0).min(2.0).max(0.5);
        
        (base_size as f64 * bandwidth_factor * latency_factor * peer_factor) as usize
    }
}
```

### 4. Checkpoint System

```rust
/// Checkpoint manager for sync recovery
pub struct CheckpointManager {
    checkpoints: BTreeMap<u64, BlockCheckpoint>,
    checkpoint_interval: u64,  // Every N blocks
    max_checkpoints: usize,
    storage: Arc<Storage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockCheckpoint {
    pub height: u64,
    pub hash: Hash256,
    pub parent_hash: Hash256,
    pub state_root: H256,
    pub timestamp: DateTime<Utc>,
    pub sync_progress: SyncProgress,
    pub verified: bool,
}

impl CheckpointManager {
    /// Create checkpoint at current height
    pub async fn create_checkpoint(
        &mut self,
        block: &SignedConsensusBlock,
        progress: SyncProgress,
    ) -> Result<()> {
        let checkpoint = BlockCheckpoint {
            height: block.message.height(),
            hash: block.canonical_root(),
            parent_hash: block.message.parent_hash,
            state_root: block.message.execution_payload.state_root,
            timestamp: Utc::now(),
            sync_progress: progress,
            verified: true,
        };
        
        // Store checkpoint
        self.checkpoints.insert(checkpoint.height, checkpoint.clone());
        self.storage.store_checkpoint(&checkpoint).await?;
        
        // Prune old checkpoints
        if self.checkpoints.len() > self.max_checkpoints {
            if let Some((height, _)) = self.checkpoints.iter().next() {
                let height = *height;
                self.checkpoints.remove(&height);
                self.storage.delete_checkpoint(height).await?;
            }
        }
        
        Ok(())
    }
    
    /// Find best checkpoint to recover from
    pub fn find_recovery_checkpoint(&self, target_height: u64) -> Option<&BlockCheckpoint> {
        self.checkpoints
            .range(..=target_height)
            .rev()
            .find(|(_, cp)| cp.verified)
            .map(|(_, cp)| cp)
    }
}
```

### 5. Better Block Production Integration

```rust
/// Enhanced sync status with more granular control
pub struct SyncStatusManager {
    // Detailed sync state
    state: SyncState,
    progress: SyncProgress,
    
    // Block production control
    allow_block_production: bool,
    production_threshold: f64,  // e.g., 99.5% synced
}

impl SyncStatusManager {
    /// Check if we can produce blocks based on sync progress
    pub fn can_produce_blocks(
        &self,
    ) -> bool {
        match self.state {
            SyncState::Synced { .. } => true,
            SyncState::CatchingUp { blocks_behind, .. } => {
                // Allow production if we're very close to synced
                blocks_behind <= 10 && self.allow_block_production
            }
            _ => false,
        }
    }
    
    /// Update sync progress and check production eligibility
    pub fn update_progress(&mut self, current: u64, target: u64) {
        let progress_percent = (current as f64 / target as f64) * 100.0;
        
        // Enable production when nearly synced
        if progress_percent >= self.production_threshold {
            self.allow_block_production = true;
            info!("Sync {:.1}% complete - enabling block production", progress_percent);
        }
        
        self.progress.current_height = current;
        self.progress.target_height = target;
    }
}
```

## Improved Sync Implementation

### 1. Sync State Machine

```rust
impl Handler<SyncMessage> for SyncActor {
    type Result = ResponseActFuture<Self, Result<()>>;
    
    fn handle(&mut self, msg: SyncMessage, _ctx: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            match msg {
                SyncMessage::StartSync { from_height, target_height } => {
                    self.start_sync_state_machine(from_height, target_height).await
                }
                
                SyncMessage::ProcessBlockBatch { blocks, from_peer } => {
                    self.process_block_batch(blocks, from_peer).await
                }
                
                SyncMessage::PeerDiscovered { peer_id, reported_height } => {
                    self.handle_peer_discovered(peer_id, reported_height).await
                }
                
                SyncMessage::CreateCheckpoint => {
                    self.create_sync_checkpoint().await
                }
                
                _ => Ok(())
            }
        }.into_actor(self))
    }
}

impl SyncActor {
    async fn start_sync_state_machine(
        &mut self,
        from_height: Option<u64>,
        target_height: Option<u64>,
    ) -> Result<()> {
        // Try to recover from checkpoint
        let start_height = if let Some(checkpoint) = self.find_latest_checkpoint() {
            info!("Recovering from checkpoint at height {}", checkpoint.height);
            self.state = SyncState::DownloadingBlocks {
                start_height: checkpoint.height,
                target_height: target_height.unwrap_or(u64::MAX),
                current_height: checkpoint.height,
                batch_size: 256,
                peers: vec![],
            };
            checkpoint.height
        } else {
            from_height.unwrap_or(0)
        };
        
        // Start sync loop
        self.run_sync_loop(start_height).await
    }
    
    async fn run_sync_loop(&mut self, start_height: u64) -> Result<()> {
        loop {
            match &self.state {
                SyncState::Discovering { started_at, attempts } => {
                    if *attempts > 30 {
                        self.state = SyncState::Failed {
                            reason: "No peers found after 30 attempts".to_string(),
                            last_good_height: start_height,
                            recovery_attempts: 0,
                            next_retry: Instant::now() + Duration::from_secs(60),
                        };
                        continue;
                    }
                    
                    // Request peers from peer manager
                    let peers = self.peer_manager
                        .send(GetAvailablePeers)
                        .await??;
                    
                    if !peers.is_empty() {
                        self.transition_to_downloading_headers(peers).await?;
                    } else {
                        // Keep discovering
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        self.state = SyncState::Discovering {
                            started_at: *started_at,
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
                    if *blocks_behind == 0 {
                        self.state = SyncState::Synced {
                            last_check: Instant::now(),
                            peer_height: self.sync_progress.highest_peer_height,
                        };
                        info!("🎉 Sync complete!");
                        break;
                    }
                    
                    self.catch_up_recent_blocks().await?;
                }
                
                SyncState::Synced { .. } => {
                    // Periodically check if we're still synced
                    self.verify_sync_status().await?;
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
                
                SyncState::Failed { next_retry, .. } => {
                    if Instant::now() >= *next_retry {
                        self.attempt_recovery().await?;
                    } else {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    async fn download_and_process_blocks(&mut self) -> Result<()> {
        if let SyncState::DownloadingBlocks {
            current_height,
            target_height,
            batch_size,
            peers,
            ..
        } = &mut self.state {
            // Get optimal batch size
            let optimal_batch = self.peer_manager
                .send(GetOptimalBatchSize)
                .await??;
            
            *batch_size = optimal_batch;
            
            // Download blocks in parallel from multiple peers
            let download_tasks = peers
                .iter()
                .take(3)  // Use up to 3 peers in parallel
                .enumerate()
                .map(|(i, peer)| {
                    let start = *current_height + (i as u64 * *batch_size as u64);
                    let count = (*batch_size).min((*target_height - start) as usize);
                    
                    self.download_block_range(*peer, start, count)
                })
                .collect::<Vec<_>>();
            
            let results = futures::future::join_all(download_tasks).await;
            
            // Process successful downloads
            for result in results {
                if let Ok(blocks) = result {
                    let processed = self.block_processor
                        .send(ProcessBlockBatch { blocks })
                        .await??;
                    
                    *current_height += processed.processed as u64;
                    self.sync_progress.blocks_processed += processed.processed as u64;
                    self.sync_progress.blocks_failed += processed.failed as u64;
                    
                    // Create checkpoint every N blocks
                    if *current_height % self.checkpoint_manager.checkpoint_interval == 0 {
                        self.create_sync_checkpoint().await?;
                    }
                }
            }
            
            // Update sync speed
            self.update_sync_metrics();
            
            // Check if we're caught up
            if *current_height >= *target_height - 10 {
                self.state = SyncState::CatchingUp {
                    blocks_behind: *target_height - *current_height,
                    sync_speed: self.sync_progress.blocks_per_second,
                    estimated_time: self.estimate_completion_time(),
                };
            }
        }
        
        Ok(())
    }
}
```

## Testing Strategy for Syncing

### 1. Sync Simulator

```rust
/// Comprehensive sync testing framework
pub struct SyncTestHarness {
    // Mock network with configurable behavior
    mock_network: MockP2PNetwork,
    
    // Simulated blockchain
    simulated_chain: SimulatedBlockchain,
    
    // Actor system under test
    sync_actor: Addr<SyncActor>,
    
    // Test configuration
    config: SyncTestConfig,
}

#[derive(Debug, Clone)]
pub struct SyncTestConfig {
    pub chain_height: u64,
    pub block_time: Duration,
    pub network_latency: Duration,
    pub peer_count: usize,
    pub failure_rate: f64,
    pub partition_probability: f64,
}

impl SyncTestHarness {
    /// Test basic sync from genesis
    pub async fn test_sync_from_genesis(&mut self) -> Result<()> {
        // Setup: Create chain with 10,000 blocks
        self.simulated_chain.generate_blocks(10_000).await?;
        
        // Act: Start sync
        self.sync_actor
            .send(SyncMessage::StartSync {
                from_height: Some(0),
                target_height: Some(10_000),
            })
            .await??;
        
        // Wait for completion
        self.wait_for_sync_completion(Duration::from_secs(60)).await?;
        
        // Assert
        let status = self.sync_actor.send(GetSyncStatus).await??;
        assert_eq!(status.current_height, 10_000);
        assert!(matches!(status.state, SyncState::Synced { .. }));
        
        Ok(())
    }
    
    /// Test sync recovery from checkpoint
    pub async fn test_checkpoint_recovery(&mut self) -> Result<()> {
        // Setup: Sync partially then fail
        self.simulated_chain.generate_blocks(5_000).await?;
        self.sync_actor.send(StartSync { .. }).await??;
        
        // Simulate failure at block 2,500
        self.wait_for_height(2_500).await?;
        self.sync_actor.stop();
        
        // Restart sync actor
        self.sync_actor = self.create_new_sync_actor().await?;
        
        // Act: Resume sync (should recover from checkpoint)
        self.sync_actor.send(StartSync { .. }).await??;
        
        // Assert: Should resume from checkpoint, not genesis
        let status = self.sync_actor.send(GetSyncStatus).await??;
        assert!(status.current_height >= 2_400);  // Near checkpoint
        
        Ok(())
    }
    
    /// Test sync with network partitions
    pub async fn test_network_partition(&mut self) -> Result<()> {
        // Setup
        self.simulated_chain.generate_blocks(1_000).await?;
        
        // Start sync
        let sync_handle = tokio::spawn(async move {
            self.sync_actor.send(StartSync { .. }).await
        });
        
        // Simulate network partition after 500 blocks
        self.wait_for_height(500).await?;
        self.mock_network.simulate_partition(Duration::from_secs(10)).await;
        
        // Network should recover
        self.mock_network.heal_partition().await;
        
        // Assert: Sync should complete despite partition
        sync_handle.await??;
        let status = self.sync_actor.send(GetSyncStatus).await??;
        assert_eq!(status.current_height, 1_000);
        
        Ok(())
    }
    
    /// Test parallel block processing
    pub async fn test_parallel_processing(&mut self) -> Result<()> {
        // Setup: Generate blocks with heavy validation
        let blocks = self.generate_complex_blocks(1_000).await?;
        
        // Measure sequential processing time
        let sequential_start = Instant::now();
        for block in &blocks {
            self.process_block_sequential(block).await?;
        }
        let sequential_time = sequential_start.elapsed();
        
        // Measure parallel processing time
        let parallel_start = Instant::now();
        self.sync_actor
            .send(ProcessBlockBatch { blocks })
            .await??;
        let parallel_time = parallel_start.elapsed();
        
        // Assert: Parallel should be significantly faster
        assert!(parallel_time < sequential_time / 2);
        
        Ok(())
    }

/// Property-based testing for sync
#[cfg(test)]
mod sync_property_tests {
    use proptest::prelude::*;
    
    proptest! {
        #[test]
        fn test_sync_completes_eventually(
            chain_height in 100u64..10_000,
            peer_count in 1usize..10,
            failure_rate in 0.0f64..0.3,
        ) {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                let mut harness = SyncTestHarness::new(SyncTestConfig {
                    chain_height,
                    peer_count,
                    failure_rate,
                    ..Default::default()
                });
                
                // Sync should always complete eventually
                let result = harness.test_sync_from_genesis().await;
                assert!(result.is_ok());
            });
        }
        
        #[test]
        fn test_checkpoint_consistency(
            checkpoint_interval in 10u64..100,
            blocks_to_sync in 100u64..1000,
        ) {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                let mut harness = SyncTestHarness::new_with_checkpoint_interval(
                    checkpoint_interval
                );
                
                // All checkpoints should be valid
                harness.sync_to_height(blocks_to_sync).await.unwrap();
                let checkpoints = harness.get_all_checkpoints().await.unwrap();
                
                for checkpoint in checkpoints {
                    assert!(checkpoint.verified);
                    assert!(checkpoint.height % checkpoint_interval == 0);
                }
            });
        }
    }
}
```

### 2. Chaos Testing

```rust
/// Chaos testing for sync resilience
pub struct SyncChaosTest {
    harness: SyncTestHarness,
    chaos_config: ChaosConfig,
}

#[derive(Debug, Clone)]
pub struct ChaosConfig {
    pub random_disconnects: bool,
    pub corrupt_blocks: bool,
    pub slow_peers: bool,
    pub byzantine_peers: bool,
    pub memory_pressure: bool,
}

impl SyncChaosTest {
    pub async fn run_chaos_test(&mut self, duration: Duration) -> Result<ChaosReport> {
        let start = Instant::now();
        let mut report = ChaosReport::default();
        
        // Start sync
        self.harness.sync_actor.send(StartSync { .. }).await??;
        
        // Run chaos events
        while start.elapsed() < duration {
            self.inject_chaos_event(&mut report).await?;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        // Check if sync recovered
        let status = self.harness.sync_actor.send(GetSyncStatus).await??;
        report.final_height = status.current_height;
        report.sync_completed = matches!(status.state, SyncState::Synced { .. });
        
        Ok(report)
    }
    
    async fn inject_chaos_event(&mut self, report: &mut ChaosReport) -> Result<()> {
        let event = self.select_random_chaos_event();
        
        match event {
            ChaosEvent::DisconnectPeer => {
                self.harness.mock_network.disconnect_random_peer().await;
                report.peer_disconnects += 1;
            }
            ChaosEvent::CorruptBlock => {
                self.harness.simulated_chain.corrupt_random_block().await;
                report.corrupted_blocks += 1;
            }
            ChaosEvent::SlowNetwork => {
                self.harness.mock_network.add_latency(Duration::from_secs(5)).await;
                report.network_delays += 1;
            }
            ChaosEvent::ByzantinePeer => {
                self.harness.mock_network.add_byzantine_peer().await;
                report.byzantine_attacks += 1;
            }
        }
        
        Ok(())
    }
}
```

## Metrics and Monitoring

```rust
lazy_static! {
    // Sync state metrics
    pub static ref SYNC_STATE: IntGauge = register_int_gauge!(
        "alys_sync_state",
        "Current sync state (0=discovering, 1=headers, 2=blocks, 3=catchup, 4=synced, 5=failed)"
    ).unwrap();
    
    pub static ref SYNC_CURRENT_HEIGHT: IntGauge = register_int_gauge!(
        "alys_sync_current_height",
        "Current synced height"
    ).unwrap();
    
    pub static ref SYNC_TARGET_HEIGHT: IntGauge = register_int_gauge!(
        "alys_sync_target_height",
        "Target sync height from peers"
    ).unwrap();
    
    pub static ref SYNC_BLOCKS_PER_SECOND: Gauge = register_gauge!(
        "alys_sync_blocks_per_second",
        "Current sync speed in blocks per second"
    ).unwrap();
    
    pub static ref SYNC_BLOCKS_BEHIND: IntGauge = register_int_gauge!(
        "alys_sync_blocks_behind",
        "Number of blocks behind the network"
    ).unwrap();
    
    // Performance metrics
    pub static ref BLOCK_VALIDATION_TIME: Histogram = register_histogram!(
        "alys_block_validation_duration_seconds",
        "Time to validate a block"
    ).unwrap();
    
    pub static ref BLOCK_EXECUTION_TIME: Histogram = register_histogram!(
        "alys_block_execution_duration_seconds",
        "Time to execute a block"
    ).unwrap();
    
    pub static ref BATCH_PROCESSING_TIME: Histogram = register_histogram!(
        "alys_batch_processing_duration_seconds",
        "Time to process a batch of blocks"
    ).unwrap();
    
    // Network metrics
    pub static ref SYNC_PEER_COUNT: IntGauge = register_int_gauge!(
        "alys_sync_peer_count",
        "Number of peers available for sync"
    ).unwrap();
    
    pub static ref PEER_RESPONSE_TIME: HistogramVec = register_histogram_vec!(
        "alys_peer_response_time_seconds",
        "Response time by peer",
        &["peer_id"]
    ).unwrap();
    
    // Error metrics
    pub static ref SYNC_ERRORS: IntCounterVec = register_int_counter_vec!(
        "alys_sync_errors_total",
        "Sync errors by type",
        &["error_type"]
    ).unwrap();
    
    pub static ref SYNC_RECOVERIES: IntCounter = register_int_counter!(
        "alys_sync_recoveries_total",
        "Number of successful sync recoveries"
    ).unwrap();
    
    // Checkpoint metrics
    pub static ref CHECKPOINT_HEIGHT: IntGauge = register_int_gauge!(
        "alys_checkpoint_height",
        "Latest checkpoint height"
    ).unwrap();
    
    pub static ref CHECKPOINTS_CREATED: IntCounter = register_int_counter!(
        "alys_checkpoints_created_total",
        "Total checkpoints created"
    ).unwrap();
}
```

## Configuration for Improved Sync

```toml
# alys-sync.toml
[sync]
# Sync strategy
strategy = "parallel"  # parallel, sequential
max_parallel_downloads = 3
batch_size = "adaptive"  # adaptive, fixed
fixed_batch_size = 256

# Checkpointing
checkpoint_interval = 100  # blocks
max_checkpoints = 10
checkpoint_storage = "/data/checkpoints"

# Recovery
max_recovery_attempts = 5
recovery_backoff_secs = 60
auto_recovery = true

# Peer management
peer_selection = "weighted"  # weighted, round_robin, lowest_latency
min_sync_peers = 3
max_sync_peers = 10
peer_score_threshold = 0.5

# Performance
validation_workers = 4
max_memory_gb = 8
cache_size_mb = 512

# Monitoring
metrics_enabled = true
metrics_port = 9091
log_level = "info"
```

## Migration Plan from Current Sync

### Phase 1: Actor Infrastructure (Week 1-2)
- [ ] Implement SyncActor with basic state machine
- [ ] Create PeerManagerActor for peer selection
- [ ] Set up BlockProcessorActor for parallel validation
- [ ] Add checkpoint system

### Phase 2: Parallel Processing (Week 3)
- [ ] Implement parallel block validation
- [ ] Add worker pool for CPU-intensive operations
- [ ] Create processing pipeline
- [ ] Benchmark performance improvements

### Phase 3: Testing and Metrics (Week 4)
- [ ] Create comprehensive test suite
- [ ] Add chaos testing
- [ ] Implement full metrics
- [ ] Performance profiling

### Phase 4: Production Rollout (Week 5)
- [ ] Gradual rollout with feature flags
- [ ] Monitor metrics and performance
- [ ] Gather feedback and iterate
- [ ] Full deployment

## Summary

The proposed actor-based sync architecture addresses all major issues with the current implementation:

1. **Granular State Management**: Replace binary sync state with detailed state machine
2. **Parallel Processing**: Validate and process blocks in parallel
3. **Smart Peer Selection**: Choose best peers based on performance metrics
4. **Checkpoint Recovery**: Resume sync from checkpoints after failures
5. **Better Production Control**: Enable block production when sync is nearly complete (99.5%)
6. **Comprehensive Testing**: Property-based and chaos testing for reliability
7. **Rich Metrics**: Detailed monitoring of sync performance and health

This architecture will dramatically improve sync reliability, performance, and developer experience while reducing the historical sync issues that have plagued Alys nodes.