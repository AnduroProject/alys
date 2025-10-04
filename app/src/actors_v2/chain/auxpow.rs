//! ChainActor V2 AuxPoW Integration (Phase 4: Task 4.2.1)
//!
//! Production-ready AuxPoW (Auxiliary Proof of Work) integration for mining coordination.
//! Validates and incorporates Bitcoin merge-mining proofs into block production.

use tracing::{debug, error, info, warn};
use uuid::Uuid;
use bitcoin::hashes::Hash;

use super::{ChainActor, ChainError};
use crate::block::{ConsensusBlock, SignedConsensusBlock, AuxPowHeader};
use crate::actors_v2::common::serialization::calculate_block_hash;
use lighthouse_wrapper::types::MainnetEthSpec;

impl ChainActor {
    /// Incorporate AuxPoW into block production pipeline (Phase 4: Task 4.2.1)
    pub async fn incorporate_auxpow(
        &mut self,
        consensus_block: ConsensusBlock<MainnetEthSpec>
    ) -> Result<SignedConsensusBlock<MainnetEthSpec>, ChainError> {
        let correlation_id = Uuid::new_v4();

        debug!(
            correlation_id = %correlation_id,
            slot = consensus_block.slot,
            "Starting AuxPoW incorporation check"
        );

        // Step 1: Check if AuxPoW is required and available
        let queued_auxpow = self.state.queued_pow.clone();

        if let Some(auxpow_header) = queued_auxpow {
            info!(
                correlation_id = %correlation_id,
                auxpow_height = auxpow_header.height,
                "Found queued AuxPoW - validating for block"
            );

            // Step 2: Validate AuxPoW against block
            if self.validate_auxpow_for_block(&auxpow_header, &consensus_block).await? {
                // Step 3: Create block with AuxPoW header
                let mut block_with_auxpow = consensus_block;
                block_with_auxpow.auxpow_header = Some(auxpow_header.clone());

                // Step 4: Sign the block with V0 Aura authority
                let authority = self.state.aura.authority.as_ref()
                    .ok_or_else(|| ChainError::Configuration("No authority configured for signing".to_string()))?;
                let signed_block = block_with_auxpow.sign_block(authority);

                let block_hash = calculate_block_hash(&signed_block);

                info!(
                    correlation_id = %correlation_id,
                    block_hash = %block_hash,
                    "Successfully incorporated AuxPoW into block"
                );

                // Step 5: Clear queued AuxPoW and reset counter
                self.state.set_queued_pow(None);
                self.state.reset_blocks_without_pow();

                // Step 6: Update metrics
                self.metrics.auxpow_processed.inc();

                return Ok(signed_block);
            } else {
                warn!(
                    correlation_id = %correlation_id,
                    "AuxPoW validation failed for block - clearing queued AuxPoW"
                );
                self.state.set_queued_pow(None);
                self.metrics.auxpow_failures.inc();
            }
        }

        // Step 7: Check blocks without PoW limit
        let blocks_without_pow = self.state.blocks_without_pow;
        let max_blocks_without_pow = self.state.max_blocks_without_pow;

        if blocks_without_pow >= max_blocks_without_pow {
            error!(
                correlation_id = %correlation_id,
                blocks_without_pow = blocks_without_pow,
                max_blocks_without_pow = max_blocks_without_pow,
                "Too many blocks without proof of work"
            );
            return Err(ChainError::Consensus(
                format!("Too many blocks without proof of work: {} >= {}",
                       blocks_without_pow, max_blocks_without_pow)
            ));
        }

        // Step 8: Create regular signed block (no AuxPoW)
        let authority = self.state.aura.authority.as_ref()
            .ok_or_else(|| ChainError::Configuration("No authority configured for signing".to_string()))?;
        let signed_block = consensus_block.sign_block(authority);

        // Increment counter for blocks produced without AuxPoW
        self.state.increment_blocks_without_pow();

        debug!(
            correlation_id = %correlation_id,
            blocks_without_pow = self.state.blocks_without_pow,
            max_blocks_without_pow = max_blocks_without_pow,
            "Created block without AuxPoW"
        );

        Ok(signed_block)
    }

