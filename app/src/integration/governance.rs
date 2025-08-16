//! Governance client for gRPC streaming communication with Anduro governance system
//!
//! This module provides a high-level client interface for interacting with the Anduro
//! governance system via gRPC streaming connections, handling proposals, votes, and
//! real-time governance events.

use crate::config::GovernanceConfig;
use crate::types::*;
use actor_system::{ActorError, ActorResult, AlysMessage, SerializableMessage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::StreamExt;
use tonic::{transport::Channel, Request, Response, Status, Streaming};
use uuid::Uuid;

/// Anduro Governance integration interface
#[async_trait]
pub trait GovernanceIntegration: Send + Sync {
    /// Connect to governance node
    async fn connect(&self, endpoint: String) -> Result<GovernanceConnectionHandle, SystemError>;
    
    /// Send block proposal to governance nodes
    async fn send_block_proposal(&self, block: ConsensusBlock) -> Result<(), SystemError>;
    
    /// Send attestation to governance nodes
    async fn send_attestation(&self, attestation: Attestation) -> Result<(), SystemError>;
    
    /// Send federation update
    async fn send_federation_update(&self, update: FederationUpdate) -> Result<(), SystemError>;
    
    /// Send chain status update
    async fn send_chain_status(&self, status: ChainStatus) -> Result<(), SystemError>;
    
    /// Submit proposal vote
    async fn submit_vote(&self, vote: ProposalVote) -> Result<(), SystemError>;
    
    /// Listen for governance messages
    async fn listen_for_messages(&self) -> Result<mpsc::Receiver<GovernanceMessage>, SystemError>;
    
    /// Get connected governance nodes
    async fn get_connected_nodes(&self) -> Result<Vec<GovernanceNodeInfo>, SystemError>;
    
    /// Disconnect from governance node
    async fn disconnect(&self, node_id: String) -> Result<(), SystemError>;
    
    /// Check connection health
    async fn health_check(&self, node_id: String) -> Result<GovernanceHealthStatus, SystemError>;
}

/// Handle for a governance connection
#[derive(Debug, Clone)]
pub struct GovernanceConnectionHandle {
    pub node_id: String,
    pub endpoint: String,
    pub connected_at: std::time::SystemTime,
    pub stream_sender: mpsc::Sender<GovernanceMessage>,
}

/// Governance node information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceNodeInfo {
    pub node_id: String,
    pub endpoint: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub connected_at: std::time::SystemTime,
    pub last_activity: std::time::SystemTime,
    pub message_count: u64,
    pub health_status: GovernanceHealthStatus,
}

/// Health status of governance connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GovernanceHealthStatus {
    Healthy,
    Degraded { issues: Vec<String> },
    Unhealthy { critical_issues: Vec<String> },
    Disconnected,
}

/// Generic governance message wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceMessage {
    pub message_id: String,
    pub from_node: String,
    pub timestamp: std::time::SystemTime,
    pub message_type: GovernanceMessageType,
    pub payload: GovernancePayload,
    pub signature: Option<Signature>,
}

/// Types of governance messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GovernanceMessageType {
    BlockProposal,
    Attestation,
    FederationUpdate,
    ChainStatus,
    ProposalVote,
    Heartbeat,
    NodeAnnouncement,
    ConsensusRequest,
    ConsensusResponse,
}

/// Attestation for consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    pub slot: u64,
    pub block_hash: BlockHash,
    pub attester: Address,
    pub signature: Signature,
    pub timestamp: std::time::SystemTime,
}

/// Chain status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStatus {
    pub head_block_hash: BlockHash,
    pub head_block_number: u64,
    pub finalized_block_hash: Option<BlockHash>,
    pub finalized_block_number: Option<u64>,
    pub total_difficulty: U256,
    pub chain_id: u64,
    pub sync_status: SyncStatus,
    pub peer_count: u32,
    pub timestamp: std::time::SystemTime,
}

/// Sync status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncStatus {
    Synced,
    Syncing {
        current_block: u64,
        highest_block: u64,
        progress: f64,
    },
    NotSyncing,
}

/// gRPC client for Anduro Governance
#[derive(Debug)]
pub struct GovernanceGrpcClient {
    connections: std::sync::RwLock<HashMap<String, GovernanceConnectionHandle>>,
    message_sender: mpsc::Sender<GovernanceMessage>,
    message_receiver: std::sync::Mutex<Option<mpsc::Receiver<GovernanceMessage>>>,
    tls_config: Option<ClientTlsConfig>,
}

impl GovernanceGrpcClient {
    /// Create new governance gRPC client
    pub fn new(tls_config: Option<ClientTlsConfig>) -> Self {
        let (tx, rx) = mpsc::channel(1000);
        
        Self {
            connections: std::sync::RwLock::new(HashMap::new()),
            message_sender: tx,
            message_receiver: std::sync::Mutex::new(Some(rx)),
            tls_config,
        }
    }
    
