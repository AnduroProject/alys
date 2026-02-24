//! Mock Actors for Tendermint Integration Tests
//!
//! These mock actors provide test doubles for StorageActor, EngineActor,
//! and NetworkActor, enabling self-contained integration tests.

use actix::prelude::*;
use ethereum_types::H256;
use lighthouse_wrapper::types::MainnetEthSpec;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

use crate::actors_v2::chain::tendermint::{BlockHash, Commit, Height, Round, ValidatorSet, VoteType};
use crate::block::SignedConsensusBlock;

// ============================================================================
// Captured Broadcast Types
// ============================================================================

/// Captured network broadcast for test assertions
#[derive(Debug, Clone)]
pub enum CapturedBroadcast {
    /// Proposal broadcast
    Proposal {
        height: Height,
        round: Round,
        block_hash: BlockHash,
    },
    /// Vote broadcast (prevote or precommit)
    Vote {
        height: Height,
        round: Round,
        vote_type: VoteType,
        block_hash: Option<BlockHash>,
    },
    /// Commit broadcast
    Commit {
        height: Height,
        round: Round,
        block_hash: BlockHash,
    },
}

// ============================================================================
// Mock Storage Actor
// ============================================================================

/// In-memory storage for testing
pub struct MockStorageActor {
    /// Stored blocks by height
    blocks: Arc<RwLock<HashMap<Height, SignedConsensusBlock<MainnetEthSpec>>>>,
    /// Block hash to height mapping
    hash_to_height: Arc<RwLock<HashMap<BlockHash, Height>>>,
    /// Stored commits by height
    commits: Arc<RwLock<HashMap<Height, Commit>>>,
    /// Current chain head
    head_height: Arc<RwLock<Height>>,
    /// Validator set history (height -> validator set that produces height+1)
    validator_sets: Arc<RwLock<HashMap<Height, ValidatorSet>>>,
}

impl MockStorageActor {
    pub fn new() -> Self {
        Self {
            blocks: Arc::new(RwLock::new(HashMap::new())),
            hash_to_height: Arc::new(RwLock::new(HashMap::new())),
            commits: Arc::new(RwLock::new(HashMap::new())),
            head_height: Arc::new(RwLock::new(0)),
            validator_sets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get stored block at height
    pub async fn get_block(&self, height: Height) -> Option<SignedConsensusBlock<MainnetEthSpec>> {
        self.blocks.read().await.get(&height).cloned()
    }

    /// Get stored commit at height
    pub async fn get_commit(&self, height: Height) -> Option<Commit> {
        self.commits.read().await.get(&height).cloned()
    }

    /// Get current head height
    pub async fn get_head_height(&self) -> Height {
        *self.head_height.read().await
    }

    /// Store a block (called by test harness to simulate storage)
    pub async fn store_block(
        &self,
        height: Height,
        block: SignedConsensusBlock<MainnetEthSpec>,
        commit: Commit,
    ) {
        let block_hash = block.canonical_root();

        let mut blocks = self.blocks.write().await;
        let mut hash_to_height = self.hash_to_height.write().await;
        let mut commits = self.commits.write().await;
        let mut head = self.head_height.write().await;

        blocks.insert(height, block);
        hash_to_height.insert(block_hash, height);
        commits.insert(height, commit);

        if height > *head {
            *head = height;
        }

        debug!("MockStorage: stored block at height {}", height);
    }

    /// Set validator set for a height
    pub async fn set_validator_set(&self, height: Height, validator_set: ValidatorSet) {
        self.validator_sets.write().await.insert(height, validator_set);
    }

    /// Get validator set for a height
    pub async fn get_validator_set(&self, height: Height) -> Option<ValidatorSet> {
        self.validator_sets.read().await.get(&height).cloned()
    }
}

impl Default for MockStorageActor {
    fn default() -> Self {
        Self::new()
    }
}

impl Actor for MockStorageActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("MockStorageActor started");
    }
}

// ============================================================================
// Mock Engine Actor
// ============================================================================

/// Mock execution engine for testing
pub struct MockEngineActor {
    /// Whether the next validation should succeed
    next_validation_result: Arc<RwLock<bool>>,
    /// Count of validation calls
    validation_call_count: Arc<RwLock<u32>>,
    /// Whether to simulate engine errors
    simulate_errors: Arc<RwLock<bool>>,
}

impl MockEngineActor {
    pub fn new() -> Self {
        Self {
            next_validation_result: Arc::new(RwLock::new(true)),
            validation_call_count: Arc::new(RwLock::new(0)),
            simulate_errors: Arc::new(RwLock::new(false)),
        }
    }

    /// Set the result for the next validation call
    pub async fn set_next_validation_result(&self, is_valid: bool) {
        *self.next_validation_result.write().await = is_valid;
    }

    /// Get the number of validation calls made
    pub async fn get_validation_call_count(&self) -> u32 {
        *self.validation_call_count.read().await
    }

    /// Set whether to simulate errors
    pub async fn set_simulate_errors(&self, simulate: bool) {
        *self.simulate_errors.write().await = simulate;
    }

    /// Validate a block (returns configured result)
    pub async fn validate_block(&self) -> Result<bool, String> {
        let mut count = self.validation_call_count.write().await;
        *count += 1;

        if *self.simulate_errors.read().await {
            return Err("Simulated engine error".to_string());
        }

        let result = *self.next_validation_result.read().await;
        debug!("MockEngine: validate_block -> {}", result);
        Ok(result)
    }
}

