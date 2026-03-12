//! Tendermint Consensus Driver
//!
//! Replaces AuraSlotWorkerV2 with event-driven Tendermint timing.
//! Instead of fixed time slots, this driver:
//! 1. Listens for consensus events (new height, timeout, votes)
//! 2. Triggers appropriate actions (propose, prevote, precommit)
//! 3. Advances height only after commit
//!
//! Key differences from Aura:
//! - Event-driven vs time-based
//! - Height+Round based proposer selection
//! - Adaptive timeouts with exponential backoff
//! - Instant finality (no orphan blocks)

use crate::actors_v2::chain::messages::{ChainMessage, ChainResponse};
use crate::actors_v2::chain::tendermint::wal::{ConsensusWAL, RecoveredState, WALEntry};
use crate::actors_v2::chain::tendermint::{
    BlockHash, Commit, TendermintStep, TimeoutConfig, ValidatorId, ValidatorSet,
};
use crate::actors_v2::chain::ChainActor;
use actix::prelude::*;
use lighthouse_wrapper::bls::PublicKey;
use std::collections::BTreeMap;
// Issue 3.2: Removed HashMap - vote tracking now in ChainActor.tendermint_state
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

/// Error types for TendermintDriver
#[derive(Debug, Clone, Error)]
pub enum DriverError {
    #[error("ChainActor not reachable: {0}")]
    ChainActorUnreachable(String),

    #[error("Invalid driver state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },

    #[error("Timeout scheduler error: {0}")]
    TimeoutSchedulerError(String),

    #[error("WAL write failed: {0}")]
    WalWriteFailed(String),

    #[error("Driver is paused during sync")]
    PausedDuringSync,

    #[error("Missing last commit for height {0}")]
    MissingLastCommit(u64),

    #[error("Validator set update failed: {0}")]
    ValidatorSetUpdateFailed(String),

    #[error("Recovery failed: {0}")]
    RecoveryFailed(String),
}

/// Node operation mode
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeMode {
    /// Full consensus participant
    Validator,
    /// Follows consensus, doesn't vote
    Observer,
}

/// Configuration for TendermintDriver
#[derive(Debug, Clone)]
pub struct TendermintDriverConfig {
    /// Node operation mode
    pub mode: NodeMode,
    /// Timeout configuration
    pub timeout_config: TimeoutConfig,
    /// Data directory for WAL storage
    pub data_dir: PathBuf,
    /// Whether WAL is enabled (disable for testing)
    pub wal_enabled: bool,
}

impl Default for TendermintDriverConfig {
    fn default() -> Self {
        Self {
            mode: NodeMode::Observer,
            timeout_config: TimeoutConfig::default(),
            data_dir: PathBuf::from("/data/alys"),
            wal_enabled: true,
        }
    }
}

/// Messages to the Tendermint Driver
#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub enum TendermintDriverMessage {
    /// Start consensus at a new height
    NewHeight { height: u64 },

    /// Timeout expired for current step
    Timeout {
        height: u64,
        round: u32,
        step: TendermintStep,
    },

    /// Move to next round (after timeout or 2/3+ NIL)
    NextRound { height: u64, round: u32 },

    /// Block committed, advance to next height
    Committed { height: u64, last_commit: Commit },

    /// Pause consensus (during sync)
    Pause,

    /// Resume consensus after sync
    Resume { height: u64 },

    /// Update validator set (from governance)
    UpdateValidatorSet {
        validator_set: Arc<ValidatorSet>,
        activation_height: u64,
    },

    /// Notification that we advanced to a higher round via future round votes
    /// (triggered by 2/3+ votes from that round)
    RoundAdvanced {
        round: u32,
        step: TendermintStep,
    },

    /// Block request timeout - need to retry or give up
    BlockRequestTimeout {
        block_hash: ethereum_types::H256,
        correlation_id: uuid::Uuid,
    },

    /// Stop the driver
    Stop,
}

/// Tendermint Consensus Driver
///
/// Replaces AuraSlotWorkerV2 with event-driven consensus timing.
///
/// Issue 3.2: This driver coordinates timeouts and delegates consensus state
/// to ChainActor (the single source of truth). It no longer maintains duplicate
/// consensus state like locked_round, locked_value, sent_prevotes, etc.
pub struct TendermintDriver {
    /// Configuration
    config: TendermintDriverConfig,

    /// Our validator identity (None if observer)
    validator_id: Option<ValidatorId>,

    /// Our validator public key (for membership verification across validator set changes)
    validator_pubkey: Option<PublicKey>,

    /// Current validator set
    validator_set: Arc<ValidatorSet>,

    /// Pending validator set updates: activation_height -> new_set
    /// Uses BTreeMap to maintain sorted order by activation height
    pending_validator_updates: BTreeMap<u64, Arc<ValidatorSet>>,

    /// Timeout configuration (used directly for calculating timeouts)
    timeout_config: TimeoutConfig,

    /// Address of ChainActor
    chain_actor: Option<Addr<ChainActor>>,

    /// Current step (for timeout scheduling)
    current_step: TendermintStep,