    /// Create gRPC channel to endpoint
    async fn create_channel(&self, endpoint: String) -> Result<Channel, SystemError> {
        let mut channel = Channel::from_shared(endpoint.clone())
            .map_err(|e| SystemError::ConfigurationError {
                parameter: "governance_endpoint".to_string(),
                reason: format!("Invalid endpoint: {}", e),
            })?;
        
        if let Some(tls) = &self.tls_config {
            channel = channel.tls_config(tls.clone())
                .map_err(|e| SystemError::ConfigurationError {
                    parameter: "tls_config".to_string(),
                    reason: format!("TLS config error: {}", e),
                })?;
        }
        
        channel.connect().await
            .map_err(|e| SystemError::ActorCommunicationFailed {
                from: "alys_node".to_string(),
                to: endpoint,
                reason: format!("Failed to connect: {}", e),
            })
    }
    
    /// Start bi-directional stream with governance node
    async fn start_stream(&self, channel: Channel, node_id: String) -> Result<(), SystemError> {
        let (stream_tx, mut stream_rx) = mpsc::channel(100);
        
        // TODO: Implement actual gRPC streaming using generated protobuf clients
        // This would involve:
        // 1. Creating gRPC service client
        // 2. Establishing bi-directional stream
        // 3. Handling incoming messages
        // 4. Sending outgoing messages
        
        // Spawn task to handle incoming messages from this governance node
        let message_sender = self.message_sender.clone();
        tokio::spawn(async move {
            while let Some(message) = stream_rx.recv().await {
                if let Err(e) = message_sender.send(message).await {
                    eprintln!("Failed to forward governance message: {}", e);
                    break;
                }
            }
        });
        
        Ok(())
    }
    
    /// Generate message ID
    fn generate_message_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("msg_{}", timestamp)
    }
    
    /// Send message to all connected governance nodes
    async fn broadcast_to_governance_nodes(&self, message: GovernanceMessage) -> Result<(), SystemError> {
        let connections = self.connections.read().unwrap();
        
        if connections.is_empty() {
            return Err(SystemError::ActorNotFound {
                actor_name: "governance_nodes".to_string(),
            });
        }
        
        for (node_id, handle) in connections.iter() {
            if let Err(e) = handle.stream_sender.send(message.clone()).await {
                eprintln!("Failed to send message to governance node {}: {}", node_id, e);
            }
        }
        
        Ok(())
    }
}

#[async_trait]
impl GovernanceIntegration for GovernanceGrpcClient {
    async fn connect(&self, endpoint: String) -> Result<GovernanceConnectionHandle, SystemError> {
        // Create gRPC channel
        let channel = self.create_channel(endpoint.clone()).await?;
        
        // Generate node ID from endpoint
        let node_id = format!("node_{}", 
            endpoint.split("://").nth(1)
                .unwrap_or(&endpoint)
                .replace([':', '.', '/'], "_"));
        
        // Create message channel for this connection
        let (stream_tx, stream_rx) = mpsc::channel(100);
        
        // Start bi-directional stream
        self.start_stream(channel, node_id.clone()).await?;
        
        // Create connection handle
        let handle = GovernanceConnectionHandle {
            node_id: node_id.clone(),
            endpoint: endpoint.clone(),
            connected_at: std::time::SystemTime::now(),
            stream_sender: stream_tx,
        };
        
        // Store connection
        {
            let mut connections = self.connections.write().unwrap();
            connections.insert(node_id.clone(), handle.clone());
        }
        
        Ok(handle)
    }
    
    async fn send_block_proposal(&self, block: ConsensusBlock) -> Result<(), SystemError> {
        let message = GovernanceMessage {
            message_id: Self::generate_message_id(),
            from_node: "alys_consensus".to_string(),
            timestamp: std::time::SystemTime::now(),
            message_type: GovernanceMessageType::BlockProposal,
            payload: GovernancePayload::BlockProposal(block),
            signature: None, // Would be signed in production
        };
        
        self.broadcast_to_governance_nodes(message).await
    }
    
    async fn send_attestation(&self, attestation: Attestation) -> Result<(), SystemError> {
        let message = GovernanceMessage {
            message_id: Self::generate_message_id(),
            from_node: "alys_consensus".to_string(),
            timestamp: std::time::SystemTime::now(),
            message_type: GovernanceMessageType::Attestation,
            payload: GovernancePayload::Attestation(attestation),
            signature: None,
        };
        
        self.broadcast_to_governance_nodes(message).await
    }
    
    async fn send_federation_update(&self, update: FederationUpdate) -> Result<(), SystemError> {
        let message = GovernanceMessage {
            message_id: Self::generate_message_id(),
            from_node: "alys_federation".to_string(),
            timestamp: std::time::SystemTime::now(),
            message_type: GovernanceMessageType::FederationUpdate,
            payload: GovernancePayload::FederationUpdate(update),
            signature: None,
        };
        
        self.broadcast_to_governance_nodes(message).await
    }
    
