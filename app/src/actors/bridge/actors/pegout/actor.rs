//! PegOut Actor Implementation
//! 
//! Specialized actor for processing Bitcoin withdrawals (peg-out operations)

use actix::prelude::*;
use bitcoin::{Transaction, Txid, Address as BtcAddress};
use ethereum_types::{H160, H256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use crate::actors::bridge::{
    config::PegOutConfig,
    messages::*,
    shared::*,
};
use crate::types::*;
use super::{handlers::*, transaction_builder::*, signature_coordinator::*, state::*, metrics::*};

/// PegOut actor for Bitcoin withdrawal processing
pub struct PegOutActor {
    /// Configuration
    config: PegOutConfig,
    
    /// UTXO and transaction management
    utxo_manager: UtxoManager,
    transaction_builder: TransactionBuilder,
    fee_estimator: FeeEstimator,
    
    /// Pending peg-out operations
    pending_pegouts: HashMap<String, PendingPegOut>,
    
    /// Signature coordination
    signature_coordinator: SignatureCoordinator,
    
    /// Actor references
    bridge_coordinator: Option<Addr<super::super::bridge::BridgeActor>>,
    stream_actor: Option<Addr<super::super::stream::StreamActor>>,
    chain_actor: Option<Addr<crate::actors::chain::ChainActor>>,
    
    /// External services
    bitcoin_client: Arc<dyn BitcoinRpc>,
    
    /// State management
    state: PegOutState,
    
    /// Metrics and monitoring
    metrics: PegOutMetrics,
    performance_tracker: OperationTracker,
    
    /// Error tracking and retry
    recent_errors: Vec<PegOutError>,
    retry_queue: Vec<RetryablePegOut>,
}

/// Retryable peg-out operation
#[derive(Debug, Clone)]
pub struct RetryablePegOut {
    pub pegout_id: String,
    pub operation: PegOutOperation,
    pub retry_count: u32,
    pub last_attempt: SystemTime,
    pub next_retry: SystemTime,
    pub error: PegOutError,
}

/// Peg-out operation types for retry
#[derive(Debug, Clone)]
pub enum PegOutOperation {
    ProcessBurnEvent {
        burn_tx: H256,
        destination: BtcAddress,
        amount: u64,
        requester: H160,
    },
    BuildTransaction {
        pegout_id: String,
    },
    RequestSignatures {
        pegout_id: String,
        unsigned_tx: Transaction,
    },
    BroadcastTransaction {
        pegout_id: String,
        signed_tx: Transaction,
    },
}

impl PegOutActor {
    /// Create new PegOut actor
    pub fn new(
        config: PegOutConfig,
        utxo_manager: UtxoManager,
        bitcoin_client: Arc<dyn BitcoinRpc>,
        federation_config: FederationConfig,
    ) -> Result<Self, PegOutError> {
        let transaction_builder = TransactionBuilder::new(
            bitcoin_client.clone(),
            federation_config.clone(),
        )?;
        
        let fee_estimator = FeeEstimator::new(bitcoin_client.clone(), config.transaction_fee_rate);
        
        let signature_coordinator = SignatureCoordinator::new(
            federation_config,
            config.signature_timeout,
        );

        let metrics = PegOutMetrics::new()?;
        let performance_tracker = OperationTracker::new();

        Ok(Self {
            config,
            utxo_manager,
            transaction_builder,
            fee_estimator,
            pending_pegouts: HashMap::new(),
            signature_coordinator,
            bridge_coordinator: None,
            stream_actor: None,
            chain_actor: None,
            bitcoin_client,
            state: PegOutState::Initializing,
            metrics,
            performance_tracker,
            recent_errors: Vec::new(),
            retry_queue: Vec::new(),
        })
    }

    /// Initialize PegOut actor
    async fn initialize(&mut self, ctx: &mut Context<Self>) -> Result<(), PegOutError> {
        info!("Initializing PegOut actor");

        // Start periodic tasks
        self.start_signature_monitoring(ctx);
        self.start_transaction_broadcasting(ctx);
        self.start_retry_processing(ctx);
        self.start_utxo_refresh(ctx);

        // Update state
        self.state = PegOutState::Operational;
        self.metrics.record_actor_started();

        info!("PegOut actor initialized successfully");
        Ok(())
    }

    /// Process burn event from Alys chain
    async fn process_burn_event(
        &mut self,
        burn_tx: H256,
        destination: BtcAddress,
        amount: u64,
        requester: H160,
    ) -> Result<String, PegOutError> {
        let pegout_id = format!("pegout_{}", Uuid::new_v4());
        
        info!("Processing burn event {} -> pegout {}", burn_tx, pegout_id);

        // Validate burn event
        self.validate_burn_event(&burn_tx, &destination, amount, &requester)?;

        // Create pending peg-out
        let pending_pegout = PendingPegOut {
            pegout_id: pegout_id.clone(),
            burn_tx_hash: burn_tx,
            destination_address: destination,
            amount,
            requester,
            unsigned_tx: None,
            signature_status: SignatureStatus {
                request_id: None,
                requested_at: None,
                signatures_collected: 0,
                signatures_required: self.signature_coordinator.get_required_signatures(),
                status: SignatureCollectionStatus::NotRequested,
            },
            witnesses: Vec::new(),
            signed_tx: None,
            broadcast_txid: None,
            status: PegOutStatus::BurnDetected,
            created_at: SystemTime::now(),
            last_updated: SystemTime::now(),
            retry_count: 0,
        };

        self.pending_pegouts.insert(pegout_id.clone(), pending_pegout);
        self.metrics.record_burn_event_processed();

        // Start operation tracking
        self.performance_tracker.start_operation(pegout_id.clone());

        // Initiate transaction building
        self.initiate_transaction_building(pegout_id.clone()).await?;

        Ok(pegout_id)
    }

    /// Validate burn event
    fn validate_burn_event(
        &self,
        burn_tx: &H256,
        destination: &BtcAddress,
        amount: u64,
        requester: &H160,
    ) -> Result<(), PegOutError> {
        // Amount validation
        if amount < MIN_PEGOUT_AMOUNT {
            return Err(PegOutError::InvalidAmount {
                amount,
                minimum: MIN_PEGOUT_AMOUNT,
            });
        }

        if amount > MAX_PEGOUT_AMOUNT {
            return Err(PegOutError::InvalidAmount {
                amount,
                minimum: MAX_PEGOUT_AMOUNT,
            });
        }

        // Address validation
        // In practice, we'd validate the destination address format and network

        // Check for sufficient UTXOs
        let available_utxos = self.utxo_manager.get_spendable_utxos();
        let total_available: u64 = available_utxos.iter().map(|u| u.output.value).sum();
        
        if total_available < amount + 10000 { // Add buffer for fees
            return Err(PegOutError::InsufficientFunds {
                required: amount,
                available: total_available,
            });
        }

        Ok(())
    }

    /// Initiate transaction building
    async fn initiate_transaction_building(&mut self, pegout_id: String) -> Result<(), PegOutError> {
        info!("Initiating transaction building for pegout {}", pegout_id);

        if let Some(pegout) = self.pending_pegouts.get_mut(&pegout_id) {
            pegout.status = PegOutStatus::BuildingTransaction;
            pegout.last_updated = SystemTime::now();

            // Build unsigned transaction
            let unsigned_tx = self.transaction_builder.build_withdrawal_transaction(
                pegout.destination_address.clone(),
                pegout.amount,
                &mut self.utxo_manager,
            ).await?;

            pegout.unsigned_tx = Some(unsigned_tx.clone());
            pegout.status = PegOutStatus::TransactionBuilt {
                fee: unsigned_tx.output.iter().map(|o| o.value).sum::<u64>() - pegout.amount,
            };

            self.metrics.record_transaction_built();

            // Request signatures
            self.request_signatures(pegout_id, unsigned_tx).await?;
        } else {
            return Err(PegOutError::OperationNotFound(pegout_id));
        }

        Ok(())
    }

    /// Request signatures from governance
    async fn request_signatures(
        &mut self,
        pegout_id: String,
        unsigned_tx: Transaction,
    ) -> Result<(), PegOutError> {
        info!("Requesting signatures for pegout {}", pegout_id);

        if let Some(stream_actor) = &self.stream_actor {
            let signature_request = PegOutSignatureRequest {
                request_id: format!("sig_req_{}", Uuid::new_v4()),
                pegout_id: pegout_id.clone(),
                unsigned_transaction: unsigned_tx,
                destination_address: self.pending_pegouts[&pegout_id].destination_address.clone(),
                amount: self.pending_pegouts[&pegout_id].amount,
                fee: 10000, // Would be calculated properly
                utxo_commitments: Vec::new(), // Would be populated
                requester: self.pending_pegouts[&pegout_id].requester,
                requested_at: SystemTime::now(),
                timeout: self.config.signature_timeout,
            };

            // Send signature request
            let msg = StreamMessage::RequestPegOutSignatures {
                request: signature_request,
            };

            match stream_actor.send(msg).await {
                Ok(Ok(_)) => {
                    if let Some(pegout) = self.pending_pegouts.get_mut(&pegout_id) {
                        pegout.status = PegOutStatus::RequestingSignatures;
                        pegout.signature_status.status = SignatureCollectionStatus::Requested;
                        pegout.signature_status.requested_at = Some(SystemTime::now());
                        pegout.last_updated = SystemTime::now();
                    }

                    self.metrics.record_signatures_requested();
                    info!("Signature request sent for pegout {}", pegout_id);
                }
                Ok(Err(e)) => {
                    error!("StreamActor returned error for signature request: {:?}", e);
                    return Err(PegOutError::SignatureRequestFailed(format!("{:?}", e)));
                }
                Err(e) => {
                    error!("Failed to send signature request: {:?}", e);
                    return Err(PegOutError::ActorCommunicationError(e.to_string()));
                }
            }
        } else {
            return Err(PegOutError::StreamActorNotAvailable);
        }

        Ok(())
    }

    /// Apply signatures to transaction
    async fn apply_signatures(
        &mut self,
        pegout_id: String,
        signature_set: SignatureSet,
    ) -> Result<(), PegOutError> {
        info!("Applying signatures to pegout {}", pegout_id);

        if let Some(pegout) = self.pending_pegouts.get_mut(&pegout_id) {
            if let Some(unsigned_tx) = &pegout.unsigned_tx {
                // Apply signatures to create signed transaction
                let signed_tx = self.signature_coordinator.apply_signatures(
                    unsigned_tx,
                    &signature_set,
                )?;

                pegout.signed_tx = Some(signed_tx.clone());
                pegout.signature_status.status = SignatureCollectionStatus::Complete;
                pegout.status = PegOutStatus::SignaturesComplete;
                pegout.last_updated = SystemTime::now();

                self.metrics.record_signatures_applied();

                // Initiate broadcasting
                self.initiate_broadcasting(pegout_id, signed_tx).await?;
            } else {
                return Err(PegOutError::MissingUnsignedTransaction(pegout_id));
            }
        } else {
            return Err(PegOutError::OperationNotFound(pegout_id));
        }

        Ok(())
    }

    /// Initiate transaction broadcasting
    async fn initiate_broadcasting(
        &mut self,
        pegout_id: String,
        signed_tx: Transaction,
    ) -> Result<(), PegOutError> {
        info!("Initiating broadcasting for pegout {}", pegout_id);

        if let Some(pegout) = self.pending_pegouts.get_mut(&pegout_id) {
            pegout.status = PegOutStatus::Broadcasting;
            pegout.last_updated = SystemTime::now();

            // Broadcast transaction
            match self.bitcoin_client.send_raw_transaction(&signed_tx).await {
                Ok(txid) => {
                    pegout.broadcast_txid = Some(txid);
                    pegout.status = PegOutStatus::Broadcast { txid, confirmations: 0 };
                    pegout.last_updated = SystemTime::now();

                    self.metrics.record_transaction_broadcast();
                    self.performance_tracker.complete_operation(
                        pegout_id,
                        OperationEventType::TransactionBroadcast,
                    );

                    info!("Successfully broadcast pegout {} transaction: {}", pegout_id, txid);
                }
                Err(e) => {
                    error!("Failed to broadcast transaction for pegout {}: {:?}", pegout_id, e);
                    pegout.status = PegOutStatus::Failed {
                        reason: format!("Broadcast failed: {:?}", e),
                        recoverable: true,
                    };
                    self.record_error(PegOutError::BroadcastFailed(e.to_string()));
                    return Err(PegOutError::BroadcastFailed(e.to_string()));
                }
            }
        }

        Ok(())
    }

    /// Start periodic signature monitoring
    fn start_signature_monitoring(&mut self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(30), |actor, _ctx| {
            // Check for signature timeouts
            let now = SystemTime::now();
            let mut timed_out_pegouts = Vec::new();

            for (pegout_id, pegout) in &actor.pending_pegouts {
                if matches!(pegout.status, PegOutStatus::RequestingSignatures | PegOutStatus::CollectingSignatures { .. }) {
                    if let Some(requested_at) = pegout.signature_status.requested_at {
                        if now.duration_since(requested_at).unwrap_or_default() > actor.config.signature_timeout {
                            timed_out_pegouts.push(pegout_id.clone());
                        }
                    }
                }
            }

            // Handle timeouts
            for pegout_id in timed_out_pegouts {
                warn!("Signature request timed out for pegout {}", pegout_id);
                if let Some(pegout) = actor.pending_pegouts.get_mut(&pegout_id) {
                    pegout.status = PegOutStatus::Failed {
                        reason: "Signature collection timed out".to_string(),
                        recoverable: true,
                    };
                    pegout.signature_status.status = SignatureCollectionStatus::Timeout;
                    actor.metrics.record_signature_timeout();
                }
            }
        });
    }

    /// Start transaction broadcasting monitoring
    fn start_transaction_broadcasting(&mut self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(60), |actor, _ctx| {
            // Monitor broadcast transactions for confirmations
            let broadcast_pegouts: Vec<(String, Txid)> = actor.pending_pegouts
                .iter()
                .filter_map(|(id, pegout)| {
                    if let PegOutStatus::Broadcast { txid, .. } = pegout.status {
                        Some((id.clone(), txid))
                    } else {
                        None
                    }
                })
                .collect();

            for (pegout_id, txid) in broadcast_pegouts {
                let bitcoin_client = actor.bitcoin_client.clone();
                let fut = async move {
                    bitcoin_client.get_transaction_confirmations(&txid).await
                };
                
                let fut = actix::fut::wrap_future::<_, Self>(fut);
                ctx.spawn(fut.map(move |result, actor, _ctx| {
                    match result {
                        Ok(confirmations) => {
                            actor.update_transaction_confirmations(pegout_id, txid, confirmations);
                        }
                        Err(e) => {
                            warn!("Error getting confirmations for {}: {:?}", txid, e);
                        }
                    }
                }));
            }
        });
    }

    /// Update transaction confirmations
    fn update_transaction_confirmations(&mut self, pegout_id: String, txid: Txid, confirmations: u32) {
        if let Some(pegout) = self.pending_pegouts.get_mut(&pegout_id) {
            let required_confirmations = MIN_PEGOUT_CONFIRMATIONS;
            
            pegout.status = if confirmations >= required_confirmations {
                PegOutStatus::Completed {
                    txid,
                    final_confirmations: confirmations,
                }
            } else {
                PegOutStatus::Confirmed { txid, confirmations }
            };
            
            pegout.last_updated = SystemTime::now();

            if confirmations >= required_confirmations {
                self.metrics.record_pegout_completed();
                self.performance_tracker.complete_operation(
                    pegout_id,
                    OperationEventType::PegOutCompleted,
                );
                info!("PegOut {} completed with {} confirmations", pegout_id, confirmations);
            }
        }
    }

    /// Start UTXO refresh
    fn start_utxo_refresh(&mut self, ctx: &mut Context<Self>) {
        ctx.run_interval(UTXO_REFRESH_INTERVAL, |_actor, _ctx| {
            // Refresh UTXO set from Bitcoin node
            // This would be implemented to periodically update the UTXO manager
        });
    }

    /// Start retry processing
    fn start_retry_processing(&mut self, ctx: &mut Context<Self>) {
        ctx.run_interval(self.config.broadcast_retry_delay, |actor, _ctx| {
            let now = SystemTime::now();
            let mut operations_to_retry = Vec::new();

            // Find operations ready for retry
            for (i, retry_op) in actor.retry_queue.iter().enumerate() {
                if now >= retry_op.next_retry {
                    operations_to_retry.push(i);
                }
            }

            // Process retries
            for &index in operations_to_retry.iter().rev() {
                if let Some(retry_op) = actor.retry_queue.get(index).cloned() {
                    actor.retry_queue.remove(index);
                    
                    if retry_op.retry_count < actor.config.broadcast_retry_attempts {
                        info!("Retrying pegout operation {} (attempt {})", 
                              retry_op.pegout_id, retry_op.retry_count + 1);
                        actor.execute_retry_operation(retry_op);
                    } else {
                        error!("Max retries exceeded for pegout {}", retry_op.pegout_id);
                        actor.metrics.record_max_retries_exceeded();
                    }
                }
            }
        });
    }

    /// Execute retry operation
    fn execute_retry_operation(&mut self, retry_op: RetryablePegOut) {
        // Implementation would retry the specific operation
        // This is a placeholder for the retry logic
    }

    /// Record error for tracking
    fn record_error(&mut self, error: PegOutError) {
        self.recent_errors.push(error.clone());
        
        // Keep only recent errors
        if self.recent_errors.len() > 100 {
            self.recent_errors.drain(0..10);
        }

        self.metrics.record_error(&error);
    }

    /// Get actor status
    pub fn get_status(&self) -> PegOutActorStatus {
        PegOutActorStatus {
            state: self.state.clone(),
            pending_pegouts: self.pending_pegouts.len(),
            total_pegouts_processed: self.metrics.get_pegouts_processed(),
            recent_errors: self.recent_errors.len(),
            uptime: SystemTime::now().duration_since(self.metrics.start_time).unwrap_or_default(),
        }
    }
}

