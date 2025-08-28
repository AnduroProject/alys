//! Payload Handler Implementation
//!
//! Handles all payload-related operations including building, getting, and executing payloads.
//! These are the core operations that integrate with the Ethereum execution layer.

use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;
use tracing::*;
use actix::prelude::*;

use crate::types::*;
use super::super::{
    actor::EngineActor,
    messages::*,
    state::{PendingPayload, PayloadStatus, PayloadPriority},
    engine::{AddBalance, ConsensusAmount},
    EngineError, EngineResult,
};

/// Handler for BuildPayloadMessage - builds new execution payloads
impl Handler<BuildPayloadMessage> for EngineActor {
    type Result = ResponseFuture<MessageResult<PayloadId>>;

    fn handle(&mut self, msg: BuildPayloadMessage, _ctx: &mut Self::Context) -> Self::Result {
        let engine = self.engine.clone();
        let correlation_id = msg.correlation_id;
        let payload_id = format!("payload_{}_{}", msg.timestamp, Uuid::new_v4());
        let started_at = Instant::now();
        
        info!(
            correlation_id = ?correlation_id,
            payload_id = %payload_id,
            parent_hash = %msg.parent_hash,
            timestamp = %msg.timestamp,
            withdrawals = %msg.withdrawals.len(),
            priority = ?msg.priority,
            "Building new execution payload"
        );

        // Update metrics
        self.metrics.payload_build_requested();
        
        // Convert withdrawals to AddBalance format for engine
        let add_balances: Vec<AddBalance> = msg.withdrawals
            .iter()
            .map(|w| AddBalance::from((w.address, ConsensusAmount(w.amount))))
            .collect();

        Box::pin(async move {
            let build_start = Instant::now();
            
            match engine.build_block(
                Duration::from_secs(msg.timestamp),
                Some(msg.parent_hash),
                add_balances,
            ).await {
                Ok(execution_payload) => {
                    let build_duration = build_start.elapsed();
                    
                    info!(
                        correlation_id = ?correlation_id,
                        payload_id = %payload_id,
                        build_time_ms = %build_duration.as_millis(),
                        block_hash = %execution_payload.block_hash(),
                        gas_used = %execution_payload.gas_used(),
                        "Successfully built execution payload"
                    );
                    
                    Ok(payload_id)
                },
                Err(e) => {
                    let build_duration = build_start.elapsed();
                    
                    error!(
                        correlation_id = ?correlation_id,
                        payload_id = %payload_id,
                        build_time_ms = %build_duration.as_millis(),
                        error = %e,
                        "Failed to build execution payload"
                    );
                    
                    Err(EngineError::ClientError(super::super::ClientError::RpcError(format!("{}", e))))
                }
            }
        })
    }
}

/// Handler for GetPayloadMessage - retrieves built payloads
impl Handler<GetPayloadMessage> for EngineActor {
    type Result = ResponseFuture<MessageResult<ExecutionPayload>>;

    fn handle(&mut self, msg: GetPayloadMessage, _ctx: &mut Self::Context) -> Self::Result {
        let correlation_id = msg.correlation_id;
        let payload_id = msg.payload_id.clone();
        
        debug!(
            correlation_id = ?correlation_id,
            payload_id = %payload_id,
            "Retrieving execution payload"
        );

        // Check if we have this payload in our pending payloads
        if let Some(pending_payload) = self.state.pending_payloads.get(&msg.payload_id) {
            let payload = pending_payload.payload.clone();
            
            info!(
                correlation_id = ?correlation_id,
                payload_id = %payload_id,
                block_hash = %payload.block_hash(),
                "Found payload in pending list"
            );
            
            // Update payload status to indicate it was retrieved
            if let Some(pending) = self.state.pending_payloads.get_mut(&msg.payload_id) {
                if matches!(pending.status, PayloadStatus::Building { .. }) {
                    pending.status = PayloadStatus::Built {
                        completed_at: SystemTime::now(),
                        build_duration: Instant::now().duration_since(pending.created_at),
                    };
                }
            }
            
            self.metrics.payload_retrieved();
            
            Box::pin(async move { Ok(payload) })
        } else {
            warn!(
                correlation_id = ?correlation_id,
                payload_id = %payload_id,
                "Payload not found in pending list"
            );
            
            self.metrics.payload_not_found();
            
            Box::pin(async move {
                Err(EngineError::PayloadNotFound(payload_id))
            })
        }
    }
}

/// Handler for ExecutePayloadMessage - executes payloads on the execution client
impl Handler<ExecutePayloadMessage> for EngineActor {
    type Result = ResponseFuture<MessageResult<PayloadExecutionResult>>;

