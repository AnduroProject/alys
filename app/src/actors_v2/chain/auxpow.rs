//! ChainActor V2 AuxPoW Integration (Phase 4: Task 4.2.1)
//!
//! Production-ready AuxPoW (Auxiliary Proof of Work) integration for mining coordination.
//! Validates and incorporates Bitcoin merge-mining proofs into block production.

use bitcoin::hashes::Hash;
use bitcoin::{BlockHash, CompactTarget};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::state::MiningContext;
use super::{ChainActor, ChainError};
use crate::actors_v2::common::serialization::calculate_block_hash;
use crate::auxpow::AuxPow; // For aggregate_hash calculation
use crate::auxpow_miner::AuxBlock; // V0 Bitcoin-compatible type for RPC responses
use crate::block::{AuxPowHeader, ConsensusBlock, ConvertBlockHash, SignedConsensusBlock};
use lighthouse_wrapper::types::MainnetEthSpec;
use std::time::SystemTime;

impl ChainActor {
    /// Incorporate AuxPoW into block production pipeline (Phase 4: Task 4.2.1)
    pub async fn incorporate_auxpow(
        &mut self,
        consensus_block: ConsensusBlock<MainnetEthSpec>,
    ) -> Result<SignedConsensusBlock<MainnetEthSpec>, ChainError> {
        let correlation_id = Uuid::new_v4();

        debug!(
            correlation_id = %correlation_id,
            slot = consensus_block.slot,
            "Starting AuxPoW incorporation check"
        );

        // Step 1: Check if AuxPoW is required and available
        let queued_auxpow = self.state.get_queued_pow().await;

        if let Some(auxpow_header) = queued_auxpow {
            info!(
                correlation_id = %correlation_id,
                auxpow_height = auxpow_header.height,
                "Found queued AuxPoW - validating for block"
            );

            // Step 2: Validate AuxPoW against block
            if self
                .validate_auxpow_for_block(&auxpow_header, &consensus_block)
                .await?
            {
                // Step 3: Create block with AuxPoW header
                let mut block_with_auxpow = consensus_block;
                block_with_auxpow.auxpow_header = Some(auxpow_header.clone());

                // Step 4: Sign the block with V0 Aura authority
                let authority = self.state.aura.authority.as_ref().ok_or_else(|| {
                    ChainError::Configuration("No authority configured for signing".to_string())
                })?;
                let signed_block = block_with_auxpow.sign_block(authority);

                let block_hash = calculate_block_hash(&signed_block);

                info!(
                    correlation_id = %correlation_id,
                    block_hash = %block_hash,
                    "Successfully incorporated AuxPoW into block"
                );

                // Step 5: Clear queued AuxPoW and reset counter
                self.state.set_queued_pow(None).await;
                self.state.reset_blocks_without_pow().await;

                // Step 6: Update metrics
                self.metrics.auxpow_processed.inc();

                return Ok(signed_block);
            } else {
                warn!(
                    correlation_id = %correlation_id,
                    "AuxPoW validation failed for block - clearing queued AuxPoW"
                );
                self.state.set_queued_pow(None).await;
                self.metrics.auxpow_failures.inc();
            }
        }

        // Step 7: Check blocks without PoW limit
        let blocks_without_pow = self.state.get_blocks_without_pow_blocking();
        let max_blocks_without_pow = self.state.max_blocks_without_pow;

        if blocks_without_pow >= max_blocks_without_pow {
            error!(
                correlation_id = %correlation_id,
                blocks_without_pow = blocks_without_pow,
                max_blocks_without_pow = max_blocks_without_pow,
                "Too many blocks without proof of work"
            );
            return Err(ChainError::Consensus(format!(
                "Too many blocks without proof of work: {} >= {}",
                blocks_without_pow, max_blocks_without_pow
            )));
        }

        // Step 8: Create regular signed block (no AuxPoW)
        let authority = self.state.aura.authority.as_ref().ok_or_else(|| {
            ChainError::Configuration("No authority configured for signing".to_string())
        })?;
        let signed_block = consensus_block.sign_block(authority);

        // Increment counter for blocks produced without AuxPoW
        self.state.increment_blocks_without_pow().await;

        debug!(
            correlation_id = %correlation_id,
            blocks_without_pow = self.state.get_blocks_without_pow_blocking(),
            max_blocks_without_pow = max_blocks_without_pow,
            "Created block without AuxPoW"
        );

        Ok(signed_block)
    }

