//! Bridge Stream Protocol Implementation
//! 
//! gRPC protocol for governance communication optimized for bridge operations

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tonic::{transport::Channel, Request, Response, Status, Streaming};
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use serde::{Deserialize, Serialize};

use crate::actors::bridge::{
    messages::stream_messages::*,
    shared::errors::BridgeError,
    config::StreamConfig,
};
use crate::integration::{GovernanceMessage, GovernanceMessageType};
use super::metrics::StreamMetrics;
use lighthouse_facade::bls::SignatureSet;
use actor_system::message::MessagePriority;

/// Bridge-optimized governance protocol handler
#[derive(Debug)]
pub struct BridgeGovernanceProtocol {
    /// Protocol configuration
    config: ProtocolConfig,
    
    /// Active gRPC connections by node ID
    connections: Arc<RwLock<HashMap<String, GovernanceConnection>>>,
    
    /// Message sender for outbound communication
    message_sender: Option<mpsc::Sender<OutboundMessage>>,
    
    /// Response handlers for request/response correlation
    response_handlers: Arc<RwLock<HashMap<String, ResponseHandler>>>,
    
    /// Protocol metrics
    metrics: Arc<StreamMetrics>,
    
    /// Authentication tokens by endpoint
    auth_tokens: Arc<RwLock<HashMap<String, AuthToken>>>,
}

/// Protocol configuration for bridge governance communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    /// Protocol version - should match governance nodes
    pub version: String,
    
    /// Connection timeout for initial connection establishment
    pub connection_timeout: Duration,
    
    /// Request timeout for individual requests
    pub request_timeout: Duration,
    
    /// Keepalive interval for connections
    pub keepalive_interval: Duration,
    
    /// Maximum message size for gRPC
    pub max_message_size: usize,
    
    /// TLS configuration
    pub tls_config: Option<TlsConfig>,
    
    /// Authentication configuration
    pub auth_config: AuthConfig,
    
    /// Retry configuration
    pub retry_config: RetryConfig,
    
    /// Compression settings
    pub compression_enabled: bool,
}

/// TLS configuration for secure gRPC connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to client certificate
    pub cert_path: String,
    
    /// Path to client private key
    pub key_path: String,
    
    /// Path to CA certificate for server verification
    pub ca_cert_path: Option<String>,
    
    /// Server name for SNI
    pub server_name: String,
    
    /// Whether to verify server certificate
    pub verify_server: bool,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Authentication method to use
    pub method: AuthMethod,
    
    /// API key or token for authentication
    pub token: Option<String>,
    
    /// Token refresh interval
    pub refresh_interval: Duration,
    
    /// Maximum authentication retries
    pub max_retries: u32,
}

/// Authentication methods supported
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    None,
    Bearer,
    ApiKey,
    Mutual,
}

/// Retry configuration for failed operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retries
    pub max_retries: u32,
    
    /// Initial retry delay
    pub initial_delay: Duration,
    
    /// Maximum retry delay
    pub max_delay: Duration,
    
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    
    /// Jitter factor (0.0 to 1.0)
    pub jitter_factor: f64,
}

/// Individual governance node connection
#[derive(Debug, Clone)]
pub struct GovernanceConnection {
    /// Node identifier
    pub node_id: String,
    
    /// Endpoint URL
    pub endpoint: String,
    
    /// gRPC channel
    pub channel: Option<Channel>,
    
    /// Connection status
    pub status: ConnectionStatus,
    
    /// Last successful communication
    pub last_success: Option<SystemTime>,
    
    /// Connection establishment time
    pub connected_at: Option<SystemTime>,
    
    /// Number of failed attempts
    pub failure_count: u32,
    
    /// Latency measurements
    pub latency_history: Vec<Duration>,
}

/// Connection status for individual nodes
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Authenticating,
    Authenticated,
    Failed { reason: String },
    Reconnecting,
}

/// Authentication token for a connection
#[derive(Debug, Clone)]
pub struct AuthToken {
    /// The actual token value
    pub token: String,
    
    /// Token expiration time
    pub expires_at: SystemTime,
    
    /// Whether the token is currently valid
    pub is_valid: bool,
    
    /// Token refresh count
    pub refresh_count: u32,
}

