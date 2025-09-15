//! Stream Actor Messages  
//! 
//! Messages for governance communication and bridge-specific streaming

use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, Duration};
use crate::types::*;
use super::pegout_messages::{SignatureSet, PegOutActor};

// Import actor_system message traits
use actor_system::message::{AlysMessage, MessagePriority};

// Import the actual actor instead of forward declaration
pub use super::super::actors::stream::actor::StreamActor;

/// Stream actor status for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamActorStatus {
    pub connected_nodes: Vec<String>,
    pub active_connections: usize,
    pub last_heartbeat: Option<SystemTime>,
    pub status: String,
}

/// Stream actor messages (enhanced for bridge integration)
#[derive(Debug, Clone, Message)]
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
    
    /// Register peg-out actor for direct communication (not serializable)
    RegisterPegOutActor(Addr<PegOutActor>),
    
    /// Reconnect to governance nodes
    ReconnectToGovernance,
    
    /// Update governance endpoints
    UpdateGovernanceEndpoints {
        endpoints: Vec<String>,
    },
    
    /// Initialize the stream actor
    Initialize,
    
    /// Get actor status
    GetStatus,
    
    /// Shutdown the actor
    Shutdown,
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
    Initialized,
    StatusReported(StreamActorStatus),
    Shutdown,
}

/// Peg-out signature request to governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegOutSignatureRequest {
    pub request_id: String,
    pub pegout_id: String,
    pub unsigned_transaction: bitcoin::Transaction,
    pub destination_address: String, // Bitcoin address as string for serde compatibility
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
    pub signatures: Vec<StreamFederationSignature>,
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
pub struct StreamFederationSignature {
    pub member_id: String,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
    pub timestamp: SystemTime,
}

impl AlysMessage for StreamMessage {
    fn priority(&self) -> MessagePriority {
        match self {
            // Critical governance operations - highest priority
            StreamMessage::RequestPegOutSignatures { .. } => MessagePriority::Critical,
            StreamMessage::ReceiveSignatureResponse { .. } => MessagePriority::Critical,
            
            // High priority bridge operations
            StreamMessage::HandleFederationUpdate { .. } => MessagePriority::High,
            StreamMessage::NotifyPegIn { .. } => MessagePriority::High,
            StreamMessage::RegisterPegOutActor(_) => MessagePriority::High,
            
            // Medium priority connection management
            StreamMessage::EstablishGovernanceConnection { .. } => MessagePriority::Normal,
            StreamMessage::ReconnectToGovernance => MessagePriority::Normal,
            StreamMessage::UpdateGovernanceEndpoints { .. } => MessagePriority::Normal,
            
            // Low priority monitoring and status
            StreamMessage::SendHeartbeat => MessagePriority::Low,
            StreamMessage::GetConnectionStatus => MessagePriority::Low,

            // Lifecycle management
            StreamMessage::Initialize => MessagePriority::High,
            StreamMessage::GetStatus => MessagePriority::Low,
            StreamMessage::Shutdown => MessagePriority::High,
        }
    }

    fn timeout(&self) -> Duration {
        match self {
            // Signature operations have extended timeouts due to consensus requirements
            StreamMessage::RequestPegOutSignatures { request } => {
                request.timeout
            }
            StreamMessage::ReceiveSignatureResponse { .. } => Duration::from_secs(30),
            
            // Federation updates need time for propagation
            StreamMessage::HandleFederationUpdate { .. } => Duration::from_secs(120),
            
            // Connection operations need reasonable timeouts
            StreamMessage::EstablishGovernanceConnection { .. } => Duration::from_secs(60),
            StreamMessage::ReconnectToGovernance => Duration::from_secs(45),
            StreamMessage::UpdateGovernanceEndpoints { .. } => Duration::from_secs(30),
            
            // Notifications and registration should be fast
            StreamMessage::NotifyPegIn { .. } => Duration::from_secs(30),
            StreamMessage::RegisterPegOutActor(_) => Duration::from_secs(15),
            
            // Quick operations
            StreamMessage::SendHeartbeat => Duration::from_secs(10),
            StreamMessage::GetConnectionStatus => Duration::from_secs(5),

            // Lifecycle operations
            StreamMessage::Initialize => Duration::from_secs(30),
            StreamMessage::GetStatus => Duration::from_secs(5),
            StreamMessage::Shutdown => Duration::from_secs(15),
        }
    }

