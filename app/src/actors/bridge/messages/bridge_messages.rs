//! Bridge Coordinator Messages
//!
//! Messages for bridge actor coordination and system management

use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};
use crate::types::errors::BridgeError as TypesBridgeError;
use crate::types::H256;
use super::pegin_messages::PegInActor;
use super::pegout_messages::PegOutActor;
use super::stream_messages::StreamActor;

// Import actor_system message traits
use actor_system::message::{AlysMessage, MessagePriority};

// Default functions for serde skip
fn default_pegin_addr() -> Option<Addr<PegInActor>> { None }
fn default_pegout_addr() -> Option<Addr<PegOutActor>> { None }
fn default_stream_addr() -> Option<Addr<StreamActor>> { None }


/// Bridge coordination messages
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "Result<(), TypesBridgeError>")]
pub enum BridgeCoordinationMessage {
    /// Initialize the bridge system
    InitializeSystem,

    /// Register specialized actors using string identifiers
    RegisterPegInActor {
        actor_id: String,
        /// Non-serializable actor address for internal use
        #[serde(skip, default = "default_pegin_addr")]
        addr: Option<Addr<PegInActor>>,
    },
    RegisterPegOutActor {
        actor_id: String,
        /// Non-serializable actor address for internal use
        #[serde(skip, default = "default_pegout_addr")]
        addr: Option<Addr<PegOutActor>>,
    },
    RegisterStreamActor {
        actor_id: String,
        /// Non-serializable actor address for internal use
        #[serde(skip, default = "default_stream_addr")]
        addr: Option<Addr<StreamActor>>,
    },
    
    /// System status and health
    GetSystemStatus,
    GetSystemMetrics,
    
    /// Operation coordination
    CoordinatePegIn {
        pegin_id: String,
        bitcoin_txid: bitcoin::Txid,
    },
    
    CoordinatePegOut {
        pegout_id: String, 
        burn_tx_hash: H256,
    },
    
    /// Error handling and recovery
    HandleActorFailure {
        actor_type: ActorType,
        error: TypesBridgeError,
    },
    
    /// Graceful shutdown
    ShutdownSystem,

    /// Operation completion notifications
    PegInCompleted {
        pegin_id: String,
        bitcoin_txid: bitcoin::Txid,
        recipient: ethereum_types::Address,
        amount: u64,
    },

    PegOutCompleted {
        pegout_id: String,
        burn_tx_hash: H256,
        bitcoin_destination: String, // Bitcoin address as string for serialization
        amount: u64,
    },
}

impl AlysMessage for BridgeCoordinationMessage {
    fn priority(&self) -> MessagePriority {
        match self {
            BridgeCoordinationMessage::ShutdownSystem => MessagePriority::Critical,
            BridgeCoordinationMessage::HandleActorFailure { .. } => MessagePriority::Critical,
            BridgeCoordinationMessage::InitializeSystem => MessagePriority::High,
            BridgeCoordinationMessage::CoordinatePegIn { .. } => MessagePriority::High,
            BridgeCoordinationMessage::CoordinatePegOut { .. } => MessagePriority::High,
            BridgeCoordinationMessage::RegisterPegInActor { .. } => MessagePriority::High,
            BridgeCoordinationMessage::RegisterPegOutActor { .. } => MessagePriority::High,
            BridgeCoordinationMessage::RegisterStreamActor { .. } => MessagePriority::High,
            BridgeCoordinationMessage::PegInCompleted { .. } => MessagePriority::Normal,
            BridgeCoordinationMessage::PegOutCompleted { .. } => MessagePriority::Normal,
            BridgeCoordinationMessage::GetSystemStatus => MessagePriority::Low,
            BridgeCoordinationMessage::GetSystemMetrics => MessagePriority::Low,
        }
    }

    fn timeout(&self) -> Duration {
        match self {
            BridgeCoordinationMessage::ShutdownSystem => Duration::from_secs(60),
            BridgeCoordinationMessage::InitializeSystem => Duration::from_secs(120),
            BridgeCoordinationMessage::CoordinatePegIn { .. } => Duration::from_secs(300), // 5 minutes for peg-in
            BridgeCoordinationMessage::CoordinatePegOut { .. } => Duration::from_secs(600), // 10 minutes for peg-out
            BridgeCoordinationMessage::HandleActorFailure { .. } => Duration::from_secs(30),
            BridgeCoordinationMessage::RegisterPegInActor { .. } => Duration::from_secs(30),
            BridgeCoordinationMessage::RegisterPegOutActor { .. } => Duration::from_secs(30),
            BridgeCoordinationMessage::RegisterStreamActor { .. } => Duration::from_secs(30),
            BridgeCoordinationMessage::PegInCompleted { .. } => Duration::from_secs(10),
            BridgeCoordinationMessage::PegOutCompleted { .. } => Duration::from_secs(10),
            BridgeCoordinationMessage::GetSystemStatus => Duration::from_secs(5),
            BridgeCoordinationMessage::GetSystemMetrics => Duration::from_secs(5),
        }
    }

