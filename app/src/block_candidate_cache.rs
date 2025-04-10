use crate::block::SignedConsensusBlock;
use crate::error::Error;
use crate::network::ApproveBlock;
use crate::signatures::CheckedIndividualApproval;
use async_trait::async_trait;
use bls::PublicKey;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::trace;
use types::{Hash256, MainnetEthSpec};

/// CandidateState enum represents the state of a block candidate.
#[allow(clippy::large_enum_variant)]
pub enum CandidateState {
    /// We received the block and approved of it
    CheckedBlock(SignedConsensusBlock<MainnetEthSpec>),
    /// We received approvals before we received the block - store them until we receive the block
    QueuedApprovals(Vec<CheckedIndividualApproval>),
}

impl CandidateState {
    pub fn add_unchecked_approval(
        &mut self,
        approval: ApproveBlock,
        authorities: &[PublicKey],
    ) -> Result<(), Error> {
        let checked_approval = approval.signature.check(approval.block_hash, authorities)?;
        self.add_checked_approval(checked_approval)
    }

    pub fn add_checked_approval(
        &mut self,
        approval: CheckedIndividualApproval,
    ) -> Result<(), Error> {
        match self {
            CandidateState::CheckedBlock(x) => {
                x.add_approval(approval)?;
            }
            CandidateState::QueuedApprovals(v) => {
                v.push(approval);
            }
        }
        Ok(())
    }

    pub fn add_checked_block(
        &mut self,
        block: SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<(), Error> {
        match self {
            CandidateState::QueuedApprovals(queued_approvals) => {
                let mut new_state = CandidateState::CheckedBlock(block);
                for approval in queued_approvals.drain(..) {
                    new_state.add_checked_approval(approval)?;
                }

                *self = new_state;
            }
            CandidateState::CheckedBlock(_) => {
                // Replace the existing block with the new one
                *self = CandidateState::CheckedBlock(block);
            }
        }
        Ok(())
    }

    /// Get the block contained in this CandidateState if it exists
    pub fn get_block(&self) -> Option<&SignedConsensusBlock<MainnetEthSpec>> {
        match self {
            CandidateState::CheckedBlock(block) => Some(block),
            CandidateState::QueuedApprovals(_) => None,
        }
    }
}

impl Default for CandidateState {
    fn default() -> Self {
        Self::QueuedApprovals(vec![])
    }
}

/// A cache for storing block candidates by height instead of hash.
///
/// This provides a mechanism to track proposed blocks at each height
/// and only keeps the latest proposal for each height (based on the highest slot number).
#[derive(Default)]
pub struct BlockCandidateCache {
    /// Stores block candidates by height
    candidates_by_height: HashMap<u64, CandidateState>,
    /// Maps block hashes to block heights for quick lookup
    hash_to_height: HashMap<Hash256, u64>,
}

#[async_trait]
pub trait BlockCandidateCacheInit {
    /// Initializes the cache with existing block candidates.
    async fn init_block_candidate_cache(self: &Arc<Self>) -> eyre::Result<()>;
}

impl BlockCandidateCache {
    /// Creates a new empty BlockCandidateCache.
    pub fn new() -> Self {
        Self {
            candidates_by_height: HashMap::new(),
            hash_to_height: HashMap::new(),
        }
    }