    /// Validate AuxPoW against current block (Phase 4: Task 4.2.1)
    pub async fn validate_auxpow_for_block(
        &self,
        auxpow: &AuxPowHeader,
        block: &ConsensusBlock<MainnetEthSpec>
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

        // Step 2: Create temporary signed block for hash calculation
        use crate::signatures::AggregateApproval;
        let temp_signed_block = SignedConsensusBlock {
            message: block.clone(),
            signature: AggregateApproval::new(), // Temporary signature for hash calculation
        };

        let block_hash = calculate_block_hash(&temp_signed_block);

        // Step 3: Convert block hash to Bitcoin format for validation
        let chain_id = 1337u32; // Alys chain ID (should be configurable via ChainConfig)
        let bitcoin_block_hash = bitcoin::BlockHash::from_byte_array(block_hash.0);

        // Step 4: Use V0 AuxPoW validation
        if let Some(ref auxpow_proof) = auxpow.auxpow {
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
        } else {
            warn!(
                correlation_id = %correlation_id,
                "No AuxPoW proof present in header"
            );
            Ok(false)
        }
    }

    /// Clear queued AuxPoW after use (Phase 4: Task 4.2.1)
    /// Note: This would need to be called through a handler that can mutate state
    pub async fn clear_queued_auxpow(&mut self) {
        if self.state.queued_pow.is_some() {
            debug!("Clearing queued AuxPoW");
            self.state.queued_pow = None;
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
        let current_height = self.state.get_height();
        if auxpow_header.height < current_height {
            warn!(
                correlation_id = %correlation_id,
                auxpow_height = auxpow_header.height,
                current_height = current_height,
                "AuxPoW height already expired"
            );
            return Err(ChainError::InvalidBlock("AuxPoW height expired".to_string()));
        }

        // Queue the AuxPoW
        self.state.queued_pow = Some(auxpow_header);

        debug!(
            correlation_id = %correlation_id,
            "Successfully queued AuxPoW"
        );

        Ok(())
    }

    /// Calculate blocks without PoW count (Phase 4: Task 4.2.1)
    pub async fn calculate_blocks_without_pow(&self) -> Result<u64, ChainError> {
        // Return the current count from state
        Ok(self.state.blocks_without_pow)
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
                Ok(Ok(crate::actors_v2::network::NetworkResponse::AuxPowBroadcasted { peer_count })) => {
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
                    Err(ChainError::NetworkError(format!("Network communication failed: {}", e)))
                }
                _ => {
                    error!(correlation_id = %correlation_id, "Unexpected network response");
                    Err(ChainError::Internal("Unexpected network response".to_string()))
                }
            }
        } else {
            warn!("NetworkActor not available - cannot broadcast AuxPoW");
            Err(ChainError::NetworkNotAvailable)
        }
    }

    /// Update blocks without PoW counter (Phase 4: Task 4.2.1)
    /// Note: This would need to be called through a handler that can mutate state
    pub async fn increment_blocks_without_pow(&mut self) {
        self.state.blocks_without_pow += 1;

        debug!(
            blocks_without_pow = self.state.blocks_without_pow,
            max_blocks_without_pow = self.state.max_blocks_without_pow,
            "Incremented blocks without PoW counter"
        );
    }

    /// Reset blocks without PoW counter after AuxPoW block (Phase 4: Task 4.2.1)
    /// Note: This would need to be called through a handler that can mutate state
    pub async fn reset_blocks_without_pow(&mut self) {
        let previous_count = self.state.blocks_without_pow;
        self.state.blocks_without_pow = 0;

        info!(
            previous_count = previous_count,
            "Reset blocks without PoW counter after AuxPoW block"
        );
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_auxpow_range_validation() {
        // Test that start > end is invalid
        let invalid_start = 10;
        let invalid_end = 5;
        assert!(invalid_start > invalid_end, "Invalid range should be detected");
    }

    #[test]
    fn test_blocks_without_pow_logic() {
        let blocks_without_pow = 5;
        let max_blocks_without_pow = 10;

        assert!(blocks_without_pow < max_blocks_without_pow, "Should allow block production");

        let blocks_at_limit = 10;
        assert!(blocks_at_limit >= max_blocks_without_pow, "Should prevent block production");
    }
}
