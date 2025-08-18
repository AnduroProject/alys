//! Blockchain data structure generators for property-based testing
//!
//! This module provides PropTest generators for all major Alys blockchain data structures,
//! network components, actor messages, and governance elements. These generators create
//! realistic, diverse test data for comprehensive property-based testing.

use proptest::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// ALYS-002-16: PropTest Framework with Custom Generators for Blockchain Data Structures

// ========== Blockchain Data Structure Generators ==========

/// Block hash generator - 32-byte hex strings
pub fn block_hash_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<u8>(), 32)
        .prop_map(|bytes| hex::encode(bytes))
}

/// Transaction hash generator - 32-byte hex strings
pub fn transaction_hash_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<u8>(), 32)
        .prop_map(|bytes| hex::encode(bytes))
}

/// Ethereum address generator - 20-byte hex strings
pub fn eth_address_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<u8>(), 20)
        .prop_map(|bytes| format!("0x{}", hex::encode(bytes)))
}

/// Bitcoin address generator - realistic Bitcoin addresses
pub fn btc_address_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // P2PKH addresses (start with 1)
        "[13][a-km-zA-HJ-NP-Z1-9]{25,34}",
        // P2SH addresses (start with 3)  
        "3[a-km-zA-HJ-NP-Z1-9]{25,34}",
        // Bech32 addresses (start with bc1)
        "bc1[ac-hj-np-z02-9]{39,59}"
    ].prop_map(|pattern| {
        // For property testing, we'll generate fixed-format addresses
        match pattern.chars().next().unwrap() {
            '1' => format!("1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2"),  // Example P2PKH
            '3' => format!("3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy"),  // Example P2SH
            _ => format!("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")   // Example Bech32
        }
    })
}

/// Signed block generator
#[derive(Debug, Clone)]
pub struct SignedBlock {
    pub hash: String,
    pub parent_hash: String,
    pub height: u64,
    pub timestamp: u64,
    pub transactions: Vec<Transaction>,
    pub merkle_root: String,
    pub state_root: String,
    pub federation_signatures: Vec<FederationSignature>,
    pub gas_limit: u64,
    pub gas_used: u64,
}

pub fn signed_block_strategy() -> impl Strategy<Value = SignedBlock> {
    (
        block_hash_strategy(),
        block_hash_strategy(),
        0u64..1_000_000,
        (UNIX_EPOCH.elapsed().unwrap().as_secs() - 86400)..UNIX_EPOCH.elapsed().unwrap().as_secs(),
        prop::collection::vec(transaction_strategy(), 0..50),
        block_hash_strategy(),
        block_hash_strategy(),
        prop::collection::vec(federation_signature_strategy(), 3..7),
        1_000_000u64..30_000_000,
        0u64..30_000_000,
    ).prop_map(|(hash, parent_hash, height, timestamp, transactions, merkle_root, 
                state_root, federation_signatures, gas_limit, gas_used)| {
        SignedBlock {
            hash,
            parent_hash,
            height,
            timestamp,
            transactions,
            merkle_root,
            state_root,
            federation_signatures,
            gas_limit,
            gas_used: gas_used.min(gas_limit),
        }
    })
}

/// Mined block generator (with PoW)
#[derive(Debug, Clone)]
pub struct MinedBlock {
    pub signed_blocks: Vec<SignedBlock>,
    pub block_bundle_hash: String,
    pub bitcoin_block_hash: String,
    pub auxpow: AuxPoW,
    pub difficulty_target: u32,
    pub timestamp: u64,
}

pub fn mined_block_strategy() -> impl Strategy<Value = MinedBlock> {
    (
        prop::collection::vec(signed_block_strategy(), 1..10),
        block_hash_strategy(),
        block_hash_strategy(),
        auxpow_strategy(),
        0x1d00ffffu32..0x207fffffu32,
        (UNIX_EPOCH.elapsed().unwrap().as_secs() - 3600)..UNIX_EPOCH.elapsed().unwrap().as_secs(),
    ).prop_map(|(signed_blocks, block_bundle_hash, bitcoin_block_hash, 
                auxpow, difficulty_target, timestamp)| {
        MinedBlock {
            signed_blocks,
            block_bundle_hash,
            bitcoin_block_hash,
            auxpow,
            difficulty_target,
            timestamp,
        }
    })
}