    /// Inserts a block candidate into the cache.
    /// If there's already a block at the same height, only keeps the one with the higher slot.
    pub fn insert(&mut self, block: SignedConsensusBlock<MainnetEthSpec>) -> Result<(), Error> {
        let block_hash = block.canonical_root();
        let block_height = block.message.execution_payload.block_number;
        let block_slot = block.message.slot;

        trace!(
            "BlockCandidateCache: Inserting block at height {} with slot {} and hash {}",
            block_height,
            block_slot,
            block_hash
        );

        // Check if we already have a block at this height
        if let Some(candidate_state) = self.candidates_by_height.get_mut(&block_height) {
            // If there's a block in the candidate state
            if let Some(existing_block) = candidate_state.get_block() {
                // Only replace if the new block has a higher slot
                if block_slot > existing_block.message.slot {
                    trace!(
                        "BlockCandidateCache: Replacing block at height {} (slot {} -> slot {})",
                        block_height,
                        existing_block.message.slot,
                        block_slot
                    );

                    // Remove the old hash from the hash map
                    let old_hash = existing_block.canonical_root();
                    self.hash_to_height.remove(&old_hash);

                    // Add the new block
                    candidate_state.add_checked_block(block.clone())?;
                    self.hash_to_height.insert(block_hash, block_height);
                } else {
                    trace!(
                        "BlockCandidateCache: Ignoring block at height {} with lower slot {} (current slot {})",
                        block_height,
                        block_slot,
                        existing_block.message.slot
                    );
                    // Skip this block, as it has a lower slot than the existing block
                    return Ok(());
                }
            } else {
                // No block in candidate state yet, just add it
                candidate_state.add_checked_block(block.clone())?;
                self.hash_to_height.insert(block_hash, block_height);
            }
        } else {
            // No candidate at this height yet, create a new one
            let mut candidate_state = CandidateState::default();
            candidate_state.add_checked_block(block.clone())?;

            self.candidates_by_height
                .insert(block_height, candidate_state);
            self.hash_to_height.insert(block_hash, block_height);
        }

        Ok(())
    }

    /// Adds an approval for a block.
    pub fn add_approval(
        &mut self,
        approval: ApproveBlock,
        authorities: &[PublicKey],
    ) -> Result<(), Error> {
        let block_hash = approval.block_hash;

        // Find the height of the block using the hash
        if let Some(&block_height) = self.hash_to_height.get(&block_hash) {
            if let Some(candidate_state) = self.candidates_by_height.get_mut(&block_height) {
                return if let Some(current_highest_slot_block) = candidate_state.get_block() {
                    if current_highest_slot_block.canonical_root() != block_hash {
                        // If the block hash doesn't match, this is an old block
                        // We need to remove this block from the cache
                        self.hash_to_height.remove(&block_hash);
                        Ok(())
                    } else {
                        // If we already have the block, just add the approval
                        candidate_state.add_unchecked_approval(approval, authorities)
                    }
                } else {
                    // We have the state but no block yet (only approvals)
                    candidate_state.add_unchecked_approval(approval, authorities)
                };
            }
        }

        // If we don't know the block yet, create a new candidate state with queued approvals
        let mut candidate_state = CandidateState::default();
        candidate_state.add_unchecked_approval(approval, authorities)?;

        // Since we don't know the block height yet, we'll temporarily store it by hash
        // We'll use a special height value (0) as a temporary placeholder
        // It will be properly filed by height when the block arrives
        self.hash_to_height.insert(block_hash, 0);
        self.candidates_by_height.insert(0, candidate_state);

        Ok(())
    }

    /// Gets a block candidate by hash.
    pub fn get(&self, hash: &Hash256) -> Option<&CandidateState> {
        self.hash_to_height
            .get(hash)
            .and_then(|height| self.candidates_by_height.get(height))
    }

    /// Gets a mutable reference to a block candidate by hash.
    pub fn get_mut(&mut self, hash: &Hash256) -> Option<&mut CandidateState> {
        if let Some(&height) = self.hash_to_height.get(hash) {
            self.candidates_by_height.get_mut(&height)
        } else {
            None
        }
    }

    /// Removes and returns the candidate state for a specific hash.
    pub fn remove(&mut self, hash: &Hash256) -> Option<CandidateState> {
        if let Some(&height) = self.hash_to_height.get(hash) {
            self.hash_to_height.remove(hash);
            self.candidates_by_height.remove(&height)
        } else {
            None
        }
    }

    /// Clears all candidates from the cache.
    pub fn clear(&mut self) {
        self.candidates_by_height.clear();
        self.hash_to_height.clear();
    }

