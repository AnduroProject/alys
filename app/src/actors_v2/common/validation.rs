//! Block validation utilities for V2 actor system
//!
//! Provides parent hash relationship verification for Tendermint consensus.
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
/// 1. Parent block exists in storage
/// 2. Block height is exactly parent.height + 1
/// 3. Block's parent_hash matches the canonical hash of parent block
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
            "Block #1 detected - validating genesis parent relationship"
        );

        // Fetch parent block from storage
        let get_block_msg = crate::actors_v2::storage::messages::GetBlockMessage {
            block_hash: parent_hash,
            correlation_id: Some(uuid::Uuid::new_v4()),
        };

        let parent_block = match storage_actor.send(get_block_msg).await {
            Ok(Ok(Some(parent))) => parent,
            Ok(Ok(None)) => {
                // Block #1's parent (genesis) not found - this is an orphan
                tracing::debug!(
                    parent_hash = %parent_hash,
                    block_height = block_height,
                    "Block #1 parent (genesis) not found - block is orphan"
                );
                return Err(ChainError::OrphanBlock {
                    parent_hash: ethereum_types::H256::from_slice(parent_hash.as_bytes()),
                    block_height,
                });
            }
            Ok(Err(e)) => {
                return Err(ChainError::Storage(format!(
                    "Failed to fetch parent block {}: {}",
                    parent_hash, e
                )));
            }
            Err(e) => {
                return Err(ChainError::NetworkError(format!(
                    "Communication error with StorageActor while fetching parent: {}",
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

        // Validate parent hash matches
        let calculated_parent_hash = parent_block.canonical_root();
        if calculated_parent_hash != parent_hash {
            return Err(ChainError::InvalidBlock(format!(
                "Block #1 parent hash mismatch: claimed {} but actual genesis hash is {}",
                parent_hash, calculated_parent_hash
            )));
        }

        tracing::debug!(
            parent_hash = %calculated_parent_hash,
            "Block #1 genesis parent relationship validated successfully"
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

    tracing::debug!(
        block_height = block_height,
        parent_hash = %parent_hash,
        "Validating parent relationship"
    );

    // Fetch parent block from storage
    let get_block_msg = crate::actors_v2::storage::messages::GetBlockMessage {
        block_hash: parent_hash,
        correlation_id: Some(uuid::Uuid::new_v4()),
    };

    let parent_block = match storage_actor.send(get_block_msg).await {
        Ok(Ok(Some(parent))) => parent,
        Ok(Ok(None)) => {
            // Parent not found - this is an orphan block
            // Return specific error type so caller can cache it
            tracing::debug!(
                parent_hash = %parent_hash,
                block_height = block_height,
                "Parent block not found - block is orphan"
            );
            return Err(ChainError::OrphanBlock {
                parent_hash: ethereum_types::H256::from_slice(parent_hash.as_bytes()),
                block_height,
            });
        }
        Ok(Err(e)) => {
            return Err(ChainError::Storage(format!(
                "Failed to fetch parent block {}: {}",
                parent_hash, e
            )));
        }
        Err(e) => {
            return Err(ChainError::NetworkError(format!(
                "Communication error with StorageActor while fetching parent: {}",
                e
            )));
        }
    };

    // Validate height relationship
    let parent_height = parent_block.message.execution_payload.block_number;

    if block_height != parent_height + 1 {
        return Err(ChainError::InvalidBlock(format!(
            "Invalid height relationship: block is {} but parent is {} (expected {})",
            block_height,
            parent_height,
            parent_height + 1
        )));
    }

    // Validate parent hash matches
    // Calculate the canonical hash of the parent block
    let calculated_parent_hash = parent_block.canonical_root();

    if calculated_parent_hash != parent_hash {
        return Err(ChainError::InvalidBlock(format!(
            "Parent hash mismatch: block.parent_hash is {} but actual parent hash is {}",
            parent_hash, calculated_parent_hash
        )));
    }

    tracing::debug!(
        block_height = block_height,
        parent_height = parent_height,
        parent_hash = %calculated_parent_hash,
        "Parent relationship validated successfully"
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
