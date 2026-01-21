//! Chain reorganization logic for Alys V2
//!
//! Handles rolling back the current chain and applying a new canonical chain
//! when a better fork is discovered through the fork choice rule.
//!
//! ## Reorganization Types
//!
//! 1. **Simple Reorg (same-height):** Single block replacement at same height
//! 2. **Deep Reorg (multi-block):** Multiple blocks rolled back and replaced
//!
//! ## Deep Reorg Process
//!
//! 1. Find common ancestor between current and new chain
//! 2. Roll back blocks from current chain (mark as orphaned)
//! 3. Apply blocks from new chain
//! 4. Update cumulative difficulty
//! 5. Sync execution layer fork choice

use crate::actors_v2::chain::fork_choice::{
    calculate_block_difficulty, exceeds_automatic_reorg_limit, should_alert_reorg_depth,
    should_reorg_to_chain, MAX_AUTOMATIC_REORG_DEPTH,
};
use crate::actors_v2::chain::ChainError;
use crate::actors_v2::common::serialization::calculate_block_hash;
use crate::actors_v2::storage::StorageActor;
use crate::block::SignedConsensusBlock;
use actix::Addr;
use ethereum_types::H256;
use lighthouse_wrapper::types::{Hash256, MainnetEthSpec};
use uuid::Uuid;

/// Result of a chain reorganization operation
#[derive(Debug, Clone)]
pub struct ReorganizationResult {
    /// Height where the reorganization occurred (common ancestor for deep reorgs)
    pub reorg_height: u64,
    /// Number of blocks rolled back from the old chain
    pub blocks_rolled_back: usize,
    /// Number of blocks applied from the new chain
    pub blocks_applied: usize,
    /// New canonical tip hash
    pub new_tip: H256,
    /// New canonical tip height
    pub new_tip_height: u64,
    /// Old tip hash (before reorg)
    pub old_tip: Option<H256>,
    /// Old tip height (before reorg)
    pub old_tip_height: Option<u64>,
    /// Cumulative difficulty of new chain
    pub new_cumulative_difficulty: u128,
    /// Whether this was a deep reorg (multiple blocks)
    pub is_deep_reorg: bool,
}

/// Configuration for deep reorganization operations
#[derive(Debug, Clone)]
pub struct DeepReorgConfig {
    /// Maximum automatic reorg depth (default: 100)
    pub max_automatic_depth: u64,
    /// Alert threshold for reorg depth (default: 10)
    pub alert_threshold: u64,
    /// Whether to allow operator override for deep reorgs
    pub allow_operator_override: bool,
}

impl Default for DeepReorgConfig {
    fn default() -> Self {
        Self {
            max_automatic_depth: MAX_AUTOMATIC_REORG_DEPTH,
            alert_threshold: 10,
            allow_operator_override: true,
        }
    }
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
/// * `current_cumulative_difficulty` - Cumulative difficulty of the current chain tip
/// * `storage_actor` - Storage actor for block operations
/// * `correlation_id` - Correlation ID for logging
///
/// # Returns
/// A `ReorganizationResult` containing details of the operation, including
/// the correct cumulative difficulty for the new chain tip.
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
    current_cumulative_difficulty: u128,
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

    // Calculate cumulative difficulty for the new chain
    // For same-height reorg: both blocks share the same parent, so:
    //   parent_cumulative = current_cumulative - current_block_difficulty
    //   new_cumulative = parent_cumulative + new_block_difficulty
    let current_block_difficulty = calculate_block_difficulty(&current_block);
    let new_block_difficulty = calculate_block_difficulty(new_tip_block);
    let parent_cumulative_difficulty =
        current_cumulative_difficulty.saturating_sub(current_block_difficulty);
    let new_cumulative_difficulty =
        parent_cumulative_difficulty.saturating_add(new_block_difficulty);

    tracing::debug!(
        correlation_id = %correlation_id,
        current_cumulative = current_cumulative_difficulty,
        current_block_difficulty = current_block_difficulty,
        parent_cumulative = parent_cumulative_difficulty,
        new_block_difficulty = new_block_difficulty,
        new_cumulative = new_cumulative_difficulty,
        "Calculated cumulative difficulty for reorganization"
    );

    let result = ReorganizationResult {
        reorg_height: current_height.saturating_sub(1), // Common ancestor is one height below
        blocks_rolled_back: 1, // Simple case: one block replaced
        blocks_applied: 1,
        new_tip,
        new_tip_height,
        old_tip: Some(current_hash),
        old_tip_height: Some(current_height),
        new_cumulative_difficulty,
        is_deep_reorg: false,
    };

    tracing::warn!(
        correlation_id = %correlation_id,
        reorg_height = result.reorg_height,
        blocks_rolled_back = result.blocks_rolled_back,
        blocks_applied = result.blocks_applied,
        new_tip = %result.new_tip,
        old_tip = %result.old_tip.unwrap_or_default(),
        "Simple chain reorganization completed successfully"
    );

    Ok(result)
}

