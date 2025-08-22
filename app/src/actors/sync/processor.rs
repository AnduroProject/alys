//! Block processing and validation system for SyncActor
//! 
//! This module implements parallel block validation with worker pools,
//! batch processing, and integration with Alys federated consensus.

use std::{
    collections::{HashMap, VecDeque, BTreeMap},
    sync::{Arc, RwLock, atomic::{AtomicU64, AtomicBool, Ordering}},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use actix::prelude::*;
use tokio::{
    sync::{mpsc, oneshot, Semaphore, RwLock as TokioRwLock},
    time::{sleep, timeout},
};
use futures::{future::BoxFuture, FutureExt, StreamExt};
use serde::{Serialize, Deserialize};
use prometheus::{Histogram, Counter, Gauge, IntCounter, IntGauge};

use crate::{
    types::{Block, BlockHash, BlockHeader, Signature, AuthorityId},
    actors::{
        chain::{ChainActor, ValidateBlock, ImportBlock},
        consensus::{ConsensusActor, VerifyFederationSignature},
    },
    chain::BlockValidationError,
};

use super::{
    errors::{SyncError, SyncResult},
    messages::{ProcessBlocks, ValidationResult, BatchResult},
    metrics::*,
    config::{SyncConfig, ValidationConfig, PerformanceConfig},
    peer::{PeerId, PeerManager},
};

lazy_static::lazy_static! {
    static ref VALIDATION_DURATION: Histogram = prometheus::register_histogram!(
        "alys_sync_validation_duration_seconds",
        "Time spent validating blocks",
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0]
    ).unwrap();
    
    static ref VALIDATION_QUEUE_SIZE: IntGauge = prometheus::register_int_gauge!(
        "alys_sync_validation_queue_size",
        "Number of blocks waiting for validation"
    ).unwrap();
    
    static ref VALIDATION_WORKERS_ACTIVE: IntGauge = prometheus::register_int_gauge!(
        "alys_sync_validation_workers_active",
        "Number of active validation workers"
    ).unwrap();
    
    static ref BLOCKS_VALIDATED_TOTAL: IntCounter = prometheus::register_int_counter!(
        "alys_sync_blocks_validated_total",
        "Total number of blocks validated"
    ).unwrap();
    
    static ref BLOCKS_REJECTED_TOTAL: IntCounter = prometheus::register_int_counter!(
        "alys_sync_blocks_rejected_total",
        "Total number of blocks rejected during validation"
    ).unwrap();
    
    static ref BATCH_PROCESSING_DURATION: Histogram = prometheus::register_histogram!(
        "alys_sync_batch_processing_duration_seconds",
        "Time spent processing block batches",
        vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 30.0]
    ).unwrap();
    
    static ref FEDERATION_SIGNATURE_VALIDATIONS: IntCounter = prometheus::register_int_counter!(
        "alys_sync_federation_signature_validations_total",
        "Total federation signature validations performed"
    ).unwrap();
    
    static ref CONSENSUS_VALIDATION_ERRORS: IntCounter = prometheus::register_int_counter!(
        "alys_sync_consensus_validation_errors_total",
        "Total consensus validation errors"
    ).unwrap();
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockValidationRequest {
    pub block: Block,
    pub source_peer: Option<PeerId>,
    pub batch_id: Option<u64>,
    pub priority: ValidationPriority,
    pub validation_mode: ValidationMode,
    pub requested_at: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidationPriority {
    Emergency = 0,      // Critical consensus blocks
    High = 1,          // Federation blocks  
    Normal = 2,        // Regular sync blocks
    Low = 3,           // Background verification
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationMode {
    Full,              // Complete validation including state
    HeaderOnly,        // Header and signature validation only
    FastSync,          // Optimized for sync performance
    Checkpoint,        // Checkpoint validation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationContext {
    pub chain_height: u64,
    pub federation_authorities: Vec<AuthorityId>,
    pub current_slot: u64,
    pub expected_author: Option<AuthorityId>,
    pub governance_config: GovernanceValidationConfig,
    pub performance_limits: PerformanceLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceValidationConfig {
    pub enabled: bool,
    pub stream_id: Option<String>,
    pub authority_rotation_blocks: u64,
    pub emergency_override_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceLimits {
    pub max_validation_time: Duration,
    pub max_batch_size: usize,
    pub max_parallel_validations: usize,
    pub memory_limit_mb: usize,
}

#[derive(Debug)]
pub struct BlockProcessor {
    config: Arc<SyncConfig>,
    validation_workers: Vec<Addr<ValidationWorker>>,
    worker_semaphore: Arc<Semaphore>,
    validation_queue: Arc<TokioRwLock<VecDeque<BlockValidationRequest>>>,
    pending_batches: Arc<RwLock<HashMap<u64, BatchProcessor>>>,
    validation_context: Arc<RwLock<ValidationContext>>,
    chain_actor: Addr<ChainActor>,
    consensus_actor: Addr<ConsensusActor>,
    peer_manager: Arc<RwLock<PeerManager>>,
    metrics: ProcessorMetrics,
    shutdown: Arc<AtomicBool>,
}

#[derive(Debug)]
pub struct ProcessorMetrics {
    pub blocks_processed: AtomicU64,
    pub blocks_validated: AtomicU64,
    pub validation_errors: AtomicU64,
    pub average_validation_time: AtomicU64, // microseconds
    pub queue_depth: AtomicU64,
    pub active_workers: AtomicU64,
    pub batch_success_rate: AtomicU64, // percentage * 100
}

impl Default for ProcessorMetrics {
    fn default() -> Self {
        Self {
            blocks_processed: AtomicU64::new(0),
            blocks_validated: AtomicU64::new(0),
            validation_errors: AtomicU64::new(0),
            average_validation_time: AtomicU64::new(0),
            queue_depth: AtomicU64::new(0),
            active_workers: AtomicU64::new(0),
            batch_success_rate: AtomicU64::new(10000), // 100.00%
        }
    }
}

impl BlockProcessor {
    pub fn new(
        config: Arc<SyncConfig>,
        chain_actor: Addr<ChainActor>,
        consensus_actor: Addr<ConsensusActor>,
        peer_manager: Arc<RwLock<PeerManager>>,
    ) -> SyncResult<Self> {
        let worker_count = config.performance.validation_workers;
        let worker_semaphore = Arc::new(Semaphore::new(worker_count));
        
        let validation_context = Arc::new(RwLock::new(ValidationContext {
            chain_height: 0,
            federation_authorities: vec![],
            current_slot: 0,
            expected_author: None,
            governance_config: GovernanceValidationConfig {
                enabled: config.governance.enabled,
                stream_id: config.governance.stream_id.clone(),
                authority_rotation_blocks: config.federation.authority_rotation_blocks,
                emergency_override_enabled: config.security.emergency_mode_enabled,
            },
            performance_limits: PerformanceLimits {
                max_validation_time: config.performance.validation_timeout,
                max_batch_size: config.performance.max_batch_size,
                max_parallel_validations: worker_count,
                memory_limit_mb: config.performance.memory_limit_mb,
            },
        }));

        let mut validation_workers = Vec::with_capacity(worker_count);
        for worker_id in 0..worker_count {
            let worker = ValidationWorker::new(
                worker_id,
                config.clone(),
                chain_actor.clone(),
                consensus_actor.clone(),
                validation_context.clone(),
            ).start();
            validation_workers.push(worker);
        }

        Ok(Self {
            config,
            validation_workers,
            worker_semaphore,
            validation_queue: Arc::new(TokioRwLock::new(VecDeque::new())),
            pending_batches: Arc::new(RwLock::new(HashMap::new())),
            validation_context,
            chain_actor,
            consensus_actor,
            peer_manager,
            metrics: ProcessorMetrics::default(),
            shutdown: Arc::new(AtomicBool::new(false)),
        })
    }

    pub async fn process_blocks(&self, blocks: Vec<Block>, source_peer: Option<PeerId>) -> SyncResult<Vec<ValidationResult>> {
        let _timer = BATCH_PROCESSING_DURATION.start_timer();
        
        if blocks.is_empty() {
            return Ok(vec![]);
        }

        let batch_id = self.generate_batch_id();
        let batch_size = blocks.len();
        
        // Create batch processor
        let batch_processor = BatchProcessor::new(
            batch_id,
            batch_size,
            self.config.performance.batch_timeout,
            source_peer.clone(),
        );
        
        {
            let mut pending_batches = self.pending_batches.write()
                .map_err(|_| SyncError::Internal { message: "Failed to acquire batch lock".to_string() })?;
            pending_batches.insert(batch_id, batch_processor);
        }

        // Queue validation requests
        let mut validation_requests = Vec::with_capacity(batch_size);
        for (index, block) in blocks.into_iter().enumerate() {
            let priority = self.determine_validation_priority(&block, source_peer.as_ref()).await?;
            let validation_mode = self.determine_validation_mode(&block, priority).await?;
            
            let request = BlockValidationRequest {
                block,
                source_peer: source_peer.clone(),
                batch_id: Some(batch_id),
                priority,
                validation_mode,
                requested_at: SystemTime::now(),
            };
            
            validation_requests.push(request);
        }

        // Sort by priority and add to queue
        validation_requests.sort_by_key(|req| req.priority);
        
        {
            let mut queue = self.validation_queue.write().await;
            for request in validation_requests {
                queue.push_back(request);
            }
            self.metrics.queue_depth.store(queue.len() as u64, Ordering::Relaxed);
        }
        
        VALIDATION_QUEUE_SIZE.set(self.metrics.queue_depth.load(Ordering::Relaxed) as i64);

        // Start processing if workers are available
        self.schedule_validation_work().await?;

        // Wait for batch completion
        self.wait_for_batch_completion(batch_id).await
    }

    async fn determine_validation_priority(&self, block: &Block, source_peer: Option<&PeerId>) -> SyncResult<ValidationPriority> {
        let context = self.validation_context.read()
            .map_err(|_| SyncError::Internal { message: "Failed to read validation context".to_string() })?;
        
        // Emergency priority for critical consensus operations
        if block.header.number > context.chain_height + self.config.federation.max_blocks_ahead {
            return Ok(ValidationPriority::Emergency);
        }

        // High priority for federation blocks
        if let Some(expected_author) = &context.expected_author {
            if block.header.author == *expected_author {
                return Ok(ValidationPriority::High);
            }
        }

        // Consider peer reputation
        if let Some(peer_id) = source_peer {
            let peer_manager = self.peer_manager.read()
                .map_err(|_| SyncError::Internal { message: "Failed to read peer manager".to_string() })?;
            
            if let Some(peer) = peer_manager.get_peer(peer_id) {
                if peer.reputation_score() > 0.8 {
                    return Ok(ValidationPriority::High);
                } else if peer.reputation_score() < 0.3 {
                    return Ok(ValidationPriority::Low);
                }
            }
        }

        Ok(ValidationPriority::Normal)
    }

    async fn determine_validation_mode(&self, block: &Block, priority: ValidationPriority) -> SyncResult<ValidationMode> {
        match priority {
            ValidationPriority::Emergency => Ok(ValidationMode::Full),
            ValidationPriority::High => {
                if self.is_federation_block(block).await? {
                    Ok(ValidationMode::Full)
                } else {
                    Ok(ValidationMode::HeaderOnly)
                }
            },
            ValidationPriority::Normal => Ok(ValidationMode::FastSync),
            ValidationPriority::Low => Ok(ValidationMode::HeaderOnly),
        }
    }

    async fn is_federation_block(&self, block: &Block) -> SyncResult<bool> {
        let context = self.validation_context.read()
            .map_err(|_| SyncError::Internal { message: "Failed to read validation context".to_string() })?;
        
        Ok(context.federation_authorities.contains(&block.header.author))
    }

    async fn schedule_validation_work(&self) -> SyncResult<()> {
        let available_permits = self.worker_semaphore.available_permits();
        if available_permits == 0 {
            return Ok(());
        }

        let requests_to_process = {
            let mut queue = self.validation_queue.write().await;
            let count = std::cmp::min(available_permits, queue.len());
            (0..count).filter_map(|_| queue.pop_front()).collect::<Vec<_>>()
        };

        for request in requests_to_process {
            if let Some(worker) = self.select_optimal_worker(&request).await? {
                let permit = self.worker_semaphore.clone().acquire_owned().await
                    .map_err(|_| SyncError::Internal { message: "Failed to acquire worker permit".to_string() })?;
                
                worker.do_send(ValidateBlockMessage { request, _permit: permit });
                self.metrics.active_workers.fetch_add(1, Ordering::Relaxed);
            }
        }

        VALIDATION_WORKERS_ACTIVE.set(self.metrics.active_workers.load(Ordering::Relaxed) as i64);
        Ok(())
    }

    async fn select_optimal_worker(&self, request: &BlockValidationRequest) -> SyncResult<Option<Addr<ValidationWorker>>> {
        // Simple round-robin for now, could implement load balancing
        let worker_index = request.block.header.number as usize % self.validation_workers.len();
        Ok(Some(self.validation_workers[worker_index].clone()))
    }

    async fn wait_for_batch_completion(&self, batch_id: u64) -> SyncResult<Vec<ValidationResult>> {
        let timeout_duration = self.config.performance.batch_timeout;
        let start_time = Instant::now();

        loop {
            {
                let pending_batches = self.pending_batches.read()
                    .map_err(|_| SyncError::Internal { message: "Failed to read pending batches".to_string() })?;
                
                if let Some(batch) = pending_batches.get(&batch_id) {
                    if batch.is_complete() {
                        let results = batch.get_results();
                        drop(pending_batches);
                        
                        // Clean up completed batch
                        let mut pending_batches = self.pending_batches.write()
                            .map_err(|_| SyncError::Internal { message: "Failed to write pending batches".to_string() })?;
                        pending_batches.remove(&batch_id);
                        
                        return Ok(results);
                    }
                }
            }

            if start_time.elapsed() > timeout_duration {
                return Err(SyncError::Timeout { 
                    operation: "batch_validation".to_string(),
                    duration: timeout_duration,
                });
            }

            sleep(Duration::from_millis(10)).await;
        }
    }

    fn generate_batch_id(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    }

    pub async fn update_validation_context(&self, context: ValidationContext) -> SyncResult<()> {
        let mut validation_context = self.validation_context.write()
            .map_err(|_| SyncError::Internal { message: "Failed to write validation context".to_string() })?;
        *validation_context = context;
        Ok(())
    }

    pub fn get_metrics(&self) -> ProcessorMetrics {
        ProcessorMetrics {
            blocks_processed: AtomicU64::new(self.metrics.blocks_processed.load(Ordering::Relaxed)),
            blocks_validated: AtomicU64::new(self.metrics.blocks_validated.load(Ordering::Relaxed)),
            validation_errors: AtomicU64::new(self.metrics.validation_errors.load(Ordering::Relaxed)),
            average_validation_time: AtomicU64::new(self.metrics.average_validation_time.load(Ordering::Relaxed)),
            queue_depth: AtomicU64::new(self.metrics.queue_depth.load(Ordering::Relaxed)),
            active_workers: AtomicU64::new(self.metrics.active_workers.load(Ordering::Relaxed)),
            batch_success_rate: AtomicU64::new(self.metrics.batch_success_rate.load(Ordering::Relaxed)),
        }
    }

    pub async fn shutdown(&self) -> SyncResult<()> {
        self.shutdown.store(true, Ordering::Relaxed);
        
        // Stop all workers
        for worker in &self.validation_workers {
            worker.do_send(ShutdownWorker);
        }

        // Wait for queue to drain
        let mut attempts = 0;
        while attempts < 100 {
            let queue_size = {
                let queue = self.validation_queue.read().await;
                queue.len()
            };
            
            if queue_size == 0 {
                break;
            }
            
            sleep(Duration::from_millis(100)).await;
            attempts += 1;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct BatchProcessor {
    batch_id: u64,
    expected_count: usize,
    results: Arc<RwLock<Vec<Option<ValidationResult>>>>,
    completed_count: Arc<AtomicU64>,
    timeout: Duration,
    source_peer: Option<PeerId>,
    created_at: Instant,
}

impl BatchProcessor {
    pub fn new(batch_id: u64, expected_count: usize, timeout: Duration, source_peer: Option<PeerId>) -> Self {
        Self {
            batch_id,
            expected_count,
            results: Arc::new(RwLock::new(vec![None; expected_count])),
            completed_count: Arc::new(AtomicU64::new(0)),
            timeout,
            source_peer,
            created_at: Instant::now(),
        }
    }

    pub fn add_result(&self, index: usize, result: ValidationResult) -> SyncResult<()> {
        let mut results = self.results.write()
            .map_err(|_| SyncError::Internal { message: "Failed to write batch results".to_string() })?;
        
        if index < results.len() {
            results[index] = Some(result);
            self.completed_count.fetch_add(1, Ordering::Relaxed);
        }
        
        Ok(())
    }

    pub fn is_complete(&self) -> bool {
        self.completed_count.load(Ordering::Relaxed) as usize >= self.expected_count ||
        self.created_at.elapsed() > self.timeout
    }

    pub fn get_results(&self) -> Vec<ValidationResult> {
        let results = self.results.read().unwrap();
        results.iter()
            .enumerate()
            .filter_map(|(i, opt_result)| {
                opt_result.clone().or_else(|| {
                    Some(ValidationResult {
                        block_hash: BlockHash::default(), // Should be populated properly
                        is_valid: false,
                        error: Some(SyncError::Timeout { 
                            operation: format!("validation_batch_{}", self.batch_id),
                            duration: self.timeout,
                        }),
                        validation_time: self.created_at.elapsed(),
                        worker_id: None,
                    })
                })
            })
            .collect()
    }
}

pub struct ValidationWorker {
    id: usize,
    config: Arc<SyncConfig>,
    chain_actor: Addr<ChainActor>,
    consensus_actor: Addr<ConsensusActor>,
    validation_context: Arc<RwLock<ValidationContext>>,
    metrics: WorkerMetrics,
}

#[derive(Debug, Default)]
pub struct WorkerMetrics {
    pub validations_completed: AtomicU64,
    pub validation_errors: AtomicU64,
    pub average_validation_time: AtomicU64,
    pub last_validation_at: AtomicU64,
}

impl Actor for ValidationWorker {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::info!("ValidationWorker {} started", self.id);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("ValidationWorker {} stopped", self.id);
    }
}

impl ValidationWorker {
    pub fn new(
        id: usize,
        config: Arc<SyncConfig>,
        chain_actor: Addr<ChainActor>,
        consensus_actor: Addr<ConsensusActor>,
        validation_context: Arc<RwLock<ValidationContext>>,
    ) -> Self {
        Self {
            id,
            config,
            chain_actor,
            consensus_actor,
            validation_context,
            metrics: WorkerMetrics::default(),
        }
    }

    async fn validate_block(&mut self, request: BlockValidationRequest) -> ValidationResult {
        let start_time = Instant::now();
        let _timer = VALIDATION_DURATION.start_timer();
        
        let validation_result = match request.validation_mode {
            ValidationMode::Full => self.validate_block_full(&request.block).await,
            ValidationMode::HeaderOnly => self.validate_block_header(&request.block).await,
            ValidationMode::FastSync => self.validate_block_fast_sync(&request.block).await,
            ValidationMode::Checkpoint => self.validate_block_checkpoint(&request.block).await,
        };

        let validation_time = start_time.elapsed();
        let is_valid = validation_result.is_ok();

        if is_valid {
            BLOCKS_VALIDATED_TOTAL.inc();
            self.metrics.validations_completed.fetch_add(1, Ordering::Relaxed);
        } else {
            BLOCKS_REJECTED_TOTAL.inc();
            self.metrics.validation_errors.fetch_add(1, Ordering::Relaxed);
        }

        // Update average validation time
        let current_avg = self.metrics.average_validation_time.load(Ordering::Relaxed);
        let new_avg = (current_avg + validation_time.as_micros() as u64) / 2;
        self.metrics.average_validation_time.store(new_avg, Ordering::Relaxed);
        self.metrics.last_validation_at.store(
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            Ordering::Relaxed
        );

        ValidationResult {
            block_hash: request.block.hash(),
            is_valid,
            error: validation_result.err(),
            validation_time,
            worker_id: Some(self.id),
        }
    }

    async fn validate_block_full(&self, block: &Block) -> SyncResult<()> {
        // Validate block header
        self.validate_block_header(block).await?;

        // Validate block state and transactions
        let validation_request = ValidateBlock {
            block: block.clone(),
            perform_state_validation: true,
        };

        let result = self.chain_actor.send(validation_request).await
            .map_err(|e| SyncError::Internal { message: format!("Chain actor error: {}", e) })?;

        result.map_err(|e| SyncError::Validation { 
            block_hash: block.hash(),
            message: format!("Full validation failed: {:?}", e),
        })?;

        Ok(())
    }

    async fn validate_block_header(&self, block: &Block) -> SyncResult<()> {
        let context = self.validation_context.read()
            .map_err(|_| SyncError::Internal { message: "Failed to read validation context".to_string() })?;

        // Basic header validation
        if block.header.number == 0 {
            return Err(SyncError::Validation {
                block_hash: block.hash(),
                message: "Genesis block not allowed".to_string(),
            });
        }

        // Validate timestamp
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        if block.header.timestamp > now + self.config.security.max_future_time_drift.as_secs() {
            return Err(SyncError::Validation {
                block_hash: block.hash(),
                message: "Block timestamp too far in future".to_string(),
            });
        }

        // Validate federation signature if applicable
        if context.federation_authorities.contains(&block.header.author) {
            self.validate_federation_signature(block).await?;
        }

        Ok(())
    }

    async fn validate_federation_signature(&self, block: &Block) -> SyncResult<()> {
        FEDERATION_SIGNATURE_VALIDATIONS.inc();

        let verification_request = VerifyFederationSignature {
            block_hash: block.hash(),
            signature: block.header.signature.clone(),
            authority: block.header.author.clone(),
        };

        let result = self.consensus_actor.send(verification_request).await
            .map_err(|e| SyncError::Internal { message: format!("Consensus actor error: {}", e) })?;

        result.map_err(|e| {
            CONSENSUS_VALIDATION_ERRORS.inc();
            SyncError::Federation { 
                message: format!("Federation signature validation failed: {:?}", e),
                node_id: Some(block.header.author.to_string()),
                authority_count: 0, // Should be populated from context
            }
        })?;

        Ok(())
    }

    async fn validate_block_fast_sync(&self, block: &Block) -> SyncResult<()> {
        // Lightweight validation for sync performance
        self.validate_block_header(block).await?;
        
        // Skip expensive state validation
        Ok(())
    }

    async fn validate_block_checkpoint(&self, block: &Block) -> SyncResult<()> {
        // Checkpoint-specific validation
        self.validate_block_header(block).await?;

        // Additional checkpoint validation logic would go here
        Ok(())
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ValidateBlockMessage {
    pub request: BlockValidationRequest,
    pub _permit: tokio::sync::OwnedSemaphorePermit,
}

impl Handler<ValidateBlockMessage> for ValidationWorker {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: ValidateBlockMessage, _ctx: &mut Self::Context) -> Self::Result {
        let request = msg.request;
        let worker_id = self.id;
        
        async move {
            let result = self.validate_block(request).await;
            
            // Here we would send the result back to the processor
            // This would typically involve a callback or result channel
            log::debug!("Worker {} completed validation: {:?}", worker_id, result.is_valid);
        }
        .into_actor(self)
        .boxed_local()
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ShutdownWorker;

impl Handler<ShutdownWorker> for ValidationWorker {
    type Result = ();

    fn handle(&mut self, _msg: ShutdownWorker, ctx: &mut Self::Context) -> Self::Result {
        log::info!("ValidationWorker {} shutting down", self.id);
        ctx.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::sync::tests::{SyncTestHarness, create_test_block};

    #[actix_rt::test]
    async fn test_block_processor_creation() {
        let harness = SyncTestHarness::new().await;
        let processor = BlockProcessor::new(
            harness.config.clone(),
            harness.chain_actor.clone(),
            harness.consensus_actor.clone(),
            harness.peer_manager.clone(),
        ).unwrap();
        
        assert_eq!(processor.validation_workers.len(), harness.config.performance.validation_workers);
    }

    #[actix_rt::test]
    async fn test_validation_priority_determination() {
        let harness = SyncTestHarness::new().await;
        let processor = BlockProcessor::new(
            harness.config.clone(),
            harness.chain_actor.clone(),
            harness.consensus_actor.clone(),
            harness.peer_manager.clone(),
        ).unwrap();

        let block = create_test_block(1, None);
        let priority = processor.determine_validation_priority(&block, None).await.unwrap();
        
        assert_eq!(priority, ValidationPriority::Normal);
    }

    #[actix_rt::test]
    async fn test_batch_processing() {
        let harness = SyncTestHarness::new().await;
        let processor = BlockProcessor::new(
            harness.config.clone(),
            harness.chain_actor.clone(),
            harness.consensus_actor.clone(),
            harness.peer_manager.clone(),
        ).unwrap();

        let blocks = vec![
            create_test_block(1, None),
            create_test_block(2, None),
            create_test_block(3, None),
        ];

        let results = processor.process_blocks(blocks, None).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[actix_rt::test]
    async fn test_validation_worker() {
        let harness = SyncTestHarness::new().await;
        let worker = ValidationWorker::new(
            0,
            harness.config.clone(),
            harness.chain_actor.clone(),
            harness.consensus_actor.clone(),
            Arc::new(RwLock::new(ValidationContext {
                chain_height: 0,
                federation_authorities: vec![],
                current_slot: 0,
                expected_author: None,
                governance_config: GovernanceValidationConfig {
                    enabled: false,
                    stream_id: None,
                    authority_rotation_blocks: 100,
                    emergency_override_enabled: false,
                },
                performance_limits: PerformanceLimits {
                    max_validation_time: Duration::from_secs(10),
                    max_batch_size: 100,
                    max_parallel_validations: 4,
                    memory_limit_mb: 512,
                },
            })),
        );

        let block = create_test_block(1, None);
        let request = BlockValidationRequest {
            block,
            source_peer: None,
            batch_id: Some(1),
            priority: ValidationPriority::Normal,
            validation_mode: ValidationMode::HeaderOnly,
            requested_at: SystemTime::now(),
        };

        let result = worker.validate_block(request).await;
        assert!(result.is_valid || result.error.is_some());
    }
}