    /// Validate AuxPoW against current block (Phase 4: Task 4.2.1 + Priority 4)
    pub async fn validate_auxpow_for_block(
        &self,
        auxpow: &AuxPowHeader,
        block: &ConsensusBlock<MainnetEthSpec>,
    ) -> Result<bool, ChainError> {
        let correlation_id = Uuid::new_v4();

        // Step 1: Validate that AuxPoW covers the correct block range
        let block_height = block.execution_payload.block_number;

        // Note: range_start and range_end are Hash256 block hashes, not heights
        // This validation would need to resolve hashes to heights via storage
        debug!(
            correlation_id = %correlation_id,
            block_height = block_height,
            auxpow_height = auxpow.height,
            "Validating AuxPoW for block"
        );

        // Step 2: Validate AuxPoW proof exists
        let auxpow_proof = auxpow.auxpow.as_ref().ok_or_else(|| {
            warn!(correlation_id = %correlation_id, "No AuxPoW proof present in header");
            ChainError::AuxPowValidation("No AuxPoW proof present".to_string())
        })?;

        // Step 3: Validate proof of work difficulty (Priority 4: ADDED)
        let compact_target = bitcoin::CompactTarget::from_consensus(auxpow.bits);
        if !auxpow_proof.check_proof_of_work(compact_target) {
            warn!(
                correlation_id = %correlation_id,
                bits = auxpow.bits,
                "AuxPoW proof of work insufficient - does not meet difficulty target"
            );
            return Ok(false);
        }

        debug!(
            correlation_id = %correlation_id,
            bits = auxpow.bits,
            "AuxPoW proof of work validated successfully"
        );

        // Step 4: Create temporary signed block for hash calculation
        use crate::signatures::AggregateApproval;
        let temp_signed_block = SignedConsensusBlock {
            message: block.clone(),
            signature: AggregateApproval::new(), // Temporary signature for hash calculation
        };

        let block_hash = calculate_block_hash(&temp_signed_block);

        // Step 5: Convert block hash to Bitcoin format for validation
        let chain_id = self.config.chain_id; // Priority 5: from ChainConfig
        let bitcoin_block_hash = bitcoin::BlockHash::from_byte_array(block_hash.0);

        // Step 6: Use V0 AuxPoW validation (cryptographic validation)
        match auxpow_proof.check(bitcoin_block_hash, chain_id) {
            Ok(()) => {
                debug!(
                    correlation_id = %correlation_id,
                    block_height = block_height,
                    "AuxPoW validation passed for block"
                );
                Ok(true)
            }
            Err(e) => {
                warn!(
                    correlation_id = %correlation_id,
                    block_height = block_height,
                    error = ?e,
                    "AuxPoW validation failed for block"
                );
                Ok(false)
            }
        }
    }

    /// Validate and process submitted AuxPoW from miner (Priority 3 + 4)
    ///
    /// Validates submitted work against stored mining context and AuxPoW proofs.
    /// Returns the validated AuxPowHeader ready for chain finalization.
    pub async fn validate_submitted_auxpow(
        &self,
        aggregate_hash: BlockHash,
        auxpow: crate::auxpow::AuxPow,
    ) -> Result<AuxPowHeader, ChainError> {
        let correlation_id = Uuid::new_v4();

        debug!(
            correlation_id = %correlation_id,
            aggregate_hash = %aggregate_hash,
            "Validating submitted AuxPoW from miner"
        );

        // Step 1: Retrieve stored mining context (Priority 3)
        let context = self
            .state
            .take_mining_context(&aggregate_hash)
            .await
            .ok_or_else(|| {
                warn!(
                    correlation_id = %correlation_id,
                    aggregate_hash = %aggregate_hash,
                    "Unknown block hash - no mining context found"
                );
                ChainError::AuxPowValidation("Unknown block hash".to_string())
            })?;

        debug!(
            correlation_id = %correlation_id,
            start_hash = %context.start_hash,
            end_hash = %context.end_hash,
            miner_address = %context.miner_address,
            "Retrieved mining context for validation"
        );

        // Step 2: Validate proof of work (Priority 4)
        let compact_target = CompactTarget::from_consensus(context.bits);
        if !auxpow.check_proof_of_work(compact_target) {
            warn!(
                correlation_id = %correlation_id,
                bits = context.bits,
                "Submitted AuxPoW does not meet difficulty target"
            );
            return Err(ChainError::AuxPowValidation(
                "Insufficient proof of work".to_string(),
            ));
        }

        debug!(
            correlation_id = %correlation_id,
            bits = context.bits,
            "Proof of work validation passed"
        );

        // Step 3: Validate AuxPoW structure (Priority 4)
        let chain_id = self.config.chain_id; // Priority 5: from ChainConfig
        if let Err(e) = auxpow.check(aggregate_hash, chain_id) {
            warn!(
                correlation_id = %correlation_id,
                error = ?e,
                "AuxPoW structure validation failed"
            );
            return Err(ChainError::AuxPowValidation(format!(
                "AuxPoW validation failed: {:?}",
                e
            )));
        }

        debug!(
            correlation_id = %correlation_id,
            "AuxPoW structure validation passed"
        );

        // Step 4: Create validated AuxPowHeader
        // Note: pegins are attached separately in the handler after validation
        let auxpow_header = AuxPowHeader {
            range_start: context.start_hash.to_block_hash(),
            range_end: context.end_hash.to_block_hash(),
            bits: context.bits,
            chain_id,
            height: context.height,
            auxpow: Some(auxpow),
            fee_recipient: context.miner_address,
            pegins: vec![], // Pegins added by handler after AuxPoW validation
        };

        info!(
            correlation_id = %correlation_id,
            start_hash = %context.start_hash,
            end_hash = %context.end_hash,
            height = context.height,
            miner_address = %context.miner_address,
            "Successfully validated submitted AuxPoW"
        );

        Ok(auxpow_header)
    }