/// Outbound message to be sent to governance nodes
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    /// Target node ID (None = broadcast to all)
    pub target_node: Option<String>,
    
    /// Message payload
    pub payload: GovernancePayload,
    
    /// Request ID for correlation
    pub request_id: Option<String>,
    
    /// Message timeout
    pub timeout: Duration,
    
    /// Number of retry attempts remaining
    pub retries_remaining: u32,
}

/// Response handler for request/response correlation
#[derive(Debug)]
pub struct ResponseHandler {
    /// Request ID being handled
    pub request_id: String,
    
    /// Response sender channel
    pub response_sender: oneshot::Sender<Result<SignatureResponse, BridgeError>>,
    
    /// Request timeout
    pub timeout: SystemTime,
    
    /// Original request context
    pub request_context: RequestContext,
}

/// Context information for requests
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// Type of request
    pub request_type: RequestType,
    
    /// Associated pegout ID if applicable
    pub pegout_id: Option<String>,
    
    /// Request priority
    pub priority: MessagePriority,
    
    /// Request creation time
    pub created_at: SystemTime,
}

/// Types of requests that can be made
#[derive(Debug, Clone, PartialEq)]
pub enum RequestType {
    PegOutSignature,
    FederationUpdate,
    Heartbeat,
    StatusCheck,
}

/// Governance message payload types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GovernancePayload {
    /// Signature request for peg-out operations
    SignatureRequest {
        pegout_id: String,
        transaction: bitcoin::Transaction,
        destination: bitcoin::Address,
        amount: u64,
        fee: u64,
    },
    
    /// Signature response from governance
    SignatureResponse {
        request_id: String,
        signatures: SignatureSet,
        approval_status: ApprovalStatus,
    },
    
    /// Federation configuration update
    FederationUpdate {
        update_type: FederationUpdateType,
        new_config: actor_system::blockchain::FederationConfig,
        effective_height: u64,
    },
    
    /// Heartbeat message
    Heartbeat {
        timestamp: SystemTime,
        status: NodeStatus,
    },
    
    /// Status check request/response
    StatusCheck {
        node_id: String,
        last_block: u64,
        synced: bool,
    },
}

/// Node status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub healthy: bool,
    pub last_block: u64,
    pub peer_count: u32,
    pub uptime: Duration,
}

impl BridgeGovernanceProtocol {
    /// Create new bridge governance protocol instance from StreamConfig
    pub async fn new(stream_config: StreamConfig) -> Result<Self, BridgeError> {
        // Convert StreamConfig to ProtocolConfig
        let config = ProtocolConfig::from_stream_config(stream_config)?;
        let metrics = Arc::new(StreamMetrics::new().map_err(|e| 
            BridgeError::InternalError(format!("Failed to create metrics: {:?}", e)))?);
        
        Self::new_with_config(config, metrics).await
    }

    /// Create new bridge governance protocol instance with full config
    pub async fn new_with_config(
        config: ProtocolConfig,
        metrics: Arc<StreamMetrics>,
    ) -> Result<Self, BridgeError> {
        info!("Creating BridgeGovernanceProtocol with version {}", config.version);
        
        let connections = Arc::new(RwLock::new(HashMap::new()));
        let response_handlers = Arc::new(RwLock::new(HashMap::new()));
        let auth_tokens = Arc::new(RwLock::new(HashMap::new()));
        
        Ok(Self {
            config,
            connections,
            message_sender: None,
            response_handlers,
            metrics,
            auth_tokens,
        })
    }

    /// Establish connection to a governance node
    pub async fn connect_to_node(
        &mut self,
        node_id: String,
        endpoint: String,
    ) -> Result<(), BridgeError> {
        info!("Connecting to governance node {} at {}", node_id, endpoint);
        
        // Create gRPC channel with configuration
        let channel = self.create_grpc_channel(&endpoint).await?;
        
        // Create connection entry
        let connection = GovernanceConnection {
            node_id: node_id.clone(),
            endpoint: endpoint.clone(),
            channel: Some(channel),
            status: ConnectionStatus::Connecting,
            last_success: None,
            connected_at: Some(SystemTime::now()),
            failure_count: 0,
            latency_history: Vec::new(),
        };
        
        // Store connection
        {
            let mut connections = self.connections.write().await;
            connections.insert(node_id.clone(), connection);
        }
        
        // Perform authentication if required
        if !matches!(self.config.auth_config.method, AuthMethod::None) {
            self.authenticate_connection(&node_id).await?;
        }
        
        // Update connection status
        self.update_connection_status(&node_id, ConnectionStatus::Connected).await;
        
        info!("Successfully connected to governance node {}", node_id);
        self.metrics.record_connection_established(&node_id);
        
        Ok(())
    }