/// Transaction generator
#[derive(Debug, Clone)]
pub struct Transaction {
    pub hash: String,
    pub from: String,
    pub to: Option<String>,
    pub value: u64,
    pub gas_price: u64,
    pub gas_limit: u64,
    pub nonce: u64,
    pub data: Vec<u8>,
    pub signature: TransactionSignature,
}

pub fn transaction_strategy() -> impl Strategy<Value = Transaction> {
    (
        transaction_hash_strategy(),
        eth_address_strategy(),
        prop::option::of(eth_address_strategy()),
        0u64..1_000_000_000_000_000_000, // Up to 1 ETH in wei
        1_000_000_000u64..100_000_000_000, // 1-100 gwei
        21_000u64..10_000_000,
        0u64..1000,
        prop::collection::vec(any::<u8>(), 0..1024),
        transaction_signature_strategy(),
    ).prop_map(|(hash, from, to, value, gas_price, gas_limit, nonce, data, signature)| {
        Transaction {
            hash,
            from,
            to,
            value,
            gas_price,
            gas_limit,
            nonce,
            data,
            signature,
        }
    })
}

/// AuxPoW (Auxiliary Proof of Work) generator
#[derive(Debug, Clone)]
pub struct AuxPoW {
    pub bitcoin_block_header: BitcoinBlockHeader,
    pub coinbase_transaction: CoinbaseTransaction,
    pub merkle_branch: Vec<String>,
    pub merkle_index: u32,
    pub parent_merkle_branch: Vec<String>,
    pub parent_merkle_index: u32,
}

pub fn auxpow_strategy() -> impl Strategy<Value = AuxPoW> {
    (
        bitcoin_block_header_strategy(),
        coinbase_transaction_strategy(),
        prop::collection::vec(block_hash_strategy(), 1..15),
        any::<u32>(),
        prop::collection::vec(block_hash_strategy(), 1..15),
        any::<u32>(),
    ).prop_map(|(bitcoin_block_header, coinbase_transaction, merkle_branch,
                merkle_index, parent_merkle_branch, parent_merkle_index)| {
        AuxPoW {
            bitcoin_block_header,
            coinbase_transaction,
            merkle_branch,
            merkle_index,
            parent_merkle_branch,
            parent_merkle_index,
        }
    })
}

/// Bitcoin block header generator
#[derive(Debug, Clone)]
pub struct BitcoinBlockHeader {
    pub version: u32,
    pub previous_block_hash: String,
    pub merkle_root: String,
    pub timestamp: u32,
    pub bits: u32,
    pub nonce: u32,
}

pub fn bitcoin_block_header_strategy() -> impl Strategy<Value = BitcoinBlockHeader> {
    (
        0x20000000u32..0x3fffffffu32,
        block_hash_strategy(),
        block_hash_strategy(),
        (UNIX_EPOCH.elapsed().unwrap().as_secs() as u32 - 3600)..(UNIX_EPOCH.elapsed().unwrap().as_secs() as u32),
        0x1d00ffffu32..0x207fffffu32,
        any::<u32>(),
    ).prop_map(|(version, previous_block_hash, merkle_root, timestamp, bits, nonce)| {
        BitcoinBlockHeader {
            version,
            previous_block_hash,
            merkle_root,
            timestamp,
            bits,
            nonce,
        }
    })
}

/// Coinbase transaction generator
#[derive(Debug, Clone)]
pub struct CoinbaseTransaction {
    pub version: u32,
    pub inputs: Vec<CoinbaseInput>,
    pub outputs: Vec<TransactionOutput>,
    pub lock_time: u32,
}

#[derive(Debug, Clone)]
pub struct CoinbaseInput {
    pub previous_output: OutPoint,
    pub script_sig: Vec<u8>,
    pub sequence: u32,
}

