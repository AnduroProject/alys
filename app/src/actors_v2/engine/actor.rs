//! EngineActor V2 Implementation
//!
//! Execution layer coordination and payload management. This actor isolates the complex
//! V0 Engine operations behind a proper actor interface, resolving the architectural
//! violation where ChainState directly held the Engine instance.

use actix::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::{EngineError, EngineMessage, EngineResponse, EngineActorMetrics};
use crate::engine::Engine;
use lighthouse_wrapper::types::{ExecutionBlockHash, ExecutionPayload, MainnetEthSpec};

/// Pending payload building operation
#[derive(Debug)]
struct PendingPayload {
    correlation_id: Uuid,
    started_at: Instant,
}

/// EngineActor V2 - Execution layer coordination and payload management
pub struct EngineActor {
    /// V0 Engine instance (encapsulated behind actor interface)
    engine: Arc<Engine>,

    /// Current finalized execution block
    finalized: Arc<RwLock<Option<ExecutionBlockHash>>>,

    /// Active payload building operations
    pending_payloads: HashMap<Uuid, PendingPayload>,

    /// Execution metrics
    metrics: EngineActorMetrics,

    /// Actor state
    is_ready: bool,

    /// Last activity timestamp
    last_activity: Instant,
}

impl EngineActor {
    /// Create new EngineActor
    pub fn new(engine: Engine) -> Self {
        let metrics = EngineActorMetrics::new();

        Self {
            engine: Arc::new(engine),
            finalized: Arc::new(RwLock::new(None)),
            pending_payloads: HashMap::new(),
            metrics,
            is_ready: true,
            last_activity: Instant::now(),
        }
    }

    /// Record activity and update metrics
    fn record_activity(&mut self) {
        self.last_activity = Instant::now();
        self.metrics.set_active_operations(self.pending_payloads.len() as i64);
    }

    /// Handle build payload message
    async fn handle_build_payload(
        &mut self,
        timestamp: Duration,
        parent_hash: Option<ExecutionBlockHash>,
        add_balances: Vec<crate::engine::AddBalance>,
        correlation_id: Option<Uuid>,
    ) -> Result<EngineResponse, EngineError> {
        let start_time = Instant::now();
        let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());

        debug!(
            correlation_id = %correlation_id,
            parent_hash = ?parent_hash,
            add_balances_count = add_balances.len(),
            "Building execution payload"
        );

        // Track pending operation
        self.pending_payloads.insert(correlation_id, PendingPayload {
            correlation_id,
            started_at: start_time,
        });
        self.metrics.set_active_operations(self.pending_payloads.len() as i64);

        // Call V0 Engine
        let result = self.engine.build_block(timestamp, parent_hash, add_balances).await;

        // Remove from pending operations
        self.pending_payloads.remove(&correlation_id);
        let duration = start_time.elapsed();