    /// Send signature request to governance nodes
    pub async fn request_signatures(
        &self,
        request: PegOutSignatureRequest,
    ) -> Result<oneshot::Receiver<Result<SignatureResponse, BridgeError>>, BridgeError> {
        let request_id = request.request_id.clone();
        info!("Sending signature request {} for pegout {}", request_id, request.pegout_id);
        
        // Create response handler
        let (response_sender, response_receiver) = oneshot::channel();
        let handler = ResponseHandler {
            request_id: request_id.clone(),
            response_sender,
            timeout: SystemTime::now() + request.timeout,
            request_context: RequestContext {
                request_type: RequestType::PegOutSignature,
                pegout_id: Some(request.pegout_id.clone()),
                priority: MessagePriority::Critical,
                created_at: SystemTime::now(),
            },
        };
        
        // Register response handler
        {
            let mut handlers = self.response_handlers.write().await;
            handlers.insert(request_id.clone(), handler);
        }
        
        // Create outbound message
        let message = OutboundMessage {
            target_node: None, // Broadcast to all nodes
            payload: GovernancePayload::SignatureRequest {
                pegout_id: request.pegout_id,
                transaction: request.unsigned_transaction,
                destination: request.destination_address,
                amount: request.amount,
                fee: request.fee,
            },
            request_id: Some(request_id.clone()),
            timeout: request.timeout,
            retries_remaining: self.config.retry_config.max_retries,
        };
        
        // Send message
        self.send_message(message).await?;
        
        self.metrics.record_signature_request_sent(&request_id);
        Ok(response_receiver)
    }

    /// Handle incoming signature response
    pub async fn handle_signature_response(
        &self,
        response: SignatureResponse,
    ) -> Result<(), BridgeError> {
        let request_id = response.request_id.clone();
        debug!("Handling signature response for request {}", request_id);
        
        // Find and remove response handler
        let handler = {
            let mut handlers = self.response_handlers.write().await;
            handlers.remove(&request_id)
        };
        
        if let Some(handler) = handler {
            // Send response to waiting handler
            if let Err(_) = handler.response_sender.send(Ok(response.clone())) {
                warn!("Failed to deliver signature response for request {}", request_id);
            }
            
            self.metrics.record_signature_response_received(&request_id);
            info!("Successfully delivered signature response for request {}", request_id);
        } else {
            warn!("Received signature response for unknown request {}", request_id);
            return Err(BridgeError::UnknownRequest(request_id));
        }
        
        Ok(())
    }

    /// Send heartbeat to all connected nodes
    pub async fn send_heartbeat(&self) -> Result<(), BridgeError> {
        debug!("Sending heartbeat to all governance nodes");
        
        let heartbeat_message = OutboundMessage {
            target_node: None, // Broadcast
            payload: GovernancePayload::Heartbeat {
                timestamp: SystemTime::now(),
                status: NodeStatus {
                    healthy: true,
                    last_block: 0, // Would be actual block height
                    peer_count: 0, // Would be actual peer count
                    uptime: Duration::from_secs(0), // Would be actual uptime
                },
            },
            request_id: None,
            timeout: Duration::from_secs(10),
            retries_remaining: 1,
        };
        
        self.send_message(heartbeat_message).await?;
        self.metrics.record_heartbeat_sent();
        
        Ok(())
    }

    /// Send federation update notification
    pub async fn send_federation_update(
        &self,
        update: FederationUpdate,
    ) -> Result<(), BridgeError> {
        info!("Sending federation update: {:?}", update.update_type);
        
        let message = OutboundMessage {
            target_node: None, // Broadcast
            payload: GovernancePayload::FederationUpdate {
                update_type: update.update_type,
                new_config: update.new_config,
                effective_height: update.effective_height,
            },
            request_id: Some(update.update_id),
            timeout: Duration::from_secs(60),
            retries_remaining: 3,
        };
        
        self.send_message(message).await?;
        Ok(())
    }