    /// Pending timeout handle (for cancellation)
    pending_timeout: Option<SpawnHandle>,

    /// Whether consensus is paused (during sync)
    is_paused: bool,

    /// Last committed block's commit (for next proposal)
    last_commit: Option<Commit>,

    // Issue 3.2: Removed duplicate consensus state:
    // - locked_round, locked_value: Now only in ChainActor.tendermint_state
    // - valid_round, valid_value: Now only in ChainActor.tendermint_state
    // - sent_prevotes, sent_precommits: Now only in ChainActor.tendermint_state

    /// Current height (for timeout scheduling context)
    /// Note: This is the driver's tracking of position for timeout scheduling.
    /// ChainActor.tendermint_state is the single source of truth for consensus.
    current_height: u64,

    /// Current round (for timeout scheduling context)
    current_round: u32,

    /// Write-Ahead Log for crash recovery safety
    /// Issue 3.1 will move this to ChainActor
    wal: Option<Arc<RwLock<ConsensusWAL>>>,

    /// Recovered state from WAL replay (Issue 3.1)
    /// Stored until ChainActor is available, then sent via ApplyRecoveredState
    pending_recovery: Option<RecoveredState>,
}

impl TendermintDriver {
    /// Create a new TendermintDriver
    ///
    /// # Arguments
    ///
    /// * `config` - Driver configuration
    /// * `validator_pubkey` - Our validator's public key (None if observer)
    /// * `validator_set` - Initial validator set
    ///
    /// The validator_id is derived from the public key's position in the validator set.
    /// This allows proper tracking across validator set changes.
    pub fn new(
        config: TendermintDriverConfig,
        validator_pubkey: Option<PublicKey>,
        validator_set: Arc<ValidatorSet>,
    ) -> Self {
        let timeout_config = config.timeout_config.clone();

        // Derive validator_id from public key lookup in the validator set
        let validator_id = validator_pubkey
            .as_ref()
            .and_then(|pk| validator_set.find_validator(pk));

        // Initialize WAL if enabled
        let (wal, recovered_state) = if config.wal_enabled {
            match ConsensusWAL::new(&config.data_dir) {
                Ok(wal) => {
                    // Replay WAL to recover state
                    let entries = wal.replay().unwrap_or_else(|e| {
                        warn!(error = ?e, "WAL replay failed, starting fresh");
                        Vec::new()
                    });
                    let recovered = RecoveredState::from_wal_entries(entries);
                    info!(
                        last_committed = ?recovered.last_committed_height,
                        start_height = recovered.start_height(),
                        "WAL recovery complete"
                    );
                    (Some(Arc::new(RwLock::new(wal))), Some(recovered))
                }
                Err(e) => {
                    error!(error = ?e, "Failed to open WAL, consensus safety compromised");
                    (None, None)
                }
            }
        } else {
            debug!("WAL disabled (testing mode)");
            (None, None)
        };

        // Extract recovered state for position tracking
        // Issue 3.2: Locking state and vote tracking moved to ChainActor.tendermint_state
        // Issue 3.1 will handle applying recovered state to ChainActor
        let (current_height, current_round) = if let Some(ref state) = recovered_state {
            (
                state.start_height().saturating_sub(1), // Will be incremented on NewHeight
                state.current_round.unwrap_or(0),
            )
        } else {
            (0, 0)
        };

        Self {
            config,
            validator_id,
            validator_pubkey,
            validator_set,
            pending_validator_updates: BTreeMap::new(),
            timeout_config,
            chain_actor: None,
            current_step: TendermintStep::Propose,
            pending_timeout: None,
            is_paused: false,
            last_commit: None,
            // Issue 3.2: Removed locked_round, locked_value, valid_round, valid_value,
            // sent_prevotes, sent_precommits - now only in ChainActor.tendermint_state
            current_height,
            current_round,
            wal,
            // Issue 3.1: Store recovered state for later application to ChainActor
            pending_recovery: recovered_state,
        }
    }

    /// Create a new TendermintDriver without WAL (for testing)
    #[cfg(test)]
    pub fn new_without_wal(
        config: TendermintDriverConfig,
        validator_pubkey: Option<PublicKey>,
        validator_set: Arc<ValidatorSet>,
    ) -> Self {
        let timeout_config = config.timeout_config.clone();

        // Derive validator_id from public key lookup in the validator set
        let validator_id = validator_pubkey
            .as_ref()
            .and_then(|pk| validator_set.find_validator(pk));

        Self {
            config,
            validator_id,
            validator_pubkey,
            validator_set,
            pending_validator_updates: BTreeMap::new(),
            timeout_config,
            chain_actor: None,
            current_step: TendermintStep::Propose,
            pending_timeout: None,
            is_paused: false,
            last_commit: None,
            // Issue 3.2: Removed locked_round, locked_value, valid_round, valid_value,
            // sent_prevotes, sent_precommits - now only in ChainActor.tendermint_state
            current_height: 0,
            current_round: 0,
            wal: None,
            pending_recovery: None,
        }
    }

