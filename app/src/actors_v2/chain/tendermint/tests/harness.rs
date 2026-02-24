//! TendermintTestHarness - Integration Test Infrastructure
//!
//! Provides a self-contained test environment for Tendermint consensus testing.
//! All dependencies are mocked, no external infrastructure required.

use ethereum_types::H256;
use lighthouse_wrapper::bls::Keypair as BLSKeypair;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info};

use super::mock_actors::*;
use crate::actors_v2::chain::tendermint::{
    BlockHash, Height, Round, TendermintState, TendermintStep,
    TimeoutConfig, TimeoutEvent, TimeoutScheduler, ValidatorId, ValidatorSet,
    VoteType, ConsensusWAL, RecoveredState,
};

/// Errors that can occur in test harness operations
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error("Setup failed: {0}")]
    Setup(String),

    #[error("Timeout waiting for: {0}")]
    Timeout(String),

    #[error("State error: {0}")]
    State(String),

    #[error("Assertion failed: {0}")]
    Assertion(String),
}

/// Test harness for Tendermint consensus integration tests
///
/// Provides a complete test environment with:
/// - Mock StorageActor (in-memory)
/// - Mock EngineActor (configurable validation results)
/// - Mock NetworkActor (captures broadcasts)
/// - Real TendermintState and TimeoutScheduler
/// - Temporary WAL directory
pub struct TendermintTestHarness {
    /// Tendermint consensus state
    pub tendermint_state: Arc<RwLock<TendermintState>>,

    /// Timeout scheduler
    pub timeout_scheduler: Arc<RwLock<TimeoutScheduler>>,

    /// Timeout event receiver
    pub timeout_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<TimeoutEvent>>>,

    /// Mock storage actor
    pub storage: MockStorageActor,

    /// Mock engine actor
    pub engine: MockEngineActor,

    /// Mock network actor
    pub network: MockNetworkActor,

    /// Captured network broadcasts
    pub captured_broadcasts: Arc<RwLock<Vec<CapturedBroadcast>>>,

    /// Test validator keypair
    pub validator_keypair: Arc<BLSKeypair>,

    /// Validator set
    pub validator_set: Arc<ValidatorSet>,

    /// Our validator ID
    pub our_validator_id: ValidatorId,

    /// WAL for crash recovery testing
    pub wal: Arc<RwLock<ConsensusWAL>>,

    /// WAL temporary directory (kept alive for test duration)
    pub _wal_dir: TempDir,

    /// Number of validators in the set
    pub validator_count: usize,
}

impl TendermintTestHarness {
    /// Create harness with single validator (always proposer, instant 2/3+)
    ///
    /// This is the simplest configuration for testing the happy path.
    /// The single validator is always the proposer and reaches threshold
    /// immediately with its own vote.
    pub async fn single_validator() -> Result<Self, TestError> {
        Self::multi_validator_as_index(1, 0).await
    }

