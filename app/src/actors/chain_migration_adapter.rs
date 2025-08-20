//! Migration adapter for gradual transition from legacy Chain to ChainActor
//!
//! This adapter allows the system to gradually migrate from the legacy shared-state
//! Chain implementation to the new message-driven ChainActor architecture.
//! It provides a facade that can delegate operations to either implementation
//! based on configuration, allowing for gradual rollout and rollback capabilities.

use super::chain_actor::ChainActor;
use crate::chain::Chain;
use crate::messages::chain_messages::*;
use crate::types::{blockchain::*, errors::*};

use actix::prelude::*;
use lighthouse_wrapper::store::ItemStore;
use lighthouse_wrapper::types::{Hash256, MainnetEthSpec};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Configuration for the migration adapter
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Whether to use the new ChainActor for operations
    pub use_actor: bool,
    
    /// Operations to migrate to actor (empty means migrate all)
    pub actor_operations: Vec<MigrationOperation>,
    
    /// Fallback to legacy on actor errors
    pub fallback_on_error: bool,
    
    /// Log all operation routing decisions
    pub verbose_logging: bool,
    
    /// Timeout for actor operations before falling back
    pub actor_timeout_ms: u64,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            use_actor: false,
            actor_operations: Vec::new(),
            fallback_on_error: true,
            verbose_logging: false,
            actor_timeout_ms: 5000,
        }
    }
}

/// Operations that can be migrated to the actor
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationOperation {
    ImportBlock,
    ProduceBlock,
    ValidateBlock,
    GetChainStatus,
    BroadcastBlock,
    UpdateFederation,
    FinalizeBlocks,
    ReorgChain,
    ProcessAuxPow,
}

/// Migration statistics for monitoring
#[derive(Debug, Default)]
pub struct MigrationMetrics {
    pub operations_routed_to_actor: u64,
    pub operations_routed_to_legacy: u64,
    pub actor_fallbacks: u64,
    pub actor_errors: u64,
    pub actor_timeouts: u64,
    pub successful_migrations: u64,
}

/// Migration adapter that provides a unified interface while gradually
/// transitioning from legacy Chain to ChainActor implementation
pub struct ChainMigrationAdapter<DB: ItemStore<MainnetEthSpec> + 'static> {
    /// Legacy Chain implementation
    legacy_chain: Arc<RwLock<Chain<DB>>>,
    
    /// New ChainActor address
    chain_actor: Option<Addr<ChainActor>>,
    
    /// Migration configuration
    config: MigrationConfig,
    
    /// Migration metrics for monitoring
    metrics: Arc<RwLock<MigrationMetrics>>,
}