        match result {
            Ok(payload) => {
                info!(
                    correlation_id = %correlation_id,
                    block_number = payload.block_number(),
                    gas_used = payload.gas_used(),
                    duration_ms = duration.as_millis(),
                    "Successfully built execution payload"
                );

                self.metrics.record_build_payload_success(duration);

                Ok(EngineResponse::PayloadBuilt {
                    payload,
                    build_time: duration,
                })
            }
            Err(e) => {
                error!(
                    correlation_id = %correlation_id,
                    error = ?e,
                    duration_ms = duration.as_millis(),
                    "Failed to build execution payload"
                );

                self.metrics.record_build_payload_failure(duration);
                self.metrics.record_engine_api_error();

                Err(EngineError::from(e))
            }
        }
    }

    /// Handle validate payload message
    async fn handle_validate_payload(
        &mut self,
        payload: ExecutionPayload<MainnetEthSpec>,
        correlation_id: Option<Uuid>,
    ) -> Result<EngineResponse, EngineError> {
        let start_time = Instant::now();
        let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());

        debug!(
            correlation_id = %correlation_id,
            block_number = payload.block_number(),
            "Validating execution payload"
        );

        // For now, perform basic validation
        // In full implementation, would integrate with V0 Engine validation
        let is_valid = payload.block_number() > 0 &&
                      payload.gas_limit() > 0 &&
                      !payload.transactions().is_empty() || payload.block_number() == 0; // Allow empty genesis

        let duration = start_time.elapsed();

        if is_valid {
            info!(
                correlation_id = %correlation_id,
                block_number = payload.block_number(),
                duration_ms = duration.as_millis(),
                "Payload validation successful"
            );
            self.metrics.record_validate_payload_success(duration);
        } else {
            warn!(
                correlation_id = %correlation_id,
                block_number = payload.block_number(),
                duration_ms = duration.as_millis(),
                "Payload validation failed"
            );
            self.metrics.record_validate_payload_failure(duration);
            self.metrics.record_validation_error();
        }

        Ok(EngineResponse::PayloadValid {
            is_valid,
            validation_time: duration,
        })
    }

    /// Handle commit block message
    async fn handle_commit_block(
        &mut self,
        execution_payload: ExecutionPayload<MainnetEthSpec>,
        correlation_id: Option<Uuid>,
    ) -> Result<EngineResponse, EngineError> {
        let start_time = Instant::now();
        let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());

        debug!(
            correlation_id = %correlation_id,
            block_number = execution_payload.block_number(),
            "Committing execution block"
        );

        // Call V0 Engine
        let result = self.engine.commit_block(execution_payload).await;
        let duration = start_time.elapsed();

        match result {
            Ok(block_hash) => {
                info!(
                    correlation_id = %correlation_id,
                    block_hash = ?block_hash,
                    duration_ms = duration.as_millis(),
                    "Successfully committed execution block"
                );

                self.metrics.record_commit_block_success(duration);

                Ok(EngineResponse::BlockCommitted {
                    block_hash,
                    commit_time: duration,
                })
            }
            Err(e) => {
                error!(
                    correlation_id = %correlation_id,
                    error = ?e,
                    duration_ms = duration.as_millis(),
                    "Failed to commit execution block"
                );

                self.metrics.record_commit_block_failure(duration);
                self.metrics.record_engine_api_error();

                Err(EngineError::from(e))
            }
        }
    }

    /// Handle set finalized message
    async fn handle_set_finalized(
        &mut self,
        block_hash: ExecutionBlockHash,
        correlation_id: Option<Uuid>,
    ) -> Result<EngineResponse, EngineError> {
        let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());

        debug!(
            correlation_id = %correlation_id,
            block_hash = ?block_hash,
            "Setting finalized execution block"
        );

        // Update V0 Engine
        self.engine.set_finalized(block_hash).await;

        // Update our tracking
        *self.finalized.write().await = Some(block_hash);

        info!(
            correlation_id = %correlation_id,
            block_hash = ?block_hash,
            "Updated finalized execution block"
        );

        Ok(EngineResponse::FinalizedUpdated { block_hash })
    }

    /// Handle get status message
    async fn handle_get_status(
        &self,
        correlation_id: Option<Uuid>,
    ) -> Result<EngineResponse, EngineError> {
        let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());

        debug!(correlation_id = %correlation_id, "Getting engine status");

        let finalized_block = *self.finalized.read().await;

        Ok(EngineResponse::Status {
            is_ready: self.is_ready,
            finalized_block,
            head_block: None, // Would track head block in full implementation
        })
    }
}

impl Actor for EngineActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        info!("EngineActor V2 started");
        self.record_activity();
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        info!("EngineActor V2 stopped");
    }
}

impl Handler<EngineMessage> for EngineActor {
    type Result = ResponseFuture<Result<EngineResponse, EngineError>>;

