//! Genesis block creation for V2 consensus layer
//!
//! This module handles creating the genesis block (height 0) by querying
//! the execution layer for block #0 and wrapping it in a ConsensusBlock.
//!
//! The genesis block serves as the common foundation that all validator nodes
//! share, ensuring consensus starts from the same state.

use crate::actors_v2::chain::ChainError;
use crate::actors_v2::engine::EngineActor;
use crate::block::SignedConsensusBlock;
use crate::spec::ChainSpec;
use actix::Addr;
use lighthouse_wrapper::types::MainnetEthSpec;
use tracing::{debug, info};

/// Create genesis block by querying execution layer for block #0
///
/// This function:
/// 1. Queries the execution layer (Reth/Geth) for block #0
/// 2. Wraps the execution payload in a ConsensusBlock structure
/// 3. Returns a genesis block ready for storage
///
/// # Genesis Block Properties
/// - Height: 0
/// - Slot: 0
/// - Parent hash: 0x0000...0000 (genesis has no parent)
/// - Execution payload: Retrieved from execution layer block #0
/// - Signature: Empty (genesis is not signed by any authority)
///
/// # Determinism
/// All nodes using the same genesis.json will produce identical genesis blocks
/// because the execution layer's block #0 is deterministically generated from
/// the genesis.json configuration.
///
/// # Arguments
/// * `engine_actor` - Address of the EngineActor for querying execution layer
/// * `chain_spec` - Chain specification (authorities, slot duration, etc.)
///
/// # Returns
/// - `Ok(SignedConsensusBlock)` - Genesis block ready for storage
/// - `Err(ChainError)` - If execution layer query fails
///
/// # Errors
/// Returns `ChainError::Engine` if:
/// - Cannot communicate with EngineActor
/// - Execution layer doesn't have block #0
/// - Execution payload is invalid
///
pub async fn create_genesis_block(
    engine_actor: &Addr<EngineActor>,
    chain_spec: ChainSpec,
) -> Result<SignedConsensusBlock<MainnetEthSpec>, ChainError> {
    info!("Creating genesis block from execution layer");

    // Query execution layer for block #0
    // We use the GetPayloadByTag message which accepts "0x0" or "earliest"
    let get_genesis_msg = crate::actors_v2::engine::messages::EngineMessage::GetPayloadByTag {
        block_tag: "0x0".to_string(), // Query block #0 (genesis)
        correlation_id: Some(uuid::Uuid::new_v4()),
    };

    debug!("Querying execution layer for block #0");

    let execution_payload = match engine_actor.send(get_genesis_msg).await {
        Ok(Ok(crate::actors_v2::engine::messages::EngineResponse::PayloadByTag { payload })) => {
            // Extract the Capella payload
            match payload {
                lighthouse_wrapper::types::ExecutionPayload::Capella(capella_payload) => {
                    info!(
                        block_number = capella_payload.block_number,
                        block_hash = %capella_payload.block_hash,
                        "Retrieved execution layer block #0"
                    );
                    capella_payload
                }
                _ => {
                    return Err(ChainError::Engine(
                        "Expected Capella execution payload for genesis".to_string(),
                    ));
                }
            }
        }
        Ok(Ok(_)) => {
            return Err(ChainError::Engine(
                "Unexpected response type from EngineActor".to_string(),
            ));
        }
        Ok(Err(e)) => {
            return Err(ChainError::Engine(format!(
                "Execution layer failed to provide block #0: {}",
                e
            )));
        }
        Err(e) => {
            return Err(ChainError::NetworkError(format!(
                "Failed to communicate with EngineActor: {}",
                e
            )));
        }
    };

    // Validate that we actually got block #0
    if execution_payload.block_number != 0 {
        return Err(ChainError::InvalidBlock(format!(
            "Expected block #0 from execution layer, got block #{}",
            execution_payload.block_number
        )));
    }

    // Wrap execution payload in a ConsensusBlock
    let genesis = SignedConsensusBlock::genesis(chain_spec, execution_payload);

    let genesis_hash = genesis.canonical_root();
    let genesis_exec_hash = genesis.message.execution_payload.block_hash;

    info!(
        consensus_hash = %genesis_hash,
        execution_hash = %genesis_exec_hash,
        "Genesis block created successfully"
    );

    Ok(genesis)
}

/// Check if genesis block exists in storage
///
/// Helper function to determine if genesis has already been initialized.
/// Used during ChainActor startup to decide whether to create genesis.
///
/// # Arguments
/// * `storage_actor` - Address of the StorageActor
///
/// # Returns
/// - `Ok(true)` - Genesis block exists in storage
/// - `Ok(false)` - Genesis block does not exist
/// - `Err(ChainError)` - Communication or query error
///
pub async fn genesis_exists(
    storage_actor: &Addr<crate::actors_v2::storage::StorageActor>,
) -> Result<bool, ChainError> {
    let get_genesis_msg = crate::actors_v2::storage::messages::GetBlockByHeightMessage {
        height: 0,
        correlation_id: Some(uuid::Uuid::new_v4()),
    };

    match storage_actor.send(get_genesis_msg).await {
        Ok(Ok(Some(_))) => Ok(true),
        Ok(Ok(None)) => Ok(false),
        Ok(Err(e)) => Err(ChainError::Storage(format!(
            "Failed to query genesis from storage: {}",
            e
        ))),
        Err(e) => Err(ChainError::NetworkError(format!(
            "Failed to communicate with StorageActor: {}",
            e
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aura::Authority;
    use lighthouse_wrapper::bls::Keypair;

    #[test]
    fn test_genesis_has_zero_height() {
        use crate::block::ConsensusBlock;

        let block = ConsensusBlock::default();
        let keypair = Keypair::random();
        let authority = Authority {
            signer: keypair.clone(),
            index: 0,
        };

        // Create a signed block with default values
        let signed_block = block.sign_block(&authority);

        // Default ConsensusBlock should have height 0
        assert_eq!(
            signed_block.message.execution_payload.block_number, 0,
            "Default block should have height 0"
        );
    }

    #[test]
    fn test_genesis_has_zero_parent_hash() {
        use crate::block::ConsensusBlock;
        use ethereum_types::H256;

        let block = ConsensusBlock::default();
        let keypair = Keypair::random();
        let authority = Authority {
            signer: keypair.clone(),
            index: 0,
        };

        let signed_block = block.sign_block(&authority);

        // Genesis parent hash should be zero
        assert_eq!(
            signed_block.message.parent_hash,
            H256::zero(),
            "Genesis block should have zero parent hash"
        );
    }
}