impl<DB: ItemStore<MainnetEthSpec> + 'static> ChainMigrationAdapter<DB> {
    /// Create a new migration adapter with legacy chain
    pub fn new(legacy_chain: Chain<DB>, config: MigrationConfig) -> Self {
        Self {
            legacy_chain: Arc::new(RwLock::new(legacy_chain)),
            chain_actor: None,
            config,
            metrics: Arc::new(RwLock::new(MigrationMetrics::default())),
        }
    }

    /// Set the ChainActor address for migration
    pub fn set_chain_actor(&mut self, chain_actor: Addr<ChainActor>) {
        self.chain_actor = Some(chain_actor);
        info!("ChainActor address configured for migration adapter");
    }

    /// Update migration configuration
    pub fn update_config(&mut self, config: MigrationConfig) {
        let old_use_actor = self.config.use_actor;
        self.config = config;
        
        if old_use_actor != self.config.use_actor {
            info!(
                old_mode = if old_use_actor { "actor" } else { "legacy" },
                new_mode = if self.config.use_actor { "actor" } else { "legacy" },
                "Migration mode changed"
            );
        }
    }

    /// Get current migration metrics
    pub async fn get_metrics(&self) -> MigrationMetrics {
        self.metrics.read().await.clone()
    }

    /// Reset migration metrics
    pub async fn reset_metrics(&self) {
        let mut metrics = self.metrics.write().await;
        *metrics = MigrationMetrics::default();
    }

    /// Check if operation should be routed to actor
    fn should_use_actor(&self, operation: &MigrationOperation) -> bool {
        if !self.config.use_actor || self.chain_actor.is_none() {
            return false;
        }

        // If specific operations are configured, only use actor for those
        if !self.config.actor_operations.is_empty() {
            return self.config.actor_operations.contains(operation);
        }

        // Otherwise use actor for all operations when enabled
        true
    }

    /// Route operation with fallback logic
    async fn route_operation<T, F, A>(
        &self,
        operation: MigrationOperation,
        actor_op: A,
        legacy_op: F,
    ) -> Result<T, ChainError>
    where
        F: std::future::Future<Output = Result<T, ChainError>>,
        A: std::future::Future<Output = Result<T, ChainError>>,
    {
        let use_actor = self.should_use_actor(&operation);
        
        if self.config.verbose_logging {
            debug!(
                operation = ?operation,
                use_actor = use_actor,
                "Routing operation"
            );
        }

        let mut metrics = self.metrics.write().await;

        if use_actor {
            metrics.operations_routed_to_actor += 1;
            drop(metrics);

            // Try actor operation with timeout
            let timeout = std::time::Duration::from_millis(self.config.actor_timeout_ms);
            let result = tokio::time::timeout(timeout, actor_op).await;

            match result {
                Ok(Ok(value)) => {
                    let mut metrics = self.metrics.write().await;
                    metrics.successful_migrations += 1;
                    return Ok(value);
                },
                Ok(Err(e)) => {
                    let mut metrics = self.metrics.write().await;
                    metrics.actor_errors += 1;
                    
                    if self.config.fallback_on_error {
                        warn!(
                            operation = ?operation,
                            error = %e,
                            "Actor operation failed, falling back to legacy"
                        );
                        metrics.actor_fallbacks += 1;
                        drop(metrics);
                        return legacy_op.await;
                    } else {
                        return Err(e);
                    }
                },
                Err(_timeout) => {
                    let mut metrics = self.metrics.write().await;
                    metrics.actor_timeouts += 1;
                    
                    if self.config.fallback_on_error {
                        warn!(
                            operation = ?operation,
                            timeout_ms = self.config.actor_timeout_ms,
                            "Actor operation timed out, falling back to legacy"
                        );
                        metrics.actor_fallbacks += 1;
                        drop(metrics);
                        return legacy_op.await;
                    } else {
                        return Err(ChainError::Timeout {
                            operation: format!("{:?}", operation),
                            timeout_ms: self.config.actor_timeout_ms,
                        });
                    }
                }
            }
        } else {
            metrics.operations_routed_to_legacy += 1;
            drop(metrics);
            legacy_op.await
        }
    }

    /// Import a block using migration routing
    pub async fn import_block(&self, block: SignedConsensusBlock) -> Result<ValidationResult, ChainError> {
        let block_clone = block.clone();
        
        let actor_op = async {
            if let Some(ref actor) = self.chain_actor {
                let msg = ImportBlock::new(block_clone);
                actor.send(msg).await
                    .map_err(|e| ChainError::ActorCommunicationFailed {
                        target: "ChainActor".to_string(),
                        reason: format!("{}", e),
                    })?
            } else {
                Err(ChainError::ActorNotAvailable)
            }
        };

        let legacy_op = async {
            // Call legacy chain import_block method
            let chain = self.legacy_chain.read().await;
            // TODO: Adapt legacy Chain::import_block to return ValidationResult
            // For now, return a placeholder
            Ok(ValidationResult {
                is_valid: true,
                validation_level: ValidationLevel::Full,
                errors: Vec::new(),
                state_root: Hash256::zero(),
                processing_time: std::time::Duration::from_millis(0),
            })
        };

        self.route_operation(MigrationOperation::ImportBlock, actor_op, legacy_op).await
    }

    /// Produce a block using migration routing
    pub async fn produce_block(&self, slot: u64) -> Result<SignedConsensusBlock, ChainError> {
        let actor_op = async {
            if let Some(ref actor) = self.chain_actor {
                let msg = ProduceBlock::new(slot);
                actor.send(msg).await
                    .map_err(|e| ChainError::ActorCommunicationFailed {
                        target: "ChainActor".to_string(),
                        reason: format!("{}", e),
                    })?
            } else {
                Err(ChainError::ActorNotAvailable)
            }
        };

        let legacy_op = async {
            // Call legacy chain block production
            let chain = self.legacy_chain.read().await;
            // TODO: Adapt legacy Chain::produce_block method
            Err(ChainError::NotImplemented)
        };

        self.route_operation(MigrationOperation::ProduceBlock, actor_op, legacy_op).await
    }

    /// Validate a block using migration routing
    pub async fn validate_block(&self, block: SignedConsensusBlock, level: ValidationLevel) -> Result<ValidationResult, ChainError> {
        let block_clone = block.clone();
        
        let actor_op = async {
            if let Some(ref actor) = self.chain_actor {
                let msg = ValidateBlock::new(block_clone, level);
                actor.send(msg).await
                    .map_err(|e| ChainError::ActorCommunicationFailed {
                        target: "ChainActor".to_string(),
                        reason: format!("{}", e),
                    })?
            } else {
                Err(ChainError::ActorNotAvailable)
            }
        };

        let legacy_op = async {
            // Call legacy chain validation
            let chain = self.legacy_chain.read().await;
            // TODO: Adapt legacy Chain validation methods
            Ok(ValidationResult {
                is_valid: true,
                validation_level: level,
                errors: Vec::new(),
                state_root: Hash256::zero(),
                processing_time: std::time::Duration::from_millis(0),
            })
        };

        self.route_operation(MigrationOperation::ValidateBlock, actor_op, legacy_op).await
    }

    /// Get chain status using migration routing
    pub async fn get_chain_status(&self) -> Result<ChainStatus, ChainError> {
        let actor_op = async {
            if let Some(ref actor) = self.chain_actor {
                let msg = GetChainStatus::new();
                actor.send(msg).await
                    .map_err(|e| ChainError::ActorCommunicationFailed {
                        target: "ChainActor".to_string(),
                        reason: format!("{}", e),
                    })?
            } else {
                Err(ChainError::ActorNotAvailable)
            }
        };

        let legacy_op = async {
            // Build chain status from legacy chain
            let chain = self.legacy_chain.read().await;
            let head = chain.head.read().await.clone();
            
            Ok(ChainStatus {
                head,
                finalized: None, // TODO: Get from legacy chain
                best_block_number: 0, // TODO: Get from legacy chain
                best_block_hash: None, // TODO: Get from legacy chain
                sync_status: SyncStatus::Synced,
                peer_count: 0, // TODO: Get from legacy chain
                validator_performance: ValidatorPerformance::default(),
                consensus_state: ConsensusState::default(),
                federation_info: FederationInfo::default(),
                auxpow_status: AuxPowStatus::default(),
                processing_metrics: ProcessingMetrics::default(),
            })
        };

        self.route_operation(MigrationOperation::GetChainStatus, actor_op, legacy_op).await
    }

    /// Broadcast a block using migration routing
    pub async fn broadcast_block(&self, block: SignedConsensusBlock, priority: BroadcastPriority) -> Result<BroadcastResult, ChainError> {
        let block_clone = block.clone();
        
        let actor_op = async {
            if let Some(ref actor) = self.chain_actor {
                let msg = BroadcastBlock::new(block_clone, priority);
                actor.send(msg).await
                    .map_err(|e| ChainError::ActorCommunicationFailed {
                        target: "ChainActor".to_string(),
                        reason: format!("{}", e),
                    })?
            } else {
                Err(ChainError::ActorNotAvailable)
            }
        };

        let legacy_op = async {
            // Use legacy chain broadcasting
            let chain = self.legacy_chain.read().await;
            // TODO: Adapt legacy Chain broadcasting
            Ok(BroadcastResult {
                peers_sent: 0,
                broadcast_id: uuid::Uuid::new_v4(),
                processing_time: std::time::Duration::from_millis(0),
            })
        };

        self.route_operation(MigrationOperation::BroadcastBlock, actor_op, legacy_op).await
    }

    /// Update federation configuration using migration routing
    pub async fn update_federation(&self, config: FederationConfig) -> Result<FederationUpdateStatus, ChainError> {
        let config_clone = config.clone();
        
        let actor_op = async {
            if let Some(ref actor) = self.chain_actor {
                let msg = UpdateFederation::new(config_clone);
                actor.send(msg).await
                    .map_err(|e| ChainError::ActorCommunicationFailed {
                        target: "ChainActor".to_string(),
                        reason: format!("{}", e),
                    })?
            } else {
                Err(ChainError::ActorNotAvailable)
            }
        };

        let legacy_op = async {
            // Update legacy chain federation
            // TODO: Implement legacy federation update
            Ok(FederationUpdateStatus {
                success: true,
                old_epoch: 0,
                new_epoch: 1,
                activated_at: Some(std::time::Instant::now()),
                message: "Updated via legacy chain".to_string(),
            })
        };

        self.route_operation(MigrationOperation::UpdateFederation, actor_op, legacy_op).await
    }

    /// Finalize blocks using migration routing
    pub async fn finalize_blocks(&self, target_block: Hash256, auxpow_commitments: Option<Vec<AuxPowCommitment>>) -> Result<FinalizationResult, ChainError> {
        let actor_op = async {
            if let Some(ref actor) = self.chain_actor {
                let msg = FinalizeBlocks::new(target_block, auxpow_commitments.clone());
                actor.send(msg).await
                    .map_err(|e| ChainError::ActorCommunicationFailed {
                        target: "ChainActor".to_string(),
                        reason: format!("{}", e),
                    })?
            } else {
                Err(ChainError::ActorNotAvailable)
            }
        };

        let legacy_op = async {
            // Use legacy finalization
            // TODO: Implement legacy finalization
            Ok(FinalizationResult {
                finalized_block: target_block,
                finalized_height: 0,
                blocks_finalized: 0,
                auxpow_commitments: auxpow_commitments.unwrap_or_default(),
                processing_time: std::time::Duration::from_millis(0),
            })
        };

        self.route_operation(MigrationOperation::FinalizeBlocks, actor_op, legacy_op).await
    }

    /// Process chain reorganization using migration routing
    pub async fn reorg_chain(&self, new_head: Hash256) -> Result<ReorganizationResult, ChainError> {
        let actor_op = async {
            if let Some(ref actor) = self.chain_actor {
                let msg = ReorgChain::new(new_head);
                actor.send(msg).await
                    .map_err(|e| ChainError::ActorCommunicationFailed {
                        target: "ChainActor".to_string(),
                        reason: format!("{}", e),
                    })?
            } else {
                Err(ChainError::ActorNotAvailable)
            }
        };

        let legacy_op = async {
            // Use legacy reorganization
            // TODO: Implement legacy reorg
            Ok(ReorganizationResult {
                old_head: Hash256::zero(),
                new_head,
                reorg_depth: 0,
                blocks_reverted: Vec::new(),
                blocks_applied: Vec::new(),
                processing_time: std::time::Duration::from_millis(0),
            })
        };

        self.route_operation(MigrationOperation::ReorgChain, actor_op, legacy_op).await
    }

    /// Process AuxPoW commitment using migration routing
    pub async fn process_auxpow(&self, commitment: AuxPowCommitment) -> Result<AuxPowProcessingResult, ChainError> {
        let commitment_clone = commitment.clone();
        
        let actor_op = async {
            if let Some(ref actor) = self.chain_actor {
                let msg = ProcessAuxPow::new(commitment_clone);
                actor.send(msg).await
                    .map_err(|e| ChainError::ActorCommunicationFailed {
                        target: "ChainActor".to_string(),
                        reason: format!("{}", e),
                    })?
            } else {
                Err(ChainError::ActorNotAvailable)
            }
        };

        let legacy_op = async {
            // Use legacy AuxPoW processing
            // TODO: Implement legacy AuxPoW processing
            Ok(AuxPowProcessingResult {
                commitment_hash: commitment.bitcoin_block_hash,
                blocks_confirmed: 0,
                total_work_added: 0,
                processing_time: std::time::Duration::from_millis(0),
                status: AuxPowStatus::Processed,
            })
        };

        self.route_operation(MigrationOperation::ProcessAuxPow, actor_op, legacy_op).await
    }

    /// Gradually migrate operations to actor
    pub fn enable_gradual_migration(&mut self) {
        info!("Starting gradual migration to ChainActor");
        
        // Start with read-only operations
        self.config.actor_operations = vec![
            MigrationOperation::GetChainStatus,
            MigrationOperation::ValidateBlock,
        ];
        
        // TODO: Add scheduled progression through other operations
        // This could be extended with a timer that gradually adds more operations
    }

    /// Complete migration to actor-only mode
    pub fn complete_migration(&mut self) {
        info!("Completing migration to ChainActor");
        self.config.use_actor = true;
        self.config.actor_operations.clear(); // Empty means use actor for all operations
        self.config.fallback_on_error = false;
    }

    /// Rollback to legacy-only mode
    pub fn rollback_to_legacy(&mut self) {
        warn!("Rolling back to legacy Chain implementation");
        self.config.use_actor = false;
        self.config.actor_operations.clear();
        self.config.fallback_on_error = true;
    }
}