    fn handle(&mut self, msg: ExecutePayloadMessage, _ctx: &mut Self::Context) -> Self::Result {
        let engine = self.engine.clone();
        let correlation_id = msg.correlation_id;
        let block_hash = msg.payload.block_hash();
        let validate = msg.validate;
        let timeout = msg.timeout.unwrap_or(Duration::from_secs(30));
        
        info!(
            correlation_id = ?correlation_id,
            block_hash = %block_hash,
            validate = %validate,
            timeout_ms = %timeout.as_millis(),
            "Executing payload"
        );

        // Update metrics
        self.metrics.payload_execution_requested();
        
        Box::pin(async move {
            let execution_start = Instant::now();
            
            // Execute the payload via the engine
            match engine.commit_block(msg.payload.clone()).await {
                Ok(committed_hash) => {
                    let execution_duration = execution_start.elapsed();
                    
                    info!(
                        correlation_id = ?correlation_id,
                        block_hash = %block_hash,
                        committed_hash = %committed_hash,
                        execution_time_ms = %execution_duration.as_millis(),
                        "Successfully executed payload"
                    );
                    
                    // Create successful execution result
                    let result = PayloadExecutionResult {
                        status: ExecutionStatus::Valid,
                        latest_valid_hash: Some(committed_hash),
                        validation_error: None,
                        gas_used: Some(msg.payload.gas_used()),
                        state_root: Some(msg.payload.state_root()),
                        receipts: vec![], // TODO: Fetch actual receipts
                        execution_duration,
                    };
                    
                    Ok(result)
                },
                Err(e) => {
                    let execution_duration = execution_start.elapsed();
                    
                    error!(
                        correlation_id = ?correlation_id,
                        block_hash = %block_hash,
                        execution_time_ms = %execution_duration.as_millis(),
                        error = %e,
                        "Failed to execute payload"
                    );
                    
                    // Create failed execution result
                    let result = PayloadExecutionResult {
                        status: ExecutionStatus::ExecutionFailed,
                        latest_valid_hash: None,
                        validation_error: Some(format!("{}", e)),
                        gas_used: None,
                        state_root: None,
                        receipts: vec![],
                        execution_duration,
                    };
                    
                    Ok(result) // Return the failure result, don't error the message
                }
            }
        })
    }
}

/// Handler for ChainRequestPayloadMessage - handles payload requests from ChainActor
impl Handler<ChainRequestPayloadMessage> for EngineActor {
    type Result = ResponseFuture<MessageResult<PayloadId>>;

    fn handle(&mut self, msg: ChainRequestPayloadMessage, ctx: &mut Self::Context) -> Self::Result {
        let correlation_id = msg.correlation_id;
        let block_context = msg.block_context.clone();
        
        info!(
            correlation_id = %correlation_id,
            height = %block_context.height,
            slot = %block_context.slot,
            authority_index = %block_context.authority_index,
            withdrawals = %msg.withdrawals.len(),
            "Received payload request from ChainActor"
        );

        // Create BuildPayloadMessage from the chain request
        let build_msg = BuildPayloadMessage {
            parent_hash: block_context.parent_hash,
            timestamp: block_context.timestamp,
            fee_recipient: block_context.fee_recipient,
            withdrawals: msg.withdrawals,
            prev_randao: None, // TODO: Use proper randao from beacon
            gas_limit: None, // Use default gas limit
            priority: PayloadPriority::High, // Chain requests are high priority
            correlation_id: Some(correlation_id),
            trace_context: None, // TODO: Propagate trace context
        };

        // Forward to the regular payload handler
        ctx.address().send(build_msg)
    }
}

/// Handler for ValidateTransactionMessage - validates individual transactions
impl Handler<ValidateTransactionMessage> for EngineActor {
    type Result = ResponseFuture<MessageResult<TransactionValidationResult>>;

    fn handle(&mut self, msg: ValidateTransactionMessage, _ctx: &mut Self::Context) -> Self::Result {
        let engine = self.engine.clone();
        let correlation_id = msg.correlation_id;
        let tx_hash = msg.tx_hash;
        
        debug!(
            correlation_id = ?correlation_id,
            tx_hash = %tx_hash,
            "Validating transaction"
        );

        Box::pin(async move {
            match engine.get_transaction_receipt(tx_hash).await {
                Ok(Some(receipt)) => {
                    // Transaction exists and has been executed
                    let result = TransactionValidationResult {
                        is_valid: receipt.status == Some(U64::from(1)), // Success status
                        receipt: Some(receipt.clone()),
                        errors: vec![],
                        gas_used: receipt.gas_used.map(|g| g.as_u64()),
                    };
                    
                    debug!(
                        correlation_id = ?correlation_id,
                        tx_hash = %tx_hash,
                        is_valid = %result.is_valid,
                        gas_used = ?result.gas_used,
                        "Transaction validation completed"
                    );
                    
                    Ok(result)
                },
                Ok(None) => {
                    // Transaction not found
                    debug!(
                        correlation_id = ?correlation_id,
                        tx_hash = %tx_hash,
                        "Transaction not found"
                    );
                    
                    Ok(TransactionValidationResult {
                        is_valid: false,
                        receipt: None,
                        errors: vec!["Transaction not found".to_string()],
                        gas_used: None,
                    })
                },
                Err(e) => {
                    warn!(
                        correlation_id = ?correlation_id,
                        tx_hash = %tx_hash,
                        error = %e,
                        "Failed to validate transaction"
                    );
                    
                    Ok(TransactionValidationResult {
                        is_valid: false,
                        receipt: None,
                        errors: vec![format!("Validation error: {}", e)],
                        gas_used: None,
                    })
                }
            }
        })
    }
}

