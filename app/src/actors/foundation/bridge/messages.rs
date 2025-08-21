use actix::prelude::*;
use bitcoin::{Transaction, Txid, Address as BtcAddress, Script};
use ethereum_types::{H256, H160};
use serde::{Serialize, Deserialize};

use super::errors::BridgeError;

#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct ProcessPegin {
    pub tx: Transaction,
    pub confirmations: u32,
    pub deposit_address: BtcAddress,
}

#[derive(Message)]
#[rtype(result = "Result<PegoutResult, BridgeError>")]
pub struct ProcessPegout {
    pub burn_event: BurnEvent,
    pub request_id: String,
}

#[derive(Message)]
#[rtype(result = "Result<Vec<PendingPegin>, BridgeError>")]
pub struct GetPendingPegins;

#[derive(Message)]
#[rtype(result = "Result<Vec<PendingPegout>, BridgeError>")]
pub struct GetPendingPegouts;

#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct ApplySignatures {
    pub request_id: String,
    pub witnesses: Vec<WitnessData>,
}

#[derive(Message)]
#[rtype(result = "Result<OperationStatus, BridgeError>")]
pub struct GetOperationStatus {
    pub operation_id: String,
}

#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct UpdateFederationAddress {
    pub version: u32,
    pub address: BtcAddress,
    pub script_pubkey: Script,
}

#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct RetryFailedOperations;

#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct RefreshUtxos;

#[derive(Message)]
#[rtype(result = "Result<BridgeStats, BridgeError>")]
pub struct GetBridgeStats;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnEvent {
    pub tx_hash: H256,
    pub block_number: u64,
    pub amount: u64,
    pub destination: String,  // Bitcoin address
    pub sender: H160,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPegin {
    pub txid: Txid,
    pub amount: u64,
    pub evm_address: H160,
    pub confirmations: u32,
    pub index: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPegout {
    pub request_id: String,
    pub amount: u64,
    pub destination: BtcAddress,
    pub burn_tx_hash: H256,
    pub state: PegoutState,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PegoutState {
    Pending,
    BuildingTransaction,
    SignatureRequested,
    SignaturesReceived { count: usize },
    Broadcasting,
    Broadcast { txid: Txid },
    Confirmed { confirmations: u32 },
    Failed { reason: String, retry_count: u32 },
}

#[derive(Debug, Clone)]
pub enum PegoutResult {
    Pending(String),  // Request ID
    InProgress(PegoutState),
    Completed(Txid),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct WitnessData {
    pub input_index: usize,
    pub witness: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationStatus {
    pub operation_id: String,
    pub operation_type: OperationType,
    pub state: OperationState,
    pub created_at: u64,
    pub updated_at: u64,
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationType {
    Pegin,
    Pegout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationState {
    Pending,
    InProgress,
    Completed,
    Failed { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeStats {
    pub total_pegins_processed: u64,
    pub total_pegouts_processed: u64,
    pub total_pegin_volume: u64,
    pub total_pegout_volume: u64,
    pub pending_pegins: usize,
    pub pending_pegouts: usize,
    pub failed_operations: usize,
    pub average_processing_time_ms: f64,
    pub success_rate: f64,
}

#[derive(Debug, Clone)]
pub struct SignatureRequest {
    pub request_id: String,
    pub tx_hex: String,
    pub input_indices: Vec<usize>,
    pub amounts: Vec<u64>,
}

// Messages for governance integration
#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct NotifyPegin {
    pub txid: Txid,
    pub amount: u64,
    pub evm_address: H160,
}

#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct RequestSignatures(pub SignatureRequest);

// Health and monitoring messages
#[derive(Message)]
#[rtype(result = "Result<BridgeHealth, BridgeError>")]
pub struct GetBridgeHealth;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeHealth {
    pub is_healthy: bool,
    pub last_utxo_refresh: u64,
    pub bitcoin_connection: bool,
    pub governance_connection: bool,
    pub pending_operations_count: usize,
    pub failed_operations_count: usize,
    pub uptime_seconds: u64,
}