    /// Get connection health status
    pub async fn get_connection_status(&self) -> GovernanceConnectionStatus {
        let connections = self.connections.read().await;
        
        let connected_nodes: Vec<GovernanceNodeStatus> = connections
            .values()
            .map(|conn| GovernanceNodeStatus {
                node_id: conn.node_id.clone(),
                endpoint: conn.endpoint.clone(),
                status: self.map_connection_status(&conn.status),
                last_activity: conn.last_success.unwrap_or_else(|| SystemTime::now()),
                message_count: 0, // Would track actual message count
                latency: conn.latency_history.last().cloned(),
            })
            .collect();
        
        let healthy_connections = connected_nodes
            .iter()
            .filter(|node| matches!(node.status, NodeConnectionStatus::Connected))
            .count();
        
        let connection_quality = self.calculate_connection_quality(
            healthy_connections,
            connected_nodes.len(),
        );
        
        GovernanceConnectionStatus {
            connected_nodes,
            total_connections: connections.len(),
            healthy_connections,
            last_heartbeat: None, // Would track last heartbeat
            connection_quality,
        }
    }

    /// Create gRPC channel with proper configuration
    async fn create_grpc_channel(&self, endpoint: &str) -> Result<Channel, BridgeError> {
        debug!("Creating gRPC channel to {}", endpoint);
        
        let mut channel = Channel::from_shared(endpoint.to_string())
            .map_err(|e| BridgeError::ConnectionError(format!("Invalid endpoint: {}", e)))?
            .timeout(self.config.connection_timeout)
            .keepalive_timeout(self.config.keepalive_interval);
        
        // Configure TLS if specified
        if let Some(tls_config) = &self.config.tls_config {
            let tls = self.configure_tls(tls_config)?;
            channel = channel.tls_config(tls)
                .map_err(|e| BridgeError::ConnectionError(format!("TLS configuration error: {}", e)))?;
        }
        
        // Set message size limits
        channel = channel
            .max_send_message_size(Some(self.config.max_message_size))
            .max_receive_message_size(Some(self.config.max_message_size));
        
        // Establish connection
        let channel = channel.connect().await
            .map_err(|e| BridgeError::ConnectionError(format!("Connection failed: {}", e)))?;
        
        Ok(channel)
    }

    /// Configure TLS settings
    fn configure_tls(&self, tls_config: &TlsConfig) -> Result<tonic::transport::Channel, BridgeError> {
        // For now, return a basic channel without TLS config
        // TODO: Implement proper TLS configuration when tonic version supports it
        let endpoint = tonic::transport::Endpoint::from_shared(tls_config.server_name.clone())
            .map_err(|e| BridgeError::ConfigurationError(format!("Invalid endpoint: {}", e)))?;
        
        // TODO: Add proper TLS configuration when supported by tonic version
        // For now, return a basic channel
        Ok(endpoint.connect_lazy())
    }

    /// Authenticate connection to a node
    async fn authenticate_connection(&self, node_id: &str) -> Result<(), BridgeError> {
        debug!("Authenticating connection to node {}", node_id);
        
        match self.config.auth_config.method {
            AuthMethod::None => Ok(()),
            AuthMethod::Bearer | AuthMethod::ApiKey => {
                if let Some(token) = &self.config.auth_config.token {
                    // Store auth token
                    let auth_token = AuthToken {
                        token: token.clone(),
                        expires_at: SystemTime::now() + Duration::from_secs(3600), // 1 hour default
                        is_valid: true,
                        refresh_count: 0,
                    };
                    
                    let mut auth_tokens = self.auth_tokens.write().await;
                    auth_tokens.insert(node_id.to_string(), auth_token);
                    
                    Ok(())
                } else {
                    Err(BridgeError::AuthenticationError("No token provided".to_string()))
                }
            }
            AuthMethod::Mutual => {
                // Mutual TLS authentication is handled during TLS handshake
                Ok(())
            }
        }
    }

    /// Send message to governance nodes
    async fn send_message(&self, message: OutboundMessage) -> Result<(), BridgeError> {
        debug!("Sending message to governance nodes: {:?}", message.payload);
        
        // In a real implementation, this would:
        // 1. Serialize the message
        // 2. Send via gRPC to target node(s)
        // 3. Handle retries and failures
        
        // For now, simulate successful send
        info!("Simulated message send successful");
        Ok(())
    }

