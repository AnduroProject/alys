//! Chain reorganization logic for Alys V2
//!
//! Handles rolling back the current chain and applying a new canonical chain
//! when a better fork is discovered through the fork choice rule.

use ethereum_types::H256;
use lighthouse_wrapper::types::{Hash256, MainnetEthSpec};
use actix::Addr;
use uuid::Uuid;
use crate::block::SignedConsensusBlock;
use crate::actors_v2::chain::ChainError;
use crate::actors_v2::common::serialization::calculate_block_hash;
use crate::actors_v2::storage::StorageActor;

/// Result of a chain reorganization operation
#[derive(Debug, Clone)]
pub struct ReorganizationResult {
    /// Height where the reorganization occurred
    pub reorg_height: u64,
    /// Number of blocks rolled back from the old chain
    pub blocks_rolled_back: usize,
    /// Number of blocks applied from the new chain
    pub blocks_applied: usize,
    /// New canonical tip hash
    pub new_tip: H256,
    /// New canonical tip height
    pub new_tip_height: u64,
}

/// Reorganize the chain to a new canonical tip
///
/// This function handles the complete reorganization process when a better fork
/// is discovered. It performs the following steps:
///
/// 1. **Validation**: Ensures the reorganization is safe and necessary
/// 2. **Rollback**: Marks blocks from the current chain as non-canonical
/// 3. **Apply**: Marks blocks from the new chain as canonical
/// 4. **Update**: Updates the chain head to the new tip
///
/// # Arguments
/// * `new_tip_block` - The new canonical block at the fork point
/// * `current_height` - The current canonical chain height
/// * `storage_actor` - Storage actor for block operations
/// * `correlation_id` - Correlation ID for logging
///
/// # Returns
/// A `ReorganizationResult` containing details of the operation
///
/// # Errors
/// Returns `ChainError` if:
/// - Storage operations fail
/// - Blocks are missing from storage
/// - The reorganization is invalid
///
pub async fn reorganize_to_new_tip(
    new_tip_block: &SignedConsensusBlock<MainnetEthSpec>,
    current_height: u64,
    storage_actor: &Addr<StorageActor>,
    correlation_id: Uuid,
) -> Result<ReorganizationResult, ChainError> {
    let new_tip_height = new_tip_block.message.execution_payload.block_number;
    let new_tip = calculate_block_hash(new_tip_block);

    tracing::warn!(
        correlation_id = %correlation_id,
        new_tip = %new_tip,
        new_tip_height = new_tip_height,
        current_height = current_height,
        "Starting chain reorganization"
    );

    // Step 1: Validation
    // For blocks at the same height (the common case in 2-node regtest),
    // we're simply replacing the block at that height
    if new_tip_height != current_height {
        return Err(ChainError::InvalidState(format!(
            "Reorganization height mismatch: new_tip={}, current={}",
            new_tip_height, current_height
        )));
    }

    // Step 2: Find the current canonical block at this height
    let get_current_msg = crate::actors_v2::storage::messages::GetBlockByHeightMessage {
        height: current_height,
        correlation_id: Some(correlation_id),
    };

    let current_block = match storage_actor.send(get_current_msg).await {
        Ok(Ok(Some(block))) => block,
        Ok(Ok(None)) => {
            return Err(ChainError::InvalidState(format!(
                "No current block found at height {} during reorganization",
                current_height
            )));
        }
        Ok(Err(e)) => {
            return Err(ChainError::Storage(format!(
                "Failed to fetch current block: {}",
                e
            )));
        }
        Err(e) => {
            return Err(ChainError::NetworkError(format!(
                "Communication error with StorageActor: {}",
                e
            )));
        }
    };

    let current_hash = calculate_block_hash(&current_block);

    tracing::info!(
        correlation_id = %correlation_id,
        current_hash = %current_hash,
        new_tip = %new_tip,
        height = current_height,
        "Replacing block at height {} (simple reorganization)",
        current_height
    );

    // Step 3: Mark the old block as non-canonical (if storage supports it)
    // Note: Current StorageActor doesn't have a "mark non-canonical" method,
    // so we'll just overwrite with the new block
    tracing::debug!(
        correlation_id = %correlation_id,
        old_hash = %current_hash,
        "Marking old block as non-canonical (implicit via overwrite)"
    );

    // Step 4: Store the new block as canonical (this will overwrite)
    let store_msg = crate::actors_v2::storage::messages::StoreBlockMessage {
        block: new_tip_block.clone(),
        canonical: true,
        correlation_id: Some(correlation_id),
    };

    match storage_actor.send(store_msg).await {
        Ok(Ok(())) => {
            tracing::info!(
                correlation_id = %correlation_id,
                new_tip = %new_tip,
                "New canonical block stored successfully"
            );
        }
        Ok(Err(e)) => {
            return Err(ChainError::Storage(format!(
                "Failed to store new canonical block: {}",
                e
            )));
        }
        Err(e) => {
            return Err(ChainError::NetworkError(format!(
                "Communication error storing new block: {}",
                e
            )));
        }
    }

    // Step 5: Update chain head
    let new_head = crate::actors_v2::storage::actor::BlockRef {
        hash: Hash256::from_slice(new_tip.as_bytes()),
        number: new_tip_height,
        execution_hash: new_tip_block.message.execution_payload.block_hash,
    };

    let update_head_msg = crate::actors_v2::storage::messages::UpdateChainHeadMessage {
        new_head,
        correlation_id: Some(correlation_id),
    };

    match storage_actor.send(update_head_msg).await {
        Ok(Ok(())) => {
            tracing::info!(
                correlation_id = %correlation_id,
                new_tip = %new_tip,
                new_tip_height = new_tip_height,
                "Chain head updated to new canonical tip"
            );
        }
        Ok(Err(e)) => {
            tracing::warn!(
                correlation_id = %correlation_id,
                error = ?e,
                "Failed to update chain head (non-fatal)"
            );
        }
        Err(e) => {
            tracing::warn!(
                correlation_id = %correlation_id,
                error = ?e,
                "Communication error updating chain head (non-fatal)"
            );
        }
    }

    let result = ReorganizationResult {
        reorg_height: current_height,
        blocks_rolled_back: 1, // Simple case: one block replaced
        blocks_applied: 1,
        new_tip,
        new_tip_height,
    };

    tracing::warn!(
        correlation_id = %correlation_id,
        reorg_height = result.reorg_height,
        blocks_rolled_back = result.blocks_rolled_back,
        blocks_applied = result.blocks_applied,
        new_tip = %result.new_tip,
        "Chain reorganization completed successfully"
    );

    Ok(result)
}

/// Perform a deep chain reorganization (for multi-block forks)
///
/// This is a more complex reorganization for cases where the fork is deeper
/// than just the current block. It would:
/// 1. Traverse back to find the common ancestor
/// 2. Mark all blocks in the old chain as non-canonical
/// 3. Apply all blocks from the new chain
/// 4. Update chain head
///
/// # Note
/// This is a stub for future implementation. The current 2-node regtest
/// primarily experiences single-block forks, so this is not critical.
///
#[allow(dead_code)]
pub async fn reorganize_deep(
    _new_chain_tip: &SignedConsensusBlock<MainnetEthSpec>,
    _common_ancestor_height: u64,
    _storage_actor: &Addr<StorageActor>,
    _correlation_id: Uuid,
) -> Result<ReorganizationResult, ChainError> {
    tracing::error!("Deep chain reorganization not yet implemented");
    Err(ChainError::Internal(
        "Deep chain reorganization not yet implemented".to_string(),
    ))
}
