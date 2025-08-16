//! Bridge and peg operation messages

use crate::types::*;
use actix::prelude::*;

/// Message to process a peg-in transaction
#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct ProcessPegInMessage {
    pub bitcoin_tx: bitcoin::Transaction,
    pub confirmation_count: u32,
}

/// Message to process a peg-out request
#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct ProcessPegOutMessage {
    pub burn_tx_hash: H256,
    pub recipient_address: bitcoin::Address,
    pub amount: u64,
}

/// Message to get peg-in status
#[derive(Message)]
#[rtype(result = "Result<Option<PegInStatus>, BridgeError>")]
pub struct GetPegInStatusMessage {
    pub bitcoin_tx_id: bitcoin::Txid,
}

/// Message to get peg-out status
#[derive(Message)]
#[rtype(result = "Result<Option<PegOutStatus>, BridgeError>")]
pub struct GetPegOutStatusMessage {
    pub burn_tx_hash: H256,
}

/// Message to collect federation signature for peg-out
#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct CollectSignatureMessage {
    pub peg_out_id: String,
    pub signature: FederationSignature,
    pub signer: Address,
}

/// Message to broadcast Bitcoin transaction
#[derive(Message)]
#[rtype(result = "Result<bitcoin::Txid, BridgeError>")]
pub struct BroadcastBitcoinTxMessage {
    pub transaction: bitcoin::Transaction,
}

/// Message to get bridge statistics
#[derive(Message)]
#[rtype(result = "BridgeStats")]
pub struct GetBridgeStatsMessage;

/// Message to update federation configuration
#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct UpdateFederationConfigMessage {
    pub new_config: FederationConfig,
}

/// Message to handle Bitcoin block event
#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct BitcoinBlockEventMessage {
    pub block: bitcoin::Block,
    pub height: u64,
}

/// Message to monitor Bitcoin address
#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct MonitorAddressMessage {
    pub address: bitcoin::Address,
    pub purpose: MonitorPurpose,
}

/// Message to handle Bitcoin transaction confirmation
#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct BitcoinTxConfirmationMessage {
    pub tx_id: bitcoin::Txid,
    pub confirmation_count: u32,
    pub block_height: u64,
}

/// Message to request UTXO consolidation
#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct ConsolidateUtxosMessage {
    pub threshold_amount: u64,
    pub target_count: usize,
}

/// Message to get available UTXOs
#[derive(Message)]
#[rtype(result = "Result<Vec<UtxoInfo>, BridgeError>")]
pub struct GetUtxosMessage {
    pub min_amount: Option<u64>,
    pub max_count: Option<usize>,
}

/// Message to handle fee estimation
#[derive(Message)]
#[rtype(result = "Result<FeeEstimate, BridgeError>")]
pub struct EstimateFeeMessage {
    pub tx_size_bytes: usize,
    pub confirmation_target: u32,
}

/// Peg-in operation status
#[derive(Debug, Clone)]
pub enum PegInStatus {
    Detected {
        bitcoin_tx: bitcoin::Transaction,
        detected_at: std::time::SystemTime,
    },
    Confirming {
        confirmations: u32,
        required_confirmations: u32,
    },
    Validated {
        alys_recipient: Address,
        amount: u64,
        validated_at: std::time::SystemTime,
    },
    Completed {
        alys_tx_hash: H256,
        completed_at: std::time::SystemTime,
    },
    Failed {
        error: String,
        failed_at: std::time::SystemTime,
    },
}

/// Peg-out operation status
#[derive(Debug, Clone)]
pub enum PegOutStatus {
    Initiated {
        burn_tx_hash: H256,
        recipient: bitcoin::Address,
        amount: u64,
        initiated_at: std::time::SystemTime,
    },
    CollectingSignatures {
        signatures_collected: usize,
        signatures_required: usize,
        signing_deadline: std::time::SystemTime,
    },
    SigningComplete {
        bitcoin_tx: bitcoin::Transaction,
        completed_signatures: Vec<FederationSignature>,
    },
    Broadcasting {
        bitcoin_tx: bitcoin::Transaction,
        broadcast_attempts: u32,
    },
    Confirmed {
        bitcoin_tx_id: bitcoin::Txid,
        confirmation_count: u32,
        confirmed_at: std::time::SystemTime,
    },
    Failed {
        error: String,
        failed_at: std::time::SystemTime,
    },
}

/// Federation signature for multi-sig operations
#[derive(Debug, Clone)]
pub struct FederationSignature {
    pub signature: Vec<u8>,
    pub public_key: bitcoin::PublicKey,
    pub signature_type: SignatureType,
    pub message_hash: bitcoin::secp256k1::Message,
}

