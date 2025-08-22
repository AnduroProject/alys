//! Enhanced Engine API for Lighthouse V4/V5 Compatibility
//! 
//! This module provides an upgraded Engine API implementation that works
//! seamlessly with both Lighthouse v4 and v5 through the compatibility layer.
//! It maintains backward compatibility while enabling new v5 features.

use crate::compat::LighthouseCompat;
use crate::config::MigrationMode;
use crate::error::{CompatError, CompatResult};
use crate::metrics::MetricsCollector;
use crate::types::{
    Address, ConsensusAmount, ExecutionBlockHash, ExecutionPayload, ForkchoiceState,
    PayloadAttributes, PayloadId, Withdrawal,
};
use actix::prelude::*;
use chrono::{DateTime, Utc};
use ethereum_types::{H256, U256};
use ethers_core::types::TransactionReceipt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, trace, warn};

/// Enhanced Engine API with Lighthouse compatibility
pub struct CompatibleEngine {
    /// Lighthouse compatibility layer
    compat_layer: Arc<LighthouseCompat>,
    /// Metrics collector for monitoring
    metrics: Arc<MetricsCollector>,
    /// Current finalized block
    finalized: Arc<RwLock<Option<ExecutionBlockHash>>>,
    /// Engine configuration
    config: EngineConfig,
    /// Request context tracking
    request_context: Arc<RwLock<HashMap<String, RequestContext>>>,
    /// Payload cache for optimization
    payload_cache: Arc<RwLock<HashMap<PayloadId, CachedPayload>>>,
}

/// Engine configuration with v4/v5 compatibility options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Default timeout for engine API calls
    pub default_timeout: Duration,
    /// Maximum number of retries for failed requests
    pub max_retries: u32,
    /// Enable payload caching for performance
    pub enable_payload_cache: bool,
    /// Cache expiration time
    pub cache_expiration: Duration,
    /// Enable request batching
    pub enable_batching: bool,
    /// Maximum batch size
    pub max_batch_size: u32,
    /// Enable background health monitoring
    pub enable_health_monitoring: bool,
    /// Fork configuration
    pub fork_config: ForkConfig,
}

/// Fork-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkConfig {
    /// Current fork name (Capella, Deneb, etc.)
    pub current_fork: ForkName,
    /// Upcoming fork transition block
    pub transition_block: Option<u64>,
    /// Fork-specific feature flags
    pub features: HashMap<String, bool>,
}

/// Fork names for version compatibility
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ForkName {
    Capella,
    Deneb,
    Electra,
}

/// Request context for tracking and debugging
#[derive(Debug, Clone)]
pub struct RequestContext {
    pub request_id: String,
    pub method: String,
    pub started_at: DateTime<Utc>,
    pub lighthouse_version: String,
    pub migration_mode: MigrationMode,
    pub retry_count: u32,
    pub payload_id: Option<PayloadId>,
    pub metadata: HashMap<String, String>,
}

/// Cached payload with metadata
#[derive(Debug, Clone)]
pub struct CachedPayload {
    pub payload: ExecutionPayload,
    pub cached_at: DateTime<Utc>,
    pub access_count: u32,
    pub lighthouse_version: String,
    pub block_value: U256,
}

/// Engine API response wrapper with compatibility metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineResponse<T> {
    pub result: T,
    pub lighthouse_version: String,
    pub migration_mode: MigrationMode,
    pub processing_time_ms: u64,
    pub request_id: String,
    pub metadata: HashMap<String, String>,
}

/// Enhanced forkchoice update response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkchoiceUpdateResponse {
    pub payload_status: PayloadStatus,
    pub payload_id: Option<PayloadId>,
    pub validation_error: Option<String>,
    /// V5-specific fields
    pub blob_validation_error: Option<String>,
    pub execution_optimistic: Option<bool>,
}

/// Enhanced payload status with v5 features
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PayloadStatus {
    Valid,
    Invalid,
    Syncing,
    Accepted,
    InvalidBlockHash,
    /// V5-specific statuses
    InvalidTerminalBlock,
    BlobValidationError,
}