    async fn send_chain_status(&self, status: ChainStatus) -> Result<(), SystemError> {
        let message = GovernanceMessage {
            message_id: Self::generate_message_id(),
            from_node: "alys_chain".to_string(),
            timestamp: std::time::SystemTime::now(),
            message_type: GovernanceMessageType::ChainStatus,
            payload: GovernancePayload::ChainStatus(status),
            signature: None,
        };
        
        self.broadcast_to_governance_nodes(message).await
    }
    
    async fn submit_vote(&self, vote: ProposalVote) -> Result<(), SystemError> {
        let message = GovernanceMessage {
            message_id: Self::generate_message_id(),
            from_node: "alys_governance".to_string(),
            timestamp: std::time::SystemTime::now(),
            message_type: GovernanceMessageType::ProposalVote,
            payload: GovernancePayload::ProposalVote(vote),
            signature: None,
        };
        
        self.broadcast_to_governance_nodes(message).await
    }
    
    async fn listen_for_messages(&self) -> Result<mpsc::Receiver<GovernanceMessage>, SystemError> {
        let mut receiver_guard = self.message_receiver.lock().unwrap();
        receiver_guard.take()
            .ok_or_else(|| SystemError::InvalidState {
                expected: "message receiver available".to_string(),
                actual: "message receiver already taken".to_string(),
            })
    }
    
    async fn get_connected_nodes(&self) -> Result<Vec<GovernanceNodeInfo>, SystemError> {
        let connections = self.connections.read().unwrap();
        
        let mut nodes = Vec::new();
        for (node_id, handle) in connections.iter() {
            nodes.push(GovernanceNodeInfo {
                node_id: node_id.clone(),
                endpoint: handle.endpoint.clone(),
                version: "1.0.0".to_string(), // Would be obtained from handshake
                capabilities: vec!["consensus".to_string(), "federation".to_string()],
                connected_at: handle.connected_at,
                last_activity: std::time::SystemTime::now(), // Would track actual activity
                message_count: 0, // Would track actual count
                health_status: GovernanceHealthStatus::Healthy,
            });
        }
        
        Ok(nodes)
    }
    
    async fn disconnect(&self, node_id: String) -> Result<(), SystemError> {
        let mut connections = self.connections.write().unwrap();
        
        if connections.remove(&node_id).is_some() {
            Ok(())
        } else {
            Err(SystemError::ActorNotFound {
                actor_name: format!("governance_node_{}", node_id),
            })
        }
    }
    
    async fn health_check(&self, node_id: String) -> Result<GovernanceHealthStatus, SystemError> {
        let connections = self.connections.read().unwrap();
        
        if connections.contains_key(&node_id) {
            // In production, this would send a heartbeat and check response
            Ok(GovernanceHealthStatus::Healthy)
        } else {
            Ok(GovernanceHealthStatus::Disconnected)
        }
    }
}

/// Factory for creating governance integrations
pub struct GovernanceIntegrationFactory;

impl GovernanceIntegrationFactory {
    /// Create governance integration with optional TLS
    pub fn create(tls_config: Option<ClientTlsConfig>) -> Box<dyn GovernanceIntegration> {
        Box::new(GovernanceGrpcClient::new(tls_config))
    }
    
    /// Create governance integration from config
    pub fn from_config(config: &GovernanceConfig) -> Box<dyn GovernanceIntegration> {
        let tls_config = config.tls_config.as_ref().map(|tls| {
            // Convert TLS config to tonic ClientTlsConfig
            // This would read certificates and configure TLS properly
            ClientTlsConfig::new()
        });
        
        Self::create(tls_config)
    }
}

/// Governance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
    pub endpoints: Vec<String>,
    pub tls_config: Option<GovernanceTlsConfig>,
    pub connection_timeout: std::time::Duration,
    pub heartbeat_interval: std::time::Duration,
    pub max_connections: usize,
    pub retry_attempts: u32,
    pub retry_delay: std::time::Duration,
}

/// TLS configuration for governance connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceTlsConfig {
    pub cert_path: String,
    pub key_path: String,
    pub ca_cert_path: Option<String>,
    pub server_name: Option<String>,
    pub verify_server: bool,
}

impl Default for GovernanceConfig {
    fn default() -> Self {
        Self {
            endpoints: vec!["https://governance.anduro.io:443".to_string()],
            tls_config: None,
            connection_timeout: std::time::Duration::from_secs(30),
            heartbeat_interval: std::time::Duration::from_secs(30),
            max_connections: 10,
            retry_attempts: 3,
            retry_delay: std::time::Duration::from_secs(5),
        }
    }
}