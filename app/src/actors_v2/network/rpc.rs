//! NetworkActor V2 RPC Interface
//!
//! External RPC endpoints for NetworkActor V2 system integration.
//! Provides HTTP/JSON-RPC interface for network operations.

use actix::Addr;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use anyhow::{Result, anyhow};

use crate::actors_v2::network::{
    NetworkActor, SyncActor,
    NetworkMessage, SyncMessage,
    NetworkResponse, SyncResponse,
};

/// RPC request types for NetworkActor V2
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum NetworkRpcRequest {
    /// Start networking subsystem
    StartNetwork {
        listen_addresses: Vec<String>,
        bootstrap_peers: Vec<String>,
    },
    /// Stop networking subsystem
    StopNetwork {
        graceful: bool,
    },
    /// Get network status
    GetNetworkStatus,
    /// Get connected peers
    GetConnectedPeers,
    /// Broadcast block
    BroadcastBlock {
        block_data: String, // hex-encoded
        priority: bool,
    },
    /// Broadcast transaction
    BroadcastTransaction {
        tx_data: String, // hex-encoded
    },
    /// Connect to specific peer
    ConnectToPeer {
        peer_address: String,
    },
    /// Disconnect from peer
    DisconnectPeer {
        peer_id: String,
    },
    /// Get network metrics
    GetNetworkMetrics,
    /// Start blockchain sync
    StartSync,
    /// Stop blockchain sync
    StopSync,
    /// Get sync status
    GetSyncStatus,
    /// Get sync metrics
    GetSyncMetrics,
}

/// RPC response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NetworkRpcResponse {
    /// Success response with data
    Success {
        result: NetworkRpcResult,
    },
    /// Error response
    Error {
        error: String,
        code: i32,
    },
}

/// RPC result data types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NetworkRpcResult {
    /// Simple success confirmation
    Success,
    /// Network status information
    NetworkStatus {
        local_peer_id: String,
        connected_peers: usize,
        listening_addresses: Vec<String>,
        is_running: bool,
    },
    /// Peer list
    Peers {
        peers: Vec<PeerRpcInfo>,
    },
    /// Broadcast confirmation
    Broadcast {
        message_id: String,
    },
    /// Connection result
    Connection {
        peer_id: String,
        success: bool,
    },
    /// Network metrics
    NetworkMetrics {
        connected_peers: u32,
        messages_sent: u64,
        messages_received: u64,
        gossip_messages_published: u64,
    },
    /// Sync status
    SyncStatus {
        current_height: u64,
        target_height: u64,
        is_syncing: bool,
        sync_peers: usize,
        pending_requests: usize,
    },
    /// Sync metrics
    SyncMetrics {
        blocks_synced: u64,
        blocks_processed: u64,
        sync_rate_bps: f64,
        current_height: u64,
    },
}

/// Peer information for RPC responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRpcInfo {
    pub peer_id: String,
    pub address: String,
    pub connection_time: String, // ISO 8601 timestamp
    pub reputation: f64,
}

/// NetworkActor V2 RPC Handler
pub struct NetworkRpcHandler {
    network_actor: Addr<NetworkActor>,
    sync_actor: Addr<SyncActor>,
}

impl NetworkRpcHandler {
    /// Create new RPC handler
    pub fn new(network_actor: Addr<NetworkActor>, sync_actor: Addr<SyncActor>) -> Self {
        Self {
            network_actor,
            sync_actor,
        }
    }

    /// Process RPC request
    pub async fn handle_request(&self, request: NetworkRpcRequest) -> NetworkRpcResponse {
        match self.process_request(request).await {
            Ok(result) => NetworkRpcResponse::Success { result },
            Err(e) => NetworkRpcResponse::Error {
                error: e.to_string(),
                code: -1,
            },
        }
    }

    /// Process individual RPC request
    async fn process_request(&self, request: NetworkRpcRequest) -> Result<NetworkRpcResult> {
        match request {
            NetworkRpcRequest::StartNetwork { listen_addresses, bootstrap_peers } => {
                let msg = NetworkMessage::StartNetwork {
                    listen_addrs: listen_addresses,
                    bootstrap_peers,
                };

                match self.network_actor.send(msg).await {
                    Ok(Ok(NetworkResponse::Started)) => Ok(NetworkRpcResult::Success),
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Err(e)) => Err(anyhow!("Network start failed: {:?}", e)),
                    Err(e) => Err(anyhow!("Actor communication error: {}", e)),
                }
            }

            NetworkRpcRequest::StopNetwork { graceful } => {
                let msg = NetworkMessage::StopNetwork { graceful };

                match self.network_actor.send(msg).await {
                    Ok(Ok(NetworkResponse::Stopped)) => Ok(NetworkRpcResult::Success),
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Err(e)) => Err(anyhow!("Network stop failed: {:?}", e)),
                    Err(e) => Err(anyhow!("Actor communication error: {}", e)),
                }
            }

