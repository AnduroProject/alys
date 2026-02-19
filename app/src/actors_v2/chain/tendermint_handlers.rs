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
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::tendermint::{
    check_for_equivocation, verify_proposal, verify_vote, BlockHash, Commit, CommitSig,
    ConsensusAction, GovernanceUpdate, Proposal, TendermintStep, TendermintValidationError,
    ValidatorId, Vote, VoteType, WALEntry,
};

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

        // Schedule propose timeout
        {
            let mut scheduler = timeout_scheduler.write().await;
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
            // We're locked - must propose the locked block
            info!(
                correlation_id = %correlation_id,
                locked_hash = %locked_hash,
                locked_round = lr,
                "Proposing locked block"
            );
            // TODO: Retrieve the locked block from storage/cache
            // For now, this is a placeholder - need to implement block caching
            return Err(ChainError::Internal(
                "Locked block proposal not yet implemented".into(),
            ));
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

        // Sign the proposal
        let signing_root = proposal.signing_root();
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

        // Validate proposal
        verify_proposal(&proposal, &validator_set, current_height, current_round).map_err(
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

        // TODO: Validate block via EngineActor (execution layer validation)
        // For now, we trust the proposer's block is valid
        // This should call: self.validate_block_execution(&proposal.block).await?;

        // Store proposal in state and determine prevote target
        let prevote_target = {
            let mut state = tendermint_state.write().await;

            // Check for equivocation (same proposer, different block)
            if let Some(existing) = state.proposals.get(&(round, proposer)) {
                if existing.block_hash() != block_hash {
                    warn!(
                        correlation_id = %correlation_id,
                        proposer = ?proposer,
                        existing_hash = %H256::from_slice(existing.block_hash().as_bytes()),
                        new_hash = %H256::from_slice(block_hash.as_bytes()),
                        "Detected proposal equivocation"
                    );
                    // TODO: Create and broadcast equivocation evidence
                }
            }

            state.current_proposal = Some(proposal.clone());
            state.proposals.insert((round, proposer), proposal.clone());

            // Determine prevote target based on locking rules
            state.determine_prevote_target(&proposal)
        };

        // Advance step to Prevote
        {
            let mut state = tendermint_state.write().await;
            state.set_step(TendermintStep::Prevote);
        }

        // Schedule prevote timeout
        {
            let mut scheduler = timeout_scheduler.write().await;
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

        // Validate vote
        verify_vote(&vote, &validator_set, current_height, current_round).map_err(|e| {
            ChainError::Consensus(format!("Vote validation failed: {}", e))
        })?;

        // Add vote to appropriate VoteSet and check thresholds
        let action = {
            let mut state = tendermint_state.write().await;

            match vote_type {
                VoteType::Prevote => {
                    // Check for equivocation before adding
                    let prevotes = state.prevotes.read().await;
                    if let Some(existing) = prevotes.get_vote_by_validator(&voter) {
                        if let Some(evidence) = check_for_equivocation(&vote, &[(voter, existing.clone())].into_iter().collect()) {
                            warn!(
                                correlation_id = %correlation_id,
                                voter = ?voter,
                                "Detected prevote equivocation: {:?}",
                                evidence
                            );
                            // TODO: Broadcast equivocation evidence
                        }
                    }
                    drop(prevotes);

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

                        // Lock on the block
                        drop(prevotes);
                        state.lock_on(round, block_hash);
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

                // Schedule precommit timeout
                {
                    let mut scheduler = timeout_scheduler.write().await;
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

                // Schedule propose timeout for new round
                {
                    let mut scheduler = timeout_scheduler.write().await;
                    let _ = scheduler.schedule(TendermintStep::Propose);
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

        // Verify timeout is for current state
        let (current_height, current_round, current_step) = {
            let state = tendermint_state.read().await;
            (state.height, state.round, state.step)
        };

        if height != current_height || round != current_round {
            debug!(
                correlation_id = %correlation_id,
                timeout_height = height,
                timeout_round = round,
                current_height = current_height,
                current_round = current_round,
                "Ignoring stale timeout"
            );
            return Ok(current_round);
        }

        // Only process timeout if we're at or before the timeout step
        if step as u8 > current_step as u8 {
            debug!(
                correlation_id = %correlation_id,
                timeout_step = ?step,
                current_step = ?current_step,
                "Ignoring timeout for future step"
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

                {
                    let mut scheduler = timeout_scheduler.write().await;
                    let _ = scheduler.schedule(TendermintStep::Prevote);
                }

                self.cast_prevote(height, round, None, correlation_id)
                    .await?;

                Ok(round)
            }

            TendermintStep::Prevote => {
                // Not enough prevotes received - cast nil precommit
                info!(
                    correlation_id = %correlation_id,
                    height = height,
                    round = round,
                    "Prevote timeout - casting nil precommit"
                );

                {
                    let mut state = tendermint_state.write().await;
                    state.set_step(TendermintStep::Precommit);
                }

                {
                    let mut scheduler = timeout_scheduler.write().await;
                    let _ = scheduler.schedule(TendermintStep::Precommit);
                }

                self.cast_precommit(height, round, None, correlation_id)
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

                {
                    let mut state = tendermint_state.write().await;
                    state.new_round(new_round);
                }

                {
                    let mut scheduler = timeout_scheduler.write().await;
                    let _ = scheduler.schedule(TendermintStep::Propose);
                }

                // Check if we're the proposer for the new round
                let is_proposer = {
                    let state = tendermint_state.read().await;
                    state.is_proposer()
                };

                if is_proposer {
                    info!(
                        correlation_id = %correlation_id,
                        height = height,
                        new_round = new_round,
                        "We are proposer for new round"
                    );
                    // TODO: Trigger TendermintPropose message
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

        // 1. Get parent block hash from storage
        let parent_hash = {
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
                        parent_hash = %head.hash,
                        "Found parent block"
                    );
                    ExecutionBlockHash::from_root(lighthouse_wrapper::types::Hash256::from_slice(&head.hash.0))
                }
                None => {
                    // No head means we're building on genesis
                    // Get genesis hash from engine
                    debug!(
                        correlation_id = %correlation_id,
                        "No chain head found, building on genesis"
                    );
                    ExecutionBlockHash::zero()
                }
            }
        };

        // 2. Calculate timestamp for the new block
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0));

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

        // 8. Assemble the ConsensusBlock with optional AuxPoW (retrieved in step 3)
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
        };

        info!(
            correlation_id = %correlation_id,
            height = height,
            block_number = block.execution_payload.block_number,
            parent_hash = %block.parent_hash,
            has_last_commit = block.last_commit.is_some(),
            has_auxpow = block.auxpow_header.is_some(),
            pegin_count = block.pegins().len(),
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

        // Sign the vote
        let signing_root = vote.signing_root();
        let signature = keypair.sk.sign(signing_root);
        let vote = Vote { signature, ..vote };

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

        // Record the vote
        {
            let mut state = tendermint_state.write().await;
            state.record_prevote(block_hash);
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

        // Sign the vote
        let signing_root = vote.signing_root();
        let signature = keypair.sk.sign(signing_root);
        let vote = Vote { signature, ..vote };

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

        // Record the vote
        {
            let mut state = tendermint_state.write().await;
            state.record_precommit(block_hash);
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
        use crate::actors_v2::storage::messages::{StoreBlockMessage, UpdateChainHeadMessage};
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
        {
            let mut wal_guard = wal.write().await;
            wal_guard
                .write(WALEntry::Commit {
                    height,
                    block_hash,
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

        // 5. Store block to StorageActor
        if let Some(ref storage) = self.storage_actor {
            let signed_block = crate::block::SignedConsensusBlock {
                message: block.clone(),
                signature: crate::signatures::AggregateApproval::new(),
            };

            storage.send(StoreBlockMessage {
                block: signed_block,
                canonical: true, // Tendermint blocks are always canonical
                correlation_id: Some(correlation_id),
            }).await
                .map_err(|e| ChainError::Internal(format!("Storage mailbox error: {}", e)))?
                .map_err(|e| ChainError::Storage(format!("Block storage failed: {}", e)))?;

            // 6. Update chain head
            let block_ref = BlockRef {
                hash: H256::from_slice(block_hash.as_bytes()),
                number: height,
                execution_hash,
            };

            storage.send(UpdateChainHeadMessage {
                new_head: block_ref.clone(),
                correlation_id: Some(correlation_id),
            }).await
                .map_err(|e| ChainError::Internal(format!("Storage mailbox error: {}", e)))?
                .map_err(|e| ChainError::Storage(format!("Head update failed: {}", e)))?;

            // Update local state
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

        // 10. Notify TendermintDriver of commit
        if let Some(ref driver) = self.tendermint_driver {
            driver.do_send(crate::actors_v2::tendermint_driver::TendermintDriverMessage::Committed {
                height,
                last_commit: commit.clone(),
            });
        }

        // 11. Trigger NewHeight for H+1
        self.handle_tendermint_new_height(height + 1, correlation_id).await?;

        Ok(())
    }
}