    fn handle(&mut self, msg: EngineMessage, _: &mut Context<Self>) -> Self::Result {
        self.record_activity();

        match msg {
            EngineMessage::BuildPayload { timestamp, parent_hash, add_balances, correlation_id } => {
                // Capture necessary data before async context to avoid lifetime issues
                let engine = self.engine.clone();
                let metrics = self.metrics.clone();
                let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());

                Box::pin(async move {
                    let start_time = Instant::now();

                    debug!(
                        correlation_id = %correlation_id,
                        timestamp_secs = timestamp.as_secs(),
                        parent_hash = ?parent_hash,
                        balance_count = add_balances.len(),
                        "Building execution payload"
                    );

                    // Build block using engine
                    let result = engine.build_block(
                        timestamp,
                        parent_hash,
                        add_balances,
                    ).await;

                    let duration = start_time.elapsed();

                    match result {
                        Ok(payload) => {
                            info!(
                                correlation_id = %correlation_id,
                                block_number = payload.block_number(),
                                gas_used = payload.gas_used(),
                                duration_ms = duration.as_millis(),
                                "Successfully built execution payload"
                            );

                            metrics.record_build_payload_success(duration);

                            Ok(EngineResponse::PayloadBuilt {
                                payload,
                                build_time: duration,
                            })
                        }
                        Err(e) => {
                            error!(
                                correlation_id = %correlation_id,
                                error = ?e,
                                duration_ms = duration.as_millis(),
                                "Failed to build execution payload"
                            );

                            metrics.record_build_payload_failure(duration);

                            Err(EngineError::BlockBuildingFailed(format!("{:?}", e)))
                        }
                    }
                })
            }

            EngineMessage::ValidatePayload { payload, correlation_id } => {
                let metrics = self.metrics.clone();
                let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());

                Box::pin(async move {
                    let start_time = Instant::now();

                    debug!(
                        correlation_id = %correlation_id,
                        block_number = payload.block_number(),
                        "Validating execution payload"
                    );

                    // Perform basic execution payload validation
                    // In full implementation, this would integrate with V0 Engine validation
                    let is_valid = payload.block_number() > 0 &&
                                  payload.gas_limit() > 0 &&
                                  payload.gas_used() <= payload.gas_limit() &&
                                  payload.timestamp() > 0 &&
                                  (!payload.transactions().is_empty() || payload.block_number() == 0); // Allow empty genesis

                    let duration = start_time.elapsed();

                    if is_valid {
                        info!(
                            correlation_id = %correlation_id,
                            block_number = payload.block_number(),
                            duration_ms = duration.as_millis(),
                            "Payload validation successful"
                        );
                        metrics.record_validate_payload_success(duration);
                    } else {
                        warn!(
                            correlation_id = %correlation_id,
                            block_number = payload.block_number(),
                            gas_used = payload.gas_used(),
                            gas_limit = payload.gas_limit(),
                            tx_count = payload.transactions().len(),
                            duration_ms = duration.as_millis(),
                            "Payload validation failed"
                        );
                        metrics.record_validate_payload_failure(duration);
                        metrics.record_validation_error();
                    }

                    Ok(EngineResponse::PayloadValid {
                        is_valid,
                        validation_time: duration,
                    })
                })
            }

            EngineMessage::CommitBlock { execution_payload, correlation_id } => {
                let engine = self.engine.clone();
                let metrics = self.metrics.clone();
                let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());

