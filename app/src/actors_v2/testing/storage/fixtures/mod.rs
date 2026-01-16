pub mod blocks;
pub mod config;

pub use blocks::*;
pub use config::*;

use crate::actors_v2::storage::actor::AlysConsensusBlock;
use crate::auxpow_miner::BlockIndex;
use crate::block::ConsensusBlock;
use crate::signatures::AggregateApproval;
use lighthouse_wrapper::types::{
    Address, ExecutionBlockHash, ExecutionPayloadCapella, Hash256, MainnetEthSpec,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

/// Generate a sequence of test blocks with proper chain relationships
pub fn create_test_block_sequence(count: usize) -> Vec<AlysConsensusBlock> {
    let mut blocks: Vec<AlysConsensusBlock> = Vec::with_capacity(count);

    for i in 0..count {
        let slot = i as u64 + 1;
        let parent_hash = if i == 0 {
            Hash256::zero()
        } else {
            blocks[i - 1].message.parent_hash // Use parent_hash field directly to keep Hash256 type
        };

        let execution_payload = ExecutionPayloadCapella::<MainnetEthSpec> {
            parent_hash: ExecutionBlockHash::from_root(parent_hash),
            fee_recipient: Address::zero(),
            state_root: Hash256::from_low_u64_be(slot + 1000),
            receipts_root: Hash256::from_low_u64_be(slot + 2000),
            logs_bloom: Default::default(),
            prev_randao: Hash256::from_low_u64_be(slot + 3000),
            block_number: slot,
            gas_limit: 30000000,
            gas_used: slot * 1000, // Variable gas usage for realistic testing
            timestamp: 1600000000 + slot * 12, // 12 second block time
            extra_data: format!("test_block_{}", slot).into_bytes().into(),
            base_fee_per_gas: (1000000000u64 + slot * 100).into(),
            block_hash: ExecutionBlockHash::from_root(Hash256::from_low_u64_be(slot + 4000)),
            transactions: Default::default(),
            withdrawals: Default::default(),
        };

        let consensus_block = ConsensusBlock {
            parent_hash,
            slot,
            auxpow_header: None,
            execution_payload,
            pegins: vec![],
            pegout_payment_proposal: None,
            finalized_pegouts: vec![],
        };

        blocks.push(AlysConsensusBlock {
            message: consensus_block,
            signature: AggregateApproval::new(),
        });
    }

    blocks
}

/// Create a single test block with specified slot
pub fn create_test_block(slot: u64) -> AlysConsensusBlock {
    let execution_payload = ExecutionPayloadCapella::<MainnetEthSpec> {
        parent_hash: ExecutionBlockHash::from_root(Hash256::from_low_u64_be(
            slot.saturating_sub(1),
        )),
        fee_recipient: Address::zero(),
        state_root: Hash256::from_low_u64_be(slot + 1000),
        receipts_root: Hash256::from_low_u64_be(slot + 2000),
        logs_bloom: Default::default(),
        prev_randao: Hash256::from_low_u64_be(slot + 3000),
        block_number: slot,
        gas_limit: 30000000,
        gas_used: slot * 1000,
        timestamp: 1600000000 + slot * 12,
        extra_data: format!("test_block_{}", slot).into_bytes().into(),
        base_fee_per_gas: (1000000000u64 + slot * 100).into(),
        block_hash: ExecutionBlockHash::from_root(Hash256::from_low_u64_be(slot + 4000)),
        transactions: Default::default(),
        withdrawals: Default::default(),
    };

    let consensus_block = ConsensusBlock {
        parent_hash: Hash256::from_low_u64_be(slot.saturating_sub(1)),
        slot,
        auxpow_header: None,
        execution_payload,
        pegins: vec![],
        pegout_payment_proposal: None,
        finalized_pegouts: vec![],
    };

    AlysConsensusBlock {
        message: consensus_block,
        signature: AggregateApproval::new(),
    }
}

/// Create a test block with specific properties for edge case testing
pub fn create_test_block_with_properties(
    slot: u64,
    gas_used: u64,
    timestamp: u64,
    extra_data: Vec<u8>,
) -> AlysConsensusBlock {
    let execution_payload = ExecutionPayloadCapella::<MainnetEthSpec> {
        parent_hash: ExecutionBlockHash::from_root(Hash256::from_low_u64_be(
            slot.saturating_sub(1),
        )),
        fee_recipient: Address::zero(),
        state_root: Hash256::from_low_u64_be(slot + 1000),
        receipts_root: Hash256::from_low_u64_be(slot + 2000),
        logs_bloom: Default::default(),
        prev_randao: Hash256::from_low_u64_be(slot + 3000),
        block_number: slot,
        gas_limit: 30000000,
        gas_used,
        timestamp,
        extra_data: extra_data.into(),
        base_fee_per_gas: (1000000000u64 + slot * 100).into(),
        block_hash: ExecutionBlockHash::from_root(Hash256::from_low_u64_be(slot + 4000)),
        transactions: Default::default(),
        withdrawals: Default::default(),
    };

    let consensus_block = ConsensusBlock {
        parent_hash: Hash256::from_low_u64_be(slot.saturating_sub(1)),
        slot,
        auxpow_header: None,
        execution_payload,
        pegins: vec![],
        pegout_payment_proposal: None,
        finalized_pegouts: vec![],
    };

    AlysConsensusBlock {
        message: consensus_block,
        signature: AggregateApproval::new(),
    }
}

/// Generate test blocks for fork testing
pub fn create_fork_test_blocks(
    common_ancestor_slot: u64,
    fork_length: usize,
) -> (Vec<AlysConsensusBlock>, Vec<AlysConsensusBlock>) {
    // Create common chain up to fork point
    let mut common_chain = create_test_block_sequence(common_ancestor_slot as usize);

    let fork_parent = common_chain.last().unwrap().clone();

    // Create fork A
    let mut fork_a: Vec<AlysConsensusBlock> = Vec::new();
    for i in 0..fork_length {
        let slot = common_ancestor_slot + 1 + i as u64;
        let parent_hash = if i == 0 {
            fork_parent.message.parent_hash
        } else {
            fork_a[i - 1].message.parent_hash
        };

        let signed_block = create_test_block_with_properties(
            slot,
            slot * 1000, // Different gas usage pattern
            1600000000 + slot * 12,
            format!("fork_a_{}", slot).into_bytes(),
        );
        fork_a.push(signed_block);
    }

    // Create fork B with different properties
    let mut fork_b: Vec<AlysConsensusBlock> = Vec::new();
    for i in 0..fork_length {
        let slot = common_ancestor_slot + 1 + i as u64;
        let parent_hash = if i == 0 {
            fork_parent.message.parent_hash
        } else {
            fork_b[i - 1].message.parent_hash
        };

        let signed_block = create_test_block_with_properties(
            slot,
            slot * 2000,                // Different gas usage pattern
            1600000000 + slot * 12 + 1, // Slightly different timestamp
            format!("fork_b_{}", slot).into_bytes(),
        );
        fork_b.push(signed_block);
    }

    (fork_a, fork_b)
}

/// Generate blocks with various edge cases for comprehensive testing
pub fn create_edge_case_blocks() -> Vec<AlysConsensusBlock> {
    let mut blocks = Vec::new();

    // Block with minimal gas usage
    blocks.push(create_test_block_with_properties(1, 0, 1600000000, vec![]));

    // Block with maximum gas usage
    blocks.push(create_test_block_with_properties(
        2,
        30000000,
        1600000012,
        vec![],
    ));

    // Block with large extra data
    blocks.push(create_test_block_with_properties(
        3,
        1500000,
        1600000024,
        vec![0xff; 1024],
    ));

    // Block with very old timestamp
    blocks.push(create_test_block_with_properties(
        4,
        1000000,
        946684800,
        b"year_2000".to_vec(),
    )); // Year 2000

    // Block with far future timestamp
    blocks.push(create_test_block_with_properties(
        5,
        1000000,
        4102444800,
        b"year_2100".to_vec(),
    )); // Year 2100

    blocks
}

/// Generate test blocks for performance testing
pub fn create_performance_test_blocks(
    count: usize,
    with_transactions: bool,
) -> Vec<AlysConsensusBlock> {
    let mut blocks: Vec<AlysConsensusBlock> = Vec::with_capacity(count);

    for i in 0..count {
        let slot = i as u64 + 1;
        let parent_hash = if i == 0 {
            Hash256::zero()
        } else {
            blocks[i - 1].message.parent_hash
        };

        let mut execution_payload = ExecutionPayloadCapella::<MainnetEthSpec> {
            parent_hash: ExecutionBlockHash::from_root(parent_hash),
            fee_recipient: Address::zero(),
            state_root: Hash256::from_low_u64_be(slot + 1000),
            receipts_root: Hash256::from_low_u64_be(slot + 2000),
            logs_bloom: Default::default(),
            prev_randao: Hash256::from_low_u64_be(slot + 3000),
            block_number: slot,
            gas_limit: 30000000,
            gas_used: slot * 1000,
            timestamp: 1600000000 + slot * 12,
            extra_data: format!("perf_test_{}", slot).into_bytes().into(),
            base_fee_per_gas: (1000000000u64 + slot * 100).into(),
            block_hash: ExecutionBlockHash::from_root(Hash256::from_low_u64_be(slot + 4000)),
            transactions: Default::default(),
            withdrawals: Default::default(),
        };

        // Add dummy transactions for more realistic performance testing
        if with_transactions {
            // In a real implementation, you'd add actual transactions here
            // For now, we just increase gas usage to simulate transaction presence
            execution_payload.gas_used = execution_payload.gas_limit / 2;
        }

        let consensus_block = ConsensusBlock {
            parent_hash,
            slot,
            auxpow_header: None,
            execution_payload,
            pegins: vec![],
            pegout_payment_proposal: None,
            finalized_pegouts: vec![],
        };

        blocks.push(AlysConsensusBlock {
            message: consensus_block,
            signature: AggregateApproval::new(),
        });
    }

    blocks
}

/// Create test data for state operations
pub fn create_test_state_data(count: usize) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut data = Vec::with_capacity(count);

    for i in 0..count {
        let key = format!("test_key_{}", i).into_bytes();
        let value = format!(
            "test_value_{}_{}",
            i,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        )
        .into_bytes();
        data.push((key, value));
    }

    data
}

