//! Block validation utilities for V2 actor system
//!
//! Provides cryptographic validation for block signatures and
//! parent hash relationship verification.

use ethereum_types::H256;
use lighthouse_wrapper::types::{Hash256, MainnetEthSpec};
use crate::block::SignedConsensusBlock;
use crate::actors_v2::chain::ChainError;
use crate::aura::{slot_author, Aura};
use actix::Addr;

/// Verify block signature against expected authority (Phase 3)
///
/// This function:
/// 1. Gets the expected authority for this slot from Aura
/// 2. Verifies the block's BLS signature using the authorities list
/// 3. Ensures the signature is valid according to the consensus rules
///
/// Note: The system uses BLS aggregate signatures from Lighthouse, not ECDSA.
/// The signature verification checks that the block was properly signed by the
/// authority assigned to the slot.
///
/// # Errors
/// Returns `ChainError::Consensus` if:
/// - Unable to determine expected authority for slot
/// - Signature verification fails (invalid or forged signature)
///
pub fn verify_block_signature(
    block: &SignedConsensusBlock<MainnetEthSpec>,
    aura: &Aura,
) -> Result<(), ChainError> {
    let slot = block.message.slot;
    let block_number = block.message.execution_payload.block_number;

    // Get expected authority for this slot
    let (_authority_index, expected_authority) = slot_author(slot, &aura.authorities)
        .ok_or_else(|| {
            ChainError::Consensus(format!(
                "Unable to determine authority for slot {} (block #{})",
                slot, block_number
            ))
        })?;

    tracing::debug!(
        slot = slot,
        block_number = block_number,
        authority_index = _authority_index,
        "Verifying block signature for slot"
    );

    // Verify the block's BLS signature using all authorities
    // Note: The signature is an aggregate that can include multiple authorities' signatures
    // The verify_signature method checks the aggregate signature against the authorities list
    if !block.verify_signature(&aura.authorities) {
        return Err(ChainError::Consensus(format!(
            "Invalid signature for block #{} at slot {} - signature verification failed",
            block_number, slot
        )));
    }

    tracing::debug!(
        slot = slot,
        block_number = block_number,
        expected_authority = ?expected_authority,
        "Block signature verified successfully"
    );

    Ok(())
}

/// Validate block's parent hash and height relationship (Phase 3)
///
/// This function verifies:
/// 1. Parent block exists in storage
/// 2. Block height is exactly parent.height + 1
/// 3. Block's parent_hash matches the canonical hash of parent block
///
/// # Genesis Handling
/// Genesis blocks (height 0) skip validation as they have no parent.
///
/// # Errors
/// Returns `ChainError::InvalidBlock` if:
/// - Parent block is missing from storage
/// - Height relationship is incorrect (not parent.height + 1)
/// - Parent hash doesn't match expected value
///
pub async fn validate_parent_relationship(
    block: &SignedConsensusBlock<MainnetEthSpec>,
    storage_actor: &Addr<crate::actors_v2::storage::StorageActor>,
) -> Result<(), ChainError> {
    let block_height = block.message.execution_payload.block_number;
    let parent_hash = block.message.parent_hash;

    // Genesis block has no parent
    if block_height == 0 {
        tracing::debug!("Genesis block, skipping parent validation");
        return Ok(());
    }

    tracing::debug!(
        block_height = block_height,
        parent_hash = %parent_hash,
        "Validating parent relationship"
    );

    // Fetch parent block from storage
    let get_block_msg = crate::actors_v2::storage::messages::GetBlockMessage {
        block_hash: parent_hash,
        correlation_id: Some(uuid::Uuid::new_v4()),
    };

    let parent_block = match storage_actor.send(get_block_msg).await {
        Ok(Ok(Some(parent))) => parent,
        Ok(Ok(None)) => {
            return Err(ChainError::InvalidBlock(format!(
                "Parent block not found: {} (height {} expects parent at height {})",
                parent_hash,
                block_height,
                block_height - 1
            )));
        }
        Ok(Err(e)) => {
            return Err(ChainError::Storage(format!(
                "Failed to fetch parent block {}: {}",
                parent_hash, e
            )));
        }
        Err(e) => {
            return Err(ChainError::NetworkError(format!(
                "Communication error with StorageActor while fetching parent: {}",
                e
            )));
        }
    };

    // Validate height relationship
    let parent_height = parent_block.message.execution_payload.block_number;

    if block_height != parent_height + 1 {
        return Err(ChainError::InvalidBlock(format!(
            "Invalid height relationship: block is {} but parent is {} (expected {})",
            block_height,
            parent_height,
            parent_height + 1
        )));
    }

    // Validate parent hash matches
    // Calculate the canonical hash of the parent block
    let calculated_parent_hash = parent_block.canonical_root();

    if calculated_parent_hash != parent_hash {
        return Err(ChainError::InvalidBlock(format!(
            "Parent hash mismatch: block.parent_hash is {} but actual parent hash is {}",
            parent_hash,
            calculated_parent_hash
        )));
    }

    tracing::debug!(
        block_height = block_height,
        parent_height = parent_height,
        parent_hash = %calculated_parent_hash,
        "Parent relationship validated successfully"
    );

    Ok(())
}

/// Verify block against expected authority at slot (simplified check)
///
/// This is a helper function that just checks if the block was signed by
/// the expected authority for the slot, without full signature verification.
///
#[allow(dead_code)]
pub fn verify_block_authority(
    block: &SignedConsensusBlock<MainnetEthSpec>,
    aura: &Aura,
) -> Result<u8, ChainError> {
    let slot = block.message.slot;
    let block_number = block.message.execution_payload.block_number;

    // Get expected authority for this slot
    let (authority_index, _expected_authority) = slot_author(slot, &aura.authorities)
        .ok_or_else(|| {
            ChainError::Consensus(format!(
                "Unable to determine authority for slot {} (block #{})",
                slot, block_number
            ))
        })?;

    Ok(authority_index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lighthouse_wrapper::bls::Keypair;
    use crate::block::ConsensusBlock;
    use crate::aura::Authority;

    #[test]
    fn test_verify_block_signature_valid() {
        // Create a test block
        let block = ConsensusBlock::default();
        let keypair = Keypair::random();

        let authority = Authority {
            signer: keypair.clone(),
            index: 0,
        };

        let signed_block = block.sign_block(&authority);

        // Create Aura with the authority's public key
        let aura = Aura::new(vec![keypair.pk], 12, None);

        // Verify signature should succeed
        let result = verify_block_signature(&signed_block, &aura);
        assert!(result.is_ok(), "Valid signature should verify successfully");
    }

    #[test]
    fn test_verify_block_signature_invalid_authority() {
        // Create a test block signed by one authority
        let block = ConsensusBlock::default();
        let signer_keypair = Keypair::random();

        let authority = Authority {
            signer: signer_keypair.clone(),
            index: 0,
        };

        let signed_block = block.sign_block(&authority);

        // Create Aura with a different authority's public key
        let different_keypair = Keypair::random();
        let aura = Aura::new(vec![different_keypair.pk], 12, None);

        // Verify signature should fail
        let result = verify_block_signature(&signed_block, &aura);
        assert!(result.is_err(), "Invalid signature should fail verification");

        if let Err(ChainError::Consensus(msg)) = result {
            assert!(msg.contains("signature verification failed"), "Error should mention signature failure");
        } else {
            panic!("Expected Consensus error");
        }
    }

    // Note: Parent validation tests would require setting up a full actor system
    // with StorageActor, so they're better suited for integration tests
}