/// Handler for ValidateIncomingTransactionMessage - validates transactions from network
impl Handler<ValidateIncomingTransactionMessage> for EngineActor {
    type Result = ResponseFuture<MessageResult<TransactionValidationResult>>;

    fn handle(&mut self, msg: ValidateIncomingTransactionMessage, _ctx: &mut Self::Context) -> Self::Result {
        let correlation_id = msg.correlation_id;
        let peer_info = msg.peer_info.clone();
        
        debug!(
            correlation_id = ?correlation_id,
            peer_id = %peer_info.peer_id,
            transaction_size = %msg.transaction.len(),
            "Validating incoming transaction from network"
        );

        // TODO: Implement proper transaction validation
        // This would include:
        // 1. Parse transaction from raw bytes
        // 2. Validate signature
        // 3. Check nonce and balance
        // 4. Validate gas limit and price
        // 5. Check transaction pool constraints
        
        Box::pin(async move {
            // Simplified validation for now
            let is_valid = !msg.transaction.is_empty() && msg.transaction.len() < 131072; // Max 128KB
            
            let result = TransactionValidationResult {
                is_valid,
                receipt: None, // No receipt for pending transactions
                errors: if is_valid { 
                    vec![] 
                } else { 
                    vec!["Transaction failed basic validation".to_string()] 
                },
                gas_used: None, // No gas used for validation only
            };
            
            debug!(
                correlation_id = ?correlation_id,
                peer_id = %peer_info.peer_id,
                is_valid = %result.is_valid,
                "Incoming transaction validation completed"
            );
            
            Ok(result)
        })
    }
}

impl EngineActor {
    /// Internal helper to create a pending payload entry
    pub(super) fn create_pending_payload(
        &mut self,
        payload_id: String,
        msg: &BuildPayloadMessage,
        execution_payload: ExecutionPayload,
    ) -> PendingPayload {
        let pending = PendingPayload {
            payload_id: payload_id.clone(),
            payload: execution_payload,
            status: PayloadStatus::Built {
                completed_at: SystemTime::now(),
                build_duration: Instant::now().duration_since(Instant::now()), // Will be updated
            },
            created_at: Instant::now(),
            parent_hash: msg.parent_hash,
            fee_recipient: msg.fee_recipient,
            withdrawals: msg.withdrawals.clone(),
            correlation_id: msg.correlation_id,
            priority: msg.priority.clone(),
            retry_attempts: 0,
            trace_context: msg.trace_context.clone(),
        };
        
        // Add to pending payloads
        self.state.add_pending_payload(pending.clone());
        
        pending
    }
    
    /// Internal helper to validate payload execution result
    pub(super) fn validate_execution_result(
        &self,
        payload: &ExecutionPayload,
        result: &PayloadExecutionResult,
    ) -> bool {
        // Basic validation checks
        if result.status != ExecutionStatus::Valid {
            return false;
        }
        
        // Check that we got a valid hash back
        if result.latest_valid_hash.is_none() {
            return false;
        }
        
        // Check that gas used is reasonable
        if let Some(gas_used) = result.gas_used {
            if gas_used > payload.gas_limit() {
                warn!(
                    "Execution used more gas than limit: used={}, limit={}",
                    gas_used,
                    payload.gas_limit()
                );
                return false;
            }
        }
        
        // Additional validation can be added here
        true
    }
    
    /// Internal helper to handle payload execution timeout
    pub(super) async fn handle_payload_timeout(&mut self, payload_id: &str) {
        if let Some(mut payload) = self.state.pending_payloads.get_mut(payload_id) {
            warn!(
                payload_id = %payload_id,
                age_ms = %Instant::now().duration_since(payload.created_at).as_millis(),
                "Payload execution timed out"
            );
            
            payload.status = PayloadStatus::TimedOut {
                timed_out_at: SystemTime::now(),
                timeout_duration: Instant::now().duration_since(payload.created_at),
            };
            
            self.metrics.payload_timeout();
        }
    }
    
    /// Internal helper to retry failed payload operations
    pub(super) async fn retry_payload_operation(
        &mut self,
        payload_id: &str,
        max_retries: u32,
    ) -> EngineResult<()> {
        if let Some(payload) = self.state.pending_payloads.get_mut(payload_id) {
            if payload.retry_attempts >= max_retries {
                warn!(
                    payload_id = %payload_id,
                    retry_attempts = %payload.retry_attempts,
                    "Maximum retry attempts exceeded for payload"
                );
                
                payload.status = PayloadStatus::Failed {
                    error: "Maximum retry attempts exceeded".to_string(),
                    failed_at: SystemTime::now(),
                    retryable: false,
                };
                
                return Err(EngineError::ExecutionTimeout);
            }
            
            payload.retry_attempts += 1;
            
            info!(
                payload_id = %payload_id,
                retry_attempt = %payload.retry_attempts,
                max_retries = %max_retries,
                "Retrying payload operation"
            );
            
            // TODO: Implement actual retry logic
            // This would involve re-submitting the operation to the engine
        }
        
        Ok(())
    }
}