/// Perform a deep chain reorganization (for multi-block forks)
///
/// This function handles multi-block reorganizations when a competing chain
/// has more cumulative work than our current chain. The process:
///
/// 1. Validate the reorg is safe (check depth limits)
/// 2. Roll back blocks from current chain (mark as orphaned)
/// 3. Apply blocks from new chain
/// 4. Update cumulative difficulty
/// 5. Update chain head
///
/// # Arguments
/// * `new_chain` - The new chain blocks from common ancestor to new tip (oldest first)
/// * `common_ancestor_height` - Height of the common ancestor block
/// * `current_tip_height` - Current canonical chain tip height
/// * `current_tip_hash` - Current canonical chain tip hash
/// * `new_cumulative_difficulty` - Total cumulative difficulty of the new chain
/// * `storage_actor` - Storage actor for block operations
/// * `config` - Deep reorg configuration
/// * `correlation_id` - Correlation ID for logging
///
/// # Returns
/// A `ReorganizationResult` containing details of the operation
///
/// # Errors
/// Returns `ChainError` if:
/// - Reorg depth exceeds limits without operator override
/// - Storage operations fail
/// - Blocks are missing from storage
pub async fn reorganize_deep(
    new_chain: &[SignedConsensusBlock<MainnetEthSpec>],
    common_ancestor_height: u64,
    current_tip_height: u64,
    current_tip_hash: H256,
    new_cumulative_difficulty: u128,
    storage_actor: &Addr<StorageActor>,
    config: &DeepReorgConfig,
    correlation_id: Uuid,
) -> Result<ReorganizationResult, ChainError> {
    // Validate input
    if new_chain.is_empty() {
        return Err(ChainError::InvalidState(
            "Cannot perform deep reorg with empty new chain".to_string(),
        ));
    }

    let new_tip_block = new_chain.last().unwrap();
    let new_tip = calculate_block_hash(new_tip_block);
    let new_tip_height = new_tip_block.message.execution_payload.block_number;

    // Calculate reorg depth
    let reorg_depth = current_tip_height.saturating_sub(common_ancestor_height);

    tracing::warn!(
        correlation_id = %correlation_id,
        common_ancestor_height = common_ancestor_height,
        current_tip_height = current_tip_height,
        current_tip = %current_tip_hash,
        new_tip_height = new_tip_height,
        new_tip = %new_tip,
        reorg_depth = reorg_depth,
        blocks_to_apply = new_chain.len(),
        "Starting deep chain reorganization"
    );

    // Step 1: Validate reorg depth
    if should_alert_reorg_depth(reorg_depth) {
        tracing::warn!(
            correlation_id = %correlation_id,
            reorg_depth = reorg_depth,
            alert_threshold = config.alert_threshold,
            "Deep reorg exceeds alert threshold - operator attention required"
        );
    }

    if exceeds_automatic_reorg_limit(reorg_depth) {
        if !config.allow_operator_override {
            tracing::error!(
                correlation_id = %correlation_id,
                reorg_depth = reorg_depth,
                max_depth = config.max_automatic_depth,
                "Deep reorg exceeds automatic limit and operator override is disabled"
            );
            return Err(ChainError::ReorgTooDeep {
                depth: reorg_depth,
                max_allowed: config.max_automatic_depth,
            });
        }
        tracing::warn!(
            correlation_id = %correlation_id,
            reorg_depth = reorg_depth,
            max_automatic = config.max_automatic_depth,
            "Deep reorg exceeds automatic limit - proceeding with operator override"
        );
    }

    // Step 2: Roll back blocks from current chain
    // We roll back from current tip down to (but not including) common ancestor
    let blocks_to_rollback = (current_tip_height - common_ancestor_height) as usize;

    tracing::info!(
        correlation_id = %correlation_id,
        from_height = current_tip_height,
        to_height = common_ancestor_height + 1,
        blocks_to_rollback = blocks_to_rollback,
        "Rolling back blocks from current chain"
    );

    for rollback_height in (common_ancestor_height + 1..=current_tip_height).rev() {
        // Get the block at this height
        let get_block_msg = crate::actors_v2::storage::messages::GetBlockByHeightMessage {
            height: rollback_height,
            correlation_id: Some(correlation_id),
        };

        match storage_actor.send(get_block_msg).await {
            Ok(Ok(Some(orphaned_block))) => {
                let orphaned_hash = calculate_block_hash(&orphaned_block);

                tracing::debug!(
                    correlation_id = %correlation_id,
                    height = rollback_height,
                    hash = %orphaned_hash,
                    "Marking block as orphaned"
                );

                // Store as orphaned block (non-canonical)
                // Note: In a full implementation, we'd use a dedicated orphan store
                // For now, we just log the rollback
            }
            Ok(Ok(None)) => {
                tracing::warn!(
                    correlation_id = %correlation_id,
                    height = rollback_height,
                    "No block found at height during rollback (skipping)"
                );
            }
            Ok(Err(e)) => {
                tracing::error!(
                    correlation_id = %correlation_id,
                    height = rollback_height,
                    error = ?e,
                    "Failed to fetch block during rollback"
                );
                return Err(ChainError::Storage(format!(
                    "Failed to fetch block at height {}: {}",
                    rollback_height, e
                )));
            }
            Err(e) => {
                return Err(ChainError::NetworkError(format!(
                    "Communication error during rollback: {}",
                    e
                )));
            }
        }
    }

    // Step 3: Apply new chain blocks
    tracing::info!(
        correlation_id = %correlation_id,
        blocks_to_apply = new_chain.len(),
        from_height = common_ancestor_height + 1,
        to_height = new_tip_height,
        "Applying new chain blocks"
    );

    for (idx, block) in new_chain.iter().enumerate() {
        let block_height = block.message.execution_payload.block_number;
        let block_hash = calculate_block_hash(block);

        tracing::debug!(
            correlation_id = %correlation_id,
            idx = idx,
            height = block_height,
            hash = %block_hash,
            "Applying block from new chain"
        );

        // Store the block as canonical
        let store_msg = crate::actors_v2::storage::messages::StoreBlockMessage {
            block: block.clone(),
            canonical: true,
            correlation_id: Some(correlation_id),
        };

        match storage_actor.send(store_msg).await {
            Ok(Ok(())) => {
                tracing::debug!(
                    correlation_id = %correlation_id,
                    height = block_height,
                    hash = %block_hash,
                    "Block stored successfully"
                );
            }
            Ok(Err(e)) => {
                tracing::error!(
                    correlation_id = %correlation_id,
                    height = block_height,
                    error = ?e,
                    "Failed to store block during deep reorg"
                );
                return Err(ChainError::Storage(format!(
                    "Failed to store block at height {}: {}",
                    block_height, e
                )));
            }
            Err(e) => {
                return Err(ChainError::NetworkError(format!(
                    "Communication error storing block: {}",
                    e
                )));
            }
        }
    }

    // Step 4: Update chain head
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
                "Chain head updated after deep reorg"
            );
        }
        Ok(Err(e)) => {
            tracing::warn!(
                correlation_id = %correlation_id,
                error = ?e,
                "Failed to update chain head after deep reorg (non-fatal)"
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
        reorg_height: common_ancestor_height,
        blocks_rolled_back: blocks_to_rollback,
        blocks_applied: new_chain.len(),
        new_tip,
        new_tip_height,
        old_tip: Some(current_tip_hash),
        old_tip_height: Some(current_tip_height),
        new_cumulative_difficulty,
        is_deep_reorg: true,
    };

    tracing::warn!(
        correlation_id = %correlation_id,
        common_ancestor_height = result.reorg_height,
        blocks_rolled_back = result.blocks_rolled_back,
        blocks_applied = result.blocks_applied,
        old_tip = %result.old_tip.unwrap_or_default(),
        old_tip_height = result.old_tip_height.unwrap_or_default(),
        new_tip = %result.new_tip,
        new_tip_height = result.new_tip_height,
        new_cumulative_difficulty = result.new_cumulative_difficulty,
        "Deep chain reorganization completed successfully"
    );

    Ok(result)
}