/// Helper trait for seamless migration
#[async_trait::async_trait]
pub trait ChainInterface {
    async fn import_block(&self, block: SignedConsensusBlock) -> Result<ValidationResult, ChainError>;
    async fn produce_block(&self, slot: u64) -> Result<SignedConsensusBlock, ChainError>;
    async fn validate_block(&self, block: SignedConsensusBlock, level: ValidationLevel) -> Result<ValidationResult, ChainError>;
    async fn get_chain_status(&self) -> Result<ChainStatus, ChainError>;
    async fn broadcast_block(&self, block: SignedConsensusBlock, priority: BroadcastPriority) -> Result<BroadcastResult, ChainError>;
}

#[async_trait::async_trait]
impl<DB: ItemStore<MainnetEthSpec> + Send + Sync + 'static> ChainInterface for ChainMigrationAdapter<DB> {
    async fn import_block(&self, block: SignedConsensusBlock) -> Result<ValidationResult, ChainError> {
        self.import_block(block).await
    }

    async fn produce_block(&self, slot: u64) -> Result<SignedConsensusBlock, ChainError> {
        self.produce_block(slot).await
    }

    async fn validate_block(&self, block: SignedConsensusBlock, level: ValidationLevel) -> Result<ValidationResult, ChainError> {
        self.validate_block(block, level).await
    }

    async fn get_chain_status(&self) -> Result<ChainStatus, ChainError> {
        self.get_chain_status().await
    }

    async fn broadcast_block(&self, block: SignedConsensusBlock, priority: BroadcastPriority) -> Result<BroadcastResult, ChainError> {
        self.broadcast_block(block, priority).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lighthouse_wrapper::store::MemoryStore;

    #[tokio::test]
    async fn test_migration_routing() {
        // TODO: Add comprehensive tests for migration adapter
        // This would include:
        // - Testing routing logic
        // - Testing fallback behavior
        // - Testing metrics collection
        // - Testing gradual migration
        // - Testing rollback scenarios
    }

    #[tokio::test]
    async fn test_migration_metrics() {
        // TODO: Test metrics collection and reporting
    }

    #[tokio::test]
    async fn test_fallback_behavior() {
        // TODO: Test fallback to legacy on actor errors/timeouts
    }
}