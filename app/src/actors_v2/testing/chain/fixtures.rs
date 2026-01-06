//! ChainActor Test Fixtures
//!
//! Test data and utilities for ChainActor testing

use bitcoin::hashes::Hash;
use bitcoin::{BlockHash as BitcoinBlockHash, Txid};
use ethereum_types::{Address, H256, U256};
use std::str::FromStr;
use std::time::Duration;

use crate::actors_v2::chain::{
    messages::{AuxPowParams, ChainStatus, PegOutRequest},
    ChainConfig,
};
use bridge::PegInInfo;

/// Test fixture for validator configuration
pub fn validator_config() -> ChainConfig {
    let mut config = ChainConfig::default();
    config.is_validator = true;
    config.enable_auxpow = true;
    config.enable_peg_operations = true;
    config.max_blocks_without_pow = 100;
    config.federation = vec![
        Address::from_low_u64_be(1),
        Address::from_low_u64_be(2),
        Address::from_low_u64_be(3),
    ];
    config
}

/// Test fixture for non-validator configuration
pub fn non_validator_config() -> ChainConfig {
    let mut config = ChainConfig::default();
    config.is_validator = false;
    config.enable_auxpow = true;
    config.enable_peg_operations = false;
    config
}

/// Test fixture for minimal configuration
pub fn minimal_config() -> ChainConfig {
    ChainConfig {
        is_validator: false,
        validator_address: None,
        federation: vec![Address::from_low_u64_be(1)],
        max_blocks_without_pow: 10,
        block_production_timeout: Duration::from_secs(5),
        block_validation_timeout: Duration::from_secs(2),
        enable_auxpow: false,
        enable_peg_operations: false,
        retarget_params: None,
        block_hash_cache_size: Some(100),
        chain_id: 1337, // Priority 5: added field
    }
}

/// Test fixture for mock chain status
pub fn mock_chain_status() -> ChainStatus {
    ChainStatus {
        height: 100,
        head_hash: Some(H256::from_low_u64_be(42)),
        is_synced: true,
        is_validator: true,
        network_connected: true,
        peer_count: 5,
        pending_pegins: 3,
        last_block_time: Some(Duration::from_secs(1640995200)), // Mock timestamp
        auxpow_enabled: true,
        blocks_without_pow: 0,
        observed_height: 100,
        orphan_count: 0,
    }
}

/// Test fixture for mock peg-in info
pub fn mock_pegin_info() -> PegInInfo {
    PegInInfo {
        txid: Txid::from_byte_array([1u8; 32]),
        amount: 100000000, // 1 BTC in satoshis
        evm_account: Address::from_low_u64_be(123),
        block_hash: BitcoinBlockHash::from_byte_array([2u8; 32]),
        block_height: 100,
    }
}

/// Test fixture for mock peg-out request
pub fn mock_pegout_request() -> PegOutRequest {
    // Create a simple mock address for testing - in practice this would use proper Bitcoin address parsing
    let mock_address =
        bitcoin::Address::from_str("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq").unwrap();

    PegOutRequest {
        recipient: mock_address,
        amount: 50000000, // 0.5 BTC in satoshis
        requester: Address::from_low_u64_be(456),
        nonce: U256::from(1),
    }
}

/// Test fixture for AuxPoW parameters
pub fn mock_auxpow_params() -> AuxPowParams {
    AuxPowParams {
        target_difficulty: U256::from_dec_str(
            "26959946667150639794667015087019630673637144422540572481103610249215",
        )
        .expect("Valid difficulty"),
        retarget_params: Some(crate::actors_v2::chain::config::BitcoinConsensusParams::default()),
    }
}

/// Test fixture for mock AuxPow
pub fn mock_auxpow() -> crate::auxpow::AuxPow {
    crate::auxpow::AuxPow {
        coinbase_txn: bitcoin::Transaction {
            version: 1,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![bitcoin::TxIn {
                previous_output: bitcoin::OutPoint::null(),
                script_sig: bitcoin::ScriptBuf::new(),
                sequence: bitcoin::Sequence::ZERO,
                witness: bitcoin::Witness::new(),
            }],
            output: vec![bitcoin::TxOut {
                value: 5000000000, // 50 BTC
                script_pubkey: bitcoin::ScriptBuf::new(),
            }],
        },
        block_hash: bitcoin::BlockHash::from_byte_array([1u8; 32]),
        coinbase_branch: crate::auxpow::MerkleBranch {
            branch_hash: vec![],
            branch_side_mask: 0,
        },
        blockchain_branch: crate::auxpow::MerkleBranch {
            branch_hash: vec![],
            branch_side_mask: 0,
        },
        parent_block: bitcoin::block::Header {
            version: bitcoin::block::Version::ONE,
            prev_blockhash: bitcoin::BlockHash::from_byte_array([0u8; 32]),
            merkle_root: bitcoin::hash_types::TxMerkleNode::from_byte_array([1u8; 32]),
            time: 1640995200,
            bits: bitcoin::CompactTarget::from_consensus(0x207fffff),
            nonce: 12345,
        },
    }
}

/// Test fixture for multiple peg-in infos
pub fn mock_multiple_pegins() -> Vec<PegInInfo> {
    vec![
        PegInInfo {
            txid: Txid::from_byte_array([1u8; 32]),
            amount: 100000000,
            evm_account: Address::from_low_u64_be(100),
            block_hash: BitcoinBlockHash::from_byte_array([10u8; 32]),
            block_height: 100,
        },
        PegInInfo {
            txid: Txid::from_byte_array([2u8; 32]),
            amount: 200000000,
            evm_account: Address::from_low_u64_be(200),
            block_hash: BitcoinBlockHash::from_byte_array([20u8; 32]),
            block_height: 101,
        },
        PegInInfo {
            txid: Txid::from_byte_array([3u8; 32]),
            amount: 50000000,
            evm_account: Address::from_low_u64_be(300),
            block_hash: BitcoinBlockHash::from_byte_array([30u8; 32]),
            block_height: 102,
        },
    ]
}

/// Test fixture for multiple peg-out requests
pub fn mock_multiple_pegouts() -> Vec<PegOutRequest> {
    vec![
        PegOutRequest {
            recipient: bitcoin::Address::from_str("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq")
                .unwrap(),
            amount: 25000000,
            requester: Address::from_low_u64_be(100),
            nonce: U256::from(1),
        },
        PegOutRequest {
            recipient: bitcoin::Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
                .unwrap(),
            amount: 75000000,
            requester: Address::from_low_u64_be(200),
            nonce: U256::from(2),
        },
    ]
}

/// Test utility to create deterministic addresses
pub fn test_address(id: u64) -> Address {
    Address::from_low_u64_be(id)
}

/// Test utility to create deterministic Bitcoin block hashes
pub fn test_bitcoin_block_hash(id: u8) -> BitcoinBlockHash {
    let mut bytes = [0u8; 32];
    bytes[0] = id;
    BitcoinBlockHash::from_byte_array(bytes)
}

/// Test utility to create deterministic transaction IDs
pub fn test_txid(id: u8) -> Txid {
    let mut bytes = [0u8; 32];
    bytes[31] = id; // Put ID at the end for uniqueness
    Txid::from_byte_array(bytes)
}