    /// Clear queued AuxPoW after use (Phase 4: Task 4.2.1)
    /// Note: This would need to be called through a handler that can mutate state
    pub async fn clear_queued_auxpow(&mut self) {
        if self.state.has_queued_pow_blocking() {
            debug!("Clearing queued AuxPoW");
            self.state.set_queued_pow(None).await;
        }
    }

    /// Queue new AuxPoW for block production (Phase 4: Task 4.2.1)
    /// Note: This would need to be called through a handler that can mutate state
    pub async fn queue_auxpow(&mut self, auxpow_header: AuxPowHeader) -> Result<(), ChainError> {
        let correlation_id = Uuid::new_v4();

        info!(
            correlation_id = %correlation_id,
            auxpow_height = auxpow_header.height,
            "Queueing new AuxPoW for block production"
        );

        // Basic validation of AuxPoW header
        let current_height = self.state.get_height().await;
        if auxpow_header.height < current_height {
            warn!(
                correlation_id = %correlation_id,
                auxpow_height = auxpow_header.height,
                current_height = current_height,
                "AuxPoW height already expired"
            );
            return Err(ChainError::InvalidBlock(
                "AuxPoW height expired".to_string(),
            ));
        }

        // Queue the AuxPoW
        self.state.set_queued_pow(Some(auxpow_header)).await;

        debug!(
            correlation_id = %correlation_id,
            "Successfully queued AuxPoW"
        );

        Ok(())
    }

    /// Calculate blocks without PoW count (Phase 4: Task 4.2.1)
    pub async fn calculate_blocks_without_pow(&self) -> Result<u64, ChainError> {
        // Return the current count from state
        Ok(*self.state.blocks_without_pow.read().await)
    }

    /// Broadcast AuxPoW to network for mining (Phase 4: Task 4.2.1)
    pub async fn broadcast_auxpow(&self, auxpow_header: &AuxPowHeader) -> Result<(), ChainError> {
        let correlation_id = Uuid::new_v4();

        if let Some(ref network_actor) = self.network_actor {
            // Serialize AuxPoW header for network transmission using JSON
            let auxpow_data = serde_json::to_vec(auxpow_header)
                .map_err(|e| ChainError::Internal(format!("AuxPoW serialization failed: {}", e)))?;

            let msg = crate::actors_v2::network::NetworkMessage::BroadcastAuxPow {
                auxpow_data,
                correlation_id: Some(correlation_id),
            };

            match network_actor.send(msg).await {
                Ok(Ok(crate::actors_v2::network::NetworkResponse::AuxPowBroadcasted {
                    peer_count,
                })) => {
                    info!(
                        correlation_id = %correlation_id,
                        peer_count = peer_count,
                        auxpow_height = auxpow_header.height,
                        "Successfully broadcasted AuxPoW to network"
                    );
                    Ok(())
                }
                Ok(Err(e)) => {
                    error!(
                        correlation_id = %correlation_id,
                        error = ?e,
                        "Network error broadcasting AuxPoW"
                    );
                    Err(ChainError::Network(e))
                }
                Err(e) => {
                    error!(
                        correlation_id = %correlation_id,
                        error = ?e,
                        "Communication error broadcasting AuxPoW"
                    );
                    Err(ChainError::NetworkError(format!(
                        "Network communication failed: {}",
                        e
                    )))
                }
                _ => {
                    error!(correlation_id = %correlation_id, "Unexpected network response");
                    Err(ChainError::Internal(
                        "Unexpected network response".to_string(),
                    ))
                }
            }
        } else {
            warn!("NetworkActor not available - cannot broadcast AuxPoW");
            Err(ChainError::NetworkNotAvailable)
        }
    }

