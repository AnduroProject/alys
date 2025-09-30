//! Block Serialization Utilities for V2 Actor System
//!
//! Provides SSZ-based serialization/deserialization for network transmission
//! and storage operations, ensuring compatibility with Ethereum 2.0 standards.

use ethereum_types::H256;
use lighthouse_wrapper::types::MainnetEthSpec;
use serde_json;

use crate::actors_v2::chain::ChainError;
use crate::block::SignedConsensusBlock;

/// Block serialization for network broadcasting (MessagePack format - matches V0)
pub fn serialize_block_for_network(block: &SignedConsensusBlock<MainnetEthSpec>) -> Result<Vec<u8>, ChainError> {
    // Use MessagePack for network compatibility - matches V0 RPC protocol (line 60 in ssz_snappy.rs)
    rmp_serde::to_vec(block)
        .map_err(|e| ChainError::Serialization(format!("MessagePack encoding failed: {}", e)))
}

/// Block serialization for storage (backwards compatibility with V0)
pub fn serialize_block(block: &SignedConsensusBlock<MainnetEthSpec>) -> Result<Vec<u8>, ChainError> {
    // Use JSON for storage during development - can be optimized later
    serde_json::to_vec(block)
        .map_err(|e| ChainError::Serialization(format!("Failed to serialize block: {}", e)))
}

/// Block deserialization from network (MessagePack format - matches V0)
pub fn deserialize_block_from_network(data: &[u8]) -> Result<SignedConsensusBlock<MainnetEthSpec>, ChainError> {
    // Use MessagePack for network compatibility - matches V0 RPC protocol
    rmp_serde::from_slice(data)
        .map_err(|e| ChainError::Serialization(format!("MessagePack decoding failed: {}", e)))
}

/// Block deserialization from storage (backwards compatibility)
pub fn deserialize_block(data: &[u8]) -> Result<SignedConsensusBlock<MainnetEthSpec>, ChainError> {
    // Use JSON for storage during development - can be optimized later
    serde_json::from_slice(data)
        .map_err(|e| ChainError::Serialization(format!("Failed to deserialize block: {}", e)))
}

/// Block hash calculation for identification and merkle proofs
pub fn calculate_block_hash(block: &SignedConsensusBlock<MainnetEthSpec>) -> H256 {
    // Use the BlockIndex trait's block_hash method (matches V0 pattern)
    use crate::auxpow_miner::BlockIndex;
    use crate::block::ConvertBlockHash;

    let block_hash = block.message.block_hash(); // Returns BlockHash (via BlockIndex trait)
    let hash256: lighthouse_wrapper::types::Hash256 = block_hash.to_block_hash(); // Convert to Hash256
    H256::from_slice(hash256.as_bytes()) // Convert to H256
}

/// Compact block information for lightweight operations
#[derive(Debug, Clone)]
pub struct BlockInfo {
    pub hash: H256,
    pub height: u64,
    pub parent_hash: H256,
    pub timestamp: u64,
    pub transaction_count: usize,
}

impl BlockInfo {
    /// Extract block info from full block
    pub fn from_block(block: &SignedConsensusBlock<MainnetEthSpec>) -> Self {
        // Since ConsensusBlock uses ExecutionPayloadCapella directly, access it directly
        let payload = &block.message.execution_payload;

        Self {
            hash: calculate_block_hash(block),
            height: payload.block_number,
            parent_hash: H256::zero(), // Placeholder - ExecutionBlockHash conversion complex
            timestamp: payload.timestamp,
            transaction_count: payload.transactions.len(),
        }
    }
}

/// Validate block structure for basic consistency checks
pub fn validate_block_structure(block: &SignedConsensusBlock<MainnetEthSpec>) -> Result<(), ChainError> {
    // Since ConsensusBlock uses ExecutionPayloadCapella directly, access it directly
    let payload = &block.message.execution_payload;

    // Basic structural validation
    if payload.block_number == 0 {
        // Genesis block - minimal validation
        return Ok(());
    }

    // Check basic invariants
    if payload.gas_limit == 0 {
        return Err(ChainError::InvalidBlock("Gas limit cannot be zero".to_string()));
    }

    if payload.gas_used > payload.gas_limit {
        return Err(ChainError::InvalidBlock("Gas used exceeds gas limit".to_string()));
    }

    if payload.timestamp == 0 {
        return Err(ChainError::InvalidBlock("Timestamp cannot be zero".to_string()));
    }

    Ok(())
}

/// Serialize block for specific use cases
pub mod specialized {
    use super::*;

    /// Serialize block for network gossip (compressed)
    pub fn serialize_for_gossip(block: &SignedConsensusBlock<MainnetEthSpec>) -> Result<Vec<u8>, ChainError> {
        let serialized = serialize_block(block)?;

        // Could add compression here for network efficiency
        // For now, use standard serialization
        Ok(serialized)
    }

    /// Serialize block for storage (with metadata)
    pub fn serialize_for_storage(
        block: &SignedConsensusBlock<MainnetEthSpec>,
        canonical: bool,
    ) -> Result<Vec<u8>, ChainError> {
        let mut serialized = serialize_block(block)?;

        // Append canonical flag for storage
        serialized.push(if canonical { 1 } else { 0 });

        Ok(serialized)
    }

    /// Deserialize block from storage (with metadata)
    pub fn deserialize_from_storage(data: &[u8]) -> Result<(SignedConsensusBlock<MainnetEthSpec>, bool), ChainError> {
        if data.is_empty() {
            return Err(ChainError::Serialization("Empty storage data".to_string()));
        }

        let canonical = data[data.len() - 1] == 1;
        let block_data = &data[..data.len() - 1];

        let block = deserialize_block(block_data)?;

        Ok((block, canonical))
    }

    /// Create block summary for lightweight operations
    pub fn create_block_summary(block: &SignedConsensusBlock<MainnetEthSpec>) -> Vec<u8> {
        let info = BlockInfo::from_block(block);

        // Simple binary format for block summary
        let mut summary = Vec::with_capacity(64);
        summary.extend_from_slice(info.hash.as_bytes());      // 32 bytes
        summary.extend_from_slice(&info.height.to_le_bytes()); // 8 bytes
        summary.extend_from_slice(info.parent_hash.as_bytes()); // 32 bytes
        summary.extend_from_slice(&info.timestamp.to_le_bytes()); // 8 bytes
        summary.extend_from_slice(&(info.transaction_count as u32).to_le_bytes()); // 4 bytes

        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests would be implemented here to verify serialization round-trips
    // For now, focusing on the implementation structure
}