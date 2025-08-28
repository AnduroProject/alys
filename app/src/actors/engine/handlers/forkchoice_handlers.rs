//! Forkchoice Handler Implementation
//!
//! Handles forkchoice update operations that manage the execution layer's
//! understanding of head, safe, and finalized blocks.

use std::time::{Duration, Instant};
use tracing::*;
use actix::prelude::*;

use lighthouse_wrapper::execution_layer::ForkchoiceState;
use lighthouse_wrapper::types::{Address, MainnetEthSpec};

use crate::types::*;
use super::super::{
    actor::EngineActor,
    messages::*,
    state::ExecutionState,
    EngineError, EngineResult,
};

/// Handler for ForkchoiceUpdatedMessage - updates execution layer forkchoice
impl Handler<ForkchoiceUpdatedMessage> for EngineActor {
    type Result = ResponseFuture<MessageResult<ForkchoiceUpdateResult>>;

    fn handle(&mut self, msg: ForkchoiceUpdatedMessage, _ctx: &mut Self::Context) -> Self::Result {
        let engine = self.engine.clone();
        let correlation_id = msg.correlation_id;
        
        info!(
            correlation_id = ?correlation_id,
            head = %msg.head_block_hash,
            safe = %msg.safe_block_hash,
            finalized = %msg.finalized_block_hash,
            has_payload_attrs = %msg.payload_attributes.is_some(),
            "Updating forkchoice"
        );

        // Update metrics
        self.metrics.forkchoice_update_requested();
        
        // Update finalized block in engine
        let finalized_hash = msg.finalized_block_hash;
        
        Box::pin(async move {
            let update_start = Instant::now();
            
            // Set finalized block in engine
            engine.set_finalized(finalized_hash).await;
            
            // Create forkchoice state for the engine API
            let forkchoice_state = ForkchoiceState {
                head_block_hash: msg.head_block_hash,
                safe_block_hash: msg.safe_block_hash,
                finalized_block_hash: msg.finalized_block_hash,
            };
            
            // Convert payload attributes if provided
            let payload_attributes = msg.payload_attributes.map(|attrs| {
                lighthouse_wrapper::execution_layer::PayloadAttributes::new(
                    attrs.timestamp,
                    attrs.prev_randao,
                    attrs.suggested_fee_recipient,
                    attrs.withdrawals.map(|w| w.into_iter().map(Into::into).collect()),
                )
            });
            
            // Execute forkchoice update
            match engine.api.forkchoice_updated(forkchoice_state, payload_attributes).await {
                Ok(response) => {
                    let update_duration = update_start.elapsed();
                    
                    info!(
                        correlation_id = ?correlation_id,
                        update_time_ms = %update_duration.as_millis(),
                        payload_status = ?response.payload_status,
                        payload_id = ?response.payload_id,
                        "Forkchoice update completed successfully"
                    );
                    
                    // Convert response to our format
                    let result = ForkchoiceUpdateResult {
                        payload_status: convert_payload_status(response.payload_status),
                        latest_valid_hash: response.latest_valid_hash,
                        validation_error: response.validation_error,
                        payload_id: response.payload_id,
                    };
                    
                    Ok(result)
                },
                Err(e) => {
                    let update_duration = update_start.elapsed();
                    
                    error!(
                        correlation_id = ?correlation_id,
                        update_time_ms = %update_duration.as_millis(),
                        error = %e,
                        "Forkchoice update failed"
                    );
                    
                    Err(EngineError::ForkchoiceError(format!("{}", e)))
                }
            }
        })
    }
}

/// Handler for internal finalized block updates
#[derive(Message, Debug, Clone)]
#[rtype(result = "EngineResult<()>")]
pub struct SetFinalizedBlockMessage {
    /// Block hash to mark as finalized
    pub block_hash: Hash256,
    
    /// Block height for logging
    pub block_height: u64,
}

impl Handler<SetFinalizedBlockMessage> for EngineActor {
    type Result = ResponseFuture<EngineResult<()>>;