#[derive(Debug, Clone)]
pub struct OutPoint {
    pub txid: String,
    pub vout: u32,
}

#[derive(Debug, Clone)]
pub struct TransactionOutput {
    pub value: u64,
    pub script_pubkey: Vec<u8>,
}

pub fn coinbase_transaction_strategy() -> impl Strategy<Value = CoinbaseTransaction> {
    (
        1u32..2,
        prop::collection::vec(coinbase_input_strategy(), 1..1), // Coinbase has exactly 1 input
        prop::collection::vec(transaction_output_strategy(), 1..10),
        any::<u32>(),
    ).prop_map(|(version, inputs, outputs, lock_time)| {
        CoinbaseTransaction {
            version,
            inputs,
            outputs,
            lock_time,
        }
    })
}

pub fn coinbase_input_strategy() -> impl Strategy<Value = CoinbaseInput> {
    (
        outpoint_strategy(),
        prop::collection::vec(any::<u8>(), 2..100),
        any::<u32>(),
    ).prop_map(|(previous_output, script_sig, sequence)| {
        CoinbaseInput {
            previous_output,
            script_sig,
            sequence,
        }
    })
}

pub fn outpoint_strategy() -> impl Strategy<Value = OutPoint> {
    (
        transaction_hash_strategy(),
        any::<u32>(),
    ).prop_map(|(txid, vout)| {
        OutPoint { txid, vout }
    })
}

pub fn transaction_output_strategy() -> impl Strategy<Value = TransactionOutput> {
    (
        0u64..2_100_000_000_000_000, // Max 21M BTC in satoshis
        prop::collection::vec(any::<u8>(), 1..100),
    ).prop_map(|(value, script_pubkey)| {
        TransactionOutput {
            value,
            script_pubkey,
        }
    })
}

// ========== Network and P2P Generators ==========

/// P2P network message generator
#[derive(Debug, Clone)]
pub struct NetworkMessage {
    pub message_type: NetworkMessageType,
    pub sender_id: String,
    pub receiver_id: Option<String>, // None for broadcast
    pub payload: Vec<u8>,
    pub timestamp: SystemTime,
    pub sequence_id: u64,
}

#[derive(Debug, Clone)]
pub enum NetworkMessageType {
    BlockAnnouncement,
    TransactionAnnouncement,
    SyncRequest,
    SyncResponse,
    PeerHandshake,
    PeerDisconnect,
    CheckpointAnnouncement,
}

pub fn network_message_strategy() -> impl Strategy<Value = NetworkMessage> {
    (
        network_message_type_strategy(),
        peer_id_strategy(),
        prop::option::of(peer_id_strategy()),
        prop::collection::vec(any::<u8>(), 32..2048),
        system_time_strategy(),
        any::<u64>(),
    ).prop_map(|(message_type, sender_id, receiver_id, payload, timestamp, sequence_id)| {
        NetworkMessage {
            message_type,
            sender_id,
            receiver_id,
            payload,
            timestamp,
            sequence_id,
        }
    })
}

pub fn network_message_type_strategy() -> impl Strategy<Value = NetworkMessageType> {
    prop_oneof![
        Just(NetworkMessageType::BlockAnnouncement),
        Just(NetworkMessageType::TransactionAnnouncement),
        Just(NetworkMessageType::SyncRequest),
        Just(NetworkMessageType::SyncResponse),
        Just(NetworkMessageType::PeerHandshake),
        Just(NetworkMessageType::PeerDisconnect),
        Just(NetworkMessageType::CheckpointAnnouncement),
    ]
}

/// Peer information generator
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: String,
    pub address: String,
    pub port: u16,
    pub capabilities: Vec<PeerCapability>,
    pub connection_time: SystemTime,
    pub last_seen: SystemTime,
    pub reputation_score: i32,
}

#[derive(Debug, Clone)]
pub enum PeerCapability {
    FullSync,
    FastSync,
    ArchiveNode,
    LightClient,
    MergedMining,
}