    /// Update connection status for a node
    async fn update_connection_status(&self, node_id: &str, status: ConnectionStatus) {
        debug!("Updating connection status for {} to {:?}", node_id, status);
        
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.get_mut(node_id) {
            connection.status = status;
            
            if matches!(connection.status, ConnectionStatus::Connected) {
                connection.last_success = Some(SystemTime::now());
                connection.failure_count = 0;
            } else if matches!(connection.status, ConnectionStatus::Failed { .. }) {
                connection.failure_count += 1;
            }
        }
    }

    /// Map internal connection status to public status
    fn map_connection_status(&self, status: &ConnectionStatus) -> NodeConnectionStatus {
        match status {
            ConnectionStatus::Disconnected => NodeConnectionStatus::Disconnected,
            ConnectionStatus::Connecting => NodeConnectionStatus::Connecting,
            ConnectionStatus::Connected | ConnectionStatus::Authenticated => NodeConnectionStatus::Connected,
            ConnectionStatus::Authenticating => NodeConnectionStatus::Connecting,
            ConnectionStatus::Failed { reason } => NodeConnectionStatus::Failed { error: reason.clone() },
            ConnectionStatus::Reconnecting => NodeConnectionStatus::Connecting,
        }
    }

    /// Calculate overall connection quality
    fn calculate_connection_quality(&self, healthy: usize, total: usize) -> ConnectionQuality {
        if total == 0 {
            return ConnectionQuality::Failed;
        }
        
        let ratio = healthy as f64 / total as f64;
        match ratio {
            r if r >= 0.9 => ConnectionQuality::Excellent,
            r if r >= 0.7 => ConnectionQuality::Good,
            r if r >= 0.5 => ConnectionQuality::Degraded,
            r if r >= 0.2 => ConnectionQuality::Poor,
            _ => ConnectionQuality::Failed,
        }
    }

    /// Connect to all configured governance nodes
    pub async fn connect_all(&self) -> Result<HashMap<String, Result<(), BridgeError>>, BridgeError> {
        info!("Connecting to all configured governance nodes");
        let mut results = HashMap::new();
        
        // Get governance endpoints from config
        let endpoints: Vec<String> = vec![
            "https://governance1.alys.network:9000".to_string(),
            "https://governance2.alys.network:9000".to_string(),
            "https://governance3.alys.network:9000".to_string(),
        ]; // In real implementation, would get from config
        
        for (index, endpoint) in endpoints.iter().enumerate() {
            let node_id = format!("governance_node_{}", index);
            
            // Attempt connection
            match self.connect_to_node_readonly(&node_id, endpoint.clone()).await {
                Ok(_) => {
                    results.insert(endpoint.clone(), Ok(()));
                    info!("Successfully connected to {}", endpoint);
                }
                Err(e) => {
                    results.insert(endpoint.clone(), Err(e.clone()));
                    warn!("Failed to connect to {}: {:?}", endpoint, e);
                }
            }
        }
        
        Ok(results)
    }

    /// Connect to node (read-only version)
    async fn connect_to_node_readonly(&self, node_id: &str, endpoint: String) -> Result<(), BridgeError> {
        debug!("Connecting to governance node {} at {}", node_id, endpoint);
        
        // Create gRPC channel with configuration
        let channel = self.create_grpc_channel(&endpoint).await?;
        
        // Create connection entry
        let connection = GovernanceConnection {
            node_id: node_id.to_string(),
            endpoint: endpoint.clone(),
            channel: Some(channel),
            status: ConnectionStatus::Connecting,
            last_success: None,
            connected_at: Some(SystemTime::now()),
            failure_count: 0,
            latency_history: Vec::new(),
        };
        
        // Store connection
        {
            let mut connections = self.connections.write().await;
            connections.insert(node_id.to_string(), connection);
        }
        
        // Perform authentication if required
        if !matches!(self.config.auth_config.method, AuthMethod::None) {
            self.authenticate_connection(node_id).await?;
        }
        
        // Update connection status
        self.update_connection_status(node_id, ConnectionStatus::Connected).await;
        
        Ok(())
    }

