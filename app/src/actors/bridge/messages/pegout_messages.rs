//! Peg-Out Actor Messages
//! 
//! Messages for Bitcoin withdrawal processing and signature coordination

use actix::prelude::*;
use bitcoin::{Transaction, Txid, Address as BtcAddress, Witness, Network};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json;
use std::time::SystemTime;
use crate::types::*;

// Custom serde implementations for BtcAddress to handle NetworkChecked
mod btc_address_serde {
    use super::*;
    use bitcoin::address::NetworkUnchecked;

    pub fn serialize<S>(address: &BtcAddress, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        address.to_string().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BtcAddress, D::Error>
    where
        D: Deserializer<'de>,
    {
        let address_str = String::deserialize(deserializer)?;
        let unchecked: bitcoin::Address<NetworkUnchecked> = address_str
            .parse()
            .map_err(serde::de::Error::custom)?;

        // For now, assume Bitcoin mainnet. In production, this should be configurable
        unchecked
            .require_network(Network::Bitcoin)
            .map_err(serde::de::Error::custom)
    }
}

// Import the actual actor instead of forward declaration
pub use super::super::actors::pegout::actor::{PegOutActor, PegOutActorStatus};

/// UTXO selected for spending in peg-out transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedUtxo {
    pub txid: Txid,
    pub vout: u32,
    pub value: u64, // satoshis
    pub script_pubkey: bitcoin::ScriptBuf,
}

/// Peg-out workflow messages
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "Result<PegOutResponse, BridgeError>")]
pub enum PegOutMessage {
    /// Process burn event from Alys chain
    ProcessBurnEvent {
        burn_tx: H256,
        destination: String, // Bitcoin address as string for serde compatibility
        amount: u64,
        requester: H160,
    },
    
    /// Validate burn event
    ValidateBurnEvent {
        pegout_id: String,
        burn_event: BurnEvent,
    },
    
    /// Build unsigned withdrawal transaction
    BuildWithdrawal {
        pegout_id: String,
    },
    
    /// Request signatures from governance
    RequestSignatures {
        pegout_id: String,
        unsigned_tx: Transaction,
    },
    
    /// Apply collected signatures
    ApplySignatures {
        pegout_id: String,
        witnesses: Vec<Witness>,
        signature_set: SignatureSet,
    },
    
    /// Broadcast completed transaction
    BroadcastTransaction {
        pegout_id: String,
        signed_tx: Transaction,
    },
    
    /// Get peg-out status
    GetPegOutStatus {
        pegout_id: String,
    },
    
    /// List pending peg-outs
    ListPendingPegOuts,
    
    /// Force retry failed peg-out
    RetryPegOut {
        pegout_id: String,
    },
    
    /// Cancel peg-out processing
    CancelPegOut {
        pegout_id: String,
        reason: String,
    },
    
    /// Update transaction confirmations
    UpdateConfirmations {
        pegout_id: String,
        txid: Txid,
        confirmations: u32,
    },
    
    /// Process withdrawal request
    ProcessWithdrawal {
        pegout_id: String,
        amount: u64,
        destination: String, // Bitcoin address as string for serde compatibility
    },
    
    /// Select UTXOs for transaction
    SelectUtxos {
        pegout_id: String,
        required_amount: u64,
    },
    
    /// Build transaction
    BuildTransaction {
        pegout_id: String,
        utxos: Vec<SelectedUtxo>,
    },
    
    /// Collect signatures
    CollectSignatures {
        pegout_id: String,
        unsigned_tx: Transaction,
    },
    
    /// Monitor confirmations
    MonitorConfirmations {
        pegout_id: String,
        txid: Txid,
    },
    
    /// Process a generic request
    ProcessRequest {
        request_id: String,
        request_data: serde_json::Value,
    },

    /// Create Bitcoin transaction
    CreateBitcoinTransaction {
        pegout_id: String,
        inputs: Vec<bitcoin::OutPoint>,
        outputs: Vec<(String, u64)>, // address, amount pairs
    },

    /// Sign transaction
    SignTransaction {
        pegout_id: String,
        transaction: Transaction,
    },

    /// Cancel request
    CancelRequest {
        request_id: String,
        reason: String,
    },

    /// Handle timeout
    HandleTimeout {
        request_id: String,
    },

    /// Initialize the peg-out actor
    Initialize,