    /// Set the ChainActor address and apply any pending WAL recovery
    ///
    /// Issue 3.1: When ChainActor is set, any recovered state from WAL
    /// is sent to ChainActor for proper integration with TendermintState.
    pub fn set_chain_actor(&mut self, addr: Addr<ChainActor>) {
        self.chain_actor = Some(addr.clone());

        // Issue 3.1: Apply pending recovery to ChainActor
        if let Some(recovered) = self.pending_recovery.take() {
            if recovered.last_committed_height.is_some() || recovered.current_round.is_some() {
                info!(
                    last_committed = ?recovered.last_committed_height,
                    current_round = ?recovered.current_round,
                    locked_round = ?recovered.locked_round,
                    prevotes = recovered.sent_prevotes.len(),
                    precommits = recovered.sent_precommits.len(),
                    "Sending WAL recovery to ChainActor"
                );

                use crate::actors_v2::chain::messages::ApplyRecoveredState;

                actix::spawn(async move {
                    match addr
                        .send(ApplyRecoveredState {
                            recovered,
                            correlation_id: Some(Uuid::new_v4()),
                        })
                        .await
                    {
                        Ok(Ok(response)) => {
                            if response.applied {
                                info!(
                                    height = response.height,
                                    round = response.round,
                                    lock_restored = response.lock_restored,
                                    prevotes = response.prevotes_restored,
                                    precommits = response.precommits_restored,
                                    "WAL recovery applied to ChainActor"
                                );
                            } else {
                                // WAL-Storage mismatch detected - consensus will NOT start
                                // until sync recovers the missing blocks
                                warn!(
                                    storage_height = response.height,
                                    "WAL recovery NOT applied due to WAL-Storage mismatch. \
                                     Consensus paused until sync recovers missing blocks. \
                                     Node will resume consensus automatically after sync completion."
                                );
                            }
                        }
                        Ok(Err(e)) => {
                            error!(error = ?e, "Failed to apply WAL recovery to ChainActor");
                        }
                        Err(e) => {
                            error!(error = %e, "ChainActor mailbox error during WAL recovery");
                        }
                    }
                });
            }
        }
    }

    /// Write an entry to the WAL
    ///
    /// Returns Ok(()) if WAL is disabled or write succeeds.
    /// Returns Err only if WAL is enabled and write fails.
    fn wal_write(&self, entry: WALEntry) -> Result<(), DriverError> {
        if let Some(ref wal) = self.wal {
            let mut wal_guard = wal.write().map_err(|e| {
                DriverError::WalWriteFailed(format!("WAL lock poisoned: {:?}", e))
            })?;

            wal_guard.write(entry).map_err(|e| {
                DriverError::WalWriteFailed(format!("WAL write failed: {:?}", e))
            })?;
        }
        Ok(())
    }

    // Issue 3.2: Removed vote tracking methods - now handled by ChainActor.tendermint_state
    // - has_sent_prevote, has_sent_precommit: now checked in TendermintState
    // - record_sent_prevote, record_sent_precommit: now recorded in TendermintState
    // - clear_vote_tracking: handled by TendermintState.new_height()

    /// Check if running in observer mode
    pub fn is_observer(&self) -> bool {
        self.config.mode == NodeMode::Observer
    }

    /// Check if we are a validator
    pub fn is_validator(&self) -> bool {
        self.config.mode == NodeMode::Validator && self.validator_id.is_some()
    }

    /// Get proposer for a given height and round
    ///
    /// Proposer selection: (height + round) % num_validators
    /// This ensures:
    /// - Different proposer each height (fair distribution)
    /// - Round advancement rotates proposer (liveness)
    pub fn get_proposer(&self, height: u64, round: u32) -> ValidatorId {
        let validator_set = self.get_validator_set_for_height(height);
        // Use ValidatorSet's get_proposer method which implements the same formula
        validator_set.get_proposer(height, round)
    }

    /// Check if we are the proposer for this height/round
    pub fn is_proposer(&self, height: u64, round: u32) -> bool {
        match &self.validator_id {
            Some(our_id) => self.get_proposer(height, round) == *our_id,
            None => false, // Observers never propose
        }
    }

    /// Get validator set for a specific height, accounting for pending updates
    ///
    /// Returns the validator set with the highest activation height that is <= the queried height.
    /// BTreeMap ensures we iterate in sorted order, so we can use `range()` for efficiency.
    fn get_validator_set_for_height(&self, height: u64) -> Arc<ValidatorSet> {
        // Find the most recent pending update that should be active at this height
        // BTreeMap's range gives us entries in sorted order, so we take the last one <= height
        if let Some((_, pending_set)) = self
            .pending_validator_updates
            .range(..=height)
            .next_back()
        {
            return pending_set.clone();
        }

        // No pending updates apply - use the current validator set
        self.validator_set.clone()
    }

    /// Check if driver should process consensus messages
    fn should_process_consensus(&self) -> bool {
        !self.is_paused
    }

