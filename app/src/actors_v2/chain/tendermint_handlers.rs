//! Tendermint consensus handler implementations for ChainActor - V2
//!
//! This module contains the actual logic for processing Tendermint consensus messages.
//! The handlers in handlers.rs dispatch to these implementations.
//!
//! # Consensus Flow
//!
//! ```text
//! NewHeight(H) → [Initialize] → If proposer: Propose → Schedule timeout
//!                                     ↓
//! Proposal received → Validate → Cast prevote
//!                                     ↓
//! Votes received → Add to VoteSet → Check 2/3+ threshold
//!                                     ↓
//! 2/3+ prevotes → Lock on block → Cast precommit
//!                                     ↓
//! 2/3+ precommits → COMMIT → Store block → NewHeight(H+1)
//!                                     ↓
//! Timeout → Advance step/round → Possibly cast nil vote
//! ```

use crate::actors_v2::chain::{ChainActor, ChainError};
use crate::actors_v2::storage::messages::{
    GetValidatorSetForHeightMessage, StoreValidatorSetMessage,
};
use ethereum_types::H256;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::tendermint::{
    check_for_equivocation, verify_future_proposal, verify_proposal, verify_vote, BlockHash,
    Commit, CommitSig, ConsensusAction, EquivocationEvidence, EquivocationType, FutureRoundAction,
    GovernanceUpdate, PendingCommit, Proposal, TendermintMessage, TendermintStep,
    TendermintValidationError, ValidatorId, Vote, VoteSet, VoteType, WALEntry,
};
use std::cmp::Ordering;

// Type alias for WAL entry BlockHash (H256)
type WALBlockHash = ethereum_types::H256;

/// Result type for Tendermint handler operations
pub type TendermintResult<T> = Result<T, ChainError>;

impl ChainActor {
    // =========================================================================
    // TendermintNewHeight Handler Implementation
    // =========================================================================

    /// Handle the start of a new consensus height.
    ///
    /// This is called when:
    /// 1. Node starts up and initializes Tendermint
    /// 2. A block is committed and we advance to the next height
    ///
    /// # Actions
    /// - Load validator set for this height from storage
    /// - Initialize TendermintState for the new height
    /// - Determine if we are the proposer for round 0
    /// - Schedule propose timeout
    /// - If proposer: trigger TendermintPropose
    pub async fn handle_tendermint_new_height(
        &self,
        height: u64,
        correlation_id: Uuid,
    ) -> TendermintResult<(u64, u32)> {
        info!(
            correlation_id = %correlation_id,
            height = height,
            "Starting new Tendermint height"
        );

        // Get required components
        let tendermint_state = self
            .tendermint_state
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Tendermint state not initialized".into()))?;

        let timeout_scheduler = self
            .timeout_scheduler
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Timeout scheduler not initialized".into()))?;

        let wal = self
            .consensus_wal
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("WAL not initialized".into()))?;

        // CRITICAL FIX: Skip re-initialization if already at this height
        // This prevents a race condition where:
        // 1. handle_tendermint_commit() calls handle_tendermint_new_height(H+1)
        // 2. Then notifies TendermintDriver of the commit
        // 3. TendermintDriver sends ANOTHER TendermintNewHeight message
        // Without this guard, the second call clears vote sets that may have already
        // received votes from faster nodes, causing consensus failures.
        {
            let state = tendermint_state.read().await;
            if state.height == height {
                debug!(
                    correlation_id = %correlation_id,
                    height = height,
                    current_round = state.round,
                    "Height already initialized, skipping duplicate initialization"
                );
                return Ok((height, state.round));
            }
        }

        // Load validator set from storage
        let validator_set = self.load_validator_set_for_height(height).await?;
        let validator_set = Arc::new(validator_set);

        // Determine our validator ID
        let our_validator_id = self.validator_keypair.as_ref().and_then(|kp| {
            validator_set.find_validator(&kp.pk)
        });

        // Initialize state for new height
        {
            let mut state = tendermint_state.write().await;
            state.new_height(height, validator_set.clone());
            state.our_validator_id = our_validator_id;
        }

        // Write WAL entry BEFORE any actions
        {
            let mut wal_guard = wal.write().await;
            wal_guard
                .write(WALEntry::NewRound { height, round: 0 })
                .map_err(|e| ChainError::Internal(format!("WAL write failed: {}", e)))?;
        }

        // Check if we are the proposer for round 0
        let is_proposer = {
            let state = tendermint_state.read().await;
            state.is_proposer()
        };

        // Schedule propose timeout (Issue 3.3: set position before scheduling)
        {
            let mut scheduler = timeout_scheduler.write().await;
            scheduler.set_position(height, 0);
            let _ = scheduler.schedule(TendermintStep::Propose);
        }

        info!(
            correlation_id = %correlation_id,
            height = height,
            round = 0,
            is_proposer = is_proposer,
            our_validator_id = ?our_validator_id,
            "Tendermint height initialized"
        );

