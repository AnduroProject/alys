//! Block validation utilities for V2 actor system
//!
//! Provides parent hash relationship verification for Tendermint consensus.
//!
//! ## Hash Type Note (TM-B1 Fix)
//!
//! Alys uses two different hash types:
//! - **Consensus hash** (`canonical_root()`): Merkle root of the full consensus block, used for storage indexing
//! - **Execution hash** (`execution_payload.block_hash`): EVM block hash from Reth, used in parent references
//!
//! The `ConsensusBlock.parent_hash` field contains the **execution hash** of the parent block
//! (set in tendermint_handlers.rs during block assembly). Since storage indexes blocks by
//! **consensus hash**, we cannot look up parents by their execution hash directly.
//!
//! Solution: Look up parent by **height** (block_height - 1), then verify the execution hash matches.
//!
//! ## Consensus Note
//!
//! With Tendermint-only consensus, block finality is proven via the `last_commit`
//! field containing 2/3+ validator precommit signatures. Aura (PoA) signature
//! verification is no longer needed.

use crate::actors_v2::chain::ChainError;
use crate::block::SignedConsensusBlock;
use actix::Addr;
use lighthouse_wrapper::types::MainnetEthSpec;

/// Validate block's parent hash and height relationship (Phase 3)
///
/// This function verifies:
/// 1. Parent block exists in storage (looked up by height, not hash - see TM-B1)
/// 2. Block height is exactly parent.height + 1
/// 3. Block's parent_hash matches the **execution hash** of the parent block
///
/// # Genesis Handling
/// Genesis blocks (height 0) skip validation as they have no parent.
///
/// # Errors
/// Returns `ChainError::InvalidBlock` if:
/// - Parent block is missing from storage
/// - Height relationship is incorrect (not parent.height + 1)
/// - Parent hash doesn't match expected value
///
pub async fn validate_parent_relationship(
    block: &SignedConsensusBlock<MainnetEthSpec>,
    storage_actor: &Addr<crate::actors_v2::storage::StorageActor>,
) -> Result<(), ChainError> {
    let block_height = block.message.execution_payload.block_number;
    let parent_hash = block.message.parent_hash;

    // Genesis block has no parent
    if block_height == 0 {
        tracing::debug!("Genesis block, skipping parent validation");
        return Ok(());
    }

    // Special handling for block #1 - it MUST reference genesis (height 0)
    // TM-B1 FIX: Use height-based lookup for genesis too
    if block_height == 1 {
        // Block #1 should NOT have zero parent hash - it must reference genesis
        if parent_hash.is_zero() {
            return Err(ChainError::InvalidBlock(
                "Block #1 must reference genesis block, not zero hash".to_string(),
            ));
        }

        tracing::debug!(
            block_height = block_height,
            parent_hash = %parent_hash,
            "Block #1 detected - validating genesis parent via height lookup (TM-B1 fix)"
        );

        // TM-B1 FIX: Fetch genesis by HEIGHT (0), not by hash
        let get_block_msg = crate::actors_v2::storage::messages::GetBlockByHeightMessage {
            height: 0,
            correlation_id: Some(uuid::Uuid::new_v4()),
        };

        let parent_block = match storage_actor.send(get_block_msg).await {
            Ok(Ok(Some(parent))) => parent,
            Ok(Ok(None)) => {
                // Genesis not found - this is an orphan
                tracing::debug!(
                    parent_hash = %parent_hash,
                    block_height = block_height,
                    "Block #1 parent (genesis at height 0) not found - block is orphan (TM-B1)"
                );
                return Err(ChainError::OrphanBlock {
                    parent_hash: ethereum_types::H256::from_slice(parent_hash.as_bytes()),
                    block_height,
                });
            }
            Ok(Err(e)) => {
                return Err(ChainError::Storage(format!(
                    "Failed to fetch genesis block: {}",
                    e
                )));
            }
            Err(e) => {
                return Err(ChainError::NetworkError(format!(
                    "Communication error with StorageActor while fetching genesis: {}",
                    e
                )));
            }
        };

        // Verify parent is actually genesis (height 0)
        let parent_height = parent_block.message.execution_payload.block_number;
        if parent_height != 0 {
            return Err(ChainError::InvalidBlock(format!(
                "Block #1 parent is not genesis - parent height is {} (expected 0)",
                parent_height
            )));
        }

        // TM-B1 FIX: Validate using EXECUTION hash
        // The parent_hash field contains the execution hash, not consensus hash
        let genesis_execution_hash = parent_block.message.execution_payload.block_hash.into_root();
        if genesis_execution_hash != parent_hash {
            let genesis_consensus_hash = parent_block.canonical_root();
            tracing::warn!(
                claimed_parent_hash = %parent_hash,
                genesis_execution_hash = %genesis_execution_hash,
                genesis_consensus_hash = %genesis_consensus_hash,
                "Genesis hash mismatch (TM-B1 debug info)"
            );
            return Err(ChainError::InvalidBlock(format!(
                "Block #1 parent hash mismatch: claimed {} but genesis execution hash is {}",
                parent_hash, genesis_execution_hash
            )));
        }

        tracing::debug!(
            genesis_execution_hash = %genesis_execution_hash,
            "Block #1 genesis parent relationship validated successfully (TM-B1)"
        );

        return Ok(());
    }

    // For blocks height >= 2, parent hash should not be zero
    if parent_hash.is_zero() {
        return Err(ChainError::InvalidBlock(format!(
            "Block #{} has zero parent hash - only genesis can have zero parent",
            block_height
        )));
    }

    // TM-B1 FIX: Look up parent by HEIGHT, not by hash
    //
    // ConsensusBlock.parent_hash contains the EXECUTION hash of the parent,
    // but storage indexes blocks by CONSENSUS hash. These are different hashes!
    // So we look up by height (parent = block_height - 1), then verify the
    // execution hash matches.
    let parent_height = block_height - 1;

    tracing::debug!(
        block_height = block_height,
        parent_height = parent_height,
        claimed_parent_hash = %parent_hash,
        "Validating parent relationship via height lookup (TM-B1 fix)"
    );

    // Fetch parent block by HEIGHT (not by hash - see TM-B1 fix note above)
    let get_block_msg = crate::actors_v2::storage::messages::GetBlockByHeightMessage {
        height: parent_height,
        correlation_id: Some(uuid::Uuid::new_v4()),
    };

    let parent_block = match storage_actor.send(get_block_msg).await {
        Ok(Ok(Some(parent))) => parent,
        Ok(Ok(None)) => {
            // Parent not found at expected height - this is an orphan block
            tracing::debug!(
                parent_hash = %parent_hash,
                parent_height = parent_height,
                block_height = block_height,
                "Parent block not found at height {} - block is orphan (TM-B1)",
                parent_height
            );
            return Err(ChainError::OrphanBlock {
                parent_hash: ethereum_types::H256::from_slice(parent_hash.as_bytes()),
                block_height,
            });
        }
        Ok(Err(e)) => {
            return Err(ChainError::Storage(format!(
                "Failed to fetch parent block at height {}: {}",
                parent_height, e
            )));
        }
        Err(e) => {
            return Err(ChainError::NetworkError(format!(
                "Communication error with StorageActor while fetching parent: {}",
                e
            )));
        }
    };

    // Verify parent is at expected height (sanity check)
    let actual_parent_height = parent_block.message.execution_payload.block_number;
    if actual_parent_height != parent_height {
        return Err(ChainError::InvalidBlock(format!(
            "Height mismatch: expected parent at {} but found block at {}",
            parent_height, actual_parent_height
        )));
    }

    // TM-B1: Validate using EXECUTION hash (parent_hash references execution layer)
    // The block.parent_hash field contains execution_payload.parent_hash (execution hash),
    // NOT the consensus hash. So we verify against the parent's execution block hash.
    let parent_execution_hash = parent_block.message.execution_payload.block_hash.into_root();

    if parent_execution_hash != parent_hash {
        // Log both hash types for debugging
        let parent_consensus_hash = parent_block.canonical_root();
        tracing::warn!(
            block_height = block_height,
            claimed_parent_hash = %parent_hash,
            parent_execution_hash = %parent_execution_hash,
            parent_consensus_hash = %parent_consensus_hash,
            "Parent hash mismatch (TM-B1 debug info)"
        );
        return Err(ChainError::InvalidBlock(format!(
            "Parent execution hash mismatch: block claims {} but parent's execution hash is {}",
            parent_hash, parent_execution_hash
        )));
    }

    tracing::debug!(
        block_height = block_height,
        parent_height = parent_height,
        parent_execution_hash = %parent_execution_hash,
        "Parent relationship validated successfully (TM-B1 height-based lookup)"
    );

    Ok(())
}

// Note: Aura-based signature verification functions have been removed.
// With Tendermint-only consensus, block finality is proven via the last_commit
// field containing 2/3+ validator precommit signatures.

#[cfg(test)]
mod tests {
    // Note: Parent validation tests require setting up a full actor system
    // with StorageActor, so they're better suited for integration tests.
    // Aura signature tests have been removed as Aura is no longer used.
}