pub fn peer_info_strategy() -> impl Strategy<Value = PeerInfo> {
    (
        peer_id_strategy(),
        ip_address_strategy(),
        1000u16..65535,
        prop::collection::vec(peer_capability_strategy(), 1..5),
        system_time_strategy(),
        system_time_strategy(),
        -100i32..1000,
    ).prop_map(|(peer_id, address, port, capabilities, connection_time, 
                last_seen, reputation_score)| {
        PeerInfo {
            peer_id,
            address,
            port,
            capabilities,
            connection_time,
            last_seen,
            reputation_score,
        }
    })
}

pub fn peer_capability_strategy() -> impl Strategy<Value = PeerCapability> {
    prop_oneof![
        Just(PeerCapability::FullSync),
        Just(PeerCapability::FastSync),
        Just(PeerCapability::ArchiveNode),
        Just(PeerCapability::LightClient),
        Just(PeerCapability::MergedMining),
    ]
}

// ========== Sync and Checkpoint Generators ==========

/// Checkpoint data generator
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointData {
    pub height: u64,
    pub block_hash: String,
    pub state_root: String,
    pub timestamp: u64,
    pub interval: u64,
    pub signature: Option<FederationSignature>,
    pub verified: bool,
}

pub fn checkpoint_data_strategy() -> impl Strategy<Value = CheckpointData> {
    (
        0u64..1_000_000,
        block_hash_strategy(),
        block_hash_strategy(),
        (UNIX_EPOCH.elapsed().unwrap().as_secs() - 86400)..UNIX_EPOCH.elapsed().unwrap().as_secs(),
        10u64..1000,
        prop::option::of(federation_signature_strategy()),
        any::<bool>(),
    ).prop_map(|(height, block_hash, state_root, timestamp, interval, signature, verified)| {
        CheckpointData {
            height,
            block_hash,
            state_root,
            timestamp,
            interval,
            signature,
            verified,
        }
    })
}

/// Sync state generator
#[derive(Debug, Clone)]
pub struct SyncState {
    pub current_height: u64,
    pub target_height: u64,
    pub syncing_from_peer: Option<String>,
    pub sync_speed: f64, // blocks per second
    pub last_checkpoint: Option<CheckpointData>,
    pub sync_stage: SyncStage,
}

#[derive(Debug, Clone)]
pub enum SyncStage {
    NotStarted,
    HeaderSync,
    BlockSync,
    StateSync,
    Complete,
    Failed(String),
}

pub fn sync_state_strategy() -> impl Strategy<Value = SyncState> {
    (
        0u64..1_000_000,
        0u64..1_000_000,
        prop::option::of(peer_id_strategy()),
        0.1f64..1000.0,
        prop::option::of(checkpoint_data_strategy()),
        sync_stage_strategy(),
    ).prop_map(|(current_height, target_height, syncing_from_peer, 
                sync_speed, last_checkpoint, sync_stage)| {
        SyncState {
            current_height,
            target_height: target_height.max(current_height),
            syncing_from_peer,
            sync_speed,
            last_checkpoint,
            sync_stage,
        }
    })
}

pub fn sync_stage_strategy() -> impl Strategy<Value = SyncStage> {
    prop_oneof![
        Just(SyncStage::NotStarted),
        Just(SyncStage::HeaderSync),
        Just(SyncStage::BlockSync),
        Just(SyncStage::StateSync),
        Just(SyncStage::Complete),
        "[a-zA-Z0-9 ]{5,50}".prop_map(|err| SyncStage::Failed(err)),
    ]
}

// ========== Actor System Generators ==========