    fn is_retryable(&self) -> bool {
        match self {
            BridgeCoordinationMessage::ShutdownSystem => false,
            BridgeCoordinationMessage::InitializeSystem => false,
            BridgeCoordinationMessage::CoordinatePegIn { .. } => true,
            BridgeCoordinationMessage::CoordinatePegOut { .. } => true,
            BridgeCoordinationMessage::HandleActorFailure { .. } => true,
            BridgeCoordinationMessage::RegisterPegInActor { .. } => true,
            BridgeCoordinationMessage::RegisterPegOutActor { .. } => true,
            BridgeCoordinationMessage::RegisterStreamActor { .. } => true,
            BridgeCoordinationMessage::PegInCompleted { .. } => false, // Already completed
            BridgeCoordinationMessage::PegOutCompleted { .. } => false, // Already completed
            BridgeCoordinationMessage::GetSystemStatus => true,
            BridgeCoordinationMessage::GetSystemMetrics => true,
        }
    }

    fn max_retries(&self) -> u32 {
        match self {
            BridgeCoordinationMessage::CoordinatePegIn { .. } => 5,
            BridgeCoordinationMessage::CoordinatePegOut { .. } => 5,
            BridgeCoordinationMessage::HandleActorFailure { .. } => 3,
            BridgeCoordinationMessage::RegisterPegInActor { .. } => 3,
            BridgeCoordinationMessage::RegisterPegOutActor { .. } => 3,
            BridgeCoordinationMessage::RegisterStreamActor { .. } => 3,
            BridgeCoordinationMessage::GetSystemStatus => 2,
            BridgeCoordinationMessage::GetSystemMetrics => 2,
            _ => 1, // Non-retryable messages or single retry
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
                BridgeCoordinationMessage::CoordinatePegIn { pegin_id, bitcoin_txid } => serde_json::json!({
                    "pegin_id": pegin_id,
                    "bitcoin_txid": bitcoin_txid.to_string()
                }),
                BridgeCoordinationMessage::CoordinatePegOut { pegout_id, burn_tx_hash } => serde_json::json!({
                    "pegout_id": pegout_id,
                    "burn_tx_hash": format!("{:?}", burn_tx_hash)
                }),
                BridgeCoordinationMessage::HandleActorFailure { actor_type, error } => serde_json::json!({
                    "actor_type": format!("{:?}", actor_type),
                    "error": error.to_string()
                }),
                BridgeCoordinationMessage::PegInCompleted { pegin_id, bitcoin_txid, recipient, amount } => serde_json::json!({
                    "pegin_id": pegin_id,
                    "bitcoin_txid": bitcoin_txid.to_string(),
                    "recipient": format!("{:?}", recipient),
                    "amount": amount
                }),
                BridgeCoordinationMessage::PegOutCompleted { pegout_id, burn_tx_hash, bitcoin_destination, amount } => serde_json::json!({
                    "pegout_id": pegout_id,
                    "burn_tx_hash": format!("{:?}", burn_tx_hash),
                    "bitcoin_destination": bitcoin_destination.to_string(),
                    "amount": amount
                }),
                BridgeCoordinationMessage::RegisterPegInActor { actor_id, .. } => serde_json::json!({
                    "details": "RegisterPegInActor",
                    "actor_id": actor_id
                }),
                BridgeCoordinationMessage::RegisterPegOutActor { actor_id, .. } => serde_json::json!({
                    "details": "RegisterPegOutActor",
                    "actor_id": actor_id
                }),
                BridgeCoordinationMessage::RegisterStreamActor { actor_id, .. } => serde_json::json!({
                    "details": "RegisterStreamActor",
                    "actor_id": actor_id
                }),
                _ => serde_json::json!({ "details": "Basic message" })
            }
        })
    }
}

/// System status response
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<BridgeSystemStatus, TypesBridgeError>")]
pub struct GetSystemStatusResponse;

/// Bridge system status
#[derive(Debug, Clone)]
pub struct BridgeSystemStatus {
    pub status: SystemHealthStatus,
    pub active_operations: u32,
    pub registered_actors: ActorStatusRegistry,
    pub last_activity: SystemTime,
    pub uptime: std::time::Duration,
}

/// System health status
#[derive(Debug, Clone)]
pub enum SystemHealthStatus {
    Healthy,
    Degraded { issues: Vec<String> },
    Critical { errors: Vec<String> },
    Initializing,
    Shutdown,
}

/// Actor registry tracking (status)
#[derive(Debug, Clone)]
pub struct ActorStatusRegistry {
    pub pegin_actor: Option<ActorInfo>,
    pub pegout_actor: Option<ActorInfo>,
    pub stream_actor: Option<ActorInfo>,
}

/// Actor information
#[derive(Debug, Clone)]
pub struct ActorInfo {
    pub actor_type: ActorType,
    pub status: ActorStatus,
    pub registered_at: SystemTime,
    pub last_heartbeat: SystemTime,
    pub message_count: u64,
}

/// Actor type enumeration
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum ActorType {
    Bridge,
    PegIn,
    PegOut,
    Stream,
}

/// Actor status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActorStatus {
    Starting,
    Running,
    Degraded,
    Stopped,
    Failed,
}

/// Operation status tracking
#[derive(Debug, Clone)]
pub struct OperationStatus {
    pub operation_id: String,
    pub operation_type: OperationType,
    pub status: OperationState,
    pub created_at: SystemTime,
    pub last_updated: SystemTime,
    pub progress: Option<f32>,
}

/// Operation types
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum OperationType {
    PegIn,
    PegOut,
}

/// Operation states
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum OperationState {
    Initiated,
    Processing,
    WaitingForConfirmations,
    WaitingForSignatures,
    Broadcasting,
    Completed,
    Failed { reason: String },
}