impl Default for MockEngineActor {
    fn default() -> Self {
        Self::new()
    }
}

impl Actor for MockEngineActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("MockEngineActor started");
    }
}

// ============================================================================
// Mock Network Actor
// ============================================================================

/// Mock network actor that captures all broadcasts
pub struct MockNetworkActor {
    /// Captured broadcasts for test assertions
    captured: Arc<RwLock<Vec<CapturedBroadcast>>>,
    /// Whether broadcasts should "fail"
    simulate_failures: Arc<RwLock<bool>>,
}

impl MockNetworkActor {
    pub fn new() -> (Self, Arc<RwLock<Vec<CapturedBroadcast>>>) {
        let captured = Arc::new(RwLock::new(Vec::new()));
        let actor = Self {
            captured: captured.clone(),
            simulate_failures: Arc::new(RwLock::new(false)),
        };
        (actor, captured)
    }

    /// Get all captured broadcasts
    pub async fn get_broadcasts(&self) -> Vec<CapturedBroadcast> {
        self.captured.read().await.clone()
    }

    /// Clear captured broadcasts
    pub async fn clear_broadcasts(&self) {
        self.captured.write().await.clear();
    }

    /// Set whether to simulate network failures
    pub async fn set_simulate_failures(&self, simulate: bool) {
        *self.simulate_failures.write().await = simulate;
    }

    /// Capture a proposal broadcast
    pub async fn broadcast_proposal(
        &self,
        height: Height,
        round: Round,
        block_hash: BlockHash,
    ) -> Result<(), String> {
        if *self.simulate_failures.read().await {
            return Err("Simulated network failure".to_string());
        }

        self.captured.write().await.push(CapturedBroadcast::Proposal {
            height,
            round,
            block_hash,
        });
        debug!("MockNetwork: broadcast proposal h={} r={}", height, round);
        Ok(())
    }

    /// Capture a vote broadcast
    pub async fn broadcast_vote(
        &self,
        height: Height,
        round: Round,
        vote_type: VoteType,
        block_hash: Option<BlockHash>,
    ) -> Result<(), String> {
        if *self.simulate_failures.read().await {
            return Err("Simulated network failure".to_string());
        }

        self.captured.write().await.push(CapturedBroadcast::Vote {
            height,
            round,
            vote_type,
            block_hash,
        });
        debug!(
            "MockNetwork: broadcast {:?} h={} r={} hash={:?}",
            vote_type, height, round, block_hash
        );
        Ok(())
    }

    /// Capture a commit broadcast
    pub async fn broadcast_commit(
        &self,
        height: Height,
        round: Round,
        block_hash: BlockHash,
    ) -> Result<(), String> {
        if *self.simulate_failures.read().await {
            return Err("Simulated network failure".to_string());
        }

        self.captured.write().await.push(CapturedBroadcast::Commit {
            height,
            round,
            block_hash,
        });
        debug!("MockNetwork: broadcast commit h={} r={}", height, round);
        Ok(())
    }
}

impl Default for MockNetworkActor {
    fn default() -> Self {
        Self::new().0
    }
}

impl Actor for MockNetworkActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("MockNetworkActor started");
    }
}

// ============================================================================
// Test Utilities
// ============================================================================

/// Create a test block hash from a numeric ID
pub fn test_block_hash(id: u64) -> BlockHash {
    H256::from_low_u64_be(id)
}

/// Check if any broadcast matches a predicate
pub fn has_broadcast<F>(broadcasts: &[CapturedBroadcast], predicate: F) -> bool
where
    F: Fn(&CapturedBroadcast) -> bool,
{
    broadcasts.iter().any(predicate)
}

/// Count broadcasts matching a predicate
pub fn count_broadcasts<F>(broadcasts: &[CapturedBroadcast], predicate: F) -> usize
where
    F: Fn(&CapturedBroadcast) -> bool,
{
    broadcasts.iter().filter(|b| predicate(b)).count()
}

/// Find first broadcast matching a predicate
pub fn find_broadcast<F>(broadcasts: &[CapturedBroadcast], predicate: F) -> Option<&CapturedBroadcast>
where
    F: Fn(&CapturedBroadcast) -> bool,
{
    broadcasts.iter().find(|b| predicate(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_storage_basic() {
        let storage = MockStorageActor::new();

        assert_eq!(storage.get_head_height().await, 0);
        assert!(storage.get_block(1).await.is_none());
    }

    #[tokio::test]
    async fn test_mock_engine_validation() {
        let engine = MockEngineActor::new();

        // Default: validation succeeds
        assert!(engine.validate_block().await.unwrap());
        assert_eq!(engine.get_validation_call_count().await, 1);

        // Set to fail
        engine.set_next_validation_result(false).await;
        assert!(!engine.validate_block().await.unwrap());
        assert_eq!(engine.get_validation_call_count().await, 2);
    }

    #[tokio::test]
    async fn test_mock_network_captures() {
        let (network, captured) = MockNetworkActor::new();

        network.broadcast_proposal(1, 0, test_block_hash(42)).await.unwrap();
        network.broadcast_vote(1, 0, VoteType::Prevote, Some(test_block_hash(42))).await.unwrap();

        let broadcasts = captured.read().await;
        assert_eq!(broadcasts.len(), 2);

        assert!(matches!(&broadcasts[0], CapturedBroadcast::Proposal { height: 1, round: 0, .. }));
        assert!(matches!(&broadcasts[1], CapturedBroadcast::Vote { vote_type: VoteType::Prevote, .. }));
    }
}