    /// Get actor status
    GetStatus,

    /// Get metrics
    GetMetrics,

    /// Shutdown the actor
    Shutdown,
}

/// Peg-out response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PegOutResponse {
    BurnEventProcessed { pegout_id: String },
    BurnEventValidated { pegout_id: String, valid: bool },
    WithdrawalBuilt { pegout_id: String, unsigned_tx: Transaction },
    SignaturesRequested { pegout_id: String, request_id: String },
    SignaturesApplied { pegout_id: String, ready_to_broadcast: bool },
    TransactionBroadcast { pegout_id: String, txid: Txid },
    PegOutStatus(PegOutStatus),
    PendingPegOuts(Vec<PendingPegOut>),
    PegOutRetried { pegout_id: String },
    PegOutCancelled { pegout_id: String },
    ConfirmationsUpdated { pegout_id: String, confirmations: u32 },
    WithdrawalProcessed { pegout_id: String },
    UtxosSelected { pegout_id: String, utxos: Vec<SelectedUtxo> },
    TransactionBuilt { pegout_id: String, unsigned_tx: Transaction },
    SignaturesCollected { pegout_id: String, signatures: SignatureSet },
    ConfirmationsMonitored { pegout_id: String, confirmations: u32 },
    Initialized,
    StatusReported(PegOutActorStatus),
    Shutdown,
}

/// Burn event details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnEvent {
    pub burn_tx_hash: H256,
    pub block_number: u64,
    pub log_index: u32,
    #[serde(with = "btc_address_serde")]
    pub destination_address: BtcAddress,
    pub amount: u64,
    pub requester: H160,
    pub detected_at: SystemTime,
}

/// Pending peg-out state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPegOut {
    pub pegout_id: String,
    pub burn_tx_hash: H256,
    #[serde(with = "btc_address_serde")]
    pub destination_address: BtcAddress,
    pub amount: u64,
    pub requester: H160,
    pub unsigned_tx: Option<Transaction>,
    pub signature_status: SignatureStatus,
    pub witnesses: Vec<Witness>,
    pub signed_tx: Option<Transaction>,
    pub broadcast_txid: Option<Txid>,
    pub status: PegOutStatus,
    pub created_at: SystemTime,
    pub last_updated: SystemTime,
    pub retry_count: u32,
}

/// Peg-out processing status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PegOutStatus {
    BurnDetected,
    ValidatingBurn,
    ValidationFailed { reason: String },
    BuildingTransaction,
    TransactionBuilt { fee: u64 },
    RequestingSignatures,
    CollectingSignatures { 
        collected: usize, 
        required: usize 
    },
    SignaturesComplete,
    Broadcasting,
    Broadcast { 
        txid: Txid,
        confirmations: u32,
    },
    Confirmed { 
        txid: Txid,
        confirmations: u32,
    },
    Completed {
        txid: Txid,
        final_confirmations: u32,
    },
    Failed { 
        reason: String, 
        recoverable: bool 
    },
    Cancelled { reason: String },
}

/// Signature collection status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureStatus {
    pub request_id: Option<String>,
    pub requested_at: Option<SystemTime>,
    pub signatures_collected: usize,
    pub signatures_required: usize,
    pub status: SignatureCollectionStatus,
}

/// Signature collection states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignatureCollectionStatus {
    NotRequested,
    Requested,
    InProgress,
    Complete,
    Failed { reason: String },
    Timeout,
}

/// Signature set from governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureSet {
    pub request_id: String,
    pub signatures: Vec<PegoutFederationSignature>,
    pub aggregated_signature: Option<Vec<u8>>,
    pub valid: bool,
}

/// Individual federation member signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegoutFederationSignature {
    pub member_id: String,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
    pub valid: bool,
}

/// Transaction building context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionBuildContext {
    pub destination: String, // Bitcoin address as string for serde compatibility
    pub amount: u64,
    pub fee_rate: u64,
    pub selected_utxos: Vec<UtxoSelection>,
    pub change_address: Option<String>, // Bitcoin address as string for serde compatibility
    pub estimated_fee: u64,
}

/// UTXO selection for transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoSelection {
    pub outpoint: bitcoin::OutPoint,
    pub txout: bitcoin::TxOut,
    pub confirmation_height: u32,
    pub selected_for_fee_estimation: bool,
}