/// Create test data with specific patterns for edge case testing
pub fn create_edge_case_state_data() -> Vec<(Vec<u8>, Vec<u8>)> {
    vec![
        // Empty key
        (vec![], b"empty_key_value".to_vec()),
        // Empty value
        (b"empty_value_key".to_vec(), vec![]),
        // Large key
        (vec![b'k'; 1024], b"large_key_value".to_vec()),
        // Large value
        (b"large_value_key".to_vec(), vec![b'v'; 10240]),
        // Binary data key
        ((0..=255u8).collect(), b"binary_key_value".to_vec()),
        // Binary data value
        (b"binary_value_key".to_vec(), (0..=255u8).collect()),
        // UTF-8 key and value
        (
            "🚀test_key🚀".as_bytes().to_vec(),
            "🌟test_value🌟".as_bytes().to_vec(),
        ),
    ]
}

/// Create temporary directory with a descriptive prefix
pub fn create_test_temp_dir(test_name: &str) -> Result<TempDir, std::io::Error> {
    tempfile::Builder::new()
        .prefix(&format!("storage_test_{}_", test_name))
        .tempdir()
}

/// Generate deterministic test data based on a seed
pub fn create_deterministic_test_blocks(count: usize, seed: u64) -> Vec<AlysConsensusBlock> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut blocks: Vec<AlysConsensusBlock> = Vec::with_capacity(count);

    for i in 0..count {
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        i.hash(&mut hasher);
        let hash_value = hasher.finish();

        let slot = i as u64 + 1;
        let parent_hash = if i == 0 {
            Hash256::zero()
        } else {
            blocks[i - 1].message.parent_hash
        };

        let execution_payload = ExecutionPayloadCapella::<MainnetEthSpec> {
            parent_hash: ExecutionBlockHash::from_root(parent_hash),
            fee_recipient: Address::zero(),
            state_root: Hash256::from_low_u64_be((hash_value & 0xFFFFFFFF) as u64),
            receipts_root: Hash256::from_low_u64_be(((hash_value >> 32) & 0xFFFFFFFF) as u64),
            logs_bloom: Default::default(),
            prev_randao: Hash256::from_low_u64_be(hash_value),
            block_number: slot,
            gas_limit: 30000000,
            gas_used: (hash_value % 30000000) as u64,
            timestamp: 1600000000 + slot * 12,
            extra_data: format!("deterministic_{}", hash_value).into_bytes().into(),
            base_fee_per_gas: ((hash_value % 1000000000) + 1000000000).into(),
            block_hash: ExecutionBlockHash::from_root(Hash256::from_low_u64_be(slot + 4000)),
            transactions: Default::default(),
            withdrawals: Default::default(),
        };

        let consensus_block = ConsensusBlock {
            parent_hash,
            slot,
            auxpow_header: None,
            execution_payload,
            pegins: vec![],
            pegout_payment_proposal: None,
            finalized_pegouts: vec![],
        };

        blocks.push(AlysConsensusBlock {
            message: consensus_block,
            signature: AggregateApproval::new(),
        });
    }

    blocks
}