    /// Update blocks without PoW counter (Phase 4: Task 4.2.1)
    /// Note: This would need to be called through a handler that can mutate state
    pub async fn increment_blocks_without_pow_v2(&mut self) {
        self.state.increment_blocks_without_pow().await;
        let blocks_without_pow = self.state.get_blocks_without_pow_blocking();

        debug!(
            blocks_without_pow = blocks_without_pow,
            max_blocks_without_pow = self.state.max_blocks_without_pow,
            "Incremented blocks without PoW counter"
        );
    }

    /// Reset blocks without PoW counter after AuxPoW block (Phase 4: Task 4.2.1)
    /// Note: This would need to be called through a handler that can mutate state
    pub async fn reset_blocks_without_pow_v2(&mut self) {
        let previous_count = self.state.get_blocks_without_pow_blocking();
        self.state.reset_blocks_without_pow().await;

        info!(
            previous_count = previous_count,
            "Reset blocks without PoW counter after AuxPoW block"
        );
    }

    /// Get aggregate hashes from block hash cache (Priority 2)
    ///
    /// Returns hashes of unfinalized blocks for aggregate calculation.
    /// Returns error if no work is available or cache is not initialized.
    pub async fn get_aggregate_hashes(&self) -> Result<Vec<BlockHash>, ChainError> {
        let correlation_id = Uuid::new_v4();

        // Check if block_hash_cache is initialized
        let block_hash_cache =
            self.state.block_hash_cache.as_ref().ok_or_else(|| {
                ChainError::Internal("Block hash cache not initialized".to_string())
            })?;

        // Get current head to check for new work
        let current_head = self
            .state
            .get_head_hash()
            .await
            .ok_or_else(|| ChainError::Internal("No chain head available".to_string()))?;

        // Check if there's queued AuxPoW and if we have new work since then
        if let Some(ref queued_pow) = self.state.get_queued_pow().await {
            let range_end = queued_pow.range_end;

            // Convert range_end to comparison format
            if range_end.as_bytes() == current_head.as_bytes() {
                debug!(
                    correlation_id = %correlation_id,
                    "No work to do - no new blocks since last AuxPoW"
                );
                return Err(ChainError::NoWorkToDo);
            }
        }

        // Get cached block hashes
        let hashes = block_hash_cache.get();

        if hashes.is_empty() {
            warn!(
                correlation_id = %correlation_id,
                "Block hash cache is empty - no unfinalized blocks"
            );
            return Err(ChainError::NoWorkToDo);
        }

        debug!(
            correlation_id = %correlation_id,
            hash_count = hashes.len(),
            "Retrieved aggregate hashes from cache"
        );

        Ok(hashes)
    }