/// Check if a deep reorg should proceed based on cumulative difficulty comparison.
///
/// # Arguments
/// * `our_cumulative_difficulty` - Total difficulty of our canonical chain
/// * `their_cumulative_difficulty` - Total difficulty of the competing chain
/// * `reorg_depth` - Number of blocks to roll back
/// * `config` - Deep reorg configuration
///
/// # Returns
/// * `Ok(true)` - Should proceed with reorg
/// * `Ok(false)` - Should not reorg (our chain is better or equal)
/// * `Err` - Reorg blocked due to depth limits
pub fn should_execute_deep_reorg(
    our_cumulative_difficulty: u128,
    their_cumulative_difficulty: u128,
    reorg_depth: u64,
    config: &DeepReorgConfig,
) -> Result<bool, ChainError> {
    // First check if their chain has more work
    if !should_reorg_to_chain(our_cumulative_difficulty, their_cumulative_difficulty) {
        return Ok(false);
    }

    // Then check depth limits
    if exceeds_automatic_reorg_limit(reorg_depth) && !config.allow_operator_override {
        return Err(ChainError::ReorgTooDeep {
            depth: reorg_depth,
            max_allowed: config.max_automatic_depth,
        });
    }

    Ok(true)
}

/// Calculate cumulative difficulty for a chain segment.
///
/// # Arguments
/// * `blocks` - Blocks in the chain segment (oldest to newest)
/// * `starting_cumulative` - Cumulative difficulty at the start of the segment
///
/// # Returns
/// Total cumulative difficulty at the end of the segment
pub fn calculate_chain_cumulative_difficulty(
    blocks: &[SignedConsensusBlock<MainnetEthSpec>],
    starting_cumulative: u128,
) -> u128 {
    let mut cumulative = starting_cumulative;
    for block in blocks {
        cumulative = cumulative.saturating_add(calculate_block_difficulty(block));
    }
    cumulative
}