/// PegOut actor status
#[derive(Debug, Clone)]
pub struct PegOutActorStatus {
    pub state: PegOutState,
    pub pending_pegouts: usize,
    pub total_pegouts_processed: u64,
    pub recent_errors: usize,
    pub uptime: Duration,
}

impl Actor for PegOutActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("PegOut actor starting");
        
        let fut = self.initialize(ctx);
        let fut = actix::fut::wrap_future::<_, Self>(fut);
        ctx.spawn(fut.map(|result, _actor, ctx| {
            match result {
                Ok(_) => {
                    info!("PegOut actor started successfully");
                }
                Err(e) => {
                    error!("Failed to initialize PegOut actor: {:?}", e);
                    ctx.stop();
                }
            }
        }));
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("PegOut actor stopped");
        self.metrics.record_actor_stopped();
    }
}

/// PegOut errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum PegOutError {
    #[error("Invalid amount: {amount}, minimum: {minimum}")]
    InvalidAmount { amount: u64, minimum: u64 },
    
    #[error("Insufficient funds: required {required}, available {available}")]
    InsufficientFunds { required: u64, available: u64 },
    
    #[error("Operation not found: {0}")]
    OperationNotFound(String),
    
    #[error("Signature request failed: {0}")]
    SignatureRequestFailed(String),
    
    #[error("Broadcast failed: {0}")]
    BroadcastFailed(String),
    
    #[error("Actor communication error: {0}")]
    ActorCommunicationError(String),
    
    #[error("Stream actor not available")]
    StreamActorNotAvailable,
    
    #[error("Missing unsigned transaction: {0}")]
    MissingUnsignedTransaction(String),
    
    #[error("Transaction building error: {0}")]
    TransactionBuildingError(String),
    
    #[error("Signature error: {0}")]
    SignatureError(String),
    
    #[error("UTXO error: {0}")]
    UtxoError(String),
    
    #[error("Bitcoin RPC error: {0}")]
    BitcoinRpcError(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}