    /// Schedule timeout for current step
    fn schedule_timeout(&mut self, ctx: &mut Context<Self>) {
        // Cancel any pending timeout
        if let Some(handle) = self.pending_timeout.take() {
            ctx.cancel_future(handle);
        }

        let height = self.current_height;
        let round = self.current_round;
        let step = self.current_step;

        // No timeout for commit step
        if step == TendermintStep::Commit {
            return;
        }

        // Calculate timeout based on step and round using TimeoutConfig
        let timeout = self.timeout_config.timeout_for(step, round);

        debug!(
            height = height,
            round = round,
            step = ?step,
            timeout_ms = timeout.as_millis(),
            "Scheduling timeout"
        );

        // Schedule timeout message
        let handle = ctx.notify_later(
            TendermintDriverMessage::Timeout { height, round, step },
            timeout,
        );
        self.pending_timeout = Some(handle);
    }

    /// Start consensus for a new height
    fn start_height(&mut self, height: u64, ctx: &mut Context<Self>) {
        info!(height = height, "Starting consensus for new height");

        self.current_height = height;
        self.current_round = 0;
        self.current_step = TendermintStep::Propose;

        // Issue 3.2: Removed clear_lock_state() and clear_vote_tracking()
        // Lock state and vote tracking are now managed by ChainActor.tendermint_state
        // ChainActor.handle_tendermint_new_height() calls TendermintState.new_height()
        // which handles clearing locks and vote tracking

        // Write NewRound to WAL BEFORE any actions
        if let Err(e) = self.wal_write(WALEntry::NewRound { height, round: 0 }) {
            error!(error = ?e, "Failed to write NewRound to WAL");
            // Continue anyway - safety is compromised but liveness preserved
        }

        // Apply any pending validator set updates for this height
        self.apply_pending_validator_updates(height);

        // CRITICAL FIX: Notify ChainActor of new height BEFORE scheduling timeout
        // This ensures ChainActor's tendermint_state is synchronized with Driver's position.
        // Without this, ChainActor rejects timeouts as "stale" because its height doesn't match.
        if let Some(ref chain_actor) = self.chain_actor {
            chain_actor.do_send(ChainMessage::TendermintNewHeight {
                height,
                correlation_id: Some(Uuid::new_v4()),
            });
        }

        // If we're the proposer, trigger proposal
        if self.is_proposer(height, 0) {
            self.trigger_propose(height, 0, ctx);
        }

        // Schedule propose timeout
        self.schedule_timeout(ctx);
    }

    /// Advance to next round within same height
    fn advance_round(&mut self, height: u64, round: u32, ctx: &mut Context<Self>) {
        info!(
            height = height,
            round = round,
            "Advancing to round {}",
            round
        );

        self.current_round = round;
        self.current_step = TendermintStep::Propose;

        // Write NewRound to WAL BEFORE any actions
        if let Err(e) = self.wal_write(WALEntry::NewRound { height, round }) {
            error!(error = ?e, "Failed to write NewRound to WAL");
        }

        // If we're the proposer for this round, trigger proposal
        if self.is_proposer(height, round) {
            self.trigger_propose(height, round, ctx);
        }

        // Schedule propose timeout
        self.schedule_timeout(ctx);
    }

    /// Handle timeout expiration
    fn handle_timeout(
        &mut self,
        height: u64,
        round: u32,
        step: TendermintStep,
        ctx: &mut Context<Self>,
    ) {
        // Verify timeout is still relevant
        if self.current_height != height || self.current_round != round {
            trace!(
                "Ignoring stale timeout for h={} r={} s={:?}",
                height,
                round,
                step
            );
            return;
        }

        if self.current_step != step {
            trace!("Ignoring stale timeout - step changed");
            return;
        }

        warn!(
            height = height,
            round = round,
            step = ?step,
            "Timeout expired"
        );

        // Take action based on step
        match step {
            TendermintStep::Propose => {
                // Proposer failed - send NIL prevote
                self.current_step = TendermintStep::Prevote;
                self.send_nil_prevote(height, round, ctx);
            }
            TendermintStep::Prevote => {
                // Didn't get 2/3+ prevotes - send NIL precommit
                self.current_step = TendermintStep::Precommit;
                self.send_nil_precommit(height, round, ctx);
            }
            TendermintStep::Precommit => {
                // Didn't get 2/3+ precommits - advance round
                self.advance_round(height, round + 1, ctx);
            }
            TendermintStep::Commit => {
                // No timeout for commit state
            }
        }
    }

    /// Called when block is committed
    fn on_commit(&mut self, height: u64, last_commit: Commit, ctx: &mut Context<Self>) {
        info!(height = height, "Block committed, advancing to next height");

        // Write Commit to WAL BEFORE any state changes
        if let Err(e) = self.wal_write(WALEntry::Commit {
            height,
            block_hash: last_commit.block_hash,
        }) {
            error!(error = ?e, "Failed to write Commit to WAL");
        }

        // Cancel pending timeouts
        if let Some(handle) = self.pending_timeout.take() {
            ctx.cancel_future(handle);
        }

        // Store last commit for next proposal
        self.last_commit = Some(last_commit);

        // Truncate old WAL entries periodically (every 10 heights)
        if height % 10 == 0 {
            if let Some(ref wal) = self.wal {
                if let Ok(mut wal_guard) = wal.write() {
                    if let Err(e) = wal_guard.truncate_before(height.saturating_sub(5)) {
                        warn!(error = ?e, "WAL truncation failed");
                    }
                }
            }
        }

        // Start next height immediately (event-driven, no waiting)
        self.start_height(height + 1, ctx);
    }