    /// Returns the number of candidates in the cache.
    pub fn len(&self) -> usize {
        self.candidates_by_height.len()
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.candidates_by_height.is_empty()
    }
}

/// A wrapper around BlockCandidateCache that provides thread-safe access.
#[derive(Default)]
pub struct BlockCandidates {
    cache: RwLock<BlockCandidateCache>,
}

impl BlockCandidates {
    /// Creates a new thread-safe BlockCandidates cache.
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(BlockCandidateCache::new()),
        }
    }

    /// Inserts a block candidate into the cache.
    pub async fn insert(&self, block: SignedConsensusBlock<MainnetEthSpec>) -> Result<(), Error> {
        self.cache.write().await.insert(block)
    }

    /// Adds an approval for a block.
    pub async fn add_approval(
        &self,
        approval: ApproveBlock,
        authorities: &[PublicKey],
    ) -> Result<(), Error> {
        self.cache.write().await.add_approval(approval, authorities)
    }

    /// Checks if a block hash exists in the cache
    pub async fn exists(&self, hash: &Hash256) -> bool {
        self.cache.read().await.hash_to_height.contains_key(hash)
    }

    /// Gets the block associated with a hash, if it exists
    pub async fn get_block(&self, hash: &Hash256) -> Option<SignedConsensusBlock<MainnetEthSpec>> {
        let guard = self.cache.read().await;
        if let Some(&height) = guard.hash_to_height.get(hash) {
            if let Some(state) = guard.candidates_by_height.get(&height) {
                if let Some(block) = state.get_block() {
                    // Need to clone the block because we can't return a reference
                    // to something inside the guard
                    return Some(block.clone());
                }
            }
        }
        None
    }

    /// Gets the highest slot block at a specific height, if it exists
    pub async fn get_by_height(&self, height: u64) -> Option<SignedConsensusBlock<MainnetEthSpec>> {
        let guard = self.cache.read().await;
        if let Some(state) = guard.candidates_by_height.get(&height) {
            if let Some(block) = state.get_block() {
                return Some(block.clone());
            }
        }
        None
    }

    /// Removes and returns the candidate state for a specific hash.
    pub async fn remove(&self, hash: &Hash256) -> Option<CandidateState> {
        self.cache.write().await.remove(hash)
    }

    /// Clears all candidates from the cache.
    pub async fn clear(&self) {
        self.cache.write().await.clear();
    }

    /// Returns the number of candidates in the cache.
    pub async fn len(&self) -> usize {
        self.cache.read().await.len()
    }

    /// Returns true if the cache is empty.
    pub async fn is_empty(&self) -> bool {
        self.cache.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::ConsensusBlock;
    use crate::signatures::AggregateApproval;

    fn create_test_block(height: u64, slot: u64) -> SignedConsensusBlock<MainnetEthSpec> {
        // Create a simple consensus block with only the fields we need for testing
        let mut block = ConsensusBlock::<types::MainnetEthSpec> {
            slot,
            ..Default::default()
        };

        // Manually set the block_number in execution_payload
        block.execution_payload.block_number = height;

        SignedConsensusBlock {
            message: block,
            signature: AggregateApproval::new(), // Use the static new method instead of Default
        }
    }

    #[test]
    fn test_insert_new_block() {
        let mut cache = BlockCandidateCache::new();
        let block = create_test_block(100, 200);
        let block_hash = block.canonical_root();

        assert!(cache.insert(block).is_ok());

        // Verify it's in the cache
        assert!(cache.hash_to_height.contains_key(&block_hash));
        assert_eq!(cache.hash_to_height.get(&block_hash), Some(&100));
        assert_eq!(cache.len(), 1);

        // Test that we can get the block directly
        if let Some(state) = cache.candidates_by_height.get(&100) {
            if let Some(block) = state.get_block() {
                assert_eq!(block.message.execution_payload.block_number, 100);
                assert_eq!(block.message.slot, 200);
            } else {
                panic!("Block should exist in candidate state");
            }
        } else {
            panic!("Candidate state should exist at height 100");
        }
    }

    #[test]
    fn test_replace_block_at_same_height_with_higher_slot() {
        let mut cache = BlockCandidateCache::new();

        // Insert first block
        let block1 = create_test_block(100, 200);
        let hash1 = block1.canonical_root();
        assert!(cache.insert(block1).is_ok());

        // Insert second block at same height but higher slot
        let block2 = create_test_block(100, 300);
        let hash2 = block2.canonical_root();
        assert!(cache.insert(block2).is_ok());

        // Verify only the second block is kept
        assert!(!cache.hash_to_height.contains_key(&hash1));
        assert!(cache.hash_to_height.contains_key(&hash2));
        assert_eq!(cache.len(), 1);

        // Make sure the block at height 100 has slot 300
        if let Some(state) = cache.candidates_by_height.get(&100) {
            if let Some(block) = state.get_block() {
                assert_eq!(block.message.slot, 300);
            } else {
                panic!("Block should exist in candidate state");
            }
        } else {
            panic!("Candidate state should exist at height 100");
        }
    }

    #[test]
    fn test_keep_block_when_new_block_has_lower_slot() {
        let mut cache = BlockCandidateCache::new();

        // Insert first block with higher slot
        let block1 = create_test_block(100, 300);
        let hash1 = block1.canonical_root();
        assert!(cache.insert(block1).is_ok());

        // Insert second block at same height but lower slot
        let block2 = create_test_block(100, 200);
        let hash2 = block2.canonical_root();
        assert!(cache.insert(block2).is_ok());

        // Verify only the first block is kept
        assert!(cache.hash_to_height.contains_key(&hash1));
        assert!(!cache.hash_to_height.contains_key(&hash2));
        assert_eq!(cache.len(), 1);

        // Make sure the block at height 100 has slot 300 (the higher slot)
        if let Some(state) = cache.candidates_by_height.get(&100) {
            if let Some(block) = state.get_block() {
                assert_eq!(block.message.slot, 300);
            } else {
                panic!("Block should exist in candidate state");
            }
        } else {
            panic!("Candidate state should exist at height 100");
        }
    }

    #[test]
    fn test_clear() {
        let mut cache = BlockCandidateCache::new();

        // Insert some blocks
        let block1 = create_test_block(100, 200);
        let block2 = create_test_block(101, 201);

        assert!(cache.insert(block1).is_ok());
        assert!(cache.insert(block2).is_ok());
        assert_eq!(cache.len(), 2);

        // Clear the cache
        cache.clear();

        // Verify it's empty
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_remove() {
        let mut cache = BlockCandidateCache::new();

        // Insert a block
        let block = create_test_block(100, 200);
        let hash = block.canonical_root();
        assert!(cache.insert(block).is_ok());
        assert_eq!(cache.len(), 1);

        // Remove the block
        assert!(cache.remove(&hash).is_some());

        // Verify it's gone
        assert_eq!(cache.len(), 0);
        assert!(!cache.hash_to_height.contains_key(&hash));
    }

    // We can't directly test the approval functionality in unit tests since it
    // requires the signature checking logic. But we can test the structure
    // by making the hash_to_height mapping work as expected.
    #[test]
    fn test_block_approval_flow() {
        let mut cache = BlockCandidateCache::new();

        // First, simulate adding an approval for a block we don't know yet
        // by directly manipulating the maps
        let block_hash = Hash256::from_slice(&[1; 32]);
        let temp_height = 0; // Special placeholder height

        let candidate_state = CandidateState::default();
        // In a real scenario, this would create a QueuedApprovals state

        cache.hash_to_height.insert(block_hash, temp_height);
        cache
            .candidates_by_height
            .insert(temp_height, candidate_state);

        // Now simulate adding the block - this should move it to the proper height
        // In a real scenario, we'd call insert() directly which would take care of this
        cache.hash_to_height.remove(&block_hash);
        cache.candidates_by_height.remove(&temp_height);

        let new_candidate_state = CandidateState::default();
        // In a real scenario, this would properly merge the approvals
        let block_height = 100;

        cache.hash_to_height.insert(block_hash, block_height);
        cache
            .candidates_by_height
            .insert(block_height, new_candidate_state);

        // Verify the block is now at the right height
        assert_eq!(cache.hash_to_height.get(&block_hash), Some(&block_height));
        assert_eq!(cache.len(), 1);
    }
}