                Box::pin(async move {
                    let start_time = Instant::now();

                    debug!(
                        correlation_id = %correlation_id,
                        block_number = execution_payload.block_number(),
                        "Committing execution block"
                    );

                    // Call V0 Engine commit_block method
                    let result = engine.commit_block(execution_payload).await;
                    let duration = start_time.elapsed();

                    match result {
                        Ok(block_hash) => {
                            info!(
                                correlation_id = %correlation_id,
                                block_hash = ?block_hash,
                                duration_ms = duration.as_millis(),
                                "Successfully committed execution block"
                            );

                            metrics.record_commit_block_success(duration);

                            Ok(EngineResponse::BlockCommitted {
                                block_hash,
                                commit_time: duration,
                            })
                        }
                        Err(e) => {
                            error!(
                                correlation_id = %correlation_id,
                                error = ?e,
                                duration_ms = duration.as_millis(),
                                "Failed to commit execution block"
                            );

                            metrics.record_commit_block_failure(duration);

                            Err(EngineError::EngineApi(format!("Commit failed: {:?}", e)))
                        }
                    }
                })
            }

            EngineMessage::SetFinalized { block_hash, correlation_id } => {
                let engine = self.engine.clone();
                let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());

                Box::pin(async move {
                    debug!(
                        correlation_id = %correlation_id,
                        block_hash = ?block_hash,
                        "Setting finalized execution block"
                    );

                    // Update V0 Engine finalized state
                    engine.set_finalized(block_hash).await;

                    info!(
                        correlation_id = %correlation_id,
                        block_hash = ?block_hash,
                        "Updated finalized execution block"
                    );

                    Ok(EngineResponse::FinalizedUpdated { block_hash })
                })
            }

            EngineMessage::GetStatus { correlation_id } => {
                let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());

                Box::pin(async move {
                    info!(
                        correlation_id = %correlation_id,
                        "Engine status check"
                    );
                    Ok(EngineResponse::Status {
                        is_ready: true,
                        finalized_block: None,
                        head_block: None,
                    })
                })
            }

            EngineMessage::GetLatestBlock { correlation_id } => {
                let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());
                debug!(correlation_id = %correlation_id, "GetLatestBlock not yet implemented");

                Box::pin(async move {
                    Err(EngineError::Internal("GetLatestBlock not yet implemented".to_string()))
                })
            }

            EngineMessage::UpdateForkChoice { head_hash, safe_hash, finalized_hash, correlation_id } => {
                let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());
                debug!(
                    correlation_id = %correlation_id,
                    head_hash = ?head_hash,
                    safe_hash = ?safe_hash,
                    finalized_hash = ?finalized_hash,
                    "UpdateForkChoice not yet implemented"
                );

                Box::pin(async move {
                    Err(EngineError::Internal("UpdateForkChoice not yet implemented".to_string()))
                })
            }

            EngineMessage::GetBlockWithTransactions { block_hash, correlation_id } => {
                let engine = self.engine.clone();
                let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());

                Box::pin(async move {
                    debug!(
                        correlation_id = %correlation_id,
                        block_hash = ?block_hash,
                        "Getting block with transactions"
                    );

                    match engine.get_block_with_txs(&block_hash).await {
                        Ok(block) => Ok(EngineResponse::BlockWithTransactions { block }),
                        Err(e) => {
                            error!(
                                correlation_id = %correlation_id,
                                error = ?e,
                                "Failed to get block with transactions"
                            );
                            Err(EngineError::EngineApi(format!("{:?}", e)))
                        }
                    }
                })
            }

            EngineMessage::GetTransactionReceipt { transaction_hash, correlation_id } => {
                let engine = self.engine.clone();
                let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());

                Box::pin(async move {
                    debug!(
                        correlation_id = %correlation_id,
                        transaction_hash = ?transaction_hash,
                        "Getting transaction receipt"
                    );

                    match engine.get_transaction_receipt(transaction_hash).await {
                        Ok(receipt) => Ok(EngineResponse::TransactionReceipt { receipt }),
                        Err(e) => {
                            error!(
                                correlation_id = %correlation_id,
                                error = ?e,
                                "Failed to get transaction receipt"
                            );
                            Err(EngineError::EngineApi(format!("{:?}", e)))
                        }
                    }
                })
            }

            EngineMessage::Shutdown { graceful: _, correlation_id } => {
                let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());
                info!(correlation_id = %correlation_id, "Engine shutdown requested");

                Box::pin(async move {
                    Ok(EngineResponse::ShutdownComplete)
                })
            }
        }
    }
}