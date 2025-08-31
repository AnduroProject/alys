//! Stream Actor Messages  
//! 
//! Messages for governance communication and bridge-specific streaming

use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use crate::types::*;
use super::pegout_messages::{SignatureSet, PegOutActor};

// Forward declaration for circular dependency handling
pub struct StreamActor;

/// Stream actor messages (enhanced for bridge integration)
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "Result<StreamResponse, BridgeError>")]
pub enum StreamMessage {
    /// Establish governance connection
    EstablishGovernanceConnection {
        endpoints: Vec<String>,
    },
    
    /// Request peg-out signatures from governance
    RequestPegOutSignatures {
        request: PegOutSignatureRequest,
    },
    
    /// Handle signature response from governance
    ReceiveSignatureResponse {
        response: SignatureResponse,
    },
    
    /// Handle federation configuration updates
    HandleFederationUpdate {
        update: FederationUpdate,
    },
    
    /// Notify governance of peg-in completion
    NotifyPegIn {
        notification: PegInNotification,
    },
    
    /// Send heartbeat to governance nodes
    SendHeartbeat,
    
    /// Get connection status
    GetConnectionStatus,
    
    /// Register peg-out actor for direct communication
    RegisterPegOutActor(Addr<PegOutActor>),
    
    /// Reconnect to governance nodes
    ReconnectToGovernance,
    
    /// Update governance endpoints
    UpdateGovernanceEndpoints {
        endpoints: Vec<String>,
    },
}

/// Stream response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamResponse {
    ConnectionEstablished { connected_nodes: Vec<String> },
    SignatureRequestSent { request_id: String },
    SignatureResponseReceived { request_id: String },
    FederationUpdateHandled,
    PegInNotificationSent,
    HeartbeatSent,
    ConnectionStatus(GovernanceConnectionStatus),
    PegOutActorRegistered,
    ReconnectionInitiated,
    EndpointsUpdated { count: usize },
}

/// Peg-out signature request to governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegOutSignatureRequest {
    pub request_id: String,
    pub pegout_id: String,
    pub unsigned_transaction: bitcoin::Transaction,
    pub destination_address: bitcoin::Address,
    pub amount: u64,
    pub fee: u64,
    pub utxo_commitments: Vec<UtxoCommitment>,
    pub requester: H160,
    pub requested_at: SystemTime,
    pub timeout: std::time::Duration,
}

/// Signature response from governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureResponse {
    pub request_id: String,
    pub pegout_id: String,
    pub signatures: SignatureSet,
    pub approval_status: ApprovalStatus,
    pub responding_nodes: Vec<String>,
    pub response_time: SystemTime,
}

/// Governance approval status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalStatus {
    Approved,
    Rejected { reason: String },
    PartialApproval { threshold_met: bool },
    Timeout,
}

/// Federation configuration update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationUpdate {
    pub update_id: String,
    pub update_type: FederationUpdateType,
    pub new_config: FederationConfig,
    pub effective_height: u64,
    pub signatures: Vec<FederationSignature>,
    pub timestamp: SystemTime,
}

/// Types of federation updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FederationUpdateType {
    MemberAddition,
    MemberRemoval,
    ThresholdChange,
    KeyRotation,
    AddressUpdate,
}

/// Peg-in completion notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegInNotification {
    pub pegin_id: String,
    pub bitcoin_txid: bitcoin::Txid,
    pub alys_tx_hash: H256,
    pub amount: u64,
    pub recipient: H160,
    pub completed_at: SystemTime,
    pub confirmations: u32,
}

/// Governance connection status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConnectionStatus {
    pub connected_nodes: Vec<GovernanceNodeStatus>,
    pub total_connections: usize,
    pub healthy_connections: usize,
    pub last_heartbeat: Option<SystemTime>,
    pub connection_quality: ConnectionQuality,
}

/// Individual governance node status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceNodeStatus {
    pub node_id: String,
    pub endpoint: String,
    pub status: NodeConnectionStatus,
    pub last_activity: SystemTime,
    pub message_count: u64,
    pub latency: Option<std::time::Duration>,
}

/// Node connection status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeConnectionStatus {
    Connected,
    Connecting,
    Disconnected,
    Failed { error: String },
    Timeout,
}

/// Overall connection quality assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionQuality {
    Excellent,
    Good,
    Degraded,
    Poor,
    Failed,
}

/// UTXO commitment for signature request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoCommitment {
    pub outpoint: bitcoin::OutPoint,
    pub amount: u64,
    pub script_pubkey: bitcoin::ScriptBuf,
    pub commitment_proof: Vec<u8>,
}

/// Federation signature for updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationSignature {
    pub member_id: String,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
    pub timestamp: SystemTime,
}