            NetworkRpcRequest::GetNetworkStatus => {
                let msg = NetworkMessage::GetNetworkStatus;

                match self.network_actor.send(msg).await {
                    Ok(Ok(NetworkResponse::Status(status))) => {
                        Ok(NetworkRpcResult::NetworkStatus {
                            local_peer_id: status.local_peer_id,
                            connected_peers: status.connected_peers,
                            listening_addresses: status.listening_addresses,
                            is_running: status.is_running,
                        })
                    }
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Err(e)) => Err(anyhow!("Failed to get network status: {:?}", e)),
                    Err(e) => Err(anyhow!("Actor communication error: {}", e)),
                }
            }

            NetworkRpcRequest::GetConnectedPeers => {
                let msg = NetworkMessage::GetConnectedPeers;

                match self.network_actor.send(msg).await {
                    Ok(Ok(NetworkResponse::Peers(peers))) => {
                        let rpc_peers = peers.into_iter().map(|p| PeerRpcInfo {
                            peer_id: p.peer_id,
                            address: p.address,
                            connection_time: humantime::format_rfc3339_millis(p.connection_time).to_string(),
                            reputation: p.reputation,
                        }).collect();

                        Ok(NetworkRpcResult::Peers { peers: rpc_peers })
                    }
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Err(e)) => Err(anyhow!("Failed to get peers: {:?}", e)),
                    Err(e) => Err(anyhow!("Actor communication error: {}", e)),
                }
            }

            NetworkRpcRequest::BroadcastBlock { block_data, priority } => {
                // Decode hex block data
                let block_bytes = hex::decode(&block_data)
                    .map_err(|e| anyhow!("Invalid hex block data: {}", e))?;

                let msg = NetworkMessage::BroadcastBlock {
                    block_data: block_bytes,
                    priority,
                };

                match self.network_actor.send(msg).await {
                    Ok(Ok(NetworkResponse::Broadcasted { message_id })) => {
                        Ok(NetworkRpcResult::Broadcast { message_id })
                    }
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Err(e)) => Err(anyhow!("Broadcast failed: {:?}", e)),
                    Err(e) => Err(anyhow!("Actor communication error: {}", e)),
                }
            }

            NetworkRpcRequest::BroadcastTransaction { tx_data } => {
                // Decode hex transaction data
                let tx_bytes = hex::decode(&tx_data)
                    .map_err(|e| anyhow!("Invalid hex transaction data: {}", e))?;

                let msg = NetworkMessage::BroadcastTransaction { tx_data: tx_bytes };

                match self.network_actor.send(msg).await {
                    Ok(Ok(NetworkResponse::Broadcasted { message_id })) => {
                        Ok(NetworkRpcResult::Broadcast { message_id })
                    }
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Err(e)) => Err(anyhow!("Broadcast failed: {:?}", e)),
                    Err(e) => Err(anyhow!("Actor communication error: {}", e)),
                }
            }

            NetworkRpcRequest::ConnectToPeer { peer_address } => {
                let msg = NetworkMessage::ConnectToPeer { peer_addr: peer_address };

                match self.network_actor.send(msg).await {
                    Ok(Ok(NetworkResponse::Connected { peer_id })) => {
                        Ok(NetworkRpcResult::Connection { peer_id, success: true })
                    }
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Err(e)) => Err(anyhow!("Connection failed: {:?}", e)),
                    Err(e) => Err(anyhow!("Actor communication error: {}", e)),
                }
            }

            NetworkRpcRequest::DisconnectPeer { peer_id } => {
                let msg = NetworkMessage::DisconnectPeer { peer_id: peer_id.clone() };

                match self.network_actor.send(msg).await {
                    Ok(Ok(NetworkResponse::Disconnected { .. })) => {
                        Ok(NetworkRpcResult::Connection { peer_id, success: false })
                    }
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Err(e)) => Err(anyhow!("Disconnection failed: {:?}", e)),
                    Err(e) => Err(anyhow!("Actor communication error: {}", e)),
                }
            }

            NetworkRpcRequest::GetNetworkMetrics => {
                let msg = NetworkMessage::GetMetrics;

                match self.network_actor.send(msg).await {
                    Ok(Ok(NetworkResponse::Metrics(metrics))) => {
                        Ok(NetworkRpcResult::NetworkMetrics {
                            connected_peers: metrics.connected_peers,
                            messages_sent: metrics.messages_sent,
                            messages_received: metrics.messages_received,
                            gossip_messages_published: metrics.gossip_messages_published,
                        })
                    }
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Err(e)) => Err(anyhow!("Failed to get metrics: {:?}", e)),
                    Err(e) => Err(anyhow!("Actor communication error: {}", e)),
                }
            }

            NetworkRpcRequest::StartSync => {
                let msg = SyncMessage::StartSync;

                match self.sync_actor.send(msg).await {
                    Ok(Ok(SyncResponse::Started)) => Ok(NetworkRpcResult::Success),
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Err(e)) => Err(anyhow!("Sync start failed: {:?}", e)),
                    Err(e) => Err(anyhow!("Actor communication error: {}", e)),
                }
            }

            NetworkRpcRequest::StopSync => {
                let msg = SyncMessage::StopSync;

                match self.sync_actor.send(msg).await {
                    Ok(Ok(SyncResponse::Stopped)) => Ok(NetworkRpcResult::Success),
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Err(e)) => Err(anyhow!("Sync stop failed: {:?}", e)),
                    Err(e) => Err(anyhow!("Actor communication error: {}", e)),
                }
            }

            NetworkRpcRequest::GetSyncStatus => {
                let msg = SyncMessage::GetSyncStatus;

                match self.sync_actor.send(msg).await {
                    Ok(Ok(SyncResponse::Status(status))) => {
                        Ok(NetworkRpcResult::SyncStatus {
                            current_height: status.current_height,
                            target_height: status.target_height,
                            is_syncing: status.is_syncing,
                            sync_peers: status.sync_peers.len(),
                            pending_requests: status.pending_requests,
                        })
                    }
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Err(e)) => Err(anyhow!("Failed to get sync status: {:?}", e)),
                    Err(e) => Err(anyhow!("Actor communication error: {}", e)),
                }
            }

            NetworkRpcRequest::GetSyncMetrics => {
                let msg = SyncMessage::GetMetrics;

                match self.sync_actor.send(msg).await {
                    Ok(Ok(SyncResponse::Metrics(metrics))) => {
                        Ok(NetworkRpcResult::SyncMetrics {
                            blocks_synced: metrics.blocks_synced,
                            blocks_processed: metrics.blocks_processed,
                            sync_rate_bps: metrics.sync_rate_blocks_per_second,
                            current_height: metrics.current_height,
                        })
                    }
                    Ok(Ok(_)) => Err(anyhow!("Unexpected response type")),
                    Ok(Err(e)) => Err(anyhow!("Failed to get sync metrics: {:?}", e)),
                    Err(e) => Err(anyhow!("Actor communication error: {}", e)),
                }
            }
        }
    }

    /// Create RPC response from result
    pub fn create_response(result: Result<NetworkRpcResult>) -> NetworkRpcResponse {
        match result {
            Ok(result) => NetworkRpcResponse::Success { result },
            Err(e) => NetworkRpcResponse::Error {
                error: e.to_string(),
                code: -1,
            },
        }
    }

    /// Validate RPC request
    pub fn validate_request(request: &NetworkRpcRequest) -> Result<()> {
        match request {
            NetworkRpcRequest::StartNetwork { listen_addresses, bootstrap_peers } => {
                if listen_addresses.is_empty() {
                    return Err(anyhow!("At least one listen address required"));
                }

                // Validate address formats
                for addr in listen_addresses {
                    if addr.is_empty() || !addr.starts_with('/') {
                        return Err(anyhow!("Invalid multiaddr format: {}", addr));
                    }
                }

                for addr in bootstrap_peers {
                    if !addr.is_empty() && !addr.starts_with('/') {
                        return Err(anyhow!("Invalid bootstrap peer address: {}", addr));
                    }
                }
            }

            NetworkRpcRequest::BroadcastBlock { block_data, .. } => {
                if block_data.is_empty() {
                    return Err(anyhow!("Block data cannot be empty"));
                }

                // Validate hex encoding
                hex::decode(block_data)
                    .map_err(|e| anyhow!("Invalid hex block data: {}", e))?;
            }

            NetworkRpcRequest::BroadcastTransaction { tx_data } => {
                if tx_data.is_empty() {
                    return Err(anyhow!("Transaction data cannot be empty"));
                }

                // Validate hex encoding
                hex::decode(tx_data)
                    .map_err(|e| anyhow!("Invalid hex transaction data: {}", e))?;
            }

            NetworkRpcRequest::ConnectToPeer { peer_address } => {
                if peer_address.is_empty() || !peer_address.starts_with('/') {
                    return Err(anyhow!("Invalid peer address format: {}", peer_address));
                }
            }

            NetworkRpcRequest::DisconnectPeer { peer_id } => {
                if peer_id.is_empty() {
                    return Err(anyhow!("Peer ID cannot be empty"));
                }
            }

            // Other requests don't need validation
            _ => {}
        }

        Ok(())
    }
}