/// New payload response with v5 compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewPayloadResponse {
    pub status: PayloadStatus,
    pub latest_valid_hash: Option<ExecutionBlockHash>,
    pub validation_error: Option<String>,
    /// V5-specific fields
    pub blob_validation_error: Option<String>,
}

/// Enhanced get payload response supporting both v4 and v5
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPayloadResponse {
    pub execution_payload: ExecutionPayload,
    pub block_value: U256,
    /// V5-specific fields
    pub blob_bundle: Option<BlobBundle>,
    pub should_override_builder: Option<bool>,
}

/// V5 blob bundle for Deneb and later forks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobBundle {
    pub commitments: Vec<[u8; 48]>,
    pub proofs: Vec<[u8; 48]>,
    pub blobs: Vec<[u8; 131072]>, // 4096 field elements * 32 bytes
}

/// Batch request for multiple engine operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRequest {
    pub requests: Vec<EngineRequest>,
    pub batch_id: String,
    pub max_parallel: Option<u32>,
}

/// Individual engine request in a batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineRequest {
    ForkchoiceUpdated {
        state: ForkchoiceState,
        attributes: Option<PayloadAttributes>,
    },
    NewPayload {
        payload: ExecutionPayload,
    },
    GetPayload {
        payload_id: PayloadId,
    },
    GetPayloadBodies {
        block_hashes: Vec<ExecutionBlockHash>,
    },
}

/// Enhanced balance addition with v5 features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddBalance {
    pub address: Address,
    pub amount: ConsensusAmount,
    /// V5-specific fields
    pub validator_index: Option<u64>,
    pub withdrawal_credentials: Option<[u8; 32]>,
}

impl From<(Address, ConsensusAmount)> for AddBalance {
    fn from((address, amount): (Address, ConsensusAmount)) -> Self {
        Self {
            address,
            amount,
            validator_index: None,
            withdrawal_credentials: None,
        }
    }
}

impl From<AddBalance> for Withdrawal {
    fn from(value: AddBalance) -> Self {
        Withdrawal {
            index: 0,
            validator_index: value.validator_index.unwrap_or(0),
            address: value.address,
            amount: value.amount.0,
        }
    }
}

impl CompatibleEngine {
    /// Create a new compatible engine with the given compatibility layer
    pub async fn new(
        compat_layer: Arc<LighthouseCompat>,
        metrics: Arc<MetricsCollector>,
        config: EngineConfig,
    ) -> CompatResult<Self> {
        let engine = Self {
            compat_layer,
            metrics,
            finalized: Arc::new(RwLock::new(None)),
            config,
            request_context: Arc::new(RwLock::new(HashMap::new())),
            payload_cache: Arc::new(RwLock::new(HashMap::new())),
        };

        // Start background tasks
        if engine.config.enable_health_monitoring {
            engine.start_health_monitoring().await?;
        }

        if engine.config.enable_payload_cache {
            engine.start_cache_cleanup().await?;
        }

        info!("Compatible Engine initialized with fork: {:?}", engine.config.fork_config.current_fork);
        Ok(engine)
    }

    /// Set the finalized block hash
    pub async fn set_finalized(&self, block_hash: ExecutionBlockHash) -> CompatResult<()> {
        *self.finalized.write().await = Some(block_hash);
        
        // Update metrics
        self.metrics.record_finalized_block(block_hash).await;
        
        info!("Set finalized block: {:?}", block_hash);
        Ok(())
    }

    /// Build a new execution block with v4/v5 compatibility
    pub async fn build_block(
        &self,
        timestamp: Duration,
        payload_head: Option<ExecutionBlockHash>,
        add_balances: Vec<AddBalance>,
        parent_beacon_block_root: Option<H256>, // V5 feature
    ) -> CompatResult<ExecutionPayload> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        // Create request context
        let context = self.create_request_context(
            request_id.clone(),
            "build_block".to_string(),
            None,
        ).await?;

        // Record metrics
        self.metrics.record_engine_request_started("build_block").await;

        trace!(
            request_id = %request_id,
            timestamp = ?timestamp,
            payload_head = ?payload_head,
            add_balances_count = add_balances.len(),
            "Building new execution block"
        );

        // Prepare withdrawals
        let withdrawals: Vec<Withdrawal> = add_balances
            .into_iter()
            .map(Into::into)
            .collect();