    fn is_retryable(&self) -> bool {
        match self {
            // Signature operations are retryable but with limits
            StreamMessage::RequestPegOutSignatures { .. } => true,
            StreamMessage::ReceiveSignatureResponse { .. } => false, // Don't retry responses
            
            // Federation and connection operations are retryable
            StreamMessage::HandleFederationUpdate { .. } => true,
            StreamMessage::EstablishGovernanceConnection { .. } => true,
            StreamMessage::ReconnectToGovernance => true,
            StreamMessage::UpdateGovernanceEndpoints { .. } => true,
            
            // Notifications should be retried to ensure delivery
            StreamMessage::NotifyPegIn { .. } => true,
            
            // Registration and status operations are retryable
            StreamMessage::RegisterPegOutActor(_) => true,
            StreamMessage::SendHeartbeat => true,
            StreamMessage::GetConnectionStatus => true,

            // Lifecycle operations
            StreamMessage::Initialize => true,
            StreamMessage::GetStatus => true,
            StreamMessage::Shutdown => false, // Don't retry shutdown
        }
    }

    fn max_retries(&self) -> u32 {
        match self {
            // Critical operations get more retries
            StreamMessage::RequestPegOutSignatures { .. } => 5,
            StreamMessage::HandleFederationUpdate { .. } => 5,
            StreamMessage::NotifyPegIn { .. } => 5,
            
            // Connection operations get moderate retries
            StreamMessage::EstablishGovernanceConnection { .. } => 3,
            StreamMessage::ReconnectToGovernance => 3,
            StreamMessage::UpdateGovernanceEndpoints { .. } => 3,
            
            // Registration and heartbeat get fewer retries
            StreamMessage::RegisterPegOutActor(_) => 2,
            StreamMessage::SendHeartbeat => 2,
            
            // Status checks and responses get minimal retries
            StreamMessage::GetConnectionStatus => 1,
            StreamMessage::ReceiveSignatureResponse { .. } => 0, // No retries for responses

            // Lifecycle operations
            StreamMessage::Initialize => 3,
            StreamMessage::GetStatus => 1,
            StreamMessage::Shutdown => 0, // No retries for shutdown
        }
    }

    fn serialize_debug(&self) -> serde_json::Value {
        serde_json::json!({
            "type": self.message_type(),
            "priority": self.priority(),
            "timeout_secs": self.timeout().as_secs(),
            "retryable": self.is_retryable(),
            "max_retries": self.max_retries(),
            "message_data": match self {
                StreamMessage::EstablishGovernanceConnection { endpoints } => serde_json::json!({
                    "endpoint_count": endpoints.len(),
                    "endpoints": endpoints
                }),
                StreamMessage::RequestPegOutSignatures { request } => serde_json::json!({
                    "request_id": request.request_id,
                    "pegout_id": request.pegout_id,
                    "amount": request.amount,
                    "destination": request.destination_address.to_string()
                }),
                StreamMessage::ReceiveSignatureResponse { response } => serde_json::json!({
                    "request_id": response.request_id,
                    "pegout_id": response.pegout_id,
                    "approval_status": format!("{:?}", response.approval_status),
                    "responding_nodes": response.responding_nodes.len()
                }),
                StreamMessage::HandleFederationUpdate { update } => serde_json::json!({
                    "update_id": update.update_id,
                    "update_type": format!("{:?}", update.update_type),
                    "effective_height": update.effective_height
                }),
                StreamMessage::NotifyPegIn { notification } => serde_json::json!({
                    "pegin_id": notification.pegin_id,
                    "bitcoin_txid": notification.bitcoin_txid.to_string(),
                    "amount": notification.amount
                }),
                StreamMessage::UpdateGovernanceEndpoints { endpoints } => serde_json::json!({
                    "endpoint_count": endpoints.len()
                }),
                _ => serde_json::json!({ "details": "Basic message" })
            }
        })
    }
}