    /// Trigger proposal creation (we are the proposer)
    fn trigger_propose(&self, height: u64, round: u32, ctx: &mut Context<Self>) {
        if !self.is_validator() {
            return; // Observers don't propose
        }

        info!(
            height = height,
            round = round,
            "We are proposer - triggering block proposal"
        );

        let chain_actor = match &self.chain_actor {
            Some(addr) => addr.clone(),
            None => {
                error!("ChainActor not set - cannot propose");
                return;
            }
        };

        // Spawn async task to request block production
        ctx.spawn(
            async move {
                let result = chain_actor
                    .send(ChainMessage::TendermintPropose {
                        height,
                        round,
                        correlation_id: Some(uuid::Uuid::new_v4()),
                    })
                    .await;

                match result {
                    Ok(Ok(_)) => {
                        debug!(height = height, round = round, "Proposal created");
                    }
                    Ok(Err(e)) => {
                        error!(
                            height = height,
                            round = round,
                            error = ?e,
                            "Failed to create proposal"
                        );
                    }
                    Err(e) => {
                        error!(
                            height = height,
                            round = round,
                            error = %e,
                            "ChainActor mailbox error"
                        );
                    }
                }
            }
            .into_actor(self),
        );
    }

    /// Send NIL prevote (proposal timeout or invalid proposal)
    ///
    /// Issue 3.2: Removed local vote tracking - now handled by ChainActor.
    /// ChainActor.handle_tendermint_timeout() will check TendermintState.has_voted_prevote()
    /// before casting the vote, and will write to WAL before broadcasting.
    fn send_nil_prevote(&mut self, height: u64, round: u32, ctx: &mut Context<Self>) {
        if !self.is_validator() {
            return; // Observers don't vote
        }

        debug!(height = height, round = round, "Triggering NIL prevote via timeout");

        // Issue 3.2: Removed local vote tracking and WAL write
        // ChainActor handles double-vote prevention via TendermintState.has_voted_prevote()
        // ChainActor writes to WAL before broadcasting vote

        let chain_actor = match &self.chain_actor {
            Some(addr) => addr.clone(),
            None => return,
        };

        ctx.spawn(
            async move {
                let _ = chain_actor
                    .send(ChainMessage::TendermintTimeout {
                        height,
                        round,
                        step: TendermintStep::Propose,
                        correlation_id: Some(uuid::Uuid::new_v4()),
                    })
                    .await;
            }
            .into_actor(self),
        );
    }

    /// Send NIL precommit (prevote timeout)
    ///
    /// Issue 3.2: Removed local vote tracking - now handled by ChainActor.
    /// ChainActor.handle_tendermint_timeout() will check TendermintState.has_voted_precommit()
    /// before casting the vote, and will write to WAL before broadcasting.
    fn send_nil_precommit(&mut self, height: u64, round: u32, ctx: &mut Context<Self>) {
        if !self.is_validator() {
            return; // Observers don't vote
        }

        debug!(height = height, round = round, "Triggering NIL precommit via timeout");

        // Issue 3.2: Removed local vote tracking and WAL write
        // ChainActor handles double-vote prevention via TendermintState.has_voted_precommit()
        // ChainActor writes to WAL before broadcasting vote

        let chain_actor = match &self.chain_actor {
            Some(addr) => addr.clone(),
            None => return,
        };

        ctx.spawn(
            async move {
                let _ = chain_actor
                    .send(ChainMessage::TendermintTimeout {
                        height,
                        round,
                        step: TendermintStep::Prevote,
                        correlation_id: Some(uuid::Uuid::new_v4()),
                    })
                    .await;
            }
            .into_actor(self),
        );
    }

    // Issue 3.2: Removed duplicate consensus state methods:
    // - update_lock(): Now handled by ChainActor.tendermint_state.lock_on()
    // - can_vote_for(): Now handled by ChainActor.tendermint_state.determine_prevote_target()
    // - get_proposal_value(): Now handled by ChainActor.handle_tendermint_propose()
    // - clear_lock_state(): Now handled by ChainActor.tendermint_state.new_height()

    /// Store the LastCommit for the next block proposal
    fn store_last_commit(&mut self, last_commit: Commit) {
        debug!(
            height = last_commit.height,
            signatures = last_commit.signatures.len(),
            "Storing LastCommit for next proposal"
        );
        self.last_commit = Some(last_commit);
    }

    /// Get LastCommit for including in the next proposal
    ///
    /// Returns None for genesis (height 1) since there's no prior block.
    pub fn get_last_commit_for_proposal(&self, height: u64) -> Result<Option<Commit>, DriverError> {
        if height == 1 {
            // Genesis block has no prior commit
            return Ok(None);
        }

        match &self.last_commit {
            Some(lc) if lc.height == height - 1 => Ok(Some(lc.clone())),
            Some(lc) => {
                warn!(
                    expected = height - 1,
                    actual = lc.height,
                    "LastCommit height mismatch"
                );
                Err(DriverError::MissingLastCommit(height - 1))
            }
            None => {
                error!(height = height - 1, "Missing LastCommit");
                Err(DriverError::MissingLastCommit(height - 1))
            }
        }
    }