        // Create payload attributes with fork-specific features
        let mut payload_attributes = PayloadAttributes {
            timestamp: timestamp.as_secs(),
            prev_randao: H256::default(), // TODO: set proper randao
            suggested_fee_recipient: self.get_burn_address(),
            withdrawals: Some(withdrawals),
            parent_beacon_block_root,
        };

        // Apply fork-specific modifications
        self.apply_fork_specific_attributes(&mut payload_attributes).await?;

        // Determine head block
        let head = match payload_head {
            Some(head) => head,
            None => self.get_latest_block_hash().await?,
        };

        // Get finalized block
        let finalized = self.finalized.read().await.unwrap_or_default();

        // Create forkchoice state
        let forkchoice_state = ForkchoiceState {
            head_block_hash: head,
            finalized_block_hash: finalized,
            safe_block_hash: finalized,
        };

        // Execute forkchoice update through compatibility layer
        let forkchoice_response = self
            .compat_layer
            .forkchoice_updated(forkchoice_state, Some(payload_attributes))
            .await?;

        let payload_id = forkchoice_response.payload_id
            .ok_or_else(|| CompatError::PayloadIdUnavailable)?;

        trace!(
            request_id = %request_id,
            payload_id = ?payload_id,
            "Forkchoice updated successfully"
        );

        // Get the payload
        let get_payload_response = self
            .compat_layer
            .get_payload(payload_id)
            .await?;

        let execution_payload = get_payload_response.execution_payload;

        // Cache the payload if enabled
        if self.config.enable_payload_cache {
            self.cache_payload(
                payload_id,
                execution_payload.clone(),
                get_payload_response.block_value,
            ).await?;
        }

        // Update context and metrics
        let processing_time = start_time.elapsed();
        self.update_request_context(&request_id, |ctx| {
            ctx.payload_id = Some(payload_id);
        }).await?;

        self.metrics.record_engine_request_completed(
            "build_block",
            processing_time.as_millis() as u64,
            true,
        ).await;

        info!(
            request_id = %request_id,
            block_hash = ?execution_payload.block_hash,
            processing_time_ms = processing_time.as_millis(),
            block_value = %get_payload_response.block_value,
            "Block built successfully"
        );

