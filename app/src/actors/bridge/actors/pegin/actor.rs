//! PegIn Actor Implementation
//! 
//! Specialized actor for processing Bitcoin deposits (peg-in operations)

use actix::prelude::*;
use bitcoin::{Transaction, Txid, Address as BtcAddress};
use ethereum_types::{H160, H256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use crate::actors::bridge::{
    config::PegInConfig,
    messages::*,
    shared::*,
};
use crate::types::*;
use super::{handlers::*, validation::*, confirmation::*, state::*, metrics::*};

/// PegIn actor for Bitcoin deposit processing
pub struct PegInActor {
    /// Configuration
    config: PegInConfig,
    
    /// Bitcoin client for blockchain interaction
    bitcoin_client: Arc<dyn BitcoinRpc>,
    
    /// Monitored addresses (federation addresses)
    monitored_addresses: Vec<BtcAddress>,
    
    /// Pending deposits being processed
    pending_deposits: HashMap<Txid, PendingDeposit>,
    
    /// Confirmation tracking system
    confirmation_tracker: ConfirmationTracker,
    
    /// Validation engine
    validator: DepositValidator,
    
    /// Actor references
    bridge_coordinator: Option<Addr<super::super::bridge::BridgeActor>>,
    chain_actor: Option<Addr<crate::actors::chain::ChainActor>>,
    
    /// Metrics and monitoring
    metrics: PegInMetrics,
    performance_tracker: OperationTracker,
    
    /// State management
    state: PegInState,
    last_block_checked: u64,
    
    /// Error tracking
    recent_errors: Vec<PegInError>,
    retry_queue: Vec<RetryableOperation>,
}

/// Retryable operation for failed deposits
#[derive(Debug, Clone)]
pub struct RetryableOperation {
    pub operation_id: String,
    pub operation: PegInOperation,
    pub retry_count: u32,
    pub last_attempt: SystemTime,
    pub next_retry: SystemTime,
    pub error: PegInError,
}

/// Peg-in operation types
#[derive(Debug, Clone)]
pub enum PegInOperation {
    ProcessDeposit {
        txid: Txid,
        bitcoin_tx: Transaction,
    },
    ValidateDeposit {
        pegin_id: String,
        deposit: DepositTransaction,
    },
    ConfirmDeposit {
        pegin_id: String,
    },
}

impl PegInActor {
    /// Create new PegIn actor
    pub fn new(
        config: PegInConfig,
        bitcoin_client: Arc<dyn BitcoinRpc>,
        monitored_addresses: Vec<BtcAddress>,
    ) -> Result<Self, PegInError> {
        let confirmation_tracker = ConfirmationTracker::new(config.confirmation_threshold);
        let validator = DepositValidator::new(monitored_addresses.clone())?;
        let metrics = PegInMetrics::new()?;
        let performance_tracker = OperationTracker::new();

        Ok(Self {
            config,
            bitcoin_client,
            monitored_addresses,
            pending_deposits: HashMap::new(),
            confirmation_tracker,
            validator,
            bridge_coordinator: None,
            chain_actor: None,
            metrics,
            performance_tracker,
            state: PegInState::Initializing,
            last_block_checked: 0,
            recent_errors: Vec::new(),
            retry_queue: Vec::new(),
        })
    }

    /// Initialize PegIn actor
    async fn initialize(&mut self, ctx: &mut Context<Self>) -> Result<(), PegInError> {
        info!("Initializing PegIn actor");

        // Get current block height
        self.last_block_checked = self.bitcoin_client.get_block_count().await
            .map_err(|e| PegInError::BitcoinRpcError(e.to_string()))?;

        // Start monitoring tasks
        self.start_monitoring(ctx);
        self.start_confirmation_tracking(ctx);
        self.start_retry_processing(ctx);

        // Update state
        self.state = PegInState::Monitoring;
        self.metrics.record_actor_started();

        info!("PegIn actor initialized successfully, monitoring from block {}", self.last_block_checked);
        Ok(())
    }

    /// Start Bitcoin blockchain monitoring
    fn start_monitoring(&mut self, ctx: &mut Context<Self>) {
        let monitoring_interval = self.config.monitoring_interval;
        ctx.run_interval(monitoring_interval, move |actor, _ctx| {
            let bitcoin_client = actor.bitcoin_client.clone();
            let monitored_addresses = actor.monitored_addresses.clone();
            let last_block_checked = actor.last_block_checked;

            let fut = async move {
                actor.monitor_bitcoin_blockchain().await
            };
            
            let fut = actix::fut::wrap_future::<_, Self>(fut);
            ctx.spawn(fut.map(|result, actor, _ctx| {
                match result {
                    Ok(new_deposits) => {
                        for deposit in new_deposits {
                            info!("New deposit detected: {}", deposit.txid);
                            actor.handle_new_deposit(deposit);
                        }
                    }
                    Err(e) => {
                        error!("Error monitoring Bitcoin blockchain: {:?}", e);
                        actor.record_error(e);
                    }
                }
            }));
        });
    }

    /// Monitor Bitcoin blockchain for new deposits
    async fn monitor_bitcoin_blockchain(&mut self) -> Result<Vec<DepositTransaction>, PegInError> {
        let current_block = self.bitcoin_client.get_block_count().await
            .map_err(|e| PegInError::BitcoinRpcError(e.to_string()))?;

        if current_block <= self.last_block_checked {
            return Ok(Vec::new());
        }

        let mut new_deposits = Vec::new();

        // Check each block since last check
        for block_height in (self.last_block_checked + 1)..=current_block {
            let block_hash = self.bitcoin_client.get_block_hash(block_height).await
                .map_err(|e| PegInError::BitcoinRpcError(e.to_string()))?;
                
            let block = self.bitcoin_client.get_block(&block_hash).await
                .map_err(|e| PegInError::BitcoinRpcError(e.to_string()))?;

            // Check each transaction in the block
            for tx in &block.txdata {
                if let Some(deposit) = self.check_transaction_for_deposits(tx, block_height).await? {
                    new_deposits.push(deposit);
                }
            }
        }

        self.last_block_checked = current_block;
        self.metrics.record_blocks_processed(current_block - self.last_block_checked);

        Ok(new_deposits)
    }

    /// Check transaction for deposits to federation addresses
    async fn check_transaction_for_deposits(
        &self,
        tx: &Transaction,
        block_height: u64,
    ) -> Result<Option<DepositTransaction>, PegInError> {
        // Check if any output is to a monitored address
        for (vout, output) in tx.output.iter().enumerate() {
            for monitored_addr in &self.monitored_addresses {
                if output.script_pubkey == monitored_addr.script_pubkey() {
                    // Found deposit output
                    debug!("Found deposit output in tx {} vout {}", tx.compute_txid(), vout);

                    // Extract EVM address from OP_RETURN (if present)
                    let evm_address = self.extract_evm_address(tx)?;

                    let deposit = DepositTransaction {
                        txid: tx.compute_txid(),
                        bitcoin_tx: tx.clone(),
                        federation_output: output.clone(),
                        op_return_data: self.get_op_return_data(tx),
                        evm_address,
                        amount: output.value,
                        block_height: block_height as u32,
                        detected_at: SystemTime::now(),
                    };

                    return Ok(Some(deposit));
                }
            }
        }

        Ok(None)
    }

    /// Extract EVM address from OP_RETURN output
    fn extract_evm_address(&self, tx: &Transaction) -> Result<Option<H160>, PegInError> {
        // Find OP_RETURN output
        for output in &tx.output {
            if output.script_pubkey.is_op_return() {
                let script_bytes = output.script_pubkey.as_bytes();
                
                // Basic OP_RETURN parsing
                if script_bytes.len() >= 22 { // OP_RETURN + length + 20 bytes address
                    let address_bytes = &script_bytes[2..22];
                    return Ok(Some(H160::from_slice(address_bytes)));
                }
            }
        }

        Ok(None)
    }

    /// Get OP_RETURN data from transaction
    fn get_op_return_data(&self, tx: &Transaction) -> Option<Vec<u8>> {
        for output in &tx.output {
            if output.script_pubkey.is_op_return() {
                return Some(output.script_pubkey.as_bytes().to_vec());
            }
        }
        None
    }

    /// Handle new deposit detection
    fn handle_new_deposit(&mut self, deposit: DepositTransaction) {
        let pegin_id = format!("pegin_{}", Uuid::new_v4());
        
        // Validate deposit
        match self.validator.validate_deposit(&deposit) {
            Ok(validation_result) => {
                if validation_result.valid {
                    let pending_deposit = PendingDeposit {
                        pegin_id: pegin_id.clone(),
                        txid: deposit.txid,
                        bitcoin_tx: deposit.bitcoin_tx,
                        federation_output: deposit.federation_output,
                        evm_address: validation_result.extracted_address.unwrap_or(H160::zero()),
                        amount: deposit.amount,
                        confirmations: 0,
                        status: DepositStatus::Detected,
                        created_at: SystemTime::now(),
                        last_updated: SystemTime::now(),
                        retry_count: 0,
                    };

                    self.pending_deposits.insert(deposit.txid, pending_deposit);
                    self.confirmation_tracker.start_tracking(deposit.txid, deposit.block_height);
                    self.metrics.record_deposit_detected();

                    info!("Valid deposit detected: {} for {} sats to {:?}", 
                          deposit.txid, deposit.amount, validation_result.extracted_address);

                    // Notify bridge coordinator
                    self.notify_bridge_coordinator_deposit_detected(pegin_id, deposit.txid);
                } else {
                    warn!("Invalid deposit detected: {} - {:?}", 
                          deposit.txid, validation_result.errors);
                    self.metrics.record_invalid_deposit();
                }
            }
            Err(e) => {
                error!("Error validating deposit {}: {:?}", deposit.txid, e);
                self.record_error(PegInError::ValidationError(e.to_string()));
            }
        }
    }

    /// Start confirmation tracking
    fn start_confirmation_tracking(&mut self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(30), move |actor, _ctx| {
            let pending_txids: Vec<Txid> = actor.pending_deposits.keys().cloned().collect();
            
            for txid in pending_txids {
                let bitcoin_client = actor.bitcoin_client.clone();
                let fut = async move {
                    bitcoin_client.get_transaction_confirmations(&txid).await
                };
                
                let fut = actix::fut::wrap_future::<_, Self>(fut);
                ctx.spawn(fut.map(move |result, actor, _ctx| {
                    match result {
                        Ok(confirmations) => {
                            actor.update_deposit_confirmations(txid, confirmations);
                        }
                        Err(e) => {
                            warn!("Error getting confirmations for {}: {:?}", txid, e);
                        }
                    }
                }));
            }
        });
    }

    /// Update deposit confirmations
    fn update_deposit_confirmations(&mut self, txid: Txid, confirmations: u32) {
        if let Some(deposit) = self.pending_deposits.get_mut(&txid) {
            let old_confirmations = deposit.confirmations;
            deposit.confirmations = confirmations;
            deposit.last_updated = SystemTime::now();

            // Update status based on confirmations
            if confirmations >= self.config.confirmation_threshold {
                if !matches!(deposit.status, DepositStatus::Confirmed | DepositStatus::Minting | DepositStatus::Completed { .. }) {
                    deposit.status = DepositStatus::Confirmed;
                    self.metrics.record_deposit_confirmed();
                    
                    info!("Deposit {} confirmed with {} confirmations", txid, confirmations);
                    
                    // Initiate minting process
                    self.initiate_minting(deposit.pegin_id.clone(), deposit.evm_address, deposit.amount);
                }
            } else {
                deposit.status = DepositStatus::ConfirmationPending { 
                    current: confirmations, 
                    required: self.config.confirmation_threshold 
                };
            }

            debug!("Updated confirmations for {}: {} -> {}", txid, old_confirmations, confirmations);
        }
    }

    /// Initiate minting process
    fn initiate_minting(&mut self, pegin_id: String, recipient: H160, amount: u64) {
        info!("Initiating minting for pegin {} to {:?} for {} sats", pegin_id, recipient, amount);
        
        // In a real implementation, this would communicate with the ChainActor
        // to mint tokens on the Alys EVM
        
        // For now, mark as minting
        if let Some(deposit) = self.pending_deposits.values_mut()
            .find(|d| d.pegin_id == pegin_id) {
            deposit.status = DepositStatus::Minting;
            self.metrics.record_minting_initiated();
        }
    }

    /// Start retry processing
    fn start_retry_processing(&mut self, ctx: &mut Context<Self>) {
        ctx.run_interval(self.config.retry_delay, move |actor, _ctx| {
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
                    
                    if retry_op.retry_count < actor.config.retry_attempts {
                        info!("Retrying operation {} (attempt {})", 
                              retry_op.operation_id, retry_op.retry_count + 1);
                        actor.execute_retry_operation(retry_op);
                    } else {
                        error!("Max retries exceeded for operation {}", retry_op.operation_id);
                        actor.metrics.record_max_retries_exceeded();
                    }
                }
            }
        });
    }

    /// Execute retry operation
    fn execute_retry_operation(&mut self, mut retry_op: RetryableOperation) {
        retry_op.retry_count += 1;
        retry_op.last_attempt = SystemTime::now();

        match retry_op.operation {
            PegInOperation::ProcessDeposit { txid, bitcoin_tx } => {
                // Retry deposit processing
                if let Some(deposit) = self.check_transaction_for_deposits(&bitcoin_tx, 0).await.unwrap_or(None) {
                    self.handle_new_deposit(deposit);
                } else {
                    // Add back to retry queue with exponential backoff
                    retry_op.next_retry = SystemTime::now() + Duration::from_secs(60 * retry_op.retry_count as u64);
                    self.retry_queue.push(retry_op);
                }
            }
            _ => {
                // Handle other operation types
            }
        }
    }

    /// Notify bridge coordinator of deposit detection
    fn notify_bridge_coordinator_deposit_detected(&self, pegin_id: String, bitcoin_txid: Txid) {
        if let Some(bridge_coordinator) = &self.bridge_coordinator {
            let msg = BridgeCoordinationMessage::CoordinatePegIn { pegin_id, bitcoin_txid };
            
            let bridge_coordinator = bridge_coordinator.clone();
            actix::spawn(async move {
                if let Err(e) = bridge_coordinator.send(msg).await {
                    error!("Failed to notify bridge coordinator: {:?}", e);
                }
            });
        }
    }

    /// Record error for tracking
    fn record_error(&mut self, error: PegInError) {
        self.recent_errors.push(error.clone());
        
        // Keep only recent errors (last 100)
        if self.recent_errors.len() > 100 {
            self.recent_errors.drain(0..10);
        }

        self.metrics.record_error(&error);
    }

    /// Get actor status
    pub fn get_status(&self) -> PegInActorStatus {
        PegInActorStatus {
            state: self.state.clone(),
            pending_deposits: self.pending_deposits.len(),
            last_block_checked: self.last_block_checked,
            total_deposits_processed: self.metrics.get_deposits_processed(),
            recent_errors: self.recent_errors.len(),
            uptime: SystemTime::now().duration_since(self.metrics.start_time).unwrap_or_default(),
        }
    }
}