/// Actor message generator
#[derive(Debug, Clone)]
pub struct ActorMessage {
    pub message_id: String,
    pub sender_id: String,
    pub receiver_id: String,
    pub message_type: ActorMessageType,
    pub payload: Vec<u8>,
    pub timestamp: SystemTime,
    pub priority: MessagePriority,
    pub retry_count: u8,
    pub sequence_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActorMessageType {
    Lifecycle(LifecycleMessage),
    Sync(SyncMessage),
    Network(NetworkCommand),
    Mining(MiningMessage),
    Governance(GovernanceMessage),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleMessage {
    Start,
    Stop,
    Restart,
    HealthCheck,
    StatusQuery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncMessage {
    StartSync { target_height: u64 },
    StopSync,
    SyncProgress { current_height: u64 },
    CheckpointReached { checkpoint: CheckpointData },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkCommand {
    ConnectToPeer { peer_id: String },
    DisconnectFromPeer { peer_id: String },
    BroadcastBlock { block_hash: String },
    RequestBlocks { start_height: u64, count: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiningMessage {
    StartMining,
    StopMining,
    NewBlockTemplate { template: Vec<u8> },
    SubmitBlock { block: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernanceMessage {
    ProposalSubmitted { proposal_id: String },
    VoteCast { proposal_id: String, vote: bool },
    ProposalExecuted { proposal_id: String },
    SignatureRequest { data: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    Low,
    Normal,
    High,
    Critical,
}

pub fn actor_message_strategy() -> impl Strategy<Value = ActorMessage> {
    (
        uuid_strategy(),
        actor_id_strategy(),
        actor_id_strategy(),
        actor_message_type_strategy(),
        prop::collection::vec(any::<u8>(), 0..1024),
        system_time_strategy(),
        message_priority_strategy(),
        0u8..5,
        1u64..1000000,
    ).prop_map(|(message_id, sender_id, receiver_id, message_type, 
                payload, timestamp, priority, retry_count, sequence_id)| {
        ActorMessage {
            message_id,
            sender_id,
            receiver_id,
            message_type,
            payload,
            timestamp,
            priority,
            retry_count,
            sequence_id,
        }
    })
}

pub fn actor_message_type_strategy() -> impl Strategy<Value = ActorMessageType> {
    prop_oneof![
        lifecycle_message_strategy().prop_map(ActorMessageType::Lifecycle),
        sync_message_strategy().prop_map(ActorMessageType::Sync),
        network_command_strategy().prop_map(ActorMessageType::Network),
        mining_message_strategy().prop_map(ActorMessageType::Mining),
        governance_message_strategy().prop_map(ActorMessageType::Governance),
    ]
}

pub fn lifecycle_message_strategy() -> impl Strategy<Value = LifecycleMessage> {
    prop_oneof![
        Just(LifecycleMessage::Start),
        Just(LifecycleMessage::Stop),
        Just(LifecycleMessage::Restart),
        Just(LifecycleMessage::HealthCheck),
        Just(LifecycleMessage::StatusQuery),
    ]
}

pub fn sync_message_strategy() -> impl Strategy<Value = SyncMessage> {
    prop_oneof![
        (0u64..1_000_000).prop_map(|target_height| SyncMessage::StartSync { target_height }),
        Just(SyncMessage::StopSync),
        (0u64..1_000_000).prop_map(|current_height| SyncMessage::SyncProgress { current_height }),
        checkpoint_data_strategy().prop_map(|checkpoint| SyncMessage::CheckpointReached { checkpoint }),
    ]
}

pub fn network_command_strategy() -> impl Strategy<Value = NetworkCommand> {
    prop_oneof![
        peer_id_strategy().prop_map(|peer_id| NetworkCommand::ConnectToPeer { peer_id }),
        peer_id_strategy().prop_map(|peer_id| NetworkCommand::DisconnectFromPeer { peer_id }),
        block_hash_strategy().prop_map(|block_hash| NetworkCommand::BroadcastBlock { block_hash }),
        (0u64..1_000_000, 1u64..1000).prop_map(|(start_height, count)| 
            NetworkCommand::RequestBlocks { start_height, count }
        ),
    ]
}

pub fn mining_message_strategy() -> impl Strategy<Value = MiningMessage> {
    prop_oneof![
        Just(MiningMessage::StartMining),
        Just(MiningMessage::StopMining),
        prop::collection::vec(any::<u8>(), 32..512)
            .prop_map(|template| MiningMessage::NewBlockTemplate { template }),
        prop::collection::vec(any::<u8>(), 100..2048)
            .prop_map(|block| MiningMessage::SubmitBlock { block }),
    ]
}

pub fn governance_message_strategy() -> impl Strategy<Value = GovernanceMessage> {
    prop_oneof![
        uuid_strategy().prop_map(|proposal_id| GovernanceMessage::ProposalSubmitted { proposal_id }),
        (uuid_strategy(), any::<bool>()).prop_map(|(proposal_id, vote)| 
            GovernanceMessage::VoteCast { proposal_id, vote }
        ),
        uuid_strategy().prop_map(|proposal_id| GovernanceMessage::ProposalExecuted { proposal_id }),
        prop::collection::vec(any::<u8>(), 32..256)
            .prop_map(|data| GovernanceMessage::SignatureRequest { data }),
    ]
}

pub fn message_priority_strategy() -> impl Strategy<Value = MessagePriority> {
    prop_oneof![
        Just(MessagePriority::Low),
        Just(MessagePriority::Normal),
        Just(MessagePriority::High),
        Just(MessagePriority::Critical),
    ]
}

// ========== Governance and Cryptographic Generators ==========

/// BLS signature generator
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BLSSignature {
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
    pub message_hash: String,
    pub signer_index: u8,
}

pub fn bls_signature_strategy() -> impl Strategy<Value = BLSSignature> {
    (
        prop::collection::vec(any::<u8>(), 96), // BLS signature is 96 bytes
        prop::collection::vec(any::<u8>(), 48), // BLS public key is 48 bytes
        block_hash_strategy(),
        0u8..10,
    ).prop_map(|(signature, public_key, message_hash, signer_index)| {
        BLSSignature {
            signature,
            public_key,
            message_hash,
            signer_index,
        }
    })
}

/// Federation signature generator
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FederationSignature {
    pub signatures: Vec<BLSSignature>,
    pub threshold: u8,
    pub signed_data_hash: String,
    pub timestamp: u64,
}

pub fn federation_signature_strategy() -> impl Strategy<Value = FederationSignature> {
    (
        prop::collection::vec(bls_signature_strategy(), 3..7),
        3u8..7,
        block_hash_strategy(),
        (UNIX_EPOCH.elapsed().unwrap().as_secs() - 3600)..UNIX_EPOCH.elapsed().unwrap().as_secs(),
    ).prop_map(|(signatures, threshold, signed_data_hash, timestamp)| {
        let sig_len = signatures.len() as u8;
        FederationSignature {
            signatures,
            threshold: threshold.min(sig_len),
            signed_data_hash,
            timestamp,
        }
    })
}

/// Transaction signature generator
#[derive(Debug, Clone)]
pub struct TransactionSignature {
    pub v: u8,
    pub r: Vec<u8>,
    pub s: Vec<u8>,
}

pub fn transaction_signature_strategy() -> impl Strategy<Value = TransactionSignature> {
    (
        0u8..4, // EIP-155: v = chainId * 2 + 35 + {0, 1}
        prop::collection::vec(any::<u8>(), 32),
        prop::collection::vec(any::<u8>(), 32),
    ).prop_map(|(v, r, s)| {
        TransactionSignature { v, r, s }
    })
}

/// Byzantine behavior generator
#[derive(Debug, Clone)]
pub struct ByzantineBehavior {
    pub behavior_type: ByzantineType,
    pub affected_nodes: Vec<String>,
    pub duration: Duration,
    pub intensity: f64, // 0.0 to 1.0
}

#[derive(Debug, Clone)]
pub enum ByzantineType {
    DoubleSigning,
    Withholding,
    EquivocationAttack,
    DelayedResponses,
    InvalidSignatures,
    NetworkPartition,
}

pub fn byzantine_behavior_strategy() -> impl Strategy<Value = ByzantineBehavior> {
    (
        byzantine_type_strategy(),
        prop::collection::vec(peer_id_strategy(), 1..5),
        duration_strategy(),
        0.0f64..1.0,
    ).prop_map(|(behavior_type, affected_nodes, duration, intensity)| {
        ByzantineBehavior {
            behavior_type,
            affected_nodes,
            duration,
            intensity,
        }
    })
}

pub fn byzantine_type_strategy() -> impl Strategy<Value = ByzantineType> {
    prop_oneof![
        Just(ByzantineType::DoubleSigning),
        Just(ByzantineType::Withholding),
        Just(ByzantineType::EquivocationAttack),
        Just(ByzantineType::DelayedResponses),
        Just(ByzantineType::InvalidSignatures),
        Just(ByzantineType::NetworkPartition),
    ]
}

// ========== Utility Generators ==========

pub fn peer_id_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        uuid_strategy(),
        "[a-f0-9]{40}".prop_map(|s| format!("peer_{}", s)),
    ]
}

pub fn actor_id_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9_]{5,20}".prop_map(|s| format!("actor_{}", s)),
        uuid_strategy(),
    ]
}

pub fn uuid_strategy() -> impl Strategy<Value = String> {
    Just(()).prop_map(|_| Uuid::new_v4().to_string())
}

pub fn ip_address_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // IPv4
        (0u8..=255, 0u8..=255, 0u8..=255, 0u8..=255)
            .prop_map(|(a, b, c, d)| format!("{}.{}.{}.{}", a, b, c, d)),
        // Common local addresses
        Just("127.0.0.1".to_string()),
        Just("localhost".to_string()),
    ]
}

pub fn duration_strategy() -> impl Strategy<Value = Duration> {
    (0u64..3600).prop_map(Duration::from_secs)
}

pub fn system_time_strategy() -> impl Strategy<Value = SystemTime> {
    (0u64..3_600_000).prop_map(|millis| {
        SystemTime::now() - Duration::from_millis(millis)
    })
}

// ========== Test Data Collections ==========

/// Generate a complete blockchain scenario with multiple blocks
pub fn blockchain_scenario_strategy() -> impl Strategy<Value = BlockchainScenario> {
    (
        prop::collection::vec(signed_block_strategy(), 10..100),
        prop::collection::vec(mined_block_strategy(), 1..10),
        prop::collection::vec(checkpoint_data_strategy(), 5..20),
        prop::collection::vec(peer_info_strategy(), 3..10),
    ).prop_map(|(signed_blocks, mined_blocks, checkpoints, peers)| {
        BlockchainScenario {
            signed_blocks,
            mined_blocks,
            checkpoints,
            peers,
        }
    })
}

#[derive(Debug, Clone)]
pub struct BlockchainScenario {
    pub signed_blocks: Vec<SignedBlock>,
    pub mined_blocks: Vec<MinedBlock>,
    pub checkpoints: Vec<CheckpointData>,
    pub peers: Vec<PeerInfo>,
}

/// Generate an actor system scenario with multiple actors and messages
pub fn actor_system_scenario_strategy() -> impl Strategy<Value = ActorSystemScenario> {
    (
        prop::collection::vec(actor_id_strategy(), 5..20),
        prop::collection::vec(actor_message_strategy(), 50..500),
        prop::collection::vec(sync_state_strategy(), 1..5),
    ).prop_map(|(actor_ids, messages, sync_states)| {
        ActorSystemScenario {
            actor_ids,
            messages,
            sync_states,
        }
    })
}

#[derive(Debug, Clone)]
pub struct ActorSystemScenario {
    pub actor_ids: Vec<String>,
    pub messages: Vec<ActorMessage>,
    pub sync_states: Vec<SyncState>,
}

/// Generate a governance scenario with multiple proposals and votes
pub fn governance_scenario_strategy() -> impl Strategy<Value = GovernanceScenario> {
    (
        prop::collection::vec(uuid_strategy(), 3..10), // proposals
        prop::collection::vec(federation_signature_strategy(), 5..15),
        prop::collection::vec(byzantine_behavior_strategy(), 0..3),
    ).prop_map(|(proposals, signatures, byzantine_behaviors)| {
        GovernanceScenario {
            proposals,
            signatures,
            byzantine_behaviors,
        }
    })
}

#[derive(Debug, Clone)]
pub struct GovernanceScenario {
    pub proposals: Vec<String>,
    pub signatures: Vec<FederationSignature>,
    pub byzantine_behaviors: Vec<ByzantineBehavior>,
}