    /// Create harness with N validators where we control validator at `our_index`
    ///
    /// Proposer selection follows Tendermint: (height + round) % count
    ///
    /// # Arguments
    /// * `count` - Total number of validators (1-15)
    /// * `our_index` - Index of the validator we control (0 to count-1)
    pub async fn multi_validator_as_index(count: usize, our_index: usize) -> Result<Self, TestError> {
        if count == 0 || count > 15 {
            return Err(TestError::Setup("Validator count must be 1-15".into()));
        }
        if our_index >= count {
            return Err(TestError::Setup(format!(
                "our_index {} must be < count {}",
                our_index, count
            )));
        }

        // Create temporary WAL directory
        let wal_dir = TempDir::new()
            .map_err(|e| TestError::Setup(format!("Failed to create temp dir: {}", e)))?;

        // Create validator keypairs
        let mut validator_pubkeys = Vec::with_capacity(count);
        let mut our_keypair = None;

        for i in 0..count {
            let keypair = BLSKeypair::random();
            validator_pubkeys.push(keypair.pk.clone());
            if i == our_index {
                our_keypair = Some(Arc::new(keypair));
            }
        }

        let validator_keypair = our_keypair.ok_or_else(|| TestError::Setup("Missing keypair".into()))?;
        let our_validator_id = ValidatorId::new(our_index as u8);

        // Create validator set
        let validator_set = Arc::new(ValidatorSet::with_equal_power(validator_pubkeys));

        // Create timeout scheduler
        let (timeout_tx, timeout_rx) = mpsc::channel(32);
        let timeout_config = TimeoutConfig::fast_for_testing();
        let timeout_scheduler = Arc::new(RwLock::new(TimeoutScheduler::new(
            timeout_config,
            timeout_tx,
        )));

        // Create WAL
        let wal = ConsensusWAL::new(wal_dir.path())
            .map_err(|e| TestError::Setup(format!("Failed to create WAL: {}", e)))?;

        // Create Tendermint state (starting at height 0)
        let tendermint_state = Arc::new(RwLock::new(TendermintState::new(
            0, // Initial height
            validator_set.clone(),
            Some(our_validator_id),
        )));

        // Create mock actors
        let storage = MockStorageActor::new();
        let engine = MockEngineActor::new();
        let (network, captured_broadcasts) = MockNetworkActor::new();

        info!(
            "Created TendermintTestHarness: {} validators, we are validator {}",
            count, our_index
        );

        Ok(Self {
            tendermint_state,
            timeout_scheduler,
            timeout_rx: Arc::new(tokio::sync::Mutex::new(timeout_rx)),
            storage,
            engine,
            network,
            captured_broadcasts,
            validator_keypair,
            validator_set,
            our_validator_id,
            wal: Arc::new(RwLock::new(wal)),
            _wal_dir: wal_dir,
            validator_count: count,
        })
    }

    /// Start consensus at a specific height
    ///
    /// Initializes state for the height and schedules propose timeout.
    /// If we are the proposer, simulates proposal creation.
    pub async fn start_height(&self, height: Height) -> Result<(), TestError> {
        {
            let mut state = self.tendermint_state.write().await;
            state.new_height(height, self.validator_set.clone());
        }

        // Set scheduler position and schedule propose timeout
        {
            let mut scheduler = self.timeout_scheduler.write().await;
            scheduler.set_position(height, 0);
            let _ = scheduler.schedule(TendermintStep::Propose);
        }

        // Check if we are the proposer
        let proposer = self.validator_set.get_proposer(height, 0);
        let is_proposer = proposer == self.our_validator_id;

        debug!("start_height({}): proposer={:?}, is_proposer={}", height, proposer, is_proposer);

        if is_proposer {
            // Simulate proposal creation
            let block_hash = H256::from_low_u64_be(height * 1000 + 1);

            // Update state with proposal
            {
                let mut state = self.tendermint_state.write().await;
                state.step = TendermintStep::Prevote;
            }

            // Cancel propose timeout and schedule prevote
            {
                let mut scheduler = self.timeout_scheduler.write().await;
                scheduler.cancel_step(TendermintStep::Propose);
                let _ = scheduler.schedule(TendermintStep::Prevote);
            }

            // Broadcast proposal
            self.network.broadcast_proposal(height, 0, block_hash).await
                .map_err(|e| TestError::State(e))?;

            // Cast our prevote (with engine validation)
            let is_valid = self.engine.validate_block().await.unwrap_or(false);
            let vote_hash = if is_valid { Some(block_hash) } else { None };

            // Record our prevote
            {
                let mut state = self.tendermint_state.write().await;
                state.record_prevote(vote_hash);
            }

            // Broadcast our prevote
            self.network.broadcast_vote(height, 0, VoteType::Prevote, vote_hash).await
                .map_err(|e| TestError::State(e))?;

            // For single validator, check if we reached 2/3+ prevotes
            if self.validator_count == 1 && is_valid {
                // We have 1/1 = 100% > 66.7%, proceed to precommit
                {
                    let mut state = self.tendermint_state.write().await;
                    state.lock_on(0, block_hash);
                    state.step = TendermintStep::Precommit;
                }

                // Cancel prevote timeout and schedule precommit
                {
                    let mut scheduler = self.timeout_scheduler.write().await;
                    scheduler.cancel_step(TendermintStep::Prevote);
                    let _ = scheduler.schedule(TendermintStep::Precommit);
                }

                // Record and broadcast our precommit
                {
                    let mut state = self.tendermint_state.write().await;
                    state.record_precommit(Some(block_hash));
                }

                self.network.broadcast_vote(height, 0, VoteType::Precommit, Some(block_hash)).await
                    .map_err(|e| TestError::State(e))?;

                // For single validator, we now have 2/3+ precommits
                // Trigger commit
                {
                    let mut state = self.tendermint_state.write().await;
                    state.step = TendermintStep::Commit;
                }

                // Cancel precommit timeout
                {
                    let mut scheduler = self.timeout_scheduler.write().await;
                    scheduler.cancel_step(TendermintStep::Precommit);
                }

                // Broadcast commit
                self.network.broadcast_commit(height, 0, block_hash).await
                    .map_err(|e| TestError::State(e))?;

                debug!("Single validator committed block at height {}", height);
            }
        }

        Ok(())
    }