/// PegIn actor status
#[derive(Debug, Clone)]
pub struct PegInActorStatus {
    pub state: PegInState,
    pub pending_deposits: usize,
    pub last_block_checked: u64,
    pub total_deposits_processed: u64,
    pub recent_errors: usize,
    pub uptime: Duration,
}

impl Actor for PegInActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("PegIn actor starting");
        
        let fut = self.initialize(ctx);
        let fut = actix::fut::wrap_future::<_, Self>(fut);
        ctx.spawn(fut.map(|result, _actor, ctx| {
            match result {
                Ok(_) => {
                    info!("PegIn actor started successfully");
                }
                Err(e) => {
                    error!("Failed to initialize PegIn actor: {:?}", e);
                    ctx.stop();
                }
            }
        }));
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("PegIn actor stopped");
        self.metrics.record_actor_stopped();
    }
}

/// PegIn errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum PegInError {
    #[error("Bitcoin RPC error: {0}")]
    BitcoinRpcError(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    
    #[error("Actor communication error: {0}")]
    ActorCommunicationError(String),
    
    #[error("Operation timeout: {0}")]
    OperationTimeout(String),
    
    #[error("Insufficient confirmations: {current} < {required}")]
    InsufficientConfirmations { current: u32, required: u32 },
    
    #[error("Invalid deposit: {reason}")]
    InvalidDeposit { reason: String },
    
    #[error("Internal error: {0}")]
    InternalError(String),
}