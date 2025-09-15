//! PegOut Actor Message Handlers
//! 
//! Message handling implementation for the PegOut actor

use actix::prelude::*;
use tracing::info;
use uuid::Uuid;

use super::actor::PegOutActor;
use crate::actors::bridge::messages::pegout_messages::{PegOutMessage, PegOutResponse, PegOutStatus};
use crate::types::errors::BridgeError;

/// Handler for PegOut messages
impl Handler<PegOutMessage> for PegOutActor {
    type Result = ResponseActFuture<Self, Result<PegOutResponse, BridgeError>>;

    fn handle(&mut self, msg: PegOutMessage, _ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            PegOutMessage::ProcessBurnEvent { burn_tx, destination, amount, requester } => {
                info!("Processing burn event: {:?}", burn_tx);
                
                Box::pin(async move {
                    // TODO: Implement process_burn_event method
                    // For now, return a mock response
                    let pegout_id = format!("pegout_{}", Uuid::new_v4());
                    Ok(PegOutResponse::BurnEventProcessed { pegout_id })
                }.into_actor(self))
            }

            PegOutMessage::ApplySignatures { pegout_id, witnesses: _, signature_set } => {
                info!("Applying signatures for pegout: {}", pegout_id);

                Box::pin(async move {
                    // TODO: Implement apply_signatures method
                    // For now, return a mock response
                    Ok(PegOutResponse::SignaturesApplied {
                        pegout_id,
                        ready_to_broadcast: true
                    })
                }.into_actor(self))
            }

            PegOutMessage::GetPegOutStatus { pegout_id } => {
                // TODO: Implement access to pending_pegouts field when it becomes public
                // For now, return a mock status
                Box::pin(async move {
                    Ok(PegOutResponse::PegOutStatus(PegOutStatus::BurnDetected))
                }.into_actor(self))
            }

            PegOutMessage::ListPendingPegOuts => {
                // TODO: Implement access to pending_pegouts field when it becomes public
                // For now, return empty list
                Box::pin(async move {
                    Ok(PegOutResponse::PendingPegOuts(Vec::new()))
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