/// Type of signature scheme used
#[derive(Debug, Clone)]
pub enum SignatureType {
    ECDSA,
    Schnorr,
    BLS,
}

/// Federation configuration
#[derive(Debug, Clone)]
pub struct FederationConfig {
    pub members: Vec<FederationMember>,
    pub threshold: usize,
    pub multisig_address: bitcoin::Address,
    pub emergency_addresses: Vec<bitcoin::Address>,
    pub signing_timeout: std::time::Duration,
}

/// Federation member information
#[derive(Debug, Clone)]
pub struct FederationMember {
    pub address: Address,
    pub bitcoin_public_key: bitcoin::PublicKey,
    pub is_active: bool,
    pub reputation_score: i32,
    pub last_activity: std::time::SystemTime,
}

/// Purpose for monitoring Bitcoin addresses
#[derive(Debug, Clone)]
pub enum MonitorPurpose {
    PegIn,
    PegOut,
    Federation,
    Emergency,
}

/// UTXO information
#[derive(Debug, Clone)]
pub struct UtxoInfo {
    pub outpoint: bitcoin::OutPoint,
    pub value: u64,
    pub script_pubkey: bitcoin::ScriptBuf,
    pub confirmations: u32,
    pub is_locked: bool,
}

/// Fee estimation result
#[derive(Debug, Clone)]
pub struct FeeEstimate {
    pub sat_per_byte: u64,
    pub total_fee: u64,
    pub confidence: f64,
    pub estimated_blocks: u32,
}

/// Bridge operation statistics
#[derive(Debug, Clone)]
pub struct BridgeStats {
    pub total_pegins: u64,
    pub total_pegouts: u64,
    pub pending_pegins: u64,
    pub pending_pegouts: u64,
    pub total_value_pegged_in: u64,
    pub total_value_pegged_out: u64,
    pub average_pegin_time: std::time::Duration,
    pub average_pegout_time: std::time::Duration,
    pub federation_health: FederationHealth,
}

/// Federation health status
#[derive(Debug, Clone)]
pub struct FederationHealth {
    pub active_members: usize,
    pub total_members: usize,
    pub threshold_met: bool,
    pub last_successful_signing: std::time::SystemTime,
    pub signing_failures: u64,
}

/// Bitcoin wallet operations
#[derive(Message)]
#[rtype(result = "Result<WalletInfo, BridgeError>")]
pub struct GetWalletInfoMessage;

/// Wallet information
#[derive(Debug, Clone)]
pub struct WalletInfo {
    pub balance: u64,
    pub unconfirmed_balance: u64,
    pub utxo_count: usize,
    pub addresses_monitored: usize,
    pub last_sync_block: u64,
}

/// Message to create new federation address
#[derive(Message)]
#[rtype(result = "Result<bitcoin::Address, BridgeError>")]
pub struct CreateFederationAddressMessage {
    pub address_type: FederationAddressType,
}

/// Types of federation addresses
#[derive(Debug, Clone)]
pub enum FederationAddressType {
    Standard,
    Emergency,
    Temporary { expires_at: std::time::SystemTime },
}

/// Message to handle Bitcoin reorg
#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct BitcoinReorgMessage {
    pub old_chain: Vec<bitcoin::BlockHash>,
    pub new_chain: Vec<bitcoin::BlockHash>,
    pub affected_transactions: Vec<bitcoin::Txid>,
}

/// Message to pause/resume bridge operations
#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct SetBridgeStateMessage {
    pub new_state: BridgeState,
    pub reason: String,
}

/// Bridge operational state
#[derive(Debug, Clone)]
pub enum BridgeState {
    Active,
    Paused { reason: String },
    Emergency { reason: String },
    Maintenance { estimated_duration: std::time::Duration },
}

/// Message to handle governance proposals affecting the bridge
#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct GovernanceProposalMessage {
    pub proposal: GovernanceProposal,
}

/// Governance proposal types
#[derive(Debug, Clone)]
pub enum GovernanceProposal {
    UpdateFederation { new_members: Vec<FederationMember> },
    UpdateThreshold { new_threshold: usize },
    UpdateFees { new_fee_structure: FeeStructure },
    EmergencyPause { duration: std::time::Duration },
}

/// Fee structure for bridge operations
#[derive(Debug, Clone)]
pub struct FeeStructure {
    pub pegin_fee_basis_points: u16,
    pub pegout_fee_basis_points: u16,
    pub min_fee_satoshis: u64,
    pub max_fee_satoshis: u64,
}