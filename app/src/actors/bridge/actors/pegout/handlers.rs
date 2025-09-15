//! PegOut Actor Message Handlers
//! 
//! Message handling implementation for the PegOut actor

use actix::prelude::*;
use tracing::{info, error};

use super::actor::PegOutActor;
use crate::actors::bridge::{messages::*, shared::errors::BridgeError};

/// Handler for PegOut messages
impl Handler<PegOutMessage> for PegOutActor {
    type Result = ResponseActFuture<Self, Result<PegOutResponse, BridgeError>>;

    fn handle(&mut self, msg: PegOutMessage, _ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            PegOutMessage::ProcessBurnEvent { burn_tx, destination, amount, requester } => {
                info!("Processing burn event: {:?}", burn_tx);
                
                Box::pin(async move {
                    match self.process_burn_event(burn_tx, destination, amount, requester).await {
                        Ok(pegout_id) => Ok(PegOutResponse::BurnEventProcessed { pegout_id }),
                        Err(e) => {
                            error!("Failed to process burn event: {:?}", e);
                            self.record_error(e);
                            Err(BridgeError::PegOutError("Failed to process burn event".to_string()))
                        }
                    }
                }.into_actor(self))
            }

            PegOutMessage::ApplySignatures { pegout_id, witnesses: _, signature_set } => {
                info!("Applying signatures for pegout: {}", pegout_id);
                
                Box::pin(async move {
                    match self.apply_signatures(pegout_id.clone(), signature_set).await {
                        Ok(_) => Ok(PegOutResponse::SignaturesApplied { 
                            pegout_id, 
                            ready_to_broadcast: true 
                        }),
                        Err(e) => {
                            error!("Failed to apply signatures: {:?}", e);
                            self.record_error(e);
                            Err(BridgeError::SignatureError("Failed to apply signatures".to_string()))
                        }
                    }
                }.into_actor(self))
            }

            PegOutMessage::GetPegOutStatus { pegout_id } => {
                if let Some(pegout) = self.pending_pegouts.get(&pegout_id) {
                    let status = pegout.status.clone();
                    Box::pin(async move {
                        Ok(PegOutResponse::PegOutStatus(status))
                    }.into_actor(self))
                } else {
                    Box::pin(async move {
                        Err(BridgeError::OperationNotFound(pegout_id))
                    }.into_actor(self))
                }
            }

            PegOutMessage::ListPendingPegOuts => {
                let pending: Vec<PendingPegOut> = self.pending_pegouts.values().cloned().collect();
                Box::pin(async move {
                    Ok(PegOutResponse::PendingPegOuts(pending))
                }.into_actor(self))
            }

            _ => {
                // Handle other message types
                Box::pin(async move {
                    Ok(PegOutResponse::PegOutStatus(PegOutStatus::Failed { 
                        reason: "Message not implemented".to_string(), 
                        recoverable: false 
                    }))
                }.into_actor(self))
            }
        }
    }
}

/// Get PegOut status message
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<PegOutStatus, BridgeError>")]
pub struct GetPegOutStatus;