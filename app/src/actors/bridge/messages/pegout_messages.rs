//! Peg-Out Actor Messages
//! 
//! Messages for Bitcoin withdrawal processing and signature coordination

use actix::prelude::*;
use bitcoin::{Transaction, Txid, Address as BtcAddress};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use crate::types::*;

// Forward declaration for circular dependency handling  
pub struct PegOutActor;

/// Peg-out workflow messages
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "Result<PegOutResponse, BridgeError>")]
pub enum PegOutMessage {
    /// Process burn event from Alys chain
    ProcessBurnEvent {
        burn_tx: H256,
        destination: BtcAddress,
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
}

/// Burn event details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnEvent {
    pub burn_tx_hash: H256,
    pub block_number: u64,
    pub log_index: u32,
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
    pub signatures: Vec<FederationSignature>,
    pub aggregated_signature: Option<Vec<u8>>,
    pub valid: bool,
}

/// Individual federation member signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationSignature {
    pub member_id: String,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
    pub valid: bool,
}

/// Transaction building context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionBuildContext {
    pub destination: BtcAddress,
    pub amount: u64,
    pub fee_rate: u64,
    pub selected_utxos: Vec<UtxoSelection>,
    pub change_address: Option<BtcAddress>,
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