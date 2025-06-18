pub mod block_candidate_cache;
mod candidate_state;

use crate::block::SignedConsensusBlock;
use crate::error::Error;
use crate::network::ApproveBlock;
use async_trait::async_trait;
use block_candidate_cache::{BlockCandidateCache, BlockCandidateCacheTrait};
use candidate_state::CandidateState;
use lighthouse_wrapper::bls::PublicKey;
use lighthouse_wrapper::execution_layer::Hash256;
use lighthouse_wrapper::store::MainnetEthSpec;
use tokio::sync::RwLock;

/// A wrapper around BlockCandidateCache that provides thread-safe access.
#[derive(Default)]
pub struct BlockCandidates {
    cache: RwLock<BlockCandidateCache>,
}

impl BlockCandidates {
    /// Creates a new thread-safe BlockCandidates cache.
    pub fn new() -> Self {
        // Self::init_block_candidate_cache()
        Self {
            cache: RwLock::new(BlockCandidateCache::new()),
        }
    }

    /// Clears all candidates from the cache.
    #[allow(dead_code)]
    pub async fn clear(&self) {
        self.cache.write().await.clear();
    }

    /// Returns the number of candidates in the cache.
    #[allow(dead_code)]
    pub async fn len(&self) -> usize {
        self.cache.read().await.len()
    }

    /// Returns true if the cache is empty.
    #[allow(dead_code)]
    pub async fn is_empty(&self) -> bool {
        self.cache.read().await.is_empty()
    }
}

#[async_trait]
impl BlockCandidateCacheTrait for BlockCandidates {
    /// Adds an approval for a block.
    async fn add_approval(
        &self,
        approval: ApproveBlock,
        authorities: &[PublicKey],
        is_syncing: bool,
    ) -> Result<(), Error> {
        self.cache
            .write()
            .await
            .add_approval(approval, authorities, is_syncing)
    }

    /// Inserts a block candidate into the cache.
    async fn insert(
        &self,
        block: SignedConsensusBlock<MainnetEthSpec>,
        is_synced: bool,
    ) -> Result<(), Error> {
        self.cache.write().await.insert(block, is_synced)
    }

    /// Gets the block associated with a hash, if it exists
    async fn get_block(&self, hash: &Hash256) -> Option<SignedConsensusBlock<MainnetEthSpec>> {
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

    /// Removes and returns the candidate state for a specific hash.
    async fn remove(&self, hash: &Hash256) -> Option<CandidateState> {
        self.cache.write().await.remove(hash)
    }
}

// impl BlockCandidateCacheInit for BlockCandidates {
//     fn init_block_candidate_cache() -> BlockCandidates where BlockCandidates:
//     where
//         T: BlockCandidateCacheTrait,
//     {
//         Self {
//             cache: RwLock::new(BlockCandidateCache::new()),
//         }
//     }
// }