    /// Broadcast message to multiple endpoints
    pub async fn broadcast_message(
        &self,
        message: GovernanceMessage,
        target_endpoints: Vec<String>,
    ) -> Result<HashMap<String, Result<(), BridgeError>>, BridgeError> {
        info!("Broadcasting message {} to {} endpoints", message.message_id, target_endpoints.len());
        let mut results = HashMap::new();
        
        for endpoint in target_endpoints {
            // Convert GovernanceMessage to OutboundMessage
            let payload = match &message.payload {
                super::governance::GovernancePayload::SignatureRequest(req) => {
                    GovernancePayload::SignatureRequest {
                        pegout_id: "unknown".to_string(), // Would extract from req
                        transaction: bitcoin::Transaction {
                            version: 1,
                            lock_time: bitcoin::absolute::LockTime::ZERO,
                            input: vec![],
                            output: vec![],
                        }, // Would extract from req
                        destination: bitcoin::Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
                            .unwrap()
                            .assume_checked(), // Would extract from req
                        amount: 0, // Would extract from req
                        fee: 0, // Would extract from req
                    }
                }
                super::governance::GovernancePayload::Heartbeat => {
                    GovernancePayload::Heartbeat {
                        timestamp: SystemTime::now(),
                        status: NodeStatus {
                            healthy: true,
                            last_block: 0,
                            peer_count: 0,
                            uptime: Duration::from_secs(0),
                        },
                    }
                }
                _ => {
                    // For other message types, create a generic heartbeat
                    GovernancePayload::Heartbeat {
                        timestamp: SystemTime::now(),
                        status: NodeStatus {
                            healthy: true,
                            last_block: 0,
                            peer_count: 0,
                            uptime: Duration::from_secs(0),
                        },
                    }
                }
            };

            let outbound_message = OutboundMessage {
                target_node: Some(endpoint.clone()),
                payload,
                request_id: Some(message.message_id.clone()),
                timeout: Duration::from_secs(30),
                retries_remaining: 2,
            };
            
            // Send message to this endpoint
            match self.send_message(outbound_message).await {
                Ok(_) => {
                    results.insert(endpoint.clone(), Ok(()));
                    debug!("Successfully sent message to {}", endpoint);
                }
                Err(e) => {
                    results.insert(endpoint.clone(), Err(e.clone()));
                    warn!("Failed to send message to {}: {:?}", endpoint, e);
                }
            }
        }
        
        Ok(results)
    }
}

impl ProtocolConfig {
    /// Convert StreamConfig to ProtocolConfig
    pub fn from_stream_config(stream_config: StreamConfig) -> Result<Self, BridgeError> {
        let tls_config = if stream_config.ca_cert_path.is_some() || 
                           stream_config.client_cert_path.is_some() || 
                           stream_config.client_key_path.is_some() {
            Some(TlsConfig {
                cert_path: stream_config.client_cert_path.unwrap_or_default(),
                key_path: stream_config.client_key_path.unwrap_or_default(),
                ca_cert_path: stream_config.ca_cert_path,
                server_name: "governance.alys.network".to_string(),
                verify_server: true,
            })
        } else {
            None
        };

        let auth_config = AuthConfig {
            method: if stream_config.auth_token.is_some() { 
                AuthMethod::Bearer 
            } else { 
                AuthMethod::None 
            },
            token: stream_config.auth_token,
            refresh_interval: Duration::from_secs(3600),
            max_retries: 3,
        };

        Ok(Self {
            version: "v1.0.0".to_string(),
            connection_timeout: stream_config.connection_timeout,
            request_timeout: Duration::from_secs(60),
            keepalive_interval: stream_config.heartbeat_interval,
            max_message_size: 4 * 1024 * 1024, // 4MB
            tls_config,
            auth_config,
            retry_config: RetryConfig {
                max_retries: stream_config.reconnect_attempts,
                initial_delay: stream_config.reconnect_delay,
                max_delay: Duration::from_secs(30),
                backoff_multiplier: 2.0,
                jitter_factor: 0.1,
            },
            compression_enabled: true,
        })
    }
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            version: "v1.0.0".to_string(),
            connection_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(60),
            keepalive_interval: Duration::from_secs(20),
            max_message_size: 4 * 1024 * 1024, // 4MB
            tls_config: None,
            auth_config: AuthConfig {
                method: AuthMethod::None,
                token: None,
                refresh_interval: Duration::from_secs(3600),
                max_retries: 3,
            },
            retry_config: RetryConfig {
                max_retries: 3,
                initial_delay: Duration::from_millis(100),
                max_delay: Duration::from_secs(30),
                backoff_multiplier: 2.0,
                jitter_factor: 0.1,
            },
            compression_enabled: true,
        }
    }
}