//! Bridge Workflows
//! 
//! End-to-end workflow implementations

use actix::prelude::*;
use tracing::{info, error};
use uuid::Uuid;

use crate::actors::bridge::{
    messages::*,
    actors::{bridge::BridgeActor, pegin::PegInActor, pegout::PegOutActor},
};

/// Workflow coordinator for bridge operations
pub struct WorkflowCoordinator {
    bridge_actor: Addr<BridgeActor>,
    pegin_actor: Addr<PegInActor>,
    pegout_actor: Addr<PegOutActor>,
}

impl WorkflowCoordinator {
    pub fn new(
        bridge_actor: Addr<BridgeActor>,
        pegin_actor: Addr<PegInActor>,
        pegout_actor: Addr<PegOutActor>,
    ) -> Self {
        Self {
            bridge_actor,
            pegin_actor,
            pegout_actor,
        }
    }

    /// Execute complete peg-in workflow
    pub async fn execute_pegin_workflow(
        &self,
        bitcoin_txid: bitcoin::Txid,
    ) -> Result<String, WorkflowError> {
        let pegin_id = format!("pegin_{}", Uuid::new_v4());
        info!("Starting peg-in workflow: {} for txid {}", pegin_id, bitcoin_txid);

        // Step 1: Coordinate with bridge
        let coordination_msg = BridgeCoordinationMessage::CoordinatePegIn {
            pegin_id: pegin_id.clone(),
            bitcoin_txid,
        };

        self.bridge_actor.send(coordination_msg).await
            .map_err(|e| WorkflowError::CoordinationFailed(e.to_string()))?
            .map_err(|e| WorkflowError::CoordinationFailed(format!("{:?}", e)))?;

        info!("Peg-in workflow {} initiated successfully", pegin_id);
        Ok(pegin_id)
    }

    /// Execute complete peg-out workflow
    pub async fn execute_pegout_workflow(
        &self,
        burn_tx_hash: ethereum_types::H256,
        destination: bitcoin::Address,
        amount: u64,
    ) -> Result<String, WorkflowError> {
        let pegout_id = format!("pegout_{}", Uuid::new_v4());
        info!("Starting peg-out workflow: {} for burn tx {:?}", pegout_id, burn_tx_hash);

        // Step 1: Coordinate with bridge
        let coordination_msg = BridgeCoordinationMessage::CoordinatePegOut {
            pegout_id: pegout_id.clone(),
            burn_tx_hash,
        };

        self.bridge_actor.send(coordination_msg).await
            .map_err(|e| WorkflowError::CoordinationFailed(e.to_string()))?
            .map_err(|e| WorkflowError::CoordinationFailed(format!("{:?}", e)))?;

        info!("Peg-out workflow {} initiated successfully", pegout_id);
        Ok(pegout_id)
    }
}

/// Workflow errors
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("Coordination failed: {0}")]
    CoordinationFailed(String),
    
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Timeout: {0}")]
    Timeout(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}