/// Network subsystem coordinator
/// Manages both NetworkActor and SyncActor for external interfaces
pub struct NetworkSubsystem {
    network_actor: Addr<NetworkActor>,
    sync_actor: Addr<SyncActor>,
    rpc_handler: NetworkRpcHandler,
}

impl NetworkSubsystem {
    /// Create new network subsystem
    pub fn new(network_actor: Addr<NetworkActor>, sync_actor: Addr<SyncActor>) -> Self {
        let rpc_handler = NetworkRpcHandler::new(network_actor.clone(), sync_actor.clone());

        Self {
            network_actor,
            sync_actor,
            rpc_handler,
        }
    }

    /// Initialize actor coordination
    pub async fn initialize(&self) -> Result<()> {
        tracing::info!("Initializing NetworkActor V2 subsystem coordination");

        // Set up actor cross-references
        let network_set_sync = NetworkMessage::SetSyncActor {
            addr: self.sync_actor.clone(),
        };

        let sync_set_network = SyncMessage::SetNetworkActor {
            addr: self.network_actor.clone(),
        };

        // Configure NetworkActor with SyncActor reference
        match self.network_actor.send(network_set_sync).await {
            Ok(Ok(_)) => tracing::debug!("NetworkActor configured with SyncActor reference"),
            Ok(Err(e)) => return Err(anyhow!("Failed to configure NetworkActor: {:?}", e)),
            Err(e) => return Err(anyhow!("NetworkActor communication error: {}", e)),
        }

        // Configure SyncActor with NetworkActor reference
        match self.sync_actor.send(sync_set_network).await {
            Ok(Ok(_)) => tracing::debug!("SyncActor configured with NetworkActor reference"),
            Ok(Err(e)) => return Err(anyhow!("Failed to configure SyncActor: {:?}", e)),
            Err(e) => return Err(anyhow!("SyncActor communication error: {}", e)),
        }

        tracing::info!("NetworkActor V2 subsystem initialized successfully");
        Ok(())
    }