        Ok((height, 0))
    }

    // =========================================================================
    // TendermintPropose Handler Implementation
    // =========================================================================

    /// Handle a request to create and broadcast a proposal.
    ///
    /// Called when we are the designated proposer for (height, round).
    ///
    /// # Actions
    /// - Build execution payload via EngineActor
    /// - Create block with last_commit from previous height
    /// - Sign proposal
    /// - Write WAL entry
    /// - Broadcast proposal via NetworkActor
    /// - Cast our own prevote for the block
    pub async fn handle_tendermint_propose(
        &self,
        height: u64,
        round: u32,
        correlation_id: Uuid,
    ) -> TendermintResult<H256> {
        info!(
            correlation_id = %correlation_id,
            height = height,
            round = round,
            "Creating Tendermint proposal"
        );

        let tendermint_state = self
            .tendermint_state
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Tendermint state not initialized".into()))?;

        let wal = self
            .consensus_wal
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("WAL not initialized".into()))?;

        let timeout_scheduler = self
            .timeout_scheduler
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Timeout scheduler not initialized".into()))?;

        let keypair = self
            .validator_keypair
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("No validator keypair configured".into()))?;

        // Verify we are the proposer
        let (is_proposer, locked_block, locked_round) = {
            let state = tendermint_state.read().await;
            (state.is_proposer(), state.locked_block, state.locked_round)
        };

        if !is_proposer {
            return Err(ChainError::Internal(format!(
                "Not the proposer for height {} round {}",
                height, round
            )));
        }

        // Determine what to propose based on locking rules
        let (block, pol_round) = if let (Some(locked_hash), Some(lr)) = (locked_block, locked_round)
        {
            // We're locked - must propose the locked block (Issue 2.1 fix)
            info!(
                correlation_id = %correlation_id,
                locked_hash = %locked_hash,
                locked_round = lr,
                "Proposing locked block"
            );

            // Try to get the locked block content from state
            let locked_block_data = {
                let state = tendermint_state.read().await;
                state.locked_block_data.clone()
            };

            match locked_block_data {
                Some(block) => {
                    info!(
                        correlation_id = %correlation_id,
                        locked_hash = %locked_hash,
                        locked_round = lr,
                        "Found locked block data - re-proposing"
                    );
                    (block, Some(lr))
                }
                None => {
                    // Fallback: try to find in proposals HashMap
                    let state = tendermint_state.read().await;
                    let found_block = state
                        .proposals
                        .values()
                        .find(|p| p.block_hash() == locked_hash)
                        .map(|p| p.block.clone());

                    match found_block {
                        Some(block) => {
                            info!(
                                correlation_id = %correlation_id,
                                locked_hash = %locked_hash,
                                locked_round = lr,
                                "Found locked block in proposals cache - re-proposing"
                            );
                            (block, Some(lr))
                        }
                        None => {
                            return Err(ChainError::Internal(format!(
                                "Locked on block {} at round {} but block data not found",
                                locked_hash, lr
                            )));
                        }
                    }
                }
            }
        } else {
            // Not locked - build a fresh block via EngineActor
            let block = self.build_proposal_block(height, correlation_id).await?;
            (block, None)
        };

        // Get our validator ID
        let proposer = {
            let state = tendermint_state.read().await;
            state
                .our_validator_id
                .ok_or_else(|| ChainError::Internal("No validator ID".into()))?
        };

        // Create and sign the proposal
        let proposal = Proposal {
            height,
            round,
            block,
            pol_round,
            proposer,
            signature: lighthouse_wrapper::bls::Signature::empty(),
        };

        // Sign the proposal (Issue 1.2: use chain_id for domain separation)
        let chain_id_str = self.config.chain_id.to_string();
        let signing_root = proposal.signing_root(&chain_id_str);
        let signature = keypair.sk.sign(signing_root);
        let proposal = Proposal { signature, ..proposal };

        let block_hash = proposal.block_hash();

        // Write WAL entry BEFORE broadcast
        {
            let mut wal_guard = wal.write().await;
            wal_guard
                .write(WALEntry::SentProposal {
                    height,
                    round,
                    block_hash,
                })
                .map_err(|e| ChainError::Internal(format!("WAL write failed: {}", e)))?;
        }

        // Store proposal in state
        {
            let mut state = tendermint_state.write().await;
            state.current_proposal = Some(proposal.clone());
            state
                .proposals
                .insert((round, proposer), proposal.clone());
        }

        // Broadcast proposal via NetworkActor
        self.broadcast_tendermint_proposal(proposal.clone()).await?;

        // Transition to Prevote step (must happen before casting prevote)
        {
            let mut state = tendermint_state.write().await;
            state.set_step(TendermintStep::Prevote);
        }

        // Schedule prevote timeout
        {
            let mut scheduler = timeout_scheduler.write().await;
            scheduler.set_position(height, round);
            let _ = scheduler.schedule(TendermintStep::Prevote);
        }

        // Cast our own prevote for the block
        self.cast_prevote(height, round, Some(block_hash), correlation_id)
            .await?;

        info!(
            correlation_id = %correlation_id,
            height = height,
            round = round,
            block_hash = %H256::from_slice(block_hash.as_bytes()),
            "Proposal created and broadcast"
        );

        Ok(H256::from_slice(block_hash.as_bytes()))
    }

    // =========================================================================
    // TendermintProposal Handler Implementation
    // =========================================================================

    /// Handle a received proposal from a peer.
    ///
    /// # Actions
    /// - Validate proposal (proposer, signature, height/round)
    /// - Validate block via EngineActor
    /// - Store proposal in state
    /// - Determine prevote target based on locking rules
    /// - Cast prevote
    /// - Schedule prevote timeout
    pub async fn handle_tendermint_proposal(
        &self,
        proposal: Proposal,
        peer_id: Option<String>,
        correlation_id: Uuid,
    ) -> TendermintResult<H256> {
        let height = proposal.height;
        let round = proposal.round;
        let proposer = proposal.proposer;
        let block_hash = proposal.block_hash();

        info!(
            correlation_id = %correlation_id,
            height = height,
            round = round,
            proposer = ?proposer,
            peer_id = ?peer_id,
            block_hash = %H256::from_slice(block_hash.as_bytes()),
            "Processing received proposal"
        );

        let tendermint_state = self
            .tendermint_state
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Tendermint state not initialized".into()))?;

        let timeout_scheduler = self
            .timeout_scheduler
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Timeout scheduler not initialized".into()))?;

        // Verify height and round match current state
        let (current_height, current_round, validator_set) = {
            let state = tendermint_state.read().await;
            (state.height, state.round, state.validator_set.clone())
        };

        let chain_id_str = self.config.chain_id.to_string();

        // Handle future round proposals - store for later replay
        if height == current_height && round > current_round {
            // Validate the proposal for the future round (proposer correct for THAT round, valid signature)
            verify_future_proposal(
                &proposal,
                &validator_set,
                current_height,
                current_round,
                &chain_id_str,
            )
            .map_err(|e| {
                ChainError::Consensus(format!("Future proposal validation failed: {}", e))
            })?;

            // Store in future messages for replay when we reach that round
            let stored = {
                let mut state = tendermint_state.write().await;
                state.future_messages.store_proposal(proposal.clone())
            };

            if stored {
                info!(
                    correlation_id = %correlation_id,
                    height = height,
                    round = round,
                    current_round = current_round,
                    proposer = ?proposer,
                    block_hash = %H256::from_slice(block_hash.as_bytes()),
                    "Stored future round proposal for later replay"
                );
            } else {
                debug!(
                    correlation_id = %correlation_id,
                    height = height,
                    round = round,
                    "Future proposal not stored (already have one for this round)"
                );
            }

            // Return early - we'll process this when we reach the round
            return Ok(H256::from_slice(block_hash.as_bytes()));
        }

        // Validate proposal for current round (Issue 1.2: pass chain_id for domain separation)
        verify_proposal(&proposal, &validator_set, current_height, current_round, &chain_id_str).map_err(
            |e| match e {
                TendermintValidationError::InvalidHeight { expected, actual } => {
                    ChainError::Consensus(format!(
                        "Proposal height mismatch: expected {}, got {}",
                        expected, actual
                    ))
                }
                TendermintValidationError::InvalidRound { expected, actual } => {
                    ChainError::Consensus(format!(
                        "Proposal round mismatch: expected {}, got {}",
                        expected, actual
                    ))
                }
                TendermintValidationError::WrongProposer { expected, actual, .. } => {
                    ChainError::Consensus(format!(
                        "Wrong proposer: expected {:?}, got {:?}",
                        expected, actual
                    ))
                }
                TendermintValidationError::InvalidSignature { validator, .. } => {
                    ChainError::Consensus(format!("Invalid proposal signature from {:?}", validator))
                }
                other => ChainError::Consensus(format!("Proposal validation failed: {}", other)),
            },
        )?;

        // Phase 4.3: Validate block execution payload via EngineActor
        // If validation fails, we vote NIL (not error out) - proper BFT behavior
        let execution_valid = self
            .validate_block_execution(&proposal.block, correlation_id)
            .await;

        if !execution_valid {
            warn!(
                correlation_id = %correlation_id,
                height = height,
                round = round,
                block_hash = %H256::from_slice(block_hash.as_bytes()),
                "Block execution validation failed - will vote NIL"
            );
        }

        // Store proposal in state and determine prevote target
        let prevote_target = {
            let mut state = tendermint_state.write().await;

            // Check for equivocation (same proposer, different block)
            if let Some(existing) = state.proposals.get(&(round, proposer)) {
                if existing.block_hash() != block_hash {
                    error!(
                        correlation_id = %correlation_id,
                        proposer = ?proposer,
                        existing_hash = %H256::from_slice(existing.block_hash().as_bytes()),
                        new_hash = %H256::from_slice(block_hash.as_bytes()),
                        "PROPOSAL EQUIVOCATION DETECTED: Validator {} proposed different blocks at height {} round {}",
                        proposer.0,
                        height,
                        round
                    );
                    // Note: Proposal equivocation evidence would require a different struct
                    // (EquivocationEvidence stores Votes, not Proposals). For now we log it
                    // prominently. Vote equivocation (prevote/precommit) is the more critical
                    // case for consensus safety and is fully implemented.
                }
            }

            state.current_proposal = Some(proposal.clone());
            state.proposals.insert((round, proposer), proposal.clone());

            // Determine prevote target based on locking rules
            // Phase 4.3: If execution validation failed, vote NIL regardless of locking
            if execution_valid {
                state.determine_prevote_target(&proposal)
            } else {
                // Invalid execution payload - vote NIL
                None
            }
        };

        // Advance step to Prevote
        {
            let mut state = tendermint_state.write().await;
            state.set_step(TendermintStep::Prevote);
        }

        // Schedule prevote timeout (Issue 3.3: set position before scheduling)
        {
            let mut scheduler = timeout_scheduler.write().await;
            scheduler.set_position(height, round);
            let _ = scheduler.schedule(TendermintStep::Prevote);
        }

        // Cast our prevote
        self.cast_prevote(height, round, prevote_target, correlation_id)
            .await?;

        Ok(H256::from_slice(block_hash.as_bytes()))
    }

    // =========================================================================
    // TendermintVote Handler Implementation
    // =========================================================================

    /// Handle a received vote (prevote or precommit) from a peer.
    ///
    /// # Actions
    /// - Validate vote (voter membership, signature, height/round)
    /// - Check for equivocation
    /// - Add to VoteSet
    /// - Check for 2/3+ threshold
    /// - If 2/3+ prevotes: lock on block, advance to precommit
    /// - If 2/3+ precommits: COMMIT the block
    pub async fn handle_tendermint_vote(
        &self,
        vote: Vote,
        peer_id: Option<String>,
        correlation_id: Uuid,
    ) -> TendermintResult<ValidatorId> {
        let height = vote.height;
        let round = vote.round;
        let voter = vote.validator;
        let vote_type = vote.vote_type;

        debug!(
            correlation_id = %correlation_id,
            height = height,
            round = round,
            voter = ?voter,
            vote_type = ?vote_type,
            block_hash = ?vote.block_hash,
            peer_id = ?peer_id,
            "Processing received vote"
        );

        let tendermint_state = self
            .tendermint_state
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Tendermint state not initialized".into()))?;

        let timeout_scheduler = self
            .timeout_scheduler
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Timeout scheduler not initialized".into()))?;

        // Get current state and validator set
        let (current_height, current_round, validator_set) = {
            let state = tendermint_state.read().await;
            (state.height, state.round, state.validator_set.clone())
        };

        // Handle votes based on height relationship to our current height
        if vote.height < current_height {
            // Past height vote - ignore silently (don't error, just skip processing)
            // This can happen when we've already committed a block that others are still voting on
            debug!(
                correlation_id = %correlation_id,
                vote_height = vote.height,
                current_height = current_height,
                "Ignoring vote for past height (already processed)"
            );
            return Ok(voter);
        }

        if vote.height > current_height {
            // Future height vote - this node may be behind
            // Use debounced sync triggering to catch up
            return self
                .handle_future_height_vote(vote.height, current_height, voter, correlation_id)
                .await;
        }

        // vote.height == current_height - continue with normal processing

        // Handle vote based on round relationship
        match vote.round.cmp(&current_round) {
            Ordering::Greater => {
                // Future round vote - validate signature and store for potential round advancement
                return self
                    .handle_future_round_vote(vote, current_round, &validator_set, correlation_id)
                    .await;
            }
            Ordering::Less => {
                // Past round vote - store for POL/evidence but don't process
                return self
                    .handle_past_round_vote(vote, current_round, correlation_id)
                    .await;
            }
            Ordering::Equal => {
                // Current round - continue with existing logic
            }
        }

        // Validate vote for current round (Issue 1.2: pass chain_id for domain separation)
        let chain_id_str = self.config.chain_id.to_string();
        verify_vote(&vote, &validator_set, current_height, current_round, &chain_id_str).map_err(|e| {
            ChainError::Consensus(format!("Vote validation failed: {}", e))
        })?;

        // Add vote to appropriate VoteSet and check thresholds
        // Capture any detected equivocation evidence to broadcast after releasing locks
        let mut detected_evidence: Option<EquivocationEvidence> = None;

        let action = {
            let mut state = tendermint_state.write().await;

            match vote_type {
                VoteType::Prevote => {
                    // Check for equivocation before adding
                    // Clone existing vote and release lock before mutating state
                    let existing_vote = {
                        let prevotes = state.prevotes.read().await;
                        prevotes.get_vote_by_validator(&voter).cloned()
                    };

                    if let Some(existing) = existing_vote {
                        if let Some(evidence) = check_for_equivocation(&vote, &[(voter, existing)].into_iter().collect()) {
                            warn!(
                                correlation_id = %correlation_id,
                                voter = ?voter,
                                "Detected prevote equivocation: {:?}",
                                evidence
                            );
                            // Store evidence in state for RPC queries and capture for broadcast
                            state.detected_evidence.push(evidence.clone());
                            detected_evidence = Some(evidence);
                        }
                    }

                    // Add vote to prevote set
                    let mut prevotes = state.prevotes.write().await;
                    if let Err(e) = prevotes.add_vote(vote.clone()) {
                        debug!(
                            correlation_id = %correlation_id,
                            error = %e,
                            "Failed to add prevote (may be duplicate)"
                        );
                    }

                    // Check for 2/3+ prevotes for a specific block
                    if let Some(block_hash) = prevotes.two_thirds_majority() {
                        info!(
                            correlation_id = %correlation_id,
                            height = height,
                            round = round,
                            block_hash = %H256::from_slice(block_hash.as_bytes()),
                            "2/3+ prevotes reached for block"
                        );

                        // Issue 2.1 FIX: Get the locked block content from current_proposal or proposals
                        let locked_block_content = state
                            .current_proposal
                            .as_ref()
                            .filter(|p| p.block_hash() == block_hash)
                            .map(|p| p.block.clone())
                            .or_else(|| {
                                // Fallback: search proposals HashMap
                                state
                                    .proposals
                                    .values()
                                    .find(|p| p.block_hash() == block_hash)
                                    .map(|p| p.block.clone())
                            });

                        // Lock on the block WITH the block content
                        drop(prevotes);
                        state.lock_on_with_block(round, block_hash, locked_block_content);
                        state.set_valid(round, block_hash);
                        ConsensusAction::BroadcastPrecommit(Some(block_hash))
                    } else if prevotes.has_two_thirds_nil() {
                        // 2/3+ for NIL - precommit NIL
                        info!(
                            correlation_id = %correlation_id,
                            height = height,
                            round = round,
                            "2/3+ prevotes for NIL"
                        );
                        drop(prevotes);
                        ConsensusAction::BroadcastPrecommit(None)
                    } else {
                        ConsensusAction::None
                    }
                }

                VoteType::Precommit => {
                    // Check for equivocation before adding
                    // Clone existing vote and release lock before mutating state
                    let existing_vote = {
                        let precommits = state.precommits.read().await;
                        precommits.get_vote_by_validator(&voter).cloned()
                    };

                    if let Some(existing) = existing_vote {
                        if let Some(evidence) = check_for_equivocation(&vote, &[(voter, existing)].into_iter().collect()) {
                            warn!(
                                correlation_id = %correlation_id,
                                voter = ?voter,
                                "Detected precommit equivocation: {:?}",
                                evidence
                            );
                            // Store evidence in state for RPC queries and capture for broadcast
                            state.detected_evidence.push(evidence.clone());
                            detected_evidence = Some(evidence);
                        }
                    }

                    // Add vote to precommit set
                    let mut precommits = state.precommits.write().await;
                    if let Err(e) = precommits.add_vote(vote.clone()) {
                        debug!(
                            correlation_id = %correlation_id,
                            error = %e,
                            "Failed to add precommit (may be duplicate)"
                        );
                    }

                    // Check for 2/3+ precommits for a specific block
                    if let Some(block_hash) = precommits.two_thirds_majority() {
                        info!(
                            correlation_id = %correlation_id,
                            height = height,
                            round = round,
                            block_hash = %H256::from_slice(block_hash.as_bytes()),
                            "2/3+ precommits reached - COMMIT"
                        );
                        // COMMIT the block!
                        ConsensusAction::CommitBlock(block_hash)
                    } else if precommits.has_two_thirds_nil() {
                        // 2/3+ precommits for NIL - advance to next round
                        info!(
                            correlation_id = %correlation_id,
                            height = height,
                            round = round,
                            "2/3+ precommits for NIL - advancing round"
                        );
                        ConsensusAction::NewRound(round + 1)
                    } else {
                        ConsensusAction::None
                    }
                }
            }
        };

        // Execute the determined action
        match action {
            ConsensusAction::BroadcastPrecommit(block_hash) => {
                // Advance to precommit step
                {
                    let mut state = tendermint_state.write().await;
                    state.set_step(TendermintStep::Precommit);
                }

                // Schedule precommit timeout (Issue 3.3: set position before scheduling)
                {
                    let mut scheduler = timeout_scheduler.write().await;
                    scheduler.set_position(height, round);
                    let _ = scheduler.schedule(TendermintStep::Precommit);
                }

                // Cast precommit
                self.cast_precommit(height, round, block_hash, correlation_id)
                    .await?;
            }

            ConsensusAction::CommitBlock(block_hash) => {
                // COMMIT the block
                self.commit_block(height, round, block_hash, correlation_id)
                    .await?;
            }

            ConsensusAction::NewRound(new_round) => {
                // Advance to new round
                {
                    let mut state = tendermint_state.write().await;
                    state.new_round(new_round);
                }

                // Schedule propose timeout for new round (Issue 3.3: set position before scheduling)
                {
                    let mut scheduler = timeout_scheduler.write().await;
                    scheduler.set_position(height, new_round);
                    let _ = scheduler.schedule(TendermintStep::Propose);
                }

                // Issue 4.1 FIX: Check if we're the proposer and trigger proposal creation
                // This was missing - when round advancement occurs via receiving 2/3+ nil precommits,
                // we need to trigger proposal creation if we're the new round's proposer
                let is_proposer = {
                    let state = tendermint_state.read().await;
                    state.is_proposer()
                };

                if is_proposer {
                    info!(
                        correlation_id = %correlation_id,
                        height = height,
                        new_round = new_round,
                        "We are proposer for new round (via vote handler) - creating proposal"
                    );

                    // Trigger proposal creation
                    if let Err(e) = self
                        .handle_tendermint_propose(height, new_round, correlation_id)
                        .await
                    {
                        warn!(
                            correlation_id = %correlation_id,
                            height = height,
                            new_round = new_round,
                            error = %e,
                            "Failed to create proposal for new round"
                        );
                        // Don't return error - the propose timeout will trigger and we'll cast nil prevote
                    }
                }

                info!(
                    correlation_id = %correlation_id,
                    height = height,
                    new_round = new_round,
                    "Advanced to new round"
                );
            }

            ConsensusAction::None | ConsensusAction::ScheduleTimeout(_) | ConsensusAction::BroadcastPrevote(_) => {
                // No action needed
            }
        }

        // Broadcast any detected equivocation evidence (after locks released)
        if let Some(evidence) = detected_evidence {
            if let Err(e) = self
                .broadcast_equivocation_evidence(evidence, correlation_id)
                .await
            {
                // Log error but don't fail the vote handling
                error!(
                    correlation_id = %correlation_id,
                    error = %e,
                    "Failed to broadcast equivocation evidence"
                );
            }
        }

        Ok(voter)
    }

    // =========================================================================
    // TendermintTimeout Handler Implementation
    // =========================================================================

    /// Handle a timeout event for a consensus step.
    ///
    /// # Actions based on step:
    /// - Propose timeout: Cast nil prevote, advance to Prevote step
    /// - Prevote timeout: Cast nil precommit, advance to Precommit step
    /// - Precommit timeout: Advance to next round
    pub async fn handle_tendermint_timeout(
        &self,
        height: u64,
        round: u32,
        step: TendermintStep,
        correlation_id: Uuid,
    ) -> TendermintResult<u32> {
        info!(
            correlation_id = %correlation_id,
            height = height,
            round = round,
            step = ?step,
            "Processing timeout"
        );

        let tendermint_state = self
            .tendermint_state
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Tendermint state not initialized".into()))?;

        let timeout_scheduler = self
            .timeout_scheduler
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Timeout scheduler not initialized".into()))?;

        let wal = self
            .consensus_wal
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("WAL not initialized".into()))?;

        // Verify timeout is for current state
        let (mut current_height, mut current_round, mut current_step) = {
            let state = tendermint_state.read().await;
            (state.height, state.round, state.step)
        };

        // FIX: Handle race where timeout arrives before TendermintNewHeight message
        // This can happen during resume when messages are delivered out of order.
        // If the timeout is for exactly the next height, initialize state first.
        if height == current_height + 1 && round == 0 {
            info!(
                correlation_id = %correlation_id,
                timeout_height = height,
                current_height = current_height,
                "Timeout for next height arrived before NewHeight - initializing state"
            );
            // Initialize state for this height
            self.handle_tendermint_new_height(height, correlation_id).await?;

            // Re-read state after initialization
            let state = tendermint_state.read().await;
            current_height = state.height;
            current_round = state.round;
            current_step = state.step;
        }

        if height != current_height || round != current_round {
            // Enhanced logging for diagnosis
            if height > current_height {
                warn!(
                    correlation_id = %correlation_id,
                    timeout_height = height,
                    timeout_round = round,
                    current_height = current_height,
                    current_round = current_round,
                    "Timeout for future height - ChainActor may not have received TendermintNewHeight yet"
                );
            } else {
                debug!(
                    correlation_id = %correlation_id,
                    timeout_height = height,
                    timeout_round = round,
                    current_height = current_height,
                    current_round = current_round,
                    "Ignoring stale timeout"
                );
            }
            return Ok(current_round);
        }

        // Only process timeout if we're at exactly this step
        // Any step mismatch means the timeout is stale (either we've moved past it,
        // or it's for a future step which shouldn't happen but we guard against it)
        if current_step as u8 != step as u8 {
            debug!(
                correlation_id = %correlation_id,
                timeout_step = ?step,
                current_step = ?current_step,
                "Ignoring stale timeout - step mismatch"
            );
            return Ok(current_round);
        }

        match step {
            TendermintStep::Propose => {
                // No proposal received in time - cast nil prevote
                info!(
                    correlation_id = %correlation_id,
                    height = height,
                    round = round,
                    "Propose timeout - casting nil prevote"
                );

                {
                    let mut state = tendermint_state.write().await;
                    state.set_step(TendermintStep::Prevote);
                }

                // Issue 3.3 FIX: Set position before scheduling
                {
                    let mut scheduler = timeout_scheduler.write().await;
                    scheduler.set_position(height, round);
                    let _ = scheduler.schedule(TendermintStep::Prevote);
                }

                self.cast_prevote(height, round, None, correlation_id)
                    .await?;

                Ok(round)
            }

            TendermintStep::Prevote => {
                // CRITICAL FIX: Check if we're locked on a block
                // According to Tendermint protocol:
                // - If locked on a block, we MUST precommit for that block
                // - Only precommit nil if NOT locked
                // This ensures that once a node locks, it continues to vote for
                // the locked block, enabling eventual consensus even after timeouts.
                let precommit_target = {
                    let state = tendermint_state.read().await;
                    state.locked_block
                };

                if let Some(locked_hash) = precommit_target {
                    info!(
                        correlation_id = %correlation_id,
                        height = height,
                        round = round,
                        locked_block = %H256::from_slice(locked_hash.as_bytes()),
                        "Prevote timeout - precommitting for locked block"
                    );
                } else {
                    info!(
                        correlation_id = %correlation_id,
                        height = height,
                        round = round,
                        "Prevote timeout - casting nil precommit (not locked)"
                    );
                }

                {
                    let mut state = tendermint_state.write().await;
                    state.set_step(TendermintStep::Precommit);
                }

                // Issue 3.3 FIX: Set position before scheduling
                {
                    let mut scheduler = timeout_scheduler.write().await;
                    scheduler.set_position(height, round);
                    let _ = scheduler.schedule(TendermintStep::Precommit);
                }

                self.cast_precommit(height, round, precommit_target, correlation_id)
                    .await?;

                Ok(round)
            }

            TendermintStep::Precommit => {
                // Not enough precommits received - advance to next round
                let new_round = round + 1;
                info!(
                    correlation_id = %correlation_id,
                    height = height,
                    round = round,
                    new_round = new_round,
                    "Precommit timeout - advancing to next round"
                );

                // Write WAL entry for new round BEFORE state change
                {
                    let mut wal_guard = wal.write().await;
                    wal_guard
                        .write(WALEntry::NewRound {
                            height,
                            round: new_round,
                        })
                        .map_err(|e| ChainError::Internal(format!("WAL write failed: {}", e)))?;
                }

                // Advance state to new round
                {
                    let mut state = tendermint_state.write().await;
                    state.new_round(new_round);
                }

                // Issue 3.3 FIX: Update scheduler position before scheduling
                {
                    let mut scheduler = timeout_scheduler.write().await;
                    scheduler.set_position(height, new_round);
                    let _ = scheduler.schedule(TendermintStep::Propose);
                }

                // Check if we're the proposer for the new round
                let is_proposer = {
                    let state = tendermint_state.read().await;
                    state.is_proposer()
                };

                // Issue 2.3 FIX: Actually trigger the proposal if we're the proposer
                if is_proposer {
                    info!(
                        correlation_id = %correlation_id,
                        height = height,
                        new_round = new_round,
                        "We are proposer for new round - creating proposal"
                    );

                    // Trigger proposal creation
                    if let Err(e) = self
                        .handle_tendermint_propose(height, new_round, correlation_id)
                        .await
                    {
                        warn!(
                            correlation_id = %correlation_id,
                            height = height,
                            new_round = new_round,
                            error = %e,
                            "Failed to create proposal for new round"
                        );
                        // Don't return error - timeout still fired successfully
                        // The propose timeout will trigger and we'll cast nil prevote
                    }
                }

                Ok(new_round)
            }

            TendermintStep::Commit => {
                // Should not receive timeout in Commit step
                warn!(
                    correlation_id = %correlation_id,
                    "Unexpected timeout in Commit step"
                );
                Ok(round)
            }
        }
    }

    // =========================================================================
    // TendermintGovernanceUpdate Handler Implementation
    // =========================================================================

    /// Handle a governance update (validator set change, parameter update, etc.).
    ///
    /// # Actions
    /// - Validate the update
    /// - Calculate effective height (H+2 for validator updates, H+1 for params)
    /// - Store to StorageActor
    pub async fn handle_tendermint_governance_update(
        &self,
        update: GovernanceUpdate,
        correlation_id: Uuid,
    ) -> TendermintResult<u64> {
        let current_height = self.state.get_height().await;
        let effective_height = update.effective_height(current_height);
        let variant_name = update.variant_name();

        info!(
            correlation_id = %correlation_id,
            update_type = variant_name,
            current_height = current_height,
            effective_height = effective_height,
            "Processing governance update"
        );

        let storage = self
            .storage_actor
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("StorageActor not configured".into()))?;

        match update {
            GovernanceUpdate::Validator(validator_update) => {
                // Validator updates activate at H+2
                info!(
                    correlation_id = %correlation_id,
                    effective_height = effective_height,
                    public_key = ?validator_update.public_key,
                    power = validator_update.power,
                    is_removal = validator_update.is_removal(),
                    "Processing validator update for H+2 activation"
                );

                // 1. Load current validator set from storage
                let mut current_set = self.load_validator_set_for_height(current_height).await?;

                // 2. Apply the update (add/modify/remove)
                current_set.apply_updates(&[validator_update.clone()]);

                // 3. Store the new validator set for activation at effective_height
                storage
                    .send(crate::actors_v2::storage::messages::StoreValidatorSetMessage {
                        effective_height,
                        validator_set: current_set.clone(),
                        correlation_id: Some(correlation_id),
                    })
                    .await
                    .map_err(|e| ChainError::Internal(format!("Mailbox error: {}", e)))?
                    .map_err(|e| ChainError::Storage(format!("Storage error: {}", e)))?;

                // Note: Local validator set is NOT updated here.
                // The new set activates at effective_height (H+2) via load_validator_set_for_height()

                // Issue 4.2: Notify sync validator of upcoming validator set change
                if let Some(ref sync_validator) = self.tendermint_sync_validator {
                    match sync_validator.write() {
                        Ok(mut validator_guard) => {
                            validator_guard.record_validator_change(effective_height, current_set.clone());
                            info!(
                                correlation_id = %correlation_id,
                                effective_height = effective_height,
                                "Notified sync validator of validator set change"
                            );
                        }
                        Err(e) => {
                            error!(
                                correlation_id = %correlation_id,
                                error = ?e,
                                "Failed to acquire sync validator lock for governance notification"
                            );
                        }
                    }
                }

                info!(
                    correlation_id = %correlation_id,
                    effective_height = effective_height,
                    "Validator update stored and will activate at height {}",
                    effective_height
                );
            }

            GovernanceUpdate::Parameter(param_update) => {
                // Serialize and store parameter
                let value = rmp_serde::to_vec(&param_update.value)
                    .map_err(|e| ChainError::Internal(format!("Serialization error: {}", e)))?;

                storage
                    .send(crate::actors_v2::storage::messages::StoreParameterUpdateMessage {
                        param_id: param_update.param,
                        effective_height,
                        value,
                        correlation_id: Some(correlation_id),
                    })
                    .await
                    .map_err(|e| ChainError::Internal(format!("Mailbox error: {}", e)))?
                    .map_err(|e| ChainError::Storage(format!("Storage error: {}", e)))?;

                info!(
                    correlation_id = %correlation_id,
                    param = ?param_update.param,
                    effective_height = effective_height,
                    "Parameter update stored"
                );
            }

            GovernanceUpdate::Emergency(emergency_action) => {
                // Emergency actions take effect immediately (H+0)
                use super::tendermint::governance::EmergencyActionKind;

                warn!(
                    correlation_id = %correlation_id,
                    action = ?emergency_action.action,
                    "Applying emergency action IMMEDIATELY"
                );

                // Update TendermintRuntimeState pause flags
                if let Some(ref runtime_state) = self.state.tendermint_runtime {
                    let mut runtime = runtime_state.write().await;
                    match emergency_action.action {
                        EmergencyActionKind::PauseChain => {
                            runtime.chain_paused = true;
                            warn!("Chain PAUSED - no new blocks will be produced");
                        }
                        EmergencyActionKind::ResumeChain => {
                            runtime.chain_paused = false;
                            info!("Chain RESUMED - normal operation restored");
                        }
                        EmergencyActionKind::PausePegIns => {
                            runtime.pegins_paused = true;
                            warn!("Peg-ins PAUSED - new deposits will be rejected");
                        }
                        EmergencyActionKind::ResumePegIns => {
                            runtime.pegins_paused = false;
                            info!("Peg-ins RESUMED");
                        }
                        EmergencyActionKind::PausePegOuts => {
                            runtime.pegouts_paused = true;
                            warn!("Peg-outs PAUSED - withdrawals halted");
                        }
                        EmergencyActionKind::ResumePegOuts => {
                            runtime.pegouts_paused = false;
                            info!("Peg-outs RESUMED");
                        }
                    }
                } else {
                    warn!(
                        correlation_id = %correlation_id,
                        "TendermintRuntimeState not initialized - emergency action not applied"
                    );
                }
            }
        }

        Ok(effective_height)
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Load validator set for a given height from storage.
    async fn load_validator_set_for_height(
        &self,
        height: u64,
    ) -> TendermintResult<super::tendermint::ValidatorSet> {
        let storage = self
            .storage_actor
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("StorageActor not configured".into()))?;

        let result = storage
            .send(GetValidatorSetForHeightMessage {
                height,
                correlation_id: None,
            })
            .await
            .map_err(|e| ChainError::Internal(format!("Mailbox error: {}", e)))?
            .map_err(|e| ChainError::Storage(format!("Storage error: {}", e)))?;

        result.ok_or_else(|| {
            ChainError::Internal(format!("No validator set found for height {}", height))
        })
    }

    /// Validate a block's execution payload via EngineActor (Phase 4.3).
    ///
    /// This performs a dry-run validation of the execution payload without
    /// committing any state changes. Used to verify proposals before voting.
    ///
    /// # Returns
    ///
    /// * `true` - Execution payload is valid
    /// * `false` - Execution payload is invalid or validation failed
    ///
    /// # Note
    ///
    /// Returns `false` (not error) on failure to ensure proper BFT behavior:
    /// invalid proposals should receive NIL votes, not cause handler errors.
    async fn validate_block_execution(
        &self,
        block: &crate::block::ConsensusBlock<lighthouse_wrapper::types::MainnetEthSpec>,
        correlation_id: Uuid,
    ) -> bool {
        use crate::actors_v2::engine::{EngineMessage, EngineResponse};
        use lighthouse_wrapper::types::ExecutionPayload;

        // Get EngineActor - if not configured, skip validation (return true)
        // This allows testing without full engine integration
        let engine = match self.engine_actor.as_ref() {
            Some(e) => e,
            None => {
                debug!(
                    correlation_id = %correlation_id,
                    "EngineActor not configured, skipping execution validation"
                );
                return true;
            }
        };

        // Convert ExecutionPayloadCapella to ExecutionPayload enum
        let payload = ExecutionPayload::Capella(block.execution_payload.clone());

        // Send validation request to EngineActor
        let result = engine
            .send(EngineMessage::ValidatePayload {
                payload,
                correlation_id: Some(correlation_id),
            })
            .await;

        match result {
            Ok(Ok(EngineResponse::PayloadValid { is_valid, validation_time })) => {
                if is_valid {
                    debug!(
                        correlation_id = %correlation_id,
                        validation_time_ms = validation_time.as_millis(),
                        "Block execution payload validated successfully"
                    );
                    true
                } else {
                    warn!(
                        correlation_id = %correlation_id,
                        validation_time_ms = validation_time.as_millis(),
                        "Block execution payload is INVALID"
                    );
                    false
                }
            }
            Ok(Ok(other)) => {
                warn!(
                    correlation_id = %correlation_id,
                    response = ?other,
                    "Unexpected response from EngineActor validation"
                );
                false
            }
            Ok(Err(e)) => {
                warn!(
                    correlation_id = %correlation_id,
                    error = %e,
                    "EngineActor validation returned error"
                );
                false
            }
            Err(e) => {
                warn!(
                    correlation_id = %correlation_id,
                    error = %e,
                    "Failed to send validation request to EngineActor"
                );
                false
            }
        }
    }

    /// Build a proposal block via EngineActor.
    ///
    /// This creates a new block for proposal by:
    /// 1. Getting the parent block hash from storage
    /// 2. Requesting an execution payload from EngineActor
    /// 3. Getting the cached last_commit (commit proof for the previous block)
    /// 4. Assembling the ConsensusBlock
    async fn build_proposal_block(
        &self,
        height: u64,
        correlation_id: Uuid,
    ) -> TendermintResult<crate::block::ConsensusBlock<lighthouse_wrapper::types::MainnetEthSpec>>
    {
        use crate::actors_v2::engine::{EngineMessage, EngineResponse};
        use crate::actors_v2::storage::messages::GetChainHeadMessage;
        use lighthouse_wrapper::types::ExecutionBlockHash;
        use std::time::{Duration, SystemTime, UNIX_EPOCH};

        info!(
            correlation_id = %correlation_id,
            height = height,
            "Building proposal block"
        );

        // Get required actors
        let engine = self
            .engine_actor
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("EngineActor not configured".into()))?;

        let storage = self
            .storage_actor
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("StorageActor not configured".into()))?;

        // 1. Get parent block hash and timestamp from storage/execution layer
        // We need both to ensure timestamp monotonicity (Reth requires timestamp > parent.timestamp)
        let (parent_hash, parent_timestamp) = {
            let head_result = storage
                .send(GetChainHeadMessage {
                    correlation_id: Some(correlation_id),
                })
                .await
                .map_err(|e| ChainError::Internal(format!("Storage mailbox error: {}", e)))?
                .map_err(|e| ChainError::Storage(format!("Failed to get chain head: {}", e)))?;

            match head_result {
                Some(head) => {
                    debug!(
                        correlation_id = %correlation_id,
                        parent_height = head.number,
                        parent_consensus_hash = %head.hash,
                        parent_execution_hash = %head.execution_hash,
                        "Found parent block"
                    );
                    // IMPORTANT: Use execution_hash (not consensus hash) for Reth parent lookup
                    // Reth only knows blocks by their execution layer hash, not the consensus block hash
                    // Also need to get the parent's timestamp to ensure monotonicity
                    let parent_payload = engine
                        .send(EngineMessage::GetPayloadByTag {
                            block_tag: "latest".to_string(),
                            correlation_id: Some(correlation_id),
                        })
                        .await
                        .map_err(|e| ChainError::Internal(format!("Engine mailbox error: {}", e)))?
                        .map_err(|e| ChainError::Engine(format!("Failed to get latest block: {}", e)))?;

                    let parent_ts = match parent_payload {
                        EngineResponse::PayloadByTag { payload } => payload.timestamp(),
                        _ => 0, // Fallback, shouldn't happen
                    };
                    (head.execution_hash, parent_ts)
                }
                None => {
                    // No head means we're building on genesis
                    // Query Reth for the actual genesis block hash (block 0)
                    debug!(
                        correlation_id = %correlation_id,
                        "No chain head found, querying Reth for genesis block"
                    );

                    // Get genesis execution block from engine
                    let genesis_result = engine
                        .send(EngineMessage::GetPayloadByTag {
                            block_tag: "earliest".to_string(),
                            correlation_id: Some(correlation_id),
                        })
                        .await
                        .map_err(|e| ChainError::Internal(format!("Engine mailbox error: {}", e)))?
                        .map_err(|e| ChainError::Engine(format!("Failed to get genesis: {}", e)))?;

                    match genesis_result {
                        EngineResponse::PayloadByTag { payload } => {
                            let genesis_hash = payload.block_hash();
                            let genesis_ts = payload.timestamp();
                            info!(
                                correlation_id = %correlation_id,
                                genesis_hash = %genesis_hash,
                                genesis_timestamp = genesis_ts,
                                "Using Reth genesis block as parent"
                            );
                            (genesis_hash, genesis_ts)
                        }
                        _ => {
                            return Err(ChainError::Engine(
                                "Reth not ready - cannot get genesis block".to_string()
                            ));
                        }
                    }
                }
            }
        };

        // 2. Calculate timestamp for the new block
        // CRITICAL: Reth requires timestamp > parent.timestamp (error -38003 if not)
        // Use max(now, parent_timestamp + 1) to ensure strict monotonicity
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();
        let min_timestamp = parent_timestamp + 1;
        let timestamp_secs = std::cmp::max(now_secs, min_timestamp);
        let timestamp = Duration::from_secs(timestamp_secs);

        if timestamp_secs > now_secs {
            debug!(
                correlation_id = %correlation_id,
                parent_timestamp = parent_timestamp,
                adjusted_timestamp = timestamp_secs,
                "Adjusted timestamp to ensure monotonicity"
            );
        }

        // 3. Get optional queued AuxPoW header FIRST (from miners via submitauxblock)
        // Path B: AuxPoW header contains pegins, which we need for EVM balance credits
        let auxpow_header = self.state.take_queued_pow().await;
        if auxpow_header.is_some() {
            info!(
                correlation_id = %correlation_id,
                pegin_count = auxpow_header.as_ref().map(|h| h.pegins.len()).unwrap_or(0),
                "Including AuxPoW header with peg-ins in proposal block"
            );
        }

        // 4. Collect peg-in withdrawals FROM AuxPoW header (Path B: pegins in auxpow_header.pegins)
        // Doc 16: proposer converts queued peg-ins to EVM withdrawals
        let add_balances = self.collect_pegin_withdrawals_from_auxpow(
            correlation_id,
            auxpow_header.as_ref(),
        ).await?;

        // 5. Request execution payload from EngineActor with peg-in withdrawals
        let execution_payload = {
            let build_msg = EngineMessage::BuildPayload {
                timestamp,
                parent_hash: Some(parent_hash),
                add_balances, // Peg-in withdrawals as balance additions
                correlation_id: Some(correlation_id),
            };

            let response = engine
                .send(build_msg)
                .await
                .map_err(|e| ChainError::Internal(format!("Engine mailbox error: {}", e)))?
                .map_err(|e| ChainError::Engine(format!("Payload build failed: {}", e)))?;

            match response {
                EngineResponse::PayloadBuilt { payload, build_time } => {
                    info!(
                        correlation_id = %correlation_id,
                        block_number = payload.block_number(),
                        block_hash = %payload.block_hash(),
                        build_time_ms = build_time.as_millis(),
                        "Execution payload built"
                    );
                    payload
                }
                other => {
                    return Err(ChainError::Engine(format!(
                        "Unexpected engine response: {:?}",
                        other
                    )));
                }
            }
        };

        // 6. Get last_commit from cache (commit proof for the previous block)
        let last_commit = if let Some(ref cached) = self.cached_last_commit {
            let guard = cached.read().await;
            // Clone the commit - it will be embedded in this block
            Some((*guard).clone())
        } else {
            // No cached commit - this might be the first Tendermint block after genesis
            // or after a restart where the cache wasn't populated
            debug!(
                correlation_id = %correlation_id,
                "No cached last_commit available"
            );
            None
        };

        // 7. Convert ExecutionPayload to ExecutionPayloadCapella
        // The execution_payload from EngineActor should already be Capella
        let execution_payload_capella = match execution_payload {
            lighthouse_wrapper::types::ExecutionPayload::Capella(capella) => capella,
            _ => {
                return Err(ChainError::Engine(
                    "Expected Capella payload from EngineActor".into(),
                ));
            }
        };

        // 8. Compute Tendermint governance schema fields
        // These fields enable light client verification and governance tracking
        let (validators_hash, next_validators_hash, params_hash) = {
            // Get current validator set from Tendermint state
            let tendermint_state = self.tendermint_state.as_ref()
                .ok_or_else(|| ChainError::Configuration("Tendermint state not initialized".into()))?;
            let state = tendermint_state.read().await;
            let current_validator_set = state.validator_set.clone();
            drop(state);

            // Compute validators_hash from current validator set
            let validators_hash = current_validator_set.compute_hash();

            // For next_validators_hash: Use same as current unless governance update pending
            // In production, this would check for pending validator set changes at H+2
            // For now, assume no changes (next_validators_hash == validators_hash)
            let next_validators_hash = validators_hash;

            // Compute params_hash from chain parameters
            // Using default params until governance integration is complete
            // TODO: Load actual ChainParams from storage when governance is fully integrated
            let chain_params = super::tendermint::params::ChainParams::default();
            let params_hash = chain_params.compute_hash();

            (
                Some(validators_hash),
                Some(next_validators_hash),
                Some(params_hash),
            )
        };

        // 9. Assemble the ConsensusBlock with optional AuxPoW (retrieved in step 3)
        // Convert ExecutionBlockHash to Hash256 using into_root()
        // Path B: Pegins are now stored in auxpow_header.pegins (not directly on ConsensusBlock)
        let block = crate::block::ConsensusBlock {
            parent_hash: execution_payload_capella.parent_hash.into_root(),
            slot: height, // In Tendermint mode, slot == height
            last_commit,
            auxpow_header, // Optional AuxPoW from miners via submitauxblock (includes pegins)
            execution_payload: execution_payload_capella,
            pegout_payment_proposal: None, // Peg-outs handled separately
            finalized_pegouts: Vec::new(),
            // Tendermint schema fields for light client verification
            validators_hash,
            next_validators_hash,
            params_hash,
            // governance_updates: Collected when governance proposals are finalized
            // For now, None until governance integration is complete (Phase 5)
            governance_updates: None,
        };

        info!(
            correlation_id = %correlation_id,
            height = height,
            block_number = block.execution_payload.block_number,
            parent_hash = %block.parent_hash,
            has_last_commit = block.last_commit.is_some(),
            has_auxpow = block.auxpow_header.is_some(),
            pegin_count = block.pegins().len(),
            validators_hash = ?block.validators_hash,
            "Proposal block built successfully"
        );

        Ok(block)
    }

    /// Collect peg-in balance additions from AuxPowHeader (Path B design).
    ///
    /// Per Doc 16: Each peg-in creates two balance additions:
    /// 1. User receives (amount - miner_fee) → pegin.evm_account
    /// 2. Miner receives miner_fee → auxpow_header.fee_recipient
    ///
    /// Path B: Peg-ins are stored in auxpow_header.pegins. This ensures the
    /// EVM balance credits match exactly with the pegins recorded in the block.
    ///
    /// Returns:
    /// - `Vec<AddBalance>`: EVM balance credits for execution_payload
    async fn collect_pegin_withdrawals_from_auxpow(
        &self,
        correlation_id: Uuid,
        auxpow_header: Option<&crate::block::AuxPowHeader>,
    ) -> TendermintResult<Vec<crate::engine::AddBalance>>
    {
        use crate::engine::{AddBalance, ConsensusAmount};
        use super::tendermint::pegin::PegInCompensation;

        let mut add_balances = Vec::new();

        // No AuxPoW = no pegins (Path B: only blocks with AuxPoW can have pegins)
        let header = match auxpow_header {
            Some(h) => h,
            None => {
                debug!(
                    correlation_id = %correlation_id,
                    "No AuxPoW header, no peg-ins to process"
                );
                return Ok(add_balances);
            }
        };

        if header.pegins.is_empty() {
            debug!(
                correlation_id = %correlation_id,
                "AuxPoW header has no peg-ins"
            );
            return Ok(add_balances);
        }

        // Get peg-in compensation params (use defaults, governance can override)
        let compensation = PegInCompensation::default();

        // Process pegins from AuxPowHeader (already validated in SubmitAuxBlock handler)
        for pegin in &header.pegins {
            // Calculate miner fee (Doc 16: fee = amount * bps / 10000, clamped)
            let miner_fee = compensation.calculate_fee(pegin.amount);
            let user_amount = pegin.amount.saturating_sub(miner_fee);

            // User balance credit
            add_balances.push(AddBalance::from((
                pegin.evm_account,
                ConsensusAmount::from_satoshi(user_amount),
            )));

            // Miner fee credit (to fee_recipient from AuxPowHeader)
            if miner_fee > 0 {
                add_balances.push(AddBalance::from((
                    header.fee_recipient,
                    ConsensusAmount::from_satoshi(miner_fee),
                )));
            }

            debug!(
                correlation_id = %correlation_id,
                txid = %pegin.txid,
                user_amount = user_amount,
                miner_fee = miner_fee,
                evm_account = ?pegin.evm_account,
                fee_recipient = ?header.fee_recipient,
                "Processed peg-in from AuxPoW header"
            );
        }

        info!(
            correlation_id = %correlation_id,
            pegins_processed = header.pegins.len(),
            balance_additions = add_balances.len(),
            "Collected peg-in withdrawals from AuxPoW header (Path B)"
        );

        Ok(add_balances)
    }

    /// DEPRECATED: Use `collect_pegin_withdrawals_from_auxpow` instead.
    /// This function used state.queued_pegins which is separate from auxpow_header.pegins.
    #[allow(dead_code)]
    async fn collect_pegin_withdrawals_legacy(
        &self,
        correlation_id: Uuid,
    ) -> TendermintResult<(Vec<crate::engine::AddBalance>, Vec<(bitcoin::Txid, bitcoin::BlockHash)>)>
    {
        use crate::engine::{AddBalance, ConsensusAmount};
        use super::tendermint::pegin::PegInCompensation;

        let mut add_balances = Vec::new();
        let mut pegin_refs = Vec::new();

        // Get peg-in compensation params (use defaults, governance can override)
        let compensation = PegInCompensation::default();

        // Drain queued peg-ins (Doc 16: queued via submitauxblock)
        let queued_pegins = self.state.drain_queued_pegins().await;

        if queued_pegins.is_empty() {
            debug!(
                correlation_id = %correlation_id,
                "No queued peg-ins to process"
            );
            return Ok((add_balances, pegin_refs));
        }

        for queued in queued_pegins {
            let pegin = &queued.info;

            // Skip if already processed (Doc 16 Layer 2: Producer filter)
            if self.state.is_pegin_processed(&pegin.txid).await {
                debug!(
                    correlation_id = %correlation_id,
                    txid = %pegin.txid,
                    "Skipping already-processed peg-in"
                );
                continue;
            }

            // Calculate miner fee (Doc 16: fee = amount * bps / 10000, clamped)
            let miner_fee = compensation.calculate_fee(pegin.amount);
            let user_amount = pegin.amount.saturating_sub(miner_fee);

            // User balance credit (tuple struct)
            add_balances.push(AddBalance::from((
                pegin.evm_account,
                ConsensusAmount::from_satoshi(user_amount),
            )));

            // Miner fee credit (to fee_recipient from QueuedPegIn)
            if miner_fee > 0 {
                add_balances.push(AddBalance::from((
                    queued.fee_recipient,
                    ConsensusAmount::from_satoshi(miner_fee),
                )));
            }

            // Track peg-in reference (Txid + BTC block) for block.pegins field
            pegin_refs.push((pegin.txid, pegin.block_hash));

            // Note: Peg-ins are marked as processed in commit_block() AFTER
            // the block containing them is committed (Doc 16 Layer 2: deduplication)

            debug!(
                correlation_id = %correlation_id,
                txid = %pegin.txid,
                user_amount = user_amount,
                miner_fee = miner_fee,
                evm_account = ?pegin.evm_account,
                fee_recipient = ?queued.fee_recipient,
                "Processed peg-in for block"
            );
        }

        info!(
            correlation_id = %correlation_id,
            pegins_processed = pegin_refs.len(),
            balance_additions = add_balances.len(),
            "Collected peg-in withdrawals for block"
        );

        Ok((add_balances, pegin_refs))
    }

    /// Broadcast a Tendermint proposal via NetworkActor.
    async fn broadcast_tendermint_proposal(&self, proposal: Proposal) -> TendermintResult<()> {
        let network = self
            .network_actor
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("NetworkActor not configured".into()))?;

        let height = proposal.height;
        let round = proposal.round;

        // Wrap in TendermintMessage for network transmission
        let message = super::tendermint::TendermintMessage::Proposal(proposal);

        let msg = crate::actors_v2::network::messages::NetworkMessage::BroadcastTendermint {
            message,
            correlation_id: Some(uuid::Uuid::new_v4()),
        };

        match network.send(msg).await {
            Ok(Ok(_)) => {
                info!(
                    height = height,
                    round = round,
                    "Proposal broadcast successfully"
                );
                Ok(())
            }
            Ok(Err(e)) => {
                warn!(
                    height = height,
                    round = round,
                    error = %e,
                    "NetworkActor rejected proposal broadcast"
                );
                Err(ChainError::Network(e))
            }
            Err(e) => {
                warn!(
                    height = height,
                    round = round,
                    error = %e,
                    "Failed to send proposal to NetworkActor"
                );
                Err(ChainError::Internal(format!("Mailbox error: {}", e)))
            }
        }
    }

    /// Cast a prevote for the given block (or nil).
    async fn cast_prevote(
        &self,
        height: u64,
        round: u32,
        block_hash: Option<BlockHash>,
        correlation_id: Uuid,
    ) -> TendermintResult<()> {
        let tendermint_state = self
            .tendermint_state
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Tendermint state not initialized".into()))?;

        let wal = self
            .consensus_wal
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("WAL not initialized".into()))?;

        let keypair = self
            .validator_keypair
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("No validator keypair".into()))?;

        // Check if already voted
        {
            let state = tendermint_state.read().await;
            if state.has_voted_prevote() {
                debug!(
                    correlation_id = %correlation_id,
                    "Already voted prevote this round"
                );
                return Ok(());
            }
        }

        // Get our validator ID
        let validator = {
            let state = tendermint_state.read().await;
            state
                .our_validator_id
                .ok_or_else(|| ChainError::Internal("Not a validator".into()))?
        };

        // Create vote
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let vote = Vote {
            height,
            round,
            vote_type: VoteType::Prevote,
            block_hash,
            validator,
            timestamp,
            signature: lighthouse_wrapper::bls::Signature::empty(),
        };

        // Sign the vote (Issue 1.2: use chain_id for domain separation)
        let chain_id_str = self.config.chain_id.to_string();
        let signing_root = vote.signing_root(&chain_id_str);
        let signature = keypair.sk.sign(signing_root);
        let vote = Vote { signature, ..vote };

        // Issue 2.2 FIX: Add our own vote to the VoteSet BEFORE WAL/broadcast
        // This ensures our vote counts towards the 2/3+ threshold immediately
        // Also check if our vote completes the 2/3+ threshold for precommit
        let should_precommit = {
            let state = tendermint_state.read().await;
            let mut prevotes = state.prevotes.write().await;

            if let Err(e) = prevotes.add_vote(vote.clone()) {
                // This should never happen for our own vote (not a duplicate)
                warn!(
                    correlation_id = %correlation_id,
                    error = %e,
                    "Failed to add own prevote to VoteSet"
                );
                None
            } else {
                debug!(
                    correlation_id = %correlation_id,
                    height = height,
                    round = round,
                    "Added own prevote to VoteSet"
                );

                // Check if our vote completes the 2/3+ threshold
                if let Some(majority_hash) = prevotes.two_thirds_majority() {
                    Some(majority_hash)
                } else if prevotes.has_two_thirds_nil() {
                    // 2/3+ nil prevotes - will precommit nil
                    Some(BlockHash::zero()) // Sentinel for nil
                } else {
                    None
                }
            }
        };

        // Write WAL entry BEFORE broadcast
        {
            let mut wal_guard = wal.write().await;
            wal_guard
                .write(WALEntry::SentPrevote {
                    height,
                    round,
                    block_hash,
                })
                .map_err(|e| ChainError::Internal(format!("WAL write failed: {}", e)))?;
        }

        // Record the vote and transition to Prevote step
        // Step transition here ensures we're in Prevote regardless of how we got here
        // (via proposal reception or timeout), making late propose timeouts stale
        {
            let mut state = tendermint_state.write().await;
            state.record_prevote(block_hash);
            state.set_step(TendermintStep::Prevote);
        }

        // Broadcast vote
        self.broadcast_tendermint_vote(vote).await?;

        info!(
            correlation_id = %correlation_id,
            height = height,
            round = round,
            block_hash = ?block_hash,
            "Cast prevote"
        );

        // If our vote completed the 2/3+ threshold, proceed to precommit
        if let Some(majority_hash) = should_precommit {
            // Determine the precommit target
            let precommit_target = if majority_hash == BlockHash::zero() {
                // Sentinel for nil
                info!(
                    correlation_id = %correlation_id,
                    height = height,
                    round = round,
                    "2/3+ prevotes for NIL reached with our vote - precommitting NIL"
                );
                None
            } else {
                info!(
                    correlation_id = %correlation_id,
                    height = height,
                    round = round,
                    block_hash = %H256::from_slice(majority_hash.as_bytes()),
                    "2/3+ prevotes reached with our vote - proceeding to precommit"
                );

                // Lock on the block before precommitting
                {
                    let mut state = tendermint_state.write().await;
                    // Get locked block content from current proposal if available
                    let locked_block_content = state
                        .current_proposal
                        .as_ref()
                        .filter(|p| p.block_hash() == majority_hash)
                        .map(|p| p.block.clone());

                    state.lock_on_with_block(round, majority_hash, locked_block_content);
                    state.set_valid(round, majority_hash);
                    state.set_step(TendermintStep::Precommit);
                }

                Some(majority_hash)
            };

            // Cast the precommit
            self.cast_precommit(height, round, precommit_target, correlation_id)
                .await?;
        }

        Ok(())
    }

    /// Cast a precommit for the given block (or nil).
    async fn cast_precommit(
        &self,
        height: u64,
        round: u32,
        block_hash: Option<BlockHash>,
        correlation_id: Uuid,
    ) -> TendermintResult<()> {
        let tendermint_state = self
            .tendermint_state
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Tendermint state not initialized".into()))?;

        let wal = self
            .consensus_wal
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("WAL not initialized".into()))?;

        let keypair = self
            .validator_keypair
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("No validator keypair".into()))?;

        // Check if already voted
        {
            let state = tendermint_state.read().await;
            if state.has_voted_precommit() {
                debug!(
                    correlation_id = %correlation_id,
                    "Already voted precommit this round"
                );
                return Ok(());
            }
        }

        // Get our validator ID
        let validator = {
            let state = tendermint_state.read().await;
            state
                .our_validator_id
                .ok_or_else(|| ChainError::Internal("Not a validator".into()))?
        };

        // Create vote
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let vote = Vote {
            height,
            round,
            vote_type: VoteType::Precommit,
            block_hash,
            validator,
            timestamp,
            signature: lighthouse_wrapper::bls::Signature::empty(),
        };

        // Sign the vote (Issue 1.2: use chain_id for domain separation)
        let chain_id_str = self.config.chain_id.to_string();
        let signing_root = vote.signing_root(&chain_id_str);
        let signature = keypair.sk.sign(signing_root);
        let vote = Vote { signature, ..vote };

        // Issue 2.2 FIX: Add our own vote to the VoteSet BEFORE WAL/broadcast
        // This ensures our vote counts towards the 2/3+ threshold immediately
        // Also check if our vote completes the 2/3+ threshold for commit
        let should_commit = {
            let state = tendermint_state.read().await;
            let mut precommits = state.precommits.write().await;

            if let Err(e) = precommits.add_vote(vote.clone()) {
                // This should never happen for our own vote
                warn!(
                    correlation_id = %correlation_id,
                    error = %e,
                    "Failed to add own precommit to VoteSet"
                );
                None
            } else {
                debug!(
                    correlation_id = %correlation_id,
                    height = height,
                    round = round,
                    "Added own precommit to VoteSet"
                );

                // Check if our vote completes the 2/3+ threshold
                precommits.two_thirds_majority()
            }
        };

        // Write WAL entry BEFORE broadcast
        {
            let mut wal_guard = wal.write().await;
            wal_guard
                .write(WALEntry::SentPrecommit {
                    height,
                    round,
                    block_hash,
                })
                .map_err(|e| ChainError::Internal(format!("WAL write failed: {}", e)))?;
        }

        // Record the vote and transition to Precommit step
        // Step transition here ensures we're in Precommit regardless of how we got here
        // (via 2/3+ prevotes or timeout), making late prevote timeouts stale
        {
            let mut state = tendermint_state.write().await;
            state.record_precommit(block_hash);
            state.set_step(TendermintStep::Precommit);
        }

        // Broadcast vote
        self.broadcast_tendermint_vote(vote).await?;

        info!(
            correlation_id = %correlation_id,
            height = height,
            round = round,
            block_hash = ?block_hash,
            "Cast precommit"
        );

        // If our vote completed the 2/3+ threshold, commit the block
        if let Some(committed_hash) = should_commit {
            info!(
                correlation_id = %correlation_id,
                height = height,
                round = round,
                block_hash = %H256::from_slice(committed_hash.as_bytes()),
                "2/3+ precommits reached with our vote - committing"
            );

            // Commit the block
            self.commit_block(height, round, committed_hash, correlation_id)
                .await?;
        }

        Ok(())
    }

    /// Broadcast a Tendermint vote via NetworkActor.
    async fn broadcast_tendermint_vote(&self, vote: Vote) -> TendermintResult<()> {
        let network = self
            .network_actor
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("NetworkActor not configured".into()))?;

        let height = vote.height;
        let round = vote.round;
        let vote_type = vote.vote_type;

        // Wrap in TendermintMessage for network transmission
        let message = super::tendermint::TendermintMessage::Vote(vote);

        let msg = crate::actors_v2::network::messages::NetworkMessage::BroadcastTendermint {
            message,
            correlation_id: Some(uuid::Uuid::new_v4()),
        };

        match network.send(msg).await {
            Ok(Ok(_)) => {
                debug!(
                    height = height,
                    round = round,
                    vote_type = ?vote_type,
                    "Vote broadcast successfully"
                );
                Ok(())
            }
            Ok(Err(e)) => {
                warn!(
                    height = height,
                    round = round,
                    vote_type = ?vote_type,
                    error = %e,
                    "NetworkActor rejected vote broadcast"
                );
                Err(ChainError::Network(e))
            }
            Err(e) => {
                warn!(
                    height = height,
                    round = round,
                    vote_type = ?vote_type,
                    error = %e,
                    "Failed to send vote to NetworkActor"
                );
                Err(ChainError::Internal(format!("Mailbox error: {}", e)))
            }
        }
    }

    /// Broadcast equivocation evidence via NetworkActor.
    ///
    /// Called when we detect a validator double-voting or double-proposing.
    /// Evidence is written to WAL before broadcast to prevent re-broadcasting after crash.
    async fn broadcast_equivocation_evidence(
        &self,
        evidence: EquivocationEvidence,
        correlation_id: Uuid,
    ) -> TendermintResult<()> {
        let network = self
            .network_actor
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("NetworkActor not configured".into()))?;

        let wal = self
            .consensus_wal
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("WAL not initialized".into()))?;

        let height = evidence.height;
        let culprit = evidence.culprit;
        let kind = evidence.kind;

        // Compute evidence hash for deduplication (uses chain_id for domain separation)
        let chain_id = format!("{}", self.config.chain_id);
        let evidence_hash = evidence.evidence_hash(&chain_id);

        // Check if we've already processed/broadcast this evidence
        let tendermint_state = self
            .tendermint_state
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Tendermint state not initialized".into()))?;

        {
            let state = tendermint_state.read().await;
            if state.processed_evidence.contains(&evidence_hash) {
                debug!(
                    correlation_id = %correlation_id,
                    height = height,
                    culprit = ?culprit,
                    "Skipping evidence broadcast - already processed"
                );
                return Ok(());
            }
        }

        // Write WAL entry BEFORE broadcast (crash safety - prevents re-broadcast)
        {
            let mut wal_guard = wal.write().await;
            wal_guard
                .write(WALEntry::SentEvidence {
                    height,
                    culprit,
                    evidence_hash,
                })
                .map_err(|e| ChainError::Internal(format!("WAL write failed: {}", e)))?;
        }

        // Wrap in TendermintMessage for network transmission
        let message = TendermintMessage::Evidence(evidence);

        let msg = crate::actors_v2::network::messages::NetworkMessage::BroadcastTendermint {
            message,
            correlation_id: Some(correlation_id),
        };

        match network.send(msg).await {
            Ok(Ok(_)) => {
                // Mark as processed after successful broadcast
                {
                    let mut state = tendermint_state.write().await;
                    state.processed_evidence.insert(evidence_hash);
                }

                warn!(
                    correlation_id = %correlation_id,
                    height = height,
                    culprit = ?culprit,
                    kind = ?kind,
                    "Equivocation evidence broadcast successfully"
                );
                Ok(())
            }
            Ok(Err(e)) => {
                error!(
                    correlation_id = %correlation_id,
                    height = height,
                    culprit = ?culprit,
                    error = %e,
                    "NetworkActor rejected evidence broadcast"
                );
                Err(ChainError::Network(e))
            }
            Err(e) => {
                error!(
                    correlation_id = %correlation_id,
                    height = height,
                    culprit = ?culprit,
                    error = %e,
                    "Failed to send evidence to NetworkActor"
                );
                Err(ChainError::Internal(format!("Mailbox error: {}", e)))
            }
        }
    }

    /// Handle equivocation evidence received from the network.
    ///
    /// Validates the evidence and stores it for potential slashing/governance action.
    ///
    /// # Validation Steps
    /// 1. Deduplication: Check if we've already processed this evidence
    /// 2. Get historical validator set for the evidence height
    /// 3. Verify both vote signatures against the historical validator set
    /// 4. Verify culprit matches both votes
    /// 5. Verify votes conflict (same h/r/type, different block_hash)
    pub async fn handle_tendermint_evidence(
        &self,
        evidence: EquivocationEvidence,
        peer_id: Option<String>,
        correlation_id: Uuid,
    ) -> TendermintResult<(ValidatorId, u64)> {
        let height = evidence.height;
        let culprit = evidence.culprit;
        let kind = evidence.kind;

        info!(
            correlation_id = %correlation_id,
            height = height,
            culprit = ?culprit,
            kind = ?kind,
            peer_id = ?peer_id,
            "Processing equivocation evidence from network"
        );

        // Compute evidence hash for deduplication
        let chain_id = format!("{}", self.config.chain_id);
        let evidence_hash = evidence.evidence_hash(&chain_id);

        // Issue 1 Fix: Check if we've already processed this evidence
        let tendermint_state = self
            .tendermint_state
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Tendermint state not initialized".into()))?;

        {
            let state = tendermint_state.read().await;
            if state.processed_evidence.contains(&evidence_hash) {
                debug!(
                    correlation_id = %correlation_id,
                    height = height,
                    culprit = ?culprit,
                    "Evidence already processed, skipping"
                );
                return Ok((culprit, height));
            }
        }

        // Issue 2 Fix: Get historical validator set for the evidence height
        // Evidence could be from a past height where the validator set was different
        let storage = self
            .storage_actor
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("StorageActor not configured".into()))?;

        let historical_validator_set = storage
            .send(GetValidatorSetForHeightMessage {
                height,
                correlation_id: Some(correlation_id),
            })
            .await
            .map_err(|e| ChainError::Internal(format!("Mailbox error: {}", e)))?
            .map_err(|e| ChainError::Storage(e.to_string()))?
            .ok_or_else(|| {
                ChainError::Consensus(format!(
                    "No validator set found for evidence height {}",
                    height
                ))
            })?;

        // For evidence verification, we use the height/round from the vote itself
        let vote_a_height = evidence.vote_a.height;
        let vote_a_round = evidence.vote_a.round;
        let vote_b_height = evidence.vote_b.height;
        let vote_b_round = evidence.vote_b.round;

        // Verify vote_a signature against historical validator set
        if let Err(e) = verify_vote(
            &evidence.vote_a,
            &historical_validator_set,
            vote_a_height,
            vote_a_round,
            &chain_id,
        ) {
            warn!(
                correlation_id = %correlation_id,
                culprit = ?culprit,
                error = ?e,
                "Evidence vote_a signature verification failed"
            );
            return Err(ChainError::Consensus(format!(
                "Invalid evidence: vote_a signature verification failed: {:?}",
                e
            )));
        }

        // Verify vote_b signature against historical validator set
        if let Err(e) = verify_vote(
            &evidence.vote_b,
            &historical_validator_set,
            vote_b_height,
            vote_b_round,
            &chain_id,
        ) {
            warn!(
                correlation_id = %correlation_id,
                culprit = ?culprit,
                error = ?e,
                "Evidence vote_b signature verification failed"
            );
            return Err(ChainError::Consensus(format!(
                "Invalid evidence: vote_b signature verification failed: {:?}",
                e
            )));
        }

        // Verify both votes are from the same validator (the culprit)
        if evidence.vote_a.validator != culprit || evidence.vote_b.validator != culprit {
            warn!(
                correlation_id = %correlation_id,
                culprit = ?culprit,
                vote_a_validator = ?evidence.vote_a.validator,
                vote_b_validator = ?evidence.vote_b.validator,
                "Evidence votes are not from the claimed culprit"
            );
            return Err(ChainError::Consensus(
                "Invalid evidence: votes are not from the claimed culprit".into(),
            ));
        }

        // Verify the votes conflict (same height/round but different block_hash)
        if evidence.vote_a.height != evidence.vote_b.height
            || evidence.vote_a.round != evidence.vote_b.round
            || evidence.vote_a.vote_type != evidence.vote_b.vote_type
        {
            warn!(
                correlation_id = %correlation_id,
                "Evidence votes are not for the same height/round/type"
            );
            return Err(ChainError::Consensus(
                "Invalid evidence: votes must be for same height/round/type".into(),
            ));
        }

        if evidence.vote_a.block_hash == evidence.vote_b.block_hash {
            warn!(
                correlation_id = %correlation_id,
                "Evidence votes have same block_hash - not equivocation"
            );
            return Err(ChainError::Consensus(
                "Invalid evidence: votes have same block_hash".into(),
            ));
        }

        // Mark evidence as processed to prevent re-processing
        {
            let mut state = tendermint_state.write().await;
            state.processed_evidence.insert(evidence_hash);
        }

        // Evidence is valid - log it prominently
        error!(
            correlation_id = %correlation_id,
            height = height,
            round = evidence.vote_a.round,
            culprit = ?culprit,
            kind = ?kind,
            vote_a_hash = ?evidence.vote_a.block_hash,
            vote_b_hash = ?evidence.vote_b.block_hash,
            "EQUIVOCATION DETECTED: Validator {} double-voted at height {} round {}",
            culprit.0,
            height,
            evidence.vote_a.round
        );

        // TODO: Store evidence for slashing/governance
        // This would involve:
        // 1. Storing to persistent evidence database
        // 2. Triggering governance action to slash the validator
        // 3. Broadcasting evidence to other validators who haven't seen it

        Ok((culprit, height))
    }

    /// Commit a block after receiving 2/3+ precommits.
    ///
    /// This is the finalization step of Tendermint consensus. Once we have 2/3+
    /// precommits for a block, it is considered finalized with instant finality.
    async fn commit_block(
        &self,
        height: u64,
        round: u32,
        block_hash: BlockHash,
        correlation_id: Uuid,
    ) -> TendermintResult<()> {
        use crate::actors_v2::engine::{EngineMessage, EngineResponse};
        use crate::actors_v2::storage::messages::AtomicCommitBlockMessage;
        use crate::actors_v2::storage::actor::BlockRef;
        use lighthouse_wrapper::types::ExecutionPayload;

        info!(
            correlation_id = %correlation_id,
            height = height,
            round = round,
            block_hash = %H256::from_slice(block_hash.as_bytes()),
            "COMMITTING BLOCK"
        );

        let tendermint_state = self
            .tendermint_state
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Tendermint state not initialized".into()))?;

        let wal = self
            .consensus_wal
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("WAL not initialized".into()))?;

        // 1. Retrieve the block from proposals
        let block = {
            let state = tendermint_state.read().await;
            let proposal = state.current_proposal.as_ref()
                .ok_or_else(|| ChainError::Internal("No proposal found for commit".into()))?;

            // Verify the block hash matches
            if proposal.block_hash() != block_hash {
                return Err(ChainError::Consensus(format!(
                    "Block hash mismatch: expected {:?}, got {:?}",
                    block_hash, proposal.block_hash()
                )));
            }

            proposal.block.clone()
        };

        // 2. Build Commit from precommit votes
        let commit = {
            let state = tendermint_state.read().await;
            let precommits = state.precommits.read().await;
            let signatures = precommits.build_commit_sigs(block_hash);

            Commit {
                height,
                round,
                block_hash,
                signatures,
            }
        };

        // 3. Write commit WAL entry BEFORE any state changes
        // Also write NewRound entry for H+1 to ensure clean recovery
        {
            let mut wal_guard = wal.write().await;
            wal_guard
                .write(WALEntry::Commit {
                    height,
                    block_hash,
                })
                .map_err(|e| ChainError::Internal(format!("WAL write failed: {}", e)))?;

            // Immediately write NewRound for next height to ensure clean recovery.
            // This ensures that after crash recovery, we start at the correct
            // height and round (H+1, round 0) instead of being stuck at the
            // committed height with round 0.
            wal_guard
                .write(WALEntry::NewRound {
                    height: height + 1,
                    round: 0,
                })
                .map_err(|e| ChainError::Internal(format!("WAL write failed: {}", e)))?;
        }

        // 4. Execute block via EngineActor (instant finality)
        let execution_hash = if let Some(ref engine) = self.engine_actor {
            let parent_hash = block.execution_payload.parent_hash;

            let engine_result = engine.send(EngineMessage::ExecuteBlock {
                execution_payload: ExecutionPayload::Capella(block.execution_payload.clone()),
                parent_hash,
                correlation_id: Some(correlation_id),
            }).await
                .map_err(|e| ChainError::Internal(format!("Engine mailbox error: {}", e)))?
                .map_err(|e| ChainError::Engine(format!("Block execution failed: {}", e)))?;

            match engine_result {
                EngineResponse::BlockExecuted { block_hash: exec_hash, .. } => exec_hash,
                other => return Err(ChainError::Engine(format!("Unexpected response: {:?}", other))),
            }
        } else {
            warn!(correlation_id = %correlation_id, "EngineActor not configured - skipping execution");
            lighthouse_wrapper::types::ExecutionBlockHash::zero()
        };

        // 5. Atomically store block + update chain head (survives SIGKILL)
        //
        // This uses a single WriteBatch with sync to ensure block data, height index,
        // and chain head are all written atomically. This prevents WAL-storage mismatch
        // where WAL shows committed blocks but storage reports height 0 after crash.
        if let Some(ref storage) = self.storage_actor {
            let signed_block = crate::block::SignedConsensusBlock {
                message: block.clone(),
                signature: crate::signatures::AggregateApproval::new(),
            };

            let block_ref = BlockRef {
                hash: H256::from_slice(block_hash.as_bytes()),
                number: height,
                execution_hash,
            };

            storage.send(AtomicCommitBlockMessage {
                block: signed_block,
                new_head: block_ref.clone(),
                correlation_id: Some(correlation_id),
            }).await
                .map_err(|e| ChainError::Internal(format!("Storage mailbox error: {}", e)))?
                .map_err(|e| ChainError::Storage(format!("Atomic commit failed: {}", e)))?;

            // Update local state after storage confirms
            self.state.update_head(block_ref).await;

            // 6b. Mark all peg-ins in this block as processed (Doc 16 Layer 2: deduplication)
            // This happens AFTER commit to ensure peg-ins aren't lost if commit fails
            // Path B: Peg-ins are stored in auxpow_header.pegins (via pegins() helper)
            for pegin_info in block.pegins() {
                self.state.mark_pegin_processed(pegin_info.txid).await;
            }

            if !block.pegins().is_empty() {
                debug!(
                    correlation_id = %correlation_id,
                    pegins_finalized = block.pegins().len(),
                    "Marked peg-ins as processed after commit"
                );
            }
        } else {
            warn!(correlation_id = %correlation_id, "StorageActor not configured - skipping storage");
        }

        // 7. Cache the commit for the next block's last_commit
        if let Some(ref cached_commit) = self.cached_last_commit {
            let mut guard = cached_commit.write().await;
            *guard = commit.clone();
        }

        // 8. Advance state to Commit step
        {
            let mut state = tendermint_state.write().await;
            state.set_step(TendermintStep::Commit);
        }

        // 9. Notify network peers of committed block
        if let Some(ref network) = self.network_actor {
            let message = super::tendermint::TendermintMessage::NewRound {
                height: height + 1,
                round: 0,
                highest_known_round: 0,
            };

            network.send(crate::actors_v2::network::messages::NetworkMessage::BroadcastTendermint {
                message,
                correlation_id: Some(correlation_id),
            }).await.ok(); // Non-critical
        }

        info!(
            correlation_id = %correlation_id,
            height = height,
            round = round,
            commit_signatures = commit.signatures.len(),
            "Block committed successfully"
        );

        // 9b. Disable recovery mode after successful commit
        // Once we've committed a block, we've rejoined consensus and no longer
        // need the extended future round acceptance window.
        {
            let mut state = tendermint_state.write().await;
            if state.future_messages.is_recovery_mode() {
                state.future_messages.disable_recovery_mode();
                info!(
                    correlation_id = %correlation_id,
                    height = height,
                    "Recovery mode disabled after successful commit"
                );
            }
        }

        // 10. Initialize state machine for H+1 BEFORE notifying TendermintDriver
        // This ensures the state machine is reset (locks cleared) before the driver
        // tries to start consensus for the new height.
        self.handle_tendermint_new_height(height + 1, correlation_id).await?;

        // 11. Notify TendermintDriver of commit (after state machine is ready)
        if let Some(ref driver) = self.tendermint_driver {
            driver.do_send(crate::actors_v2::tendermint_driver::TendermintDriverMessage::Committed {
                height,
                last_commit: commit.clone(),
            });
        }

        Ok(())
    }

    // =========================================================================
    // Height Synchronization - Future Height Vote Handling
    // =========================================================================

    /// Handle a vote from a future height (node may be behind).
    ///
    /// When we receive votes for heights we haven't reached yet, it indicates
    /// that other nodes have committed blocks we haven't seen. This triggers
    /// catch-up sync with debouncing to avoid thrashing.
    ///
    /// # Debounce Logic
    /// - Trigger sync after: 3+ future votes OR 500ms since first vote
    /// - Cooldown: 5 seconds between sync triggers
    /// - Max gap check: Log warning if gap > 100 blocks
    ///
    /// # Actions
    /// - Update future height tracker
    /// - Check debounce conditions
    /// - If conditions met: pause consensus and trigger sync
    async fn handle_future_height_vote(
        &self,
        vote_height: u64,
        current_height: u64,
        voter: ValidatorId,
        correlation_id: Uuid,
    ) -> TendermintResult<ValidatorId> {
        // Configuration constants
        const MIN_VOTES_FOR_SYNC: u32 = 3;
        const DEBOUNCE_DURATION: std::time::Duration = std::time::Duration::from_millis(500);
        const SYNC_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(5);
        const MAX_REASONABLE_GAP: u64 = 100;

        let height_gap = vote_height.saturating_sub(current_height);

        // Log warning for unusually large gaps
        if height_gap > MAX_REASONABLE_GAP {
            warn!(
                correlation_id = %correlation_id,
                vote_height = vote_height,
                current_height = current_height,
                gap = height_gap,
                "Received vote with very large height gap - may indicate serious sync issue"
            );
        }

        // Update tracker and check if we should trigger sync
        let should_trigger_sync = {
            let mut tracker = self.future_height_tracker.write().await;

            // Track the vote
            if vote_height > tracker.max_observed_height {
                tracker.max_observed_height = vote_height;
            }
            tracker.vote_count += 1;

            // Set first vote timestamp if not already set
            if tracker.first_vote_at.is_none() {
                tracker.first_vote_at = Some(std::time::Instant::now());
            }

            let now = std::time::Instant::now();

            // Check cooldown
            if let Some(last_trigger) = tracker.last_sync_trigger_at {
                if now.duration_since(last_trigger) < SYNC_COOLDOWN {
                    debug!(
                        correlation_id = %correlation_id,
                        vote_height = vote_height,
                        current_height = current_height,
                        vote_count = tracker.vote_count,
                        "Future height vote - still in cooldown period"
                    );
                    return Ok(voter);
                }
            }

            // Check if debounce conditions are met
            let votes_threshold_met = tracker.vote_count >= MIN_VOTES_FOR_SYNC;
            let time_threshold_met = tracker.first_vote_at
                .map(|t| now.duration_since(t) >= DEBOUNCE_DURATION)
                .unwrap_or(false);

            if votes_threshold_met || time_threshold_met {
                // Mark sync trigger time
                tracker.last_sync_trigger_at = Some(now);
                true
            } else {
                debug!(
                    correlation_id = %correlation_id,
                    vote_height = vote_height,
                    current_height = current_height,
                    vote_count = tracker.vote_count,
                    "Future height vote - debouncing (waiting for more votes or time)"
                );
                false
            }
        };

        if should_trigger_sync {
            info!(
                correlation_id = %correlation_id,
                vote_height = vote_height,
                current_height = current_height,
                gap = height_gap,
                "Detected future height votes - triggering catch-up sync"
            );

            // Pause consensus before starting sync
            if let Some(ref driver) = self.tendermint_driver {
                info!(
                    correlation_id = %correlation_id,
                    "Pausing consensus before catch-up sync"
                );
                driver.do_send(crate::actors_v2::tendermint_driver::TendermintDriverMessage::Pause);
            }

            // Trigger sync to catch up
            if let Some(ref sync_actor) = self.sync_actor {
                info!(
                    correlation_id = %correlation_id,
                    target_height = vote_height,
                    "Starting catch-up sync to height {}", vote_height
                );
                sync_actor.do_send(crate::actors_v2::network::SyncMessage::StartSync {
                    start_height: current_height,
                    target_height: Some(vote_height),
                });
            } else {
                warn!(
                    correlation_id = %correlation_id,
                    "Cannot trigger catch-up sync - no SyncActor reference"
                );
            }
        }

        // Return voter - we don't process the vote, but don't error either
        Ok(voter)
    }

    // =========================================================================
    // Round Synchronization - Future Round Vote Handling
    // =========================================================================

    /// Handle a vote from a future round.
    ///
    /// Per Tendermint spec, when we receive 2/3+ votes from a higher round,
    /// we should advance to that round. This enables round synchronization
    /// when nodes start at different times.
    ///
    /// # Actions
    /// - Validate vote signature (must verify before storing)
    /// - Store vote in future_messages
    /// - Check if threshold reached for any action
    /// - If threshold reached: advance to appropriate round/step
    async fn handle_future_round_vote(
        &self,
        vote: Vote,
        current_round: u32,
        validator_set: &Arc<super::tendermint::ValidatorSet>,
        correlation_id: Uuid,
    ) -> TendermintResult<ValidatorId> {
        let voter = vote.validator;
        let vote_round = vote.round;
        let vote_type = vote.vote_type;

        // IMPORTANT: Validate signature before storing (HIGH-1 fix)
        let chain_id_str = self.config.chain_id.to_string();

        // Check voter is in validator set
        if vote.validator.index() as usize >= validator_set.len() {
            return Err(ChainError::Consensus(format!(
                "Unknown validator: {:?}",
                vote.validator
            )));
        }

        // Verify signature
        let public_key = validator_set
            .get_public_key(&vote.validator)
            .map_err(|e| ChainError::Consensus(format!("Validator public key error: {}", e)))?;

        if !vote.verify_signature(public_key, &chain_id_str) {
            warn!(
                correlation_id = %correlation_id,
                validator = ?vote.validator,
                round = vote.round,
                "Rejecting future vote with invalid signature"
            );
            return Err(ChainError::Consensus("Invalid vote signature".into()));
        }

        debug!(
            correlation_id = %correlation_id,
            vote_round = vote_round,
            current_round = current_round,
            vote_type = ?vote_type,
            voter = ?voter,
            "Storing validated vote for future round"
        );

        // Store vote and check if we should take action.
        // Include our voting power in threshold calculation - this is critical for
        // n=3 networks where 2/3+ threshold is 100%. When we're behind (e.g., just
        // restarted at round 0 while others are at round N), we need to recognize
        // that "peer votes + our future vote = threshold" so we can advance.
        let action = {
            let tendermint_state = self.tendermint_state.as_ref().unwrap();
            let mut state = tendermint_state.write().await;

            // Get our voting power to include in threshold calculation.
            // Once we advance to this round, we WILL vote (Tendermint guarantee),
            // so it's safe to include our power when deciding whether to advance.
            let our_power = state.our_validator_id.and_then(|id| {
                state.validator_set.get_power(&id).ok()
            });

            state.future_messages.store_vote_with_self_power(vote, our_power)
        };

        // Handle the action based on what threshold was reached
        match action {
            FutureRoundAction::NoAction => {
                // Not enough votes yet
                Ok(voter)
            }

            FutureRoundAction::CommitBlock { round, block_hash } => {
                // CRIT-4: 2/3+ precommits for a block = COMMIT
                info!(
                    correlation_id = %correlation_id,
                    round = round,
                    block_hash = %H256::from_slice(block_hash.as_bytes()),
                    "Received 2/3+ precommits from future round - committing block"
                );
                self.commit_from_future_round(round, block_hash, correlation_id)
                    .await?;
                Ok(voter)
            }

            FutureRoundAction::AdvanceToNextRound {
                nil_round,
                target_round,
            } => {
                // 2/3+ NIL precommits - advance to NEXT round (not the NIL round)
                info!(
                    correlation_id = %correlation_id,
                    nil_round = nil_round,
                    target_round = target_round,
                    "Received 2/3+ NIL precommits - advancing to next round"
                );
                self.advance_to_round(target_round, TendermintStep::Propose, correlation_id)
                    .await?;
                Ok(voter)
            }

            FutureRoundAction::AdvanceToPrevote { round, polka_block } => {
                // CRIT-2 fix: Enter Prevote step, not Propose
                info!(
                    correlation_id = %correlation_id,
                    target_round = round,
                    polka_block = ?polka_block,
                    "Received 2/3+ prevotes - advancing to Prevote step"
                );
                self.advance_to_round(round, TendermintStep::Prevote, correlation_id)
                    .await?;
                Ok(voter)
            }

            FutureRoundAction::AdvanceToPrecommit { round, block_hash } => {
                // CRIT-2 fix: Enter Precommit step, not Propose
                info!(
                    correlation_id = %correlation_id,
                    target_round = round,
                    block_hash = ?block_hash,
                    "Received 2/3+ precommits - advancing to Precommit step"
                );
                self.advance_to_round(round, TendermintStep::Precommit, correlation_id)
                    .await?;
                Ok(voter)
            }
        }
    }

    /// Handle a vote from a past round.
    ///
    /// Past round votes are stored for:
    /// - POL (Proof-of-Lock) verification
    /// - Equivocation evidence
    async fn handle_past_round_vote(
        &self,
        vote: Vote,
        current_round: u32,
        correlation_id: Uuid,
    ) -> TendermintResult<ValidatorId> {
        let voter = vote.validator;

        debug!(
            correlation_id = %correlation_id,
            vote_round = vote.round,
            current_round = current_round,
            voter = ?voter,
            "Storing vote from past round for evidence/POL"
        );

        // Store in historical votes if we have a VoteSet for this round
        let tendermint_state = self.tendermint_state.as_ref().unwrap();
        let state = tendermint_state.read().await;

        if let Some(historical) = state.historical_prevotes.get(&vote.round) {
            // Add to historical prevotes (for POL verification)
            let mut prevotes = historical.write().await;
            let _ = prevotes.add_vote(vote);
        }

        Ok(voter)
    }

    /// Advance to a higher round with proper locking state handling.
    ///
    /// This implements CRIT-3: PoLC verification for unlocking during round skip.
    async fn advance_to_round(
        &self,
        target_round: u32,
        target_step: TendermintStep,
        correlation_id: Uuid,
    ) -> TendermintResult<u32> {
        let tendermint_state = self
            .tendermint_state
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("State not initialized".into()))?;

        let timeout_scheduler = self
            .timeout_scheduler
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("Scheduler not initialized".into()))?;

        let wal = self
            .consensus_wal
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("WAL not initialized".into()))?;

        let (current_round, height) = {
            let state = tendermint_state.read().await;
            (state.round, state.height)
        };

        if target_round <= current_round {
            debug!(
                correlation_id = %correlation_id,
                target_round = target_round,
                current_round = current_round,
                "Ignoring advancement to non-future round"
            );
            return Ok(current_round);
        }

        info!(
            correlation_id = %correlation_id,
            current_round = current_round,
            target_round = target_round,
            target_step = ?target_step,
            "Advancing to higher round via future round votes"
        );

        // MED-2 FIX: Write WAL BEFORE state update (write-ahead logging)
        {
            let mut wal_guard = wal.write().await;
            wal_guard
                .write(WALEntry::NewRound {
                    height,
                    round: target_round,
                })
                .map_err(|e| ChainError::Internal(format!("WAL write failed: {}", e)))?;
        }

        // Update state with proper locking handling
        {
            let mut state = tendermint_state.write().await;

            // CRIT-3: Check for PoLC that would allow unlocking
            if let Some(locked_round) = state.locked_round {
                if let Some(locked_block) = &state.locked_block {
                    // Check intermediate rounds for PoLC on a different block
                    if let Some((polc_round, polc_block)) = state
                        .future_messages
                        .find_polc_for_unlock(locked_round, locked_block, target_round)
                    {
                        info!(
                            correlation_id = %correlation_id,
                            locked_round = locked_round,
                            polc_round = polc_round,
                            polc_block = %H256::from_slice(polc_block.as_bytes()),
                            "Unlocking due to PoLC from intermediate round"
                        );
                        state.unlock();
                    }
                }
            }

            // Advance future message store
            state.future_messages.advance_to_round(target_round);

            // Update round and step
            state.round = target_round;
            state.step = target_step;

            // Create new vote sets for the new round
            state.prevotes = Arc::new(tokio::sync::RwLock::new(VoteSet::new(
                state.height,
                target_round,
                VoteType::Prevote,
                state.validator_set.clone(),
            )));
            state.precommits = Arc::new(tokio::sync::RwLock::new(VoteSet::new(
                state.height,
                target_round,
                VoteType::Precommit,
                state.validator_set.clone(),
            )));

            // Apply any stored votes for this round
            if let Some(future_votes) = state.future_messages.take_votes(target_round) {
                for vote in future_votes.prevotes() {
                    // Re-validate against current validator set
                    if (vote.validator.index() as usize) < state.validator_set.len() {
                        let mut prevotes = state.prevotes.write().await;
                        let _ = prevotes.add_vote(vote.clone());
                    }
                }
                for vote in future_votes.precommits() {
                    if (vote.validator.index() as usize) < state.validator_set.len() {
                        let mut precommits = state.precommits.write().await;
                        let _ = precommits.add_vote(vote.clone());
                    }
                }
            }

            // Apply stored proposal if exists
            let had_stored_proposal =
                if let Some(proposal) = state.future_messages.take_proposal(target_round) {
                    info!(
                        correlation_id = %correlation_id,
                        height = height,
                        round = target_round,
                        proposer = ?proposal.proposer,
                        block_hash = %H256::from_slice(proposal.block_hash().as_bytes()),
                        "Replaying stored proposal from future round"
                    );
                    state.current_proposal = Some(proposal.clone());
                    state
                        .proposals
                        .insert((target_round, proposal.proposer), proposal);
                    true
                } else {
                    false
                };

            // Clear current_proposal if we're starting fresh (but NOT if we just replayed one)
            if target_step == TendermintStep::Propose && !had_stored_proposal {
                state.current_proposal = None;
            }
        }

        // Update timeout scheduler
        {
            let mut scheduler = timeout_scheduler.write().await;
            scheduler.set_position(height, target_round);
            let _ = scheduler.schedule(target_step);
        }

        // Notify TendermintDriver of round change
        if let Some(ref driver) = self.tendermint_driver {
            driver.do_send(
                crate::actors_v2::tendermint_driver::TendermintDriverMessage::RoundAdvanced {
                    round: target_round,
                    step: target_step,
                },
            );
        }

        // After advancing, check if we should take any action in the new step
        self.check_step_actions_after_advance(target_round, target_step, correlation_id)
            .await?;

        Ok(target_round)
    }

    /// Check if we need to take action after advancing to a new round/step.
    ///
    /// When we skip to a higher round via future round votes, we may already have
    /// enough information to immediately vote.
    async fn check_step_actions_after_advance(
        &self,
        round: u32,
        step: TendermintStep,
        correlation_id: Uuid,
    ) -> TendermintResult<()> {
        let tendermint_state = self.tendermint_state.as_ref().unwrap();

        match step {
            TendermintStep::Prevote => {
                // If we have a proposal and haven't voted, we should prevote
                let should_prevote = {
                    let state = tendermint_state.read().await;
                    state.current_proposal.is_some() && !state.sent_prevotes.contains_key(&round)
                };

                if should_prevote {
                    debug!(
                        correlation_id = %correlation_id,
                        round = round,
                        "Triggering prevote after round advancement"
                    );

                    let (proposal, locked_block) = {
                        let state = tendermint_state.read().await;
                        (state.current_proposal.clone(), state.locked_block)
                    };

                    if let Some(proposal) = proposal {
                        // Determine what to vote for based on locking rules
                        let vote_block = self
                            .determine_prevote_block(&proposal, locked_block.as_ref())
                            .await?;

                        // Cast the prevote
                        let state = tendermint_state.read().await;
                        self.cast_prevote(state.height, round, vote_block, correlation_id)
                            .await?;
                    }
                }
            }

            TendermintStep::Precommit => {
                // Check if we have 2/3+ prevotes for a block and should precommit
                let (should_precommit, precommit_block) = {
                    let state = tendermint_state.read().await;
                    let prevotes = state.prevotes.read().await;
                    let has_polka = prevotes.has_two_thirds_any();
                    let block = prevotes.two_thirds_majority();
                    let not_voted = !state.sent_precommits.contains_key(&round);
                    (has_polka && not_voted, block)
                };

                if should_precommit {
                    debug!(
                        correlation_id = %correlation_id,
                        round = round,
                        block_hash = ?precommit_block,
                        "Triggering precommit after round advancement"
                    );

                    let state = tendermint_state.read().await;
                    self.cast_precommit(state.height, round, precommit_block, correlation_id)
                        .await?;

                    // If we precommitted for a block, update our lock
                    if let Some(block_hash) = precommit_block {
                        drop(state);
                        let mut state = tendermint_state.write().await;
                        // Only update lock if this is a newer round
                        if state.locked_round.map_or(true, |lr| round > lr) {
                            state.lock_on(round, block_hash);
                            debug!(
                                correlation_id = %correlation_id,
                                round = round,
                                block_hash = %H256::from_slice(block_hash.as_bytes()),
                                "Updated lock after precommit"
                            );
                        }
                    }
                }
            }

            TendermintStep::Propose => {
                // Check if we already have a proposal (replayed from future storage)
                let (has_proposal, should_propose) = {
                    let state = tendermint_state.read().await;
                    let has_proposal = state.current_proposal.is_some();
                    let should_propose = if let Some(our_id) = state.our_validator_id {
                        let expected_proposer = state.validator_set.get_proposer(state.height, round);
                        our_id == expected_proposer && !has_proposal
                    } else {
                        false
                    };
                    (has_proposal, should_propose)
                };

                if has_proposal {
                    // We have a replayed proposal - transition to Prevote and cast vote
                    info!(
                        correlation_id = %correlation_id,
                        round = round,
                        "Have replayed proposal - transitioning to Prevote step"
                    );

                    // Transition to Prevote step
                    {
                        let mut state = tendermint_state.write().await;
                        state.set_step(TendermintStep::Prevote);
                    }

                    // Schedule prevote timeout
                    if let Some(ref scheduler) = self.timeout_scheduler {
                        let state = tendermint_state.read().await;
                        let mut scheduler = scheduler.write().await;
                        scheduler.set_position(state.height, round);
                        let _ = scheduler.schedule(TendermintStep::Prevote);
                    }

                    // Cast prevote for the replayed proposal
                    let (proposal, locked_block, height) = {
                        let state = tendermint_state.read().await;
                        (
                            state.current_proposal.clone(),
                            state.locked_block,
                            state.height,
                        )
                    };

                    if let Some(proposal) = proposal {
                        // Validate execution payload before voting
                        let execution_valid = self
                            .validate_block_execution(&proposal.block, correlation_id)
                            .await;

                        let vote_block = if execution_valid {
                            self.determine_prevote_block(&proposal, locked_block.as_ref())
                                .await?
                        } else {
                            warn!(
                                correlation_id = %correlation_id,
                                round = round,
                                "Replayed proposal execution validation failed - voting NIL"
                            );
                            None
                        };

                        self.cast_prevote(height, round, vote_block, correlation_id)
                            .await?;
                    }
                } else if should_propose {
                    debug!(
                        correlation_id = %correlation_id,
                        round = round,
                        "We are proposer after round advancement - creating proposal"
                    );
                    let state = tendermint_state.read().await;
                    self.handle_tendermint_propose(state.height, round, correlation_id)
                        .await?;
                }
            }

            TendermintStep::Commit => {
                // Nothing to do - commit step is handled separately
            }
        }

        Ok(())
    }

    /// Determine what block to prevote for based on Tendermint locking rules.
    async fn determine_prevote_block(
        &self,
        proposal: &Proposal,
        locked_block: Option<&BlockHash>,
    ) -> TendermintResult<Option<BlockHash>> {
        // Tendermint locking rules:
        // 1. If we're locked on a block, we must prevote for it (unless unlocked via PoLC)
        // 2. If not locked, we can prevote for the proposed block if valid
        // 3. If proposal is invalid, prevote NIL

        if let Some(locked) = locked_block {
            let proposal_hash = proposal.block_hash();
            if &proposal_hash == locked {
                // Proposal matches lock - vote for it
                return Ok(Some(proposal_hash));
            }
            // We're locked on a different block - vote for our locked block
            return Ok(Some(*locked));
        }

        // Not locked - vote for the proposed block
        Ok(Some(proposal.block_hash()))
    }

    /// Commit a block discovered from future round votes.
    async fn commit_from_future_round(
        &self,
        round: u32,
        block_hash: BlockHash,
        correlation_id: Uuid,
    ) -> TendermintResult<()> {
        info!(
            correlation_id = %correlation_id,
            round = round,
            block_hash = %H256::from_slice(block_hash.as_bytes()),
            "Committing block from future round precommits"
        );

        let tendermint_state = self.tendermint_state.as_ref().unwrap();
        let state = tendermint_state.read().await;
        let height = state.height;

        // Try to find the block in stored proposals
        let block = state
            .proposals
            .values()
            .find(|p| p.block_hash() == block_hash)
            .map(|p| p.block.clone());

        drop(state);

        if let Some(_block) = block {
            // We have the block - commit it
            self.commit_block(height, round, block_hash, correlation_id)
                .await
        } else {
            // We don't have the block - need to request it
            warn!(
                correlation_id = %correlation_id,
                block_hash = %H256::from_slice(block_hash.as_bytes()),
                "Need to fetch block for commit - block not in local storage"
            );
            // Request block from peers and commit when received
            self.request_block_for_commit(block_hash, round, correlation_id)
                .await
        }
    }

    /// Request a block from peers when we need to commit but don't have the block data.
    ///
    /// This also schedules a timeout that will trigger retry logic if the block
    /// isn't received in time.
    async fn request_block_for_commit(
        &self,
        block_hash: BlockHash,
        round: u32,
        correlation_id: Uuid,
    ) -> TendermintResult<()> {
        info!(
            correlation_id = %correlation_id,
            block_hash = %H256::from_slice(block_hash.as_bytes()),
            round = round,
            "Requesting block for pending commit"
        );

        // Store the pending commit state (only on first request, not retries)
        {
            let tendermint_state = self.tendermint_state.as_ref().unwrap();
            let mut state = tendermint_state.write().await;
            if state.pending_commit.is_none() {
                state.pending_commit = Some(PendingCommit::new(block_hash, round, correlation_id));
            }
            // Note: If pending_commit exists, this is a retry - keep existing state with updated retry_count
        }

        // Send block request to network
        if let Some(ref network) = self.network_actor {
            let height = {
                let state = self.tendermint_state.as_ref().unwrap().read().await;
                state.height
            };

            // Use existing BlockRequest mechanism
            let message = TendermintMessage::BlockRequest { height };

            network
                .send(crate::actors_v2::network::messages::NetworkMessage::BroadcastTendermint {
                    message,
                    correlation_id: Some(correlation_id),
                })
                .await
                .map_err(|e| ChainError::Internal(format!("Failed to send block request: {}", e)))?;

            debug!(
                correlation_id = %correlation_id,
                height = height,
                block_hash = %H256::from_slice(block_hash.as_bytes()),
                "Sent block request to network"
            );
        } else {
            return Err(ChainError::Configuration(
                "Network actor not available for block request".into(),
            ));
        }

        // Schedule timeout for block request
        if let Some(ref driver) = self.tendermint_driver {
            let driver = driver.clone();
            let timeout_duration = std::time::Duration::from_secs(Self::BLOCK_REQUEST_TIMEOUT_SECS);

            tokio::spawn(async move {
                tokio::time::sleep(timeout_duration).await;
                driver.do_send(
                    crate::actors_v2::tendermint_driver::TendermintDriverMessage::BlockRequestTimeout {
                        block_hash,
                        correlation_id,
                    },
                );
            });

            debug!(
                correlation_id = %correlation_id,
                timeout_secs = Self::BLOCK_REQUEST_TIMEOUT_SECS,
                "Scheduled block request timeout"
            );
        }

        Ok(())
    }

    /// Handle a received block that we requested for a pending commit.
    pub async fn handle_requested_block_for_commit(
        &self,
        block_hash: BlockHash,
        correlation_id: Uuid,
    ) -> TendermintResult<()> {
        let tendermint_state = self
            .tendermint_state
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("State not initialized".into()))?;

        // Check if this block matches our pending commit
        let pending_commit = {
            let state = tendermint_state.read().await;
            state.pending_commit.clone()
        };

        if let Some(pending) = pending_commit {
            if block_hash == pending.block_hash {
                info!(
                    correlation_id = %correlation_id,
                    block_hash = %H256::from_slice(block_hash.as_bytes()),
                    round = pending.round,
                    "Received requested block for pending commit"
                );

                // Clear the pending commit
                {
                    let mut state = tendermint_state.write().await;
                    state.pending_commit = None;
                }

                // Get height and commit the block
                let height = {
                    let state = tendermint_state.read().await;
                    state.height
                };

                self.commit_block(height, pending.round, block_hash, pending.correlation_id)
                    .await
            } else {
                debug!(
                    correlation_id = %correlation_id,
                    expected_hash = %H256::from_slice(pending.block_hash.as_bytes()),
                    received_hash = %H256::from_slice(block_hash.as_bytes()),
                    "Received block doesn't match pending commit"
                );
                Ok(())
            }
        } else {
            debug!(
                correlation_id = %correlation_id,
                "Received block but no pending commit"
            );
            Ok(())
        }
    }

    /// Maximum number of block request retries before giving up.
    const MAX_BLOCK_REQUEST_RETRIES: u32 = 3;

    /// Timeout duration for block requests (in seconds).
    const BLOCK_REQUEST_TIMEOUT_SECS: u64 = 10;

    /// Handle timeout when block request doesn't receive a response.
    ///
    /// This implements retry logic for pending commits:
    /// - If under MAX_BLOCK_REQUEST_RETRIES, retry the request
    /// - If max retries exceeded, clear pending_commit and log error
    pub async fn handle_block_request_timeout(
        &self,
        block_hash: BlockHash,
        correlation_id: Uuid,
    ) -> TendermintResult<()> {
        let tendermint_state = self
            .tendermint_state
            .as_ref()
            .ok_or_else(|| ChainError::Configuration("State not initialized".into()))?;

        // Check if this timeout is for our current pending commit
        let pending_commit = {
            let state = tendermint_state.read().await;
            state.pending_commit.clone()
        };

        let Some(pending) = pending_commit else {
            // No pending commit - timeout is stale
            debug!(
                correlation_id = %correlation_id,
                block_hash = %H256::from_slice(block_hash.as_bytes()),
                "Block request timeout but no pending commit - ignoring"
            );
            return Ok(());
        };

        // Check if this timeout matches our pending commit
        if pending.block_hash != block_hash {
            debug!(
                correlation_id = %correlation_id,
                expected_hash = %H256::from_slice(pending.block_hash.as_bytes()),
                timeout_hash = %H256::from_slice(block_hash.as_bytes()),
                "Block request timeout for different block - ignoring"
            );
            return Ok(());
        }

        // Increment retry count
        let retry_count = {
            let mut state = tendermint_state.write().await;
            if let Some(ref mut pc) = state.pending_commit {
                pc.retry_count += 1;
                pc.retry_count
            } else {
                return Ok(()); // Pending commit was cleared while we were waiting
            }
        };

        if retry_count < Self::MAX_BLOCK_REQUEST_RETRIES {
            // Retry the request
            warn!(
                correlation_id = %correlation_id,
                block_hash = %H256::from_slice(block_hash.as_bytes()),
                retry_count = retry_count,
                max_retries = Self::MAX_BLOCK_REQUEST_RETRIES,
                "Block request timed out - retrying"
            );

            // Re-request the block (this will also schedule a new timeout)
            self.request_block_for_commit(block_hash, pending.round, correlation_id)
                .await
        } else {
            // Max retries exceeded - give up
            error!(
                correlation_id = %correlation_id,
                block_hash = %H256::from_slice(block_hash.as_bytes()),
                retry_count = retry_count,
                "Failed to fetch block after {} retries - clearing pending commit",
                Self::MAX_BLOCK_REQUEST_RETRIES
            );

            // Clear the pending commit
            {
                let mut state = tendermint_state.write().await;
                state.pending_commit = None;
            }

            // This is a serious issue - the node saw 2/3+ precommits but can't get the block
            // The block will eventually be synced via normal block sync
            Err(ChainError::Consensus(format!(
                "Failed to fetch block {:?} for commit after {} retries - network may be partitioned",
                block_hash, Self::MAX_BLOCK_REQUEST_RETRIES
            )))
        }
    }
}