    fn handle(&mut self, msg: SetFinalizedBlockMessage, _ctx: &mut Self::Context) -> Self::Result {
        let engine = self.engine.clone();
        let block_hash = msg.block_hash;
        let block_height = msg.block_height;
        
        info!(
            height = %block_height,
            hash = %block_hash,
            "Setting finalized block"
        );
        
        Box::pin(async move {
            engine.set_finalized(block_hash).await;
            
            info!(
                height = %block_height,
                hash = %block_hash,
                "Finalized block updated successfully"
            );
            
            Ok(())
        })
    }
}

/// Convert lighthouse payload status to our format
fn convert_payload_status(
    status: lighthouse_wrapper::execution_layer::PayloadStatus
) -> PayloadStatusType {
    use lighthouse_wrapper::execution_layer::PayloadStatus;
    
    match status {
        PayloadStatus::Valid => PayloadStatusType::Valid,
        PayloadStatus::Invalid { .. } => PayloadStatusType::Invalid,
        PayloadStatus::Syncing => PayloadStatusType::Syncing,
        PayloadStatus::Accepted => PayloadStatusType::Accepted,
        PayloadStatus::InvalidBlockHash { .. } => PayloadStatusType::InvalidBlockHash,
        PayloadStatus::InvalidTerminalBlock { .. } => PayloadStatusType::InvalidTerminalBlock,
    }
}

impl EngineActor {
    /// Internal helper to handle forkchoice state transitions
    pub(super) fn handle_forkchoice_transition(
        &mut self,
        old_head: Option<Hash256>,
        new_head: Hash256,
        finalized: Hash256,
    ) {
        // Update internal execution state if needed
        match &mut self.state.execution_state {
            ExecutionState::Ready { head_hash, head_height, last_activity } => {
                *head_hash = Some(new_head);
                *last_activity = std::time::SystemTime::now();
                // head_height would need to be determined from the block
                
                debug!(
                    old_head = ?old_head,
                    new_head = %new_head,
                    finalized = %finalized,
                    "Updated execution state head after forkchoice"
                );
            },
            other_state => {
                debug!(
                    state = ?other_state,
                    new_head = %new_head,
                    "Received forkchoice update in non-ready state"
                );
            }
        }
        
        // Clean up any payloads that are no longer valid due to forkchoice change
        if let Some(old_head) = old_head {
            if old_head != new_head {
                self.cleanup_orphaned_payloads(old_head, new_head);
            }
        }
    }
    
    /// Clean up payloads that are orphaned due to forkchoice changes
    fn cleanup_orphaned_payloads(&mut self, old_head: Hash256, new_head: Hash256) {
        let orphaned_payloads: Vec<String> = self.state.pending_payloads
            .iter()
            .filter(|(_, payload)| {
                // Payload is orphaned if it was built on the old head but we're now on a new head
                payload.parent_hash == old_head && old_head != new_head
            })
            .map(|(id, _)| id.clone())
            .collect();
        
        if !orphaned_payloads.is_empty() {
            warn!(
                old_head = %old_head,
                new_head = %new_head,
                orphaned_count = %orphaned_payloads.len(),
                "Cleaning up orphaned payloads due to forkchoice change"
            );
            
            for payload_id in orphaned_payloads {
                self.state.remove_pending_payload(&payload_id);
            }
            
            self.metrics.orphaned_payloads_cleaned += orphaned_payloads.len() as u64;
        }
    }
    
    /// Internal helper to validate forkchoice parameters
    pub(super) fn validate_forkchoice_params(
        &self,
        head: Hash256,
        safe: Hash256,
        finalized: Hash256,
    ) -> EngineResult<()> {
        // Basic validation: finalized <= safe <= head (in terms of block height)
        // Note: In practice, we'd need to query the actual block heights
        
        // For now, just ensure hashes are not zero (except for genesis)
        if head == Hash256::zero() {
            return Err(EngineError::ForkchoiceError(
                "Head block hash cannot be zero".to_string()
            ));
        }
        
        // Additional validations can be added here:
        // - Check that blocks exist in the execution client
        // - Validate the chain relationship between blocks
        // - Ensure blocks are on the canonical chain
        
        Ok(())
    }
}