        Ok(execution_payload)
    }

    /// Commit an execution block with enhanced error handling
    pub async fn commit_block(
        &self,
        execution_payload: ExecutionPayload,
    ) -> CompatResult<ExecutionBlockHash> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        // Create request context
        let context = self.create_request_context(
            request_id.clone(),
            "commit_block".to_string(),
            None,
        ).await?;

        // Record metrics
        self.metrics.record_engine_request_started("commit_block").await;

        trace!(
            request_id = %request_id,
            block_hash = ?execution_payload.block_hash,
            parent_hash = ?execution_payload.parent_hash,
            "Committing execution block"
        );

        let finalized = self.finalized.read().await.unwrap_or_default();

        // First, update forkchoice to parent
        let parent_forkchoice = ForkchoiceState {
            head_block_hash: execution_payload.parent_hash,
            safe_block_hash: finalized,
            finalized_block_hash: finalized,
        };

        self.compat_layer
            .forkchoice_updated(parent_forkchoice, None)
            .await?;

        // Submit the new payload
        let new_payload_response = self
            .compat_layer
            .new_payload(execution_payload.clone())
            .await?;

        // Validate the response
        match new_payload_response.status {
            PayloadStatus::Valid | PayloadStatus::Accepted => {
                // Payload is valid, continue
            },
            PayloadStatus::Invalid => {
                let error_msg = new_payload_response.validation_error
                    .unwrap_or_else(|| "Invalid payload".to_string());
                
                self.metrics.record_engine_request_completed(
                    "commit_block",
                    start_time.elapsed().as_millis() as u64,
                    false,
                ).await;

                return Err(CompatError::InvalidPayload { reason: error_msg });
            },
            PayloadStatus::Syncing => {
                warn!(
                    request_id = %request_id,
                    "Engine is syncing, retrying commit"
                );
                
                // Retry with exponential backoff
                return self.retry_commit_block(execution_payload, request_id).await;
            },
            _ => {
                let error_msg = format!("Unexpected payload status: {:?}", new_payload_response.status);
                
                self.metrics.record_engine_request_completed(
                    "commit_block",
                    start_time.elapsed().as_millis() as u64,
                    false,
                ).await;

                return Err(CompatError::EngineApiError { reason: error_msg });
            }
        }

        let block_hash = new_payload_response.latest_valid_hash
            .ok_or_else(|| CompatError::InvalidBlockHash)?;

        // Update forkchoice to the new head
        let new_forkchoice = ForkchoiceState {
            head_block_hash: block_hash,
            safe_block_hash: finalized,
            finalized_block_hash: finalized,
        };

        self.compat_layer
            .forkchoice_updated(new_forkchoice, None)
            .await?;

        // Update metrics and context
        let processing_time = start_time.elapsed();
        self.metrics.record_engine_request_completed(
            "commit_block",
            processing_time.as_millis() as u64,
            true,
        ).await;

        info!(
            request_id = %request_id,
            block_hash = ?block_hash,
            processing_time_ms = processing_time.as_millis(),
            "Block committed successfully"
        );

        Ok(block_hash)
    }

    /// Get block with transactions using compatibility layer
    pub async fn get_block_with_txs(
        &self,
        block_hash: &ExecutionBlockHash,
    ) -> CompatResult<Option<serde_json::Value>> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        trace!(
            request_id = %request_id,
            block_hash = ?block_hash,
            "Fetching block with transactions"
        );

        let params = json!([block_hash, true]);
        let result = self
            .compat_layer
            .rpc_request("eth_getBlockByHash", params)
            .await?;

        let processing_time = start_time.elapsed();
        
        debug!(
            request_id = %request_id,
            block_hash = ?block_hash,
            processing_time_ms = processing_time.as_millis(),
            "Block with transactions retrieved"
        );

        Ok(result)
    }

    /// Get transaction receipt with retry logic
    pub async fn get_transaction_receipt(
        &self,
        transaction_hash: H256,
    ) -> CompatResult<Option<TransactionReceipt>> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let max_retries = self.config.max_retries;
        
        for attempt in 0..max_retries {
            trace!(
                request_id = %request_id,
                transaction_hash = ?transaction_hash,
                attempt = attempt + 1,
                max_retries = max_retries,
                "Fetching transaction receipt"
            );

            let params = json!([transaction_hash]);
            match self
                .compat_layer
                .rpc_request("eth_getTransactionReceipt", params)
                .await
            {
                Ok(result) => {
                    debug!(
                        request_id = %request_id,
                        transaction_hash = ?transaction_hash,
                        attempt = attempt + 1,
                        "Transaction receipt retrieved successfully"
                    );
                    return Ok(result);
                },
                Err(e) => {
                    if attempt == max_retries - 1 {
                        error!(
                            request_id = %request_id,
                            transaction_hash = ?transaction_hash,
                            error = %e,
                            "Failed to fetch transaction receipt after all retries"
                        );
                        return Err(e);
                    } else {
                        warn!(
                            request_id = %request_id,
                            transaction_hash = ?transaction_hash,
                            attempt = attempt + 1,
                            error = %e,
                            "Transaction receipt fetch failed, retrying"
                        );
                        
                        // Exponential backoff
                        let delay = Duration::from_millis(500 * (2_u64.pow(attempt)));
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        unreachable!("Should have returned or errored in the loop");
    }

    /// Enhanced get payload by tag with fork support
    pub async fn get_payload_by_tag(
        &self,
        tag: &str,
    ) -> CompatResult<ExecutionPayload> {
        let request_id = uuid::Uuid::new_v4().to_string();
        
        trace!(
            request_id = %request_id,
            tag = tag,
            fork = ?self.config.fork_config.current_fork,
            "Fetching payload by tag"
        );

        // Use the compatibility layer to get the block
        let params = json!([tag, false]);
        let block_data: serde_json::Value = self
            .compat_layer
            .rpc_request("eth_getBlockByNumber", params)
            .await?
            .ok_or_else(|| CompatError::BlockNotFound {
                identifier: tag.to_string(),
            })?;

        // Convert to ExecutionPayload based on current fork
        let payload = self.convert_block_to_payload(block_data).await?;

        debug!(
            request_id = %request_id,
            tag = tag,
            block_hash = ?payload.block_hash,
            "Payload retrieved by tag"
        );

        Ok(payload)
    }

    /// Batch multiple engine requests for efficiency
    pub async fn batch_requests(
        &self,
        batch: BatchRequest,
    ) -> CompatResult<Vec<CompatResult<serde_json::Value>>> {
        let request_id = batch.batch_id.clone();
        let start_time = std::time::Instant::now();
        
        info!(
            request_id = %request_id,
            request_count = batch.requests.len(),
            "Processing batch request"
        );

        if !self.config.enable_batching {
            return Err(CompatError::BatchingDisabled);
        }

        if batch.requests.len() > self.config.max_batch_size as usize {
            return Err(CompatError::BatchTooLarge {
                requested: batch.requests.len(),
                max_allowed: self.config.max_batch_size as usize,
            });
        }

        // Process requests in parallel with optional concurrency limit
        let max_parallel = batch.max_parallel
            .unwrap_or(batch.requests.len() as u32)
            .min(batch.requests.len() as u32);

        let mut results = Vec::new();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_parallel as usize));

        let mut handles = Vec::new();
        
        for (index, request) in batch.requests.into_iter().enumerate() {
            let permit = semaphore.clone();
            let compat_layer = Arc::clone(&self.compat_layer);
            let req_id = format!("{}-{}", request_id, index);
            
            let handle = tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();
                Self::execute_batch_request(compat_layer, request, req_id).await
            });
            
            handles.push(handle);
        }

        // Collect results maintaining order
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(Err(CompatError::BatchRequestFailed {
                    reason: format!("Task join error: {}", e),
                })),
            }
        }

        let processing_time = start_time.elapsed();
        
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        
        info!(
            request_id = %request_id,
            total_requests = results.len(),
            successful_requests = success_count,
            processing_time_ms = processing_time.as_millis(),
            "Batch request completed"
        );

        self.metrics.record_batch_request_completed(
            results.len(),
            success_count,
            processing_time.as_millis() as u64,
        ).await;

        Ok(results)
    }

    /// Execute individual request in a batch
    async fn execute_batch_request(
        compat_layer: Arc<LighthouseCompat>,
        request: EngineRequest,
        request_id: String,
    ) -> CompatResult<serde_json::Value> {
        match request {
            EngineRequest::ForkchoiceUpdated { state, attributes } => {
                let response = compat_layer.forkchoice_updated(state, attributes).await?;
                Ok(serde_json::to_value(response)?)
            },
            EngineRequest::NewPayload { payload } => {
                let response = compat_layer.new_payload(payload).await?;
                Ok(serde_json::to_value(response)?)
            },
            EngineRequest::GetPayload { payload_id } => {
                let response = compat_layer.get_payload(payload_id).await?;
                Ok(serde_json::to_value(response)?)
            },
            EngineRequest::GetPayloadBodies { block_hashes } => {
                // V5 feature - get payload bodies for multiple blocks
                let mut bodies = Vec::new();
                for hash in block_hashes {
                    let params = json!([hash, true]);
                    if let Ok(Some(block)) = compat_layer.rpc_request("eth_getBlockByHash", params).await {
                        bodies.push(block);
                    }
                }
                Ok(serde_json::to_value(bodies)?)
            }
        }
    }

    /// Retry commit block with exponential backoff
    async fn retry_commit_block(
        &self,
        execution_payload: ExecutionPayload,
        request_id: String,
    ) -> CompatResult<ExecutionBlockHash> {
        let max_retries = self.config.max_retries;
        
        for attempt in 1..=max_retries {
            let delay = Duration::from_millis(1000 * (2_u64.pow(attempt - 1)));
            
            warn!(
                request_id = %request_id,
                attempt = attempt,
                delay_ms = delay.as_millis(),
                "Retrying commit block after delay"
            );
            
            tokio::time::sleep(delay).await;
            
            match self.commit_block(execution_payload.clone()).await {
                Ok(block_hash) => {
                    info!(
                        request_id = %request_id,
                        attempt = attempt,
                        block_hash = ?block_hash,
                        "Commit block succeeded on retry"
                    );
                    return Ok(block_hash);
                },
                Err(e) => {
                    if attempt == max_retries {
                        error!(
                            request_id = %request_id,
                            attempt = attempt,
                            error = %e,
                            "Commit block failed after all retries"
                        );
                        return Err(e);
                    }
                }
            }
        }
        
        unreachable!()
    }

    /// Get the burn address for fee recipient
    fn get_burn_address(&self) -> Address {
        // Use dead address for burning fees
        Address::from([0u8; 20]) // 0x000...000
    }

    /// Get latest block hash from the execution layer
    async fn get_latest_block_hash(&self) -> CompatResult<ExecutionBlockHash> {
        let params = json!(["latest", false]);
        let block_data: serde_json::Value = self
            .compat_layer
            .rpc_request("eth_getBlockByNumber", params)
            .await?
            .ok_or_else(|| CompatError::BlockNotFound {
                identifier: "latest".to_string(),
            })?;

        let hash_str = block_data["hash"].as_str()
            .ok_or_else(|| CompatError::InvalidBlockData {
                reason: "Missing block hash".to_string(),
            })?;

        let hash = ExecutionBlockHash::from_str(hash_str)
            .map_err(|_| CompatError::InvalidBlockData {
                reason: "Invalid block hash format".to_string(),
            })?;

        Ok(hash)
    }

    /// Apply fork-specific modifications to payload attributes
    async fn apply_fork_specific_attributes(
        &self,
        attributes: &mut PayloadAttributes,
    ) -> CompatResult<()> {
        match self.config.fork_config.current_fork {
            ForkName::Capella => {
                // Capella-specific attributes (withdrawals support)
                // Already handled in the main flow
            },
            ForkName::Deneb => {
                // Deneb-specific attributes (blob transactions support)
                if attributes.parent_beacon_block_root.is_none() {
                    // Set a default parent beacon block root for Deneb
                    attributes.parent_beacon_block_root = Some(H256::default());
                }
            },
            ForkName::Electra => {
                // Future fork support
                warn!("Electra fork not fully implemented yet");
            }
        }
        
        Ok(())
    }

    /// Convert block data to execution payload
    async fn convert_block_to_payload(
        &self,
        block_data: serde_json::Value,
    ) -> CompatResult<ExecutionPayload> {
        // This would use the compatibility layer's conversion functions
        // For now, return a placeholder
        Err(CompatError::NotImplemented {
            feature: "Block to payload conversion".to_string(),
        })
    }

    /// Cache a payload for performance optimization
    async fn cache_payload(
        &self,
        payload_id: PayloadId,
        payload: ExecutionPayload,
        block_value: U256,
    ) -> CompatResult<()> {
        if !self.config.enable_payload_cache {
            return Ok(());
        }

        let cached_payload = CachedPayload {
            payload,
            cached_at: Utc::now(),
            access_count: 0,
            lighthouse_version: self.compat_layer.get_current_version().await?,
            block_value,
        };

        let mut cache = self.payload_cache.write().await;
        cache.insert(payload_id, cached_payload);

        trace!(payload_id = ?payload_id, "Payload cached successfully");
        Ok(())
    }

    /// Create request context for tracking
    async fn create_request_context(
        &self,
        request_id: String,
        method: String,
        payload_id: Option<PayloadId>,
    ) -> CompatResult<RequestContext> {
        let context = RequestContext {
            request_id: request_id.clone(),
            method,
            started_at: Utc::now(),
            lighthouse_version: self.compat_layer.get_current_version().await?,
            migration_mode: self.compat_layer.get_current_mode().await?,
            retry_count: 0,
            payload_id,
            metadata: HashMap::new(),
        };

        let mut contexts = self.request_context.write().await;
        contexts.insert(request_id.clone(), context.clone());

        Ok(context)
    }

    /// Update request context
    async fn update_request_context<F>(
        &self,
        request_id: &str,
        updater: F,
    ) -> CompatResult<()>
    where
        F: FnOnce(&mut RequestContext),
    {
        let mut contexts = self.request_context.write().await;
        if let Some(context) = contexts.get_mut(request_id) {
            updater(context);
        }
        Ok(())
    }

    /// Start background health monitoring
    async fn start_health_monitoring(&self) -> CompatResult<()> {
        let compat_layer = Arc::clone(&self.compat_layer);
        let metrics = Arc::clone(&self.metrics);
        
        actix::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                // Monitor engine health
                if let Err(e) = Self::check_engine_health(&compat_layer, &metrics).await {
                    error!("Engine health check failed: {}", e);
                }
            }
        });
        
        Ok(())
    }

    /// Check engine health
    async fn check_engine_health(
        compat_layer: &Arc<LighthouseCompat>,
        metrics: &Arc<MetricsCollector>,
    ) -> CompatResult<()> {
        // Check if the execution layer is responsive
        let params = json!(["latest", false]);
        let result = compat_layer.rpc_request("eth_getBlockByNumber", params).await;
        
        let is_healthy = result.is_ok();
        metrics.record_engine_health(is_healthy).await;
        
        if is_healthy {
            trace!("Engine health check passed");
        } else {
            warn!("Engine health check failed: {:?}", result);
        }
        
        Ok(())
    }

    /// Start cache cleanup background task
    async fn start_cache_cleanup(&self) -> CompatResult<()> {
        let payload_cache = Arc::clone(&self.payload_cache);
        let cache_expiration = self.config.cache_expiration;
        
        actix::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // Every 5 minutes
            
            loop {
                interval.tick().await;
                
                let now = Utc::now();
                let mut cache = payload_cache.write().await;
                let initial_size = cache.len();
                
                cache.retain(|_, cached_payload| {
                    now.signed_duration_since(cached_payload.cached_at).to_std()
                        .unwrap_or(Duration::ZERO) < cache_expiration
                });
                
                let cleaned_count = initial_size - cache.len();
                if cleaned_count > 0 {
                    debug!(
                        cleaned_payloads = cleaned_count,
                        remaining_payloads = cache.len(),
                        "Cleaned expired payload cache entries"
                    );
                }
            }
        });
        
        Ok(())
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
            max_retries: 3,
            enable_payload_cache: true,
            cache_expiration: Duration::from_secs(300), // 5 minutes
            enable_batching: true,
            max_batch_size: 10,
            enable_health_monitoring: true,
            fork_config: ForkConfig {
                current_fork: ForkName::Capella,
                transition_block: None,
                features: HashMap::new(),
            },
        }
    }
}