impl StreamMessage {
    /// Get the message type as a string for debugging and metrics
    pub fn message_type(&self) -> &'static str {
        match self {
            StreamMessage::EstablishGovernanceConnection { .. } => "EstablishGovernanceConnection",
            StreamMessage::RequestPegOutSignatures { .. } => "RequestPegOutSignatures",
            StreamMessage::ReceiveSignatureResponse { .. } => "ReceiveSignatureResponse",
            StreamMessage::HandleFederationUpdate { .. } => "HandleFederationUpdate",
            StreamMessage::NotifyPegIn { .. } => "NotifyPegIn",
            StreamMessage::SendHeartbeat => "SendHeartbeat",
            StreamMessage::GetConnectionStatus => "GetConnectionStatus",
            StreamMessage::RegisterPegOutActor(_) => "RegisterPegOutActor",
            StreamMessage::ReconnectToGovernance => "ReconnectToGovernance",
            StreamMessage::UpdateGovernanceEndpoints { .. } => "UpdateGovernanceEndpoints",
            StreamMessage::Initialize => "Initialize",
            StreamMessage::GetStatus => "GetStatus",
            StreamMessage::Shutdown => "Shutdown",
        }
    }

    /// Check if this message requires active governance connections
    pub fn requires_governance_connection(&self) -> bool {
        match self {
            StreamMessage::RequestPegOutSignatures { .. } |
            StreamMessage::HandleFederationUpdate { .. } |
            StreamMessage::NotifyPegIn { .. } |
            StreamMessage::SendHeartbeat => true,
            
            StreamMessage::ReceiveSignatureResponse { .. } |
            StreamMessage::EstablishGovernanceConnection { .. } |
            StreamMessage::ReconnectToGovernance |
            StreamMessage::UpdateGovernanceEndpoints { .. } |
            StreamMessage::GetConnectionStatus |
            StreamMessage::RegisterPegOutActor(_) |
            StreamMessage::Initialize |
            StreamMessage::GetStatus |
            StreamMessage::Shutdown => false,
        }
    }

    /// Get the category of this message for routing and handling
    pub fn category(&self) -> StreamMessageCategory {
        match self {
            StreamMessage::RequestPegOutSignatures { .. } |
            StreamMessage::ReceiveSignatureResponse { .. } => StreamMessageCategory::Signatures,
            
            StreamMessage::HandleFederationUpdate { .. } => StreamMessageCategory::Federation,
            
            StreamMessage::NotifyPegIn { .. } => StreamMessageCategory::Notifications,
            
            StreamMessage::EstablishGovernanceConnection { .. } |
            StreamMessage::ReconnectToGovernance |
            StreamMessage::UpdateGovernanceEndpoints { .. } => StreamMessageCategory::ConnectionManagement,
            
            StreamMessage::SendHeartbeat |
            StreamMessage::GetConnectionStatus |
            StreamMessage::GetStatus => StreamMessageCategory::Monitoring,

            StreamMessage::RegisterPegOutActor(_) => StreamMessageCategory::Registration,

            StreamMessage::Initialize |
            StreamMessage::Shutdown => StreamMessageCategory::Lifecycle,
        }
    }
}

/// Categories of stream messages for routing and processing
#[derive(Debug, Clone, PartialEq)]
pub enum StreamMessageCategory {
    Signatures,
    Federation,
    Notifications,
    ConnectionManagement,
    Monitoring,
    Registration,
    Lifecycle,
}