    /// Process external RPC request
    pub async fn handle_rpc(&self, request: NetworkRpcRequest) -> NetworkRpcResponse {
        // Validate request
        if let Err(e) = NetworkRpcHandler::validate_request(&request) {
            return NetworkRpcResponse::Error {
                error: format!("Invalid request: {}", e),
                code: -400,
            };
        }

        // Process request
        self.rpc_handler.handle_request(request).await
    }

    /// Get subsystem status
    pub async fn get_status(&self) -> Result<HashMap<String, serde_json::Value>> {
        let mut status = HashMap::new();

        // Get network status
        match self.network_actor.send(NetworkMessage::GetNetworkStatus).await {
            Ok(Ok(NetworkResponse::Status(net_status))) => {
                status.insert("network".to_string(), serde_json::to_value(net_status)?);
            }
            Ok(Ok(_)) => {
                status.insert("network_error".to_string(), serde_json::Value::String("Unexpected response type".to_string()));
            }
            Ok(Err(e)) => {
                status.insert("network_error".to_string(), serde_json::Value::String(format!("{:?}", e)));
            }
            Err(e) => {
                status.insert("network_error".to_string(), serde_json::Value::String(e.to_string()));
            }
        }

        // Get sync status
        match self.sync_actor.send(SyncMessage::GetSyncStatus).await {
            Ok(Ok(SyncResponse::Status(sync_status))) => {
                status.insert("sync".to_string(), serde_json::to_value(sync_status)?);
            }
            Ok(Ok(_)) => {
                status.insert("sync_error".to_string(), serde_json::Value::String("Unexpected response type".to_string()));
            }
            Ok(Err(e)) => {
                status.insert("sync_error".to_string(), serde_json::Value::String(format!("{:?}", e)));
            }
            Err(e) => {
                status.insert("sync_error".to_string(), serde_json::Value::String(e.to_string()));
            }
        }

        Ok(status)
    }

    /// Shutdown subsystem gracefully
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down NetworkActor V2 subsystem");

        // Stop sync first
        let _ = self.sync_actor.send(SyncMessage::StopSync).await;

        // Stop network
        let _ = self.network_actor.send(NetworkMessage::StopNetwork { graceful: true }).await;

        tracing::info!("NetworkActor V2 subsystem shutdown complete");
        Ok(())
    }
}