use std::str::FromStr;

impl FromStr for ExecutionBlockHash {
    type Err = CompatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Parse the hex string into ExecutionBlockHash
        let s = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(s)
            .map_err(|_| CompatError::InvalidBlockData {
                reason: "Invalid hex format".to_string(),
            })?;
        
        if bytes.len() != 32 {
            return Err(CompatError::InvalidBlockData {
                reason: "Invalid hash length".to_string(),
            });
        }
        
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&bytes);
        Ok(ExecutionBlockHash(hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_engine_config_default() {
        let config = EngineConfig::default();
        assert_eq!(config.max_retries, 3);
        assert!(config.enable_payload_cache);
        assert_eq!(config.fork_config.current_fork, ForkName::Capella);
    }

    #[test]
    fn test_add_balance_conversion() {
        let add_balance = AddBalance {
            address: Address::default(),
            amount: ConsensusAmount(1000),
            validator_index: Some(42),
            withdrawal_credentials: None,
        };
        
        let withdrawal: Withdrawal = add_balance.into();
        assert_eq!(withdrawal.validator_index, 42);
        assert_eq!(withdrawal.amount, 1000);
    }

    #[test]
    fn test_execution_block_hash_from_str() {
        let hash_str = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let hash = ExecutionBlockHash::from_str(hash_str);
        assert!(hash.is_ok());
        
        let invalid_hash = "invalid_hash";
        let invalid_result = ExecutionBlockHash::from_str(invalid_hash);
        assert!(invalid_result.is_err());
    }

    #[tokio::test]
    async fn test_batch_request_validation() {
        // Test would require mocking the compatibility layer
        // This is a placeholder for the structure
        let batch = BatchRequest {
            requests: vec![],
            batch_id: "test_batch".to_string(),
            max_parallel: Some(5),
        };
        
        assert_eq!(batch.batch_id, "test_batch");
        assert_eq!(batch.max_parallel, Some(5));
    }
}