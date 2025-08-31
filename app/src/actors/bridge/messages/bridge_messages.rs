//! Bridge Coordinator Messages
//! 
//! Messages for bridge actor coordination and system management

use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use crate::types::*;
use super::pegin_messages::PegInActor;
use super::pegout_messages::PegOutActor;
use super::stream_messages::StreamActor;

/// Bridge coordination messages
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "Result<(), BridgeError>")]
pub enum BridgeCoordinationMessage {
    /// Initialize the bridge system
    InitializeSystem,
    
    /// Register specialized actors
    RegisterPegInActor(Addr<PegInActor>),
    RegisterPegOutActor(Addr<PegOutActor>),
    RegisterStreamActor(Addr<StreamActor>),
    
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
        error: BridgeError,
    },
    
    /// Graceful shutdown
    ShutdownSystem,
}

/// System status response
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "BridgeSystemStatus")]
pub struct GetSystemStatusResponse;

/// Bridge system status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeSystemStatus {
    pub status: SystemHealthStatus,
    pub active_operations: u32,
    pub registered_actors: ActorRegistry,
    pub last_activity: SystemTime,
    pub uptime: std::time::Duration,
}

/// System health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemHealthStatus {
    Healthy,
    Degraded { issues: Vec<String> },
    Critical { errors: Vec<String> },
    Initializing,
    Shutdown,
}

/// Actor registry tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorRegistry {
    pub pegin_actor: Option<ActorInfo>,
    pub pegout_actor: Option<ActorInfo>,
    pub stream_actor: Option<ActorInfo>,
}

/// Actor information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorInfo {
    pub actor_type: ActorType,
    pub status: ActorStatus,
    pub registered_at: SystemTime,
    pub last_heartbeat: SystemTime,
    pub message_count: u64,
}

/// Actor type enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationStatus {
    pub operation_id: String,
    pub operation_type: OperationType,
    pub status: OperationState,
    pub created_at: SystemTime,
    pub last_updated: SystemTime,
    pub progress: Option<f32>,
}

/// Operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationType {
    PegIn,
    PegOut,
}

/// Operation states  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationState {
    Initiated,
    Processing,
    WaitingForConfirmations,
    WaitingForSignatures,
    Broadcasting,
    Completed,
    Failed { reason: String },
}