    /// Update validator set from governance
    ///
    /// Validator set changes take effect at activation_height.
    /// This is typically H+2 where H is the height where governance
    /// approved the change.
    fn update_validator_set(&mut self, new_set: Arc<ValidatorSet>, activation_height: u64) {
        if activation_height <= self.current_height {
            // Immediate activation
            info!(
                activation_height = activation_height,
                new_validators = new_set.len(),
                "Activating new validator set immediately"
            );
            self.validator_set = new_set;
            self.check_validator_status();
        } else {
            // Schedule for future activation
            info!(
                activation_height = activation_height,
                current_height = self.current_height,
                new_validators = new_set.len(),
                "Scheduling validator set update for height {}",
                activation_height
            );
            self.pending_validator_updates.insert(activation_height, new_set);
        }
    }

    /// Apply any pending validator set updates for the given height
    fn apply_pending_validator_updates(&mut self, height: u64) {
        if let Some(new_set) = self.pending_validator_updates.remove(&height) {
            info!(
                height = height,
                validators = new_set.len(),
                "Activating pending validator set update"
            );
            self.validator_set = new_set;
            self.check_validator_status();
        }
    }

    /// Check if we're still a validator after set change
    ///
    /// Uses public key matching instead of index bounds checking.
    /// This properly handles validator set changes where indices may shift.
    fn check_validator_status(&mut self) {
        if let Some(our_pubkey) = &self.validator_pubkey {
            // Look up our public key in the new validator set
            match self.validator_set.find_validator(our_pubkey) {
                Some(new_id) => {
                    // Still in the set, update our validator ID (index may have changed)
                    if self.validator_id != Some(new_id) {
                        info!(
                            old_id = ?self.validator_id,
                            new_id = ?new_id,
                            "Validator index changed after set update"
                        );
                        self.validator_id = Some(new_id);
                    }
                }
                None => {
                    // No longer in the validator set
                    warn!(
                        pubkey = ?our_pubkey,
                        "We are no longer in the validator set - switching to observer mode"
                    );
                    self.validator_id = None;
                    self.config.mode = NodeMode::Observer;
                }
            }
        }
    }

    /// Pause consensus during sync
    fn pause_consensus(&mut self, ctx: &mut Context<Self>) {
        if self.is_paused {
            return;
        }

        info!("Pausing consensus for sync");
        self.is_paused = true;

        // Cancel pending timeout
        if let Some(handle) = self.pending_timeout.take() {
            ctx.cancel_future(handle);
        }
    }

    /// Resume consensus after sync completion
    fn resume_consensus(&mut self, height: u64, ctx: &mut Context<Self>) {
        if !self.is_paused {
            warn!("Resume called but not paused");
            return;
        }

        info!(height = height, "Resuming consensus after sync");
        self.is_paused = false;

        // Issue 3.2: Removed clear_lock_state() - now handled by ChainActor.tendermint_state
        // when TendermintState.new_height() is called

        // Start fresh at the new height
        self.start_height(height, ctx);
    }

    /// Graceful shutdown
    fn graceful_shutdown(&mut self, ctx: &mut Context<Self>) {
        info!("Initiating graceful shutdown");

        // Cancel pending timeout
        if let Some(handle) = self.pending_timeout.take() {
            ctx.cancel_future(handle);
        }

        ctx.stop();
    }
}

impl Actor for TendermintDriver {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        let mode = if self.is_validator() {
            "validator"
        } else {
            "observer"
        };

        info!(
            mode = mode,
            validators = self.validator_set.len(),
            "TendermintDriver started"
        );

        // Consensus startup is handled explicitly by app.rs after all actor wiring is complete.
        // This avoids duplicate startup race conditions where both app.rs and this callback
        // would send NewHeight messages, causing concurrent proposal builds that interfere
        // with each other via the Engine API.
    }

    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        // Cancel pending timeouts
        if let Some(handle) = self.pending_timeout.take() {
            ctx.cancel_future(handle);
        }

        info!("TendermintDriver stopping");
        Running::Stop
    }
}

impl Handler<TendermintDriverMessage> for TendermintDriver {
    type Result = ();

    fn handle(&mut self, msg: TendermintDriverMessage, ctx: &mut Context<Self>) {
        match msg {
            TendermintDriverMessage::NewHeight { height } => {
                // Idempotency guard: only start consensus for heights we haven't reached yet.
                // This prevents duplicate startup if multiple NewHeight messages arrive for
                // the same height (e.g., from recovery or redundant startup paths).
                if self.should_process_consensus() && height > self.current_height {
                    self.start_height(height, ctx);
                } else if height <= self.current_height {
                    debug!(
                        current_height = self.current_height,
                        requested_height = height,
                        "Ignoring NewHeight for already-reached height"
                    );
                }
            }

            TendermintDriverMessage::Timeout {
                height,
                round,
                step,
            } => {
                if self.should_process_consensus() {
                    self.handle_timeout(height, round, step, ctx);
                }
            }

            TendermintDriverMessage::NextRound { height, round } => {
                if self.should_process_consensus() {
                    self.advance_round(height, round, ctx);
                }
            }

            TendermintDriverMessage::Committed { height, last_commit } => {
                // Store LastCommit for next proposal
                self.store_last_commit(last_commit.clone());

                if self.should_process_consensus() {
                    self.on_commit(height, last_commit, ctx);
                }
            }

            TendermintDriverMessage::Pause => {
                self.pause_consensus(ctx);
            }

            TendermintDriverMessage::Resume { height } => {
                // TM-B5 Fix: Clear any stale pending timeouts before resuming
                // This prevents old timeouts from firing after sync completion
                if let Some(handle) = self.pending_timeout.take() {
                    ctx.cancel_future(handle);
                    debug!(
                        "Cleared stale pending timeout before resuming consensus"
                    );
                }

                // Guard against stale sync completion resetting consensus backward.
                // This can happen when sync takes long and consensus has already progressed.
                if height > self.current_height {
                    info!(
                        current_height = self.current_height,
                        resume_height = height,
                        "Resuming consensus at new height after sync"
                    );
                    self.resume_consensus(height, ctx);
                } else if self.is_paused {
                    // Same or lower height but we're paused - just unpause
                    info!(
                        current_height = self.current_height,
                        requested_height = height,
                        "Unpausing consensus at current height"
                    );
                    self.is_paused = false;
                    // Re-schedule timeout for current position
                    self.schedule_timeout(ctx);
                } else {
                    trace!(
                        current_height = self.current_height,
                        requested_height = height,
                        "Resume ignored - already running at higher height"
                    );
                }
            }

            TendermintDriverMessage::UpdateValidatorSet {
                validator_set,
                activation_height,
            } => {
                self.update_validator_set(validator_set, activation_height);
            }

            TendermintDriverMessage::RoundAdvanced { round, step } => {
                // Round was advanced via future round votes in ChainActor
                // Cancel any pending timeout and update our tracking
                info!(
                    round = round,
                    step = ?step,
                    "Round advanced via future round votes"
                );

                // Cancel pending timeout for old round
                if let Some(handle) = self.pending_timeout.take() {
                    ctx.cancel_future(handle);
                }

                // Update current position
                self.current_round = round;
                self.current_step = step;

                // Schedule timeout for the new step
                self.schedule_timeout(ctx);
            }

            TendermintDriverMessage::BlockRequestTimeout {
                block_hash,
                correlation_id,
            } => {
                // Forward to ChainActor for retry handling
                debug!(
                    block_hash = %block_hash,
                    correlation_id = %correlation_id,
                    "Block request timeout - forwarding to ChainActor"
                );

                if let Some(ref chain_actor) = self.chain_actor {
                    let chain_actor = chain_actor.clone();
                    ctx.spawn(
                        async move {
                            let _ = chain_actor
                                .send(
                                    crate::actors_v2::chain::messages::ChainMessage::TendermintBlockRequestTimeout {
                                        block_hash,
                                        correlation_id,
                                    },
                                )
                                .await;
                        }
                        .into_actor(self),
                    );
                }
            }

            TendermintDriverMessage::Stop => {
                self.graceful_shutdown(ctx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lighthouse_wrapper::bls::PublicKey;
    use std::str::FromStr;

    fn create_mock_pubkey() -> PublicKey {
        // Use a known valid BLS public key for testing
        PublicKey::from_str(
            "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb"
        ).expect("valid test public key")
    }

    fn create_test_validator_set(count: usize) -> Arc<ValidatorSet> {
        let validators = vec![create_mock_pubkey(); count];
        Arc::new(ValidatorSet::with_equal_power(validators))
    }

    fn test_config() -> TendermintDriverConfig {
        TendermintDriverConfig {
            wal_enabled: false, // Disable WAL for unit tests
            ..Default::default()
        }
    }

    #[test]
    fn test_proposer_selection_round_robin() {
        let validator_set = create_test_validator_set(4);
        let config = test_config();
        // Pass pubkey, validator_id is derived from it
        let pubkey = create_mock_pubkey();
        let driver = TendermintDriver::new_without_wal(config, Some(pubkey), validator_set);

        // Height 0: proposer = (0 + 0) % 4 = 0
        assert_eq!(driver.get_proposer(0, 0), ValidatorId::new(0));

        // Height 1: proposer = (1 + 0) % 4 = 1
        assert_eq!(driver.get_proposer(1, 0), ValidatorId::new(1));

        // Height 0, Round 1: proposer = (0 + 1) % 4 = 1
        assert_eq!(driver.get_proposer(0, 1), ValidatorId::new(1));

        // Height 3, Round 2: proposer = (3 + 2) % 4 = 1
        assert_eq!(driver.get_proposer(3, 2), ValidatorId::new(1));
    }

    #[test]
    fn test_is_proposer_validator() {
        let validator_set = create_test_validator_set(4);
        let mut config = test_config();
        config.mode = NodeMode::Validator;
        // Pass pubkey, validator_id is derived (will be 0 since all are same pubkey)
        let pubkey = create_mock_pubkey();
        let driver = TendermintDriver::new_without_wal(config, Some(pubkey), validator_set);

        // We are validator 0 (first match), should be proposer at height 0
        assert!(driver.is_proposer(0, 0));
        // Not proposer at height 1
        assert!(!driver.is_proposer(1, 0));
    }

    #[test]
    fn test_is_proposer_observer() {
        let validator_set = create_test_validator_set(4);
        let config = test_config(); // Observer by default
        let driver = TendermintDriver::new_without_wal(config, None, validator_set);

        // Observers never propose
        assert!(!driver.is_proposer(0, 0));
        assert!(!driver.is_proposer(1, 0));
    }

    // Issue 3.2: Removed test_lock_state and test_clear_lock_state
    // These tests were for duplicate state that's now only in ChainActor.tendermint_state
    // The equivalent tests exist in state_machine.rs

    #[test]
    fn test_last_commit_for_proposal() {
        let validator_set = create_test_validator_set(4);
        let config = test_config();
        let pubkey = create_mock_pubkey();
        let mut driver = TendermintDriver::new_without_wal(config, Some(pubkey), validator_set);

        // Height 1 (genesis) doesn't need last_commit
        assert!(driver.get_last_commit_for_proposal(1).unwrap().is_none());

        // Height 2 without last_commit should error
        assert!(driver.get_last_commit_for_proposal(2).is_err());

        // Store commit for height 1
        let commit = Commit::new(1, 0, BlockHash::from_low_u64_be(1), vec![]);
        driver.store_last_commit(commit);

        // Now height 2 should work
        let result = driver.get_last_commit_for_proposal(2);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    // Issue 3.2: Removed test_vote_tracking
    // Vote tracking is now in ChainActor.tendermint_state (TendermintState.sent_prevotes/sent_precommits)
    // The equivalent tests exist in state_machine.rs

    #[test]
    fn test_check_validator_status_pubkey_matching() {
        // Create a validator set with distinct pubkeys for this test
        let pubkey = create_mock_pubkey();
        let validator_set = Arc::new(ValidatorSet::with_equal_power(vec![pubkey.clone()]));

        let mut config = test_config();
        config.mode = NodeMode::Validator;
        let mut driver = TendermintDriver::new_without_wal(config, Some(pubkey.clone()), validator_set);

        // Initially we should be validator 0
        assert_eq!(driver.validator_id, Some(ValidatorId::new(0)));
        assert_eq!(driver.config.mode, NodeMode::Validator);

        // Simulate validator set change that removes us
        driver.validator_set = Arc::new(ValidatorSet::with_equal_power(vec![]));
        driver.check_validator_status();

        // Should now be in observer mode
        assert_eq!(driver.validator_id, None);
        assert_eq!(driver.config.mode, NodeMode::Observer);

        // Restore us to the set
        driver.validator_set = Arc::new(ValidatorSet::with_equal_power(vec![pubkey.clone()]));
        driver.config.mode = NodeMode::Validator; // Re-enable validator mode
        driver.check_validator_status();

        // Should be validator again
        assert_eq!(driver.validator_id, Some(ValidatorId::new(0)));
    }

    #[test]
    fn test_get_validator_set_for_height_selects_highest() {
        // Create validator sets with different sizes for easy identification
        let base_set = create_test_validator_set(4);      // 4 validators
        let set_at_100 = create_test_validator_set(5);    // 5 validators
        let set_at_200 = create_test_validator_set(6);    // 6 validators
        let set_at_300 = create_test_validator_set(7);    // 7 validators

        let config = test_config();
        let pubkey = create_mock_pubkey();
        let mut driver = TendermintDriver::new_without_wal(config, Some(pubkey), base_set);

        // Add pending updates at heights 100, 200, 300
        driver.pending_validator_updates.insert(100, set_at_100);
        driver.pending_validator_updates.insert(200, set_at_200);
        driver.pending_validator_updates.insert(300, set_at_300);

        // Height 50: should use base set (no pending updates apply)
        assert_eq!(driver.get_validator_set_for_height(50).len(), 4);

        // Height 100: should use set_at_100 (exactly at activation)
        assert_eq!(driver.get_validator_set_for_height(100).len(), 5);

        // Height 150: should use set_at_100 (highest <= 150)
        assert_eq!(driver.get_validator_set_for_height(150).len(), 5);

        // Height 200: should use set_at_200 (exactly at activation)
        assert_eq!(driver.get_validator_set_for_height(200).len(), 6);

        // Height 250: should use set_at_200 (highest <= 250)
        assert_eq!(driver.get_validator_set_for_height(250).len(), 6);

        // Height 300: should use set_at_300 (exactly at activation)
        assert_eq!(driver.get_validator_set_for_height(300).len(), 7);

        // Height 1000: should use set_at_300 (highest overall)
        assert_eq!(driver.get_validator_set_for_height(1000).len(), 7);
    }
}
