//! Block validation utilities for V2 actor system
//!
//! Provides parent hash relationship verification for Tendermint consensus.
//!
//! ## Hash Type Note (TM-B7 Fix)
//!
//! Alys uses two different hash types:
//! - **Consensus hash** (`canonical_root()`): Merkle root of the full consensus block, used for storage indexing
//! - **Execution hash** (`execution_payload.block_hash`): EVM block hash from Reth
//!
//! With the TM-B7 fix, `ConsensusBlock.parent_hash` now contains the **consensus hash** of the
//! parent block (set correctly in tendermint_handlers.rs during block assembly). This matches
//! how storage indexes blocks, enabling direct hash-based lookups.
//!
//! ## Performance Optimization (QueueFull Fix)
//!
//! During fast sync, the BLOCK_HEIGHTS column family stores `height -> block_hash` directly.
//! Instead of fetching the full parent block (~KB-MB) and computing `canonical_root()`,
//! we now use `GetBlockHashByHeightMessage` to retrieve only the 32-byte hash. This reduces
//! I/O and deserialization overhead significantly during initial sync.
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
/// 1. Parent block exists in storage (looked up by height for robustness)
/// 2. Block height is exactly parent.height + 1
/// 3. Block's parent_hash matches the **consensus hash** (canonical_root) of the parent block
///
/// # Genesis Handling
/// Genesis blocks (height 0) skip validation as they have no parent.
///
/// # Errors
/// Returns `ChainError::InvalidBlock` if:
/// - Parent block is missing from storage
/// - Height relationship is incorrect (not parent.height + 1)
/// - Parent hash doesn't match expected consensus hash
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
            "Block #1 detected - validating genesis parent via hash lookup"
        );

        // OPTIMIZATION: Fetch only the genesis hash, not the full block
        // The BLOCK_HEIGHTS CF stores height -> consensus_hash directly
        let get_hash_msg = crate::actors_v2::storage::messages::GetBlockHashByHeightMessage {
            height: 0,
            correlation_id: Some(uuid::Uuid::new_v4()),
        };

        let genesis_consensus_hash = match storage_actor.send(get_hash_msg).await {
            Ok(Ok(Some(hash))) => hash,
            Ok(Ok(None)) => {
                // Genesis not found - this is an orphan
                tracing::debug!(
                    parent_hash = %parent_hash,
                    block_height = block_height,
                    "Block #1 parent (genesis at height 0) not found - block is orphan"
                );
                return Err(ChainError::OrphanBlock {
                    parent_hash: ethereum_types::H256::from_slice(parent_hash.as_bytes()),
                    block_height,
                });
            }
            Ok(Err(e)) => {
                return Err(ChainError::Storage(format!(
                    "Failed to fetch genesis hash: {}",
                    e
                )));
            }
            Err(e) => {
                return Err(ChainError::NetworkError(format!(
                    "Communication error with StorageActor while fetching genesis hash: {}",
                    e
                )));
            }
        };

        // TM-B7 FIX: Validate using CONSENSUS hash
        // The parent_hash field contains the consensus hash (canonical_root)
        // The height index stores this same hash directly
        if genesis_consensus_hash != parent_hash {
            tracing::warn!(
                claimed_parent_hash = %parent_hash,
                genesis_consensus_hash = %genesis_consensus_hash,
                "Genesis hash mismatch"
            );
            return Err(ChainError::InvalidBlock(format!(
                "Block #1 parent hash mismatch: claimed {} but genesis consensus hash is {}",
                parent_hash, genesis_consensus_hash
            )));
        }

        tracing::debug!(
            genesis_consensus_hash = %genesis_consensus_hash,
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

    // Look up parent by HEIGHT for robustness (handles edge cases with missing blocks)
    // Then verify the consensus hash matches
    let parent_height = block_height - 1;

    tracing::debug!(
        block_height = block_height,
        parent_height = parent_height,
        claimed_parent_hash = %parent_hash,
        "Validating parent relationship via hash lookup"
    );

    // OPTIMIZATION: Fetch only the parent hash, not the full block
    // The BLOCK_HEIGHTS CF stores height -> consensus_hash directly
    // This avoids deserializing KB-MB of block data just to get the hash
    let get_hash_msg = crate::actors_v2::storage::messages::GetBlockHashByHeightMessage {
        height: parent_height,
        correlation_id: Some(uuid::Uuid::new_v4()),
    };

    let parent_consensus_hash = match storage_actor.send(get_hash_msg).await {
        Ok(Ok(Some(hash))) => hash,
        Ok(Ok(None)) => {
            // Parent not found at expected height - this is an orphan block
            tracing::debug!(
                parent_hash = %parent_hash,
                parent_height = parent_height,
                block_height = block_height,
                "Parent hash not found at height {} - block is orphan",
                parent_height
            );
            return Err(ChainError::OrphanBlock {
                parent_hash: ethereum_types::H256::from_slice(parent_hash.as_bytes()),
                block_height,
            });
        }
        Ok(Err(e)) => {
            return Err(ChainError::Storage(format!(
                "Failed to fetch parent hash at height {}: {}",
                parent_height, e
            )));
        }
        Err(e) => {
            return Err(ChainError::NetworkError(format!(
                "Communication error with StorageActor while fetching parent hash: {}",
                e
            )));
        }
    };

    // TM-B7 FIX: Validate using CONSENSUS hash (canonical_root)
    // The block.parent_hash field contains the consensus hash
    // The height index stores this same hash directly
    if parent_consensus_hash != parent_hash {
        tracing::warn!(
            block_height = block_height,
            claimed_parent_hash = %parent_hash,
            parent_consensus_hash = %parent_consensus_hash,
            "Parent hash mismatch"
        );
        return Err(ChainError::InvalidBlock(format!(
            "Parent consensus hash mismatch: block claims {} but parent's consensus hash is {}",
            parent_hash, parent_consensus_hash
        )));
    }

    tracing::debug!(
        block_height = block_height,
        parent_height = parent_height,
        parent_consensus_hash = %parent_consensus_hash,
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