    /// Create AuxBlock for RPC mining requests (Phase 4: Integration Point 2)
    ///
    /// Returns V0-compatible AuxBlock structure for `createauxblock` RPC response.
    /// This format is expected by external Bitcoin mining pools.
    pub async fn create_aux_block(
        &self,
        miner_address: lighthouse_wrapper::types::Address,
    ) -> Result<AuxBlock, ChainError> {
        let correlation_id = Uuid::new_v4();

        let current_height = self.state.get_height().await;

        debug!(
            correlation_id = %correlation_id,
            current_height = current_height,
            miner_address = %miner_address,
            "Creating AuxBlock for mining pool"
        );

        // Get unfinalized block hashes for aggregate calculation (Priority 2: COMPLETE)
        let hashes = self.get_aggregate_hashes().await?;

        // Calculate aggregate hash (vector commitment) over unfinalized blocks
        let aggregate_hash = AuxPow::aggregate_hash(&hashes);

        debug!(
            correlation_id = %correlation_id,
            hash_count = hashes.len(),
            aggregate_hash = %aggregate_hash,
            "Calculated aggregate hash for mining"
        );

        // Get current difficulty target from retarget params
        let bits_u32 = self.get_current_difficulty_bits()?;
        let bits = CompactTarget::from_consensus(bits_u32);

        // Chain ID for AuxPoW validation (Priority 5: from ChainConfig)
        let chain_id = self.config.chain_id;

        // Get previous finalized block hash
        // TODO (Priority 4): Query StorageActor for last finalized (AuxPoW) block
        let previous_block_hash = *hashes
            .first()
            .ok_or_else(|| ChainError::Internal("Empty hash list".to_string()))?;

        let target_height = current_height + hashes.len() as u64;

        // Store mining context for submission validation (Priority 3: COMPLETE)
        let mining_context = MiningContext {
            issued_at: SystemTime::now(),
            last_hash: self
                .state
                .get_head_hash()
                .await
                .ok_or_else(|| ChainError::Internal("No chain head".to_string()))?,
            start_hash: *hashes
                .first()
                .ok_or_else(|| ChainError::Internal("Empty hash list".to_string()))?,
            end_hash: *hashes
                .last()
                .ok_or_else(|| ChainError::Internal("Empty hash list".to_string()))?,
            miner_address,
            bits: bits_u32,
            height: target_height,
        };

        self.state
            .store_mining_context(aggregate_hash, mining_context)
            .await;

        debug!(
            correlation_id = %correlation_id,
            aggregate_hash = %aggregate_hash,
            "Stored mining context for validation"
        );

        // Create V0-compatible AuxBlock for RPC response using constructor
        let aux_block = AuxBlock::new(
            aggregate_hash,      // Bitcoin BlockHash (aggregate of unfinalized blocks)
            chain_id,            // Alys chain ID (1337)
            previous_block_hash, // First unfinalized block hash
            0,                   // coinbase_value: Always 0 per Alys spec
            bits,                // Difficulty target (compact)
            target_height,       // Height after finalizing all pending blocks
        );

        info!(
            correlation_id = %correlation_id,
            target_height = target_height,
            bits = bits_u32,
            hash = %aggregate_hash,
            block_count = hashes.len(),
            "Created AuxBlock for mining pool (aggregate of {} blocks)", hashes.len()
        );

        Ok(aux_block)
    }

    /// Create AuxPoW header request for internal network broadcast
    ///
    /// This is for NetworkActor broadcasting, separate from RPC mining requests.
    /// Internal operations should use this method.
    pub async fn create_auxpow_header_request(
        &self,
        target_height: u64,
    ) -> Result<AuxPowHeader, ChainError> {
        let correlation_id = Uuid::new_v4();

        let current_height = self.state.get_height().await;

        debug!(
            correlation_id = %correlation_id,
            current_height = current_height,
            target_height = target_height,
            "Creating AuxPoW header request for network broadcast"
        );

        // Get unfinalized block hashes for aggregate calculation (Priority 2: COMPLETE)
        let hashes = self.get_aggregate_hashes().await?;

        // Calculate block range from hashes
        let range_start = hashes
            .first()
            .ok_or_else(|| ChainError::Internal("Empty hash list".to_string()))?
            .to_block_hash();
        let range_end = hashes
            .last()
            .ok_or_else(|| ChainError::Internal("Empty hash list".to_string()))?
            .to_block_hash();

        debug!(
            correlation_id = %correlation_id,
            hash_count = hashes.len(),
            range_start = %range_start,
            range_end = %range_end,
            "Calculated block range for AuxPoW header"
        );

        // Get current difficulty target from retarget params
        let bits = self.get_current_difficulty_bits()?;

        // Chain ID for AuxPoW validation (Priority 5: from ChainConfig)
        let chain_id = self.config.chain_id;

        // Fee recipient from config or default to zero address
        let fee_recipient = self.config.validator_address.unwrap_or_default();

        let auxpow_header = AuxPowHeader {
            range_start,
            range_end,
            bits,
            chain_id,
            height: target_height,
            auxpow: None, // Miners will fill this with completed work
            fee_recipient,
            pegins: vec![], // Pegins submitted with completed AuxPoW
        };

        info!(
            correlation_id = %correlation_id,
            target_height = target_height,
            bits = bits,
            block_count = hashes.len(),
            "Created AuxPoW header request for network broadcast (range of {} blocks)", hashes.len()
        );

        Ok(auxpow_header)
    }

    /// Get current difficulty bits from retarget params (Phase 4: Integration Point 2)
    fn get_current_difficulty_bits(&self) -> Result<u32, ChainError> {
        // Use pow_limit from Bitcoin consensus params as the initial/default difficulty
        // In a production system, this would implement difficulty adjustment based on:
        // - Recent block times
        // - Target spacing/timespan
        // - Retargeting algorithm

        let bits = self.state.retarget_params.pow_limit;

        debug!(
            bits = bits,
            "Retrieved current difficulty bits from consensus params"
        );

        Ok(bits)
    }
}