    /// Inject a timeout event (bypasses real timer)
    pub async fn inject_timeout(&self, step: TendermintStep) -> Result<(), TestError> {
        let (height, round) = {
            let state = self.tendermint_state.read().await;
            (state.height, state.round)
        };

        let scheduler = self.timeout_scheduler.read().await;
        scheduler
            .inject_timeout_for_testing(height, round, step)
            .await
            .map_err(|e| TestError::State(format!("Failed to inject timeout: {:?}", e)))
    }

    /// Wait for block commit at height
    pub async fn wait_for_commit(&self, height: Height, max_wait: Duration) -> Result<(), TestError> {
        let start = std::time::Instant::now();

        loop {
            let state = self.tendermint_state.read().await;
            if state.height > height || (state.height == height && state.step == TendermintStep::Commit) {
                return Ok(());
            }
            drop(state);

            if start.elapsed() > max_wait {
                return Err(TestError::Timeout(format!(
                    "Waiting for commit at height {}",
                    height
                )));
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Get current consensus state snapshot
    pub async fn get_state(&self) -> TendermintStateSnapshot {
        let state = self.tendermint_state.read().await;
        TendermintStateSnapshot {
            height: state.height,
            round: state.round,
            step: state.step,
            locked_block: state.locked_block,
            locked_round: state.locked_round,
        }
    }

    /// Get all captured network broadcasts
    pub async fn get_broadcasts(&self) -> Vec<CapturedBroadcast> {
        self.captured_broadcasts.read().await.clone()
    }

    /// Clear captured broadcasts
    pub async fn clear_broadcasts(&self) {
        self.captured_broadcasts.write().await.clear();
    }

    /// Simulate crash (preserves WAL, resets in-memory state)
    pub async fn crash(&mut self) -> Result<(), TestError> {
        debug!("Simulating crash...");

        // Sync WAL to disk
        {
            let mut wal = self.wal.write().await;
            wal.sync().map_err(|e| TestError::State(format!("WAL sync failed: {:?}", e)))?;
        }

        // Reset in-memory state (WAL directory preserved)
        let state = TendermintState::new(0, self.validator_set.clone(), Some(self.our_validator_id));
        *self.tendermint_state.write().await = state;

        // Create new timeout channel
        let (timeout_tx, timeout_rx) = mpsc::channel(32);
        let timeout_config = TimeoutConfig::fast_for_testing();
        *self.timeout_scheduler.write().await = TimeoutScheduler::new(timeout_config, timeout_tx);
        *self.timeout_rx.lock().await = timeout_rx;

        Ok(())
    }

    /// Restart after crash (replay WAL)
    pub async fn restart(&mut self) -> Result<(), TestError> {
        debug!("Restarting from WAL...");

        // Replay WAL entries
        let entries = {
            let wal = self.wal.read().await;
            wal.replay()
                .map_err(|e| TestError::State(format!("WAL replay failed: {:?}", e)))?
        };

        // Recover state from WAL entries
        let recovered = RecoveredState::from_wal_entries(entries);

        debug!(
            "Recovered state: last_committed_height={:?}, current_round={:?}",
            recovered.last_committed_height, recovered.current_round
        );

        let mut state = self.tendermint_state.write().await;

        // Apply recovered state
        if let Some(height) = recovered.last_committed_height {
            state.height = height + 1; // Start at next height
        }
        if let Some(round) = recovered.current_round {
            state.round = round;
        }
        state.locked_block = recovered.locked_block;
        state.locked_round = recovered.locked_round;

        // Restore sent vote tracking from WAL
        for (round, hash) in recovered.sent_prevotes {
            state.sent_prevotes.insert(round, hash);
        }
        for (round, hash) in recovered.sent_precommits {
            state.sent_precommits.insert(round, hash);
        }

        Ok(())
    }

    /// Check if we are the proposer for current height/round
    pub async fn is_proposer(&self) -> bool {
        let state = self.tendermint_state.read().await;
        let proposer = self.validator_set.get_proposer(state.height, state.round);
        proposer == self.our_validator_id
    }

    /// Check if we are the proposer for a specific height/round
    pub fn is_proposer_for(&self, height: Height, round: Round) -> bool {
        let proposer = self.validator_set.get_proposer(height, round);
        proposer == self.our_validator_id
    }

    /// Advance round (for timeout testing)
    pub async fn advance_round(&self) -> Result<(), TestError> {
        let mut state = self.tendermint_state.write().await;
        let new_round = state.round + 1;
        state.new_round(new_round);

        let mut scheduler = self.timeout_scheduler.write().await;
        scheduler.set_position(state.height, new_round);
        let _ = scheduler.schedule(TendermintStep::Propose);

        Ok(())
    }
}

/// Snapshot of Tendermint state for test assertions
#[derive(Debug, Clone)]
pub struct TendermintStateSnapshot {
    pub height: Height,
    pub round: Round,
    pub step: TendermintStep,
    pub locked_block: Option<BlockHash>,
    pub locked_round: Option<Round>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_harness_single_validator_creation() {
        let harness = TendermintTestHarness::single_validator().await.unwrap();

        assert_eq!(harness.validator_count, 1);
        assert_eq!(harness.our_validator_id, ValidatorId::new(0));
        assert!(harness.is_proposer_for(1, 0));
    }

    #[tokio::test]
    async fn test_harness_multi_validator_creation() {
        let harness = TendermintTestHarness::multi_validator_as_index(3, 1).await.unwrap();

        assert_eq!(harness.validator_count, 3);
        assert_eq!(harness.our_validator_id, ValidatorId::new(1));

        // Proposer for height 1, round 0: (1 + 0) % 3 = 1
        assert!(harness.is_proposer_for(1, 0));
        // Proposer for height 2, round 0: (2 + 0) % 3 = 2
        assert!(!harness.is_proposer_for(2, 0));
    }

    #[tokio::test]
    async fn test_harness_start_height() {
        let harness = TendermintTestHarness::single_validator().await.unwrap();

        harness.start_height(1).await.unwrap();

        let state = harness.get_state().await;
        assert_eq!(state.height, 1);
    }

    #[tokio::test]
    async fn test_harness_single_validator_full_cycle() {
        let harness = TendermintTestHarness::single_validator().await.unwrap();

        // Start height 1 - should complete full cycle for single validator
        harness.start_height(1).await.unwrap();

        // Verify proposal was created and broadcast
        let broadcasts = harness.get_broadcasts().await;
        assert!(broadcasts.iter().any(|b| matches!(b, CapturedBroadcast::Proposal { height: 1, .. })));

        // Verify prevote was cast
        assert!(broadcasts.iter().any(|b| matches!(b, CapturedBroadcast::Vote { vote_type: VoteType::Prevote, .. })));

        // Verify precommit was cast
        assert!(broadcasts.iter().any(|b| matches!(b, CapturedBroadcast::Vote { vote_type: VoteType::Precommit, .. })));

        // Verify commit was broadcast
        assert!(broadcasts.iter().any(|b| matches!(b, CapturedBroadcast::Commit { height: 1, .. })));

        // Verify state shows commit
        let state = harness.get_state().await;
        assert_eq!(state.step, TendermintStep::Commit);
    }
}
