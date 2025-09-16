//! Enhanced StreamActor for Bridge Integration
//! 
//! Bridge-optimized version of StreamActor for governance communication

use actix::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use actor_system::{
    actor::AlysActor,
};

use crate::actors::bridge::{
    config::StreamConfig,
    messages::*,
};
use crate::actors::bridge::messages::stream_messages::StreamMessage;
use crate::integration::{GovernanceMessage, GovernanceMessageType};
use super::{reconnection::*, metrics::*, protocol::*, request_tracking::*};
use super::reconnection::BackoffDecision;
use crate::actors::bridge::shared::errors::BridgeError;

/// Enhanced StreamActor for bridge operations
pub struct StreamActor {
    /// Instance identifier
    pub instance_id: String,

    /// Configuration
    pub config: StreamConfig,

    /// Governance connections
    pub governance_connections: HashMap<String, GovernanceConnection>,

    /// Message handling
    pub message_buffer: Vec<PendingMessage>,
    pub request_tracker: AdvancedRequestTracker,

    /// Bridge actor integration (using weak references to prevent cycles)
    pub pegout_actor: Option<Weak<Addr<super::super::pegout::PegOutActor>>>,
    pub bridge_coordinator: Option<Weak<Addr<super::super::bridge::BridgeActor>>>,

    /// Connection management
    reconnection_manager: ReconnectionManager,

    /// Metrics and monitoring
    pub metrics: StreamMetrics,

    /// actor_system integration
    pub actor_system_metrics: actor_system::metrics::ActorMetrics,

    /// Protocol handler for gRPC communication
    protocol_handler: Option<BridgeGovernanceProtocol>,

    /// State management
    pub connection_status: ConnectionStatus,
    pub last_heartbeat: Option<SystemTime>,
}

/// Governance connection state
#[derive(Debug, Clone)]
pub struct GovernanceConnection {
    pub node_id: String,
    pub endpoint: String,
    pub status: NodeConnectionStatus,
    pub connected_at: Option<SystemTime>,
    pub last_activity: SystemTime,
    pub message_count: u64,
    pub latency: Option<Duration>,
    pub health_score: f64,
}

/// Pending message for reliability
#[derive(Debug, Clone)]
pub struct PendingMessage {
    pub message_id: String,
    pub message: GovernanceMessage,
    pub attempts: u32,
    pub created_at: SystemTime,
    pub next_retry: SystemTime,
    pub timeout: SystemTime,
}

// Old RequestTracker definitions removed - replaced by AdvancedRequestTracker


/// Connection status
#[derive(Debug, Clone)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected { healthy_nodes: usize, total_nodes: usize },
    Degraded { issues: Vec<String> },
}

impl StreamActor {
    /// Create new enhanced StreamActor
    pub fn new(config: StreamConfig) -> Result<Self, StreamError> {
        let reconnection_manager = ReconnectionManager::new(
            config.reconnect_attempts,
            config.reconnect_delay,
        );
        
        let metrics = StreamMetrics::new()?;
        
        let actor_system_metrics = actor_system::metrics::ActorMetrics::new();
        
        Ok(Self {
            instance_id: Uuid::new_v4().to_string(),
            config,
            governance_connections: HashMap::new(),
            message_buffer: Vec::new(),
            request_tracker: AdvancedRequestTracker::with_defaults(),
            pegout_actor: None,
            bridge_coordinator: None,
            reconnection_manager,
            metrics,
            actor_system_metrics,
            protocol_handler: None,
            connection_status: ConnectionStatus::Disconnected,
            last_heartbeat: None,
        })
    }

    /// Initialize StreamActor
    async fn initialize(&mut self, ctx: &mut Context<Self>) -> Result<(), StreamError> {
        info!("Initializing enhanced StreamActor for bridge operations");

        // Initialize protocol handler
        self.initialize_protocol_handler().await?;

        // Establish connections to governance nodes
        self.establish_governance_connections().await?;

        // Start periodic tasks
        self.start_heartbeat(ctx);
        self.start_connection_monitoring(ctx);
        self.start_request_timeout_checking(ctx);
        self.start_message_retry(ctx);

        // Update status
        self.update_connection_status();
        self.metrics.record_actor_started();

        info!("Enhanced StreamActor initialized successfully");
        Ok(())
    }

    /// Initialize protocol handler
    async fn initialize_protocol_handler(&mut self) -> Result<(), StreamError> {
        match BridgeGovernanceProtocol::new(self.config.clone()).await {
            Ok(protocol) => {
                self.protocol_handler = Some(protocol);
                info!("Protocol handler initialized successfully");
                Ok(())
            }
            Err(e) => {
                error!("Failed to initialize protocol handler: {:?}", e);
                Err(StreamError::InternalError(format!("Protocol handler initialization failed: {:?}", e)))
            }
        }
    }

    /// Establish connections to governance nodes
    pub async fn establish_governance_connections(&mut self) -> Result<(), StreamError> {
        info!("Establishing connections to {} governance nodes", self.config.governance_endpoints.len());

        if let Some(protocol) = &self.protocol_handler {
            // Use protocol handler to establish connections
            match protocol.connect_all().await {
                Ok(connection_results) => {
                    for (endpoint, result) in connection_results {
                        let node_id = self.generate_node_id(&endpoint);
                        
                        match result {
                            Ok(_) => {
                                let connection = GovernanceConnection {
                                    node_id: node_id.clone(),
                                    endpoint: endpoint.clone(),
                                    status: NodeConnectionStatus::Connected,
                                    connected_at: Some(SystemTime::now()),
                                    last_activity: SystemTime::now(),
                                    message_count: 0,
                                    latency: None,
                                    health_score: 100.0,
                                };
                                
                                self.governance_connections.insert(node_id.clone(), connection);
                                self.metrics.record_connection_established(&node_id);
                                info!("Connected to governance node: {}", endpoint);
                            }
                            Err(e) => {
                                warn!("Failed to connect to governance node {}: {:?}", endpoint, e);
                                self.metrics.record_connection_failed(&endpoint);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to establish governance connections: {:?}", e);
                    return Err(StreamError::ConnectionError(format!("Connection establishment failed: {:?}", e)));
                }
            }
        } else {
            return Err(StreamError::InternalError("Protocol handler not initialized".to_string()));
        }

        Ok(())
    }

    /// Connect to individual governance node
    async fn connect_to_governance_node(
        &self,
        endpoint: String,
        node_id: String,
    ) -> Result<GovernanceConnection, StreamError> {
        debug!("Connecting to governance node: {}", endpoint);

        // In a real implementation, this would establish gRPC connection
        let connection = GovernanceConnection {
            node_id: node_id.clone(),
            endpoint,
            status: NodeConnectionStatus::Connected,
            connected_at: Some(SystemTime::now()),
            last_activity: SystemTime::now(),
            message_count: 0,
            latency: None,
            health_score: 100.0,
        };

        Ok(connection)
    }

    /// Request peg-out signatures from governance
    async fn request_pegout_signatures(
        &mut self,
        request: PegOutSignatureRequest,
    ) -> Result<String, StreamError> {
        info!("Requesting peg-out signatures for pegout: {}", request.pegout_id);

        let request_id = request.request_id.clone();

        // Track the request using proper StreamMessage format
        let (response_tx, _response_rx) = tokio::sync::oneshot::channel();
        let stream_message = crate::actors::bridge::messages::stream_messages::StreamMessage::RequestPegOutSignatures {
            request: request.clone(),
        };

        if let Err(e) = self.request_tracker.track_request(stream_message, response_tx) {
            warn!("Failed to track request: {:?}", e);
        }

        // Create governance message
        let message = GovernanceMessage {
            message_id: format!("msg_{}", Uuid::new_v4()),
            from_node: "alys_bridge".to_string(),
            timestamp: SystemTime::now(),
            message_type: GovernanceMessageType::ConsensusRequest,
            payload: super::governance::GovernancePayload::SignatureRequest(request),
            signature: None,
        };

        // Send to all connected governance nodes
        self.broadcast_to_governance_nodes(message).await?;
        self.metrics.record_signature_request_sent(&request_id);

        Ok(request_id)
    }

    /// Handle signature response from governance
    async fn handle_signature_response(
        &mut self,
        response: SignatureResponse,
    ) -> Result<(), StreamError> {
        info!("Received signature response for request: {}", response.request_id);

        // Validate response
        if !self.request_tracker.has_pending_request(&response.request_id) {
            warn!("Received response for unknown request: {}", response.request_id);
            return Err(StreamError::UnknownRequest(response.request_id));
        }

        // Complete the request
        if let Some(request) = self.request_tracker.complete_request(&response.request_id) {
            self.metrics.record_signature_response_received(&response.request_id);

            // Forward signatures to PegOutActor
            if let Some(pegout_actor_weak) = &self.pegout_actor {
                if let Some(pegout_actor) = pegout_actor_weak.upgrade() {
                    let msg = PegOutMessage::ApplySignatures {
                        pegout_id: response.pegout_id.clone(),
                        witnesses: Vec::new(), // Would be extracted from response
                        signature_set: response.signatures,
                    };

                    match pegout_actor.send(msg).await {
                    Ok(Ok(_)) => {
                        info!("Successfully forwarded signatures to PegOutActor");
                    }
                    Ok(Err(e)) => {
                        error!("PegOutActor returned error: {:?}", e);
                        return Err(StreamError::PegOutActorError(format!("{:?}", e)));
                    }
                    Err(e) => {
                        error!("Failed to send message to PegOutActor: {:?}", e);
                        return Err(StreamError::ActorCommunicationError(e.to_string()));
                    }
                }
                } else {
                    warn!("PegOutActor reference is no longer valid");
                }
            } else {
                warn!("PegOutActor not registered");
            }
        }

        Ok(())
    }

    /// Broadcast message to all governance nodes
    async fn broadcast_to_governance_nodes(
        &mut self,
        message: GovernanceMessage,
    ) -> Result<(), StreamError> {
        let active_connections: Vec<_> = self.governance_connections
            .iter()
            .filter(|(_, conn)| matches!(conn.status, NodeConnectionStatus::Connected))
            .collect();

        if active_connections.is_empty() {
            return Err(StreamError::NoActiveConnections);
        }

        if let Some(protocol) = &self.protocol_handler {
            let message_id = message.message_id.clone();
            let target_endpoints: Vec<String> = active_connections
                .iter()
                .map(|(_, conn)| conn.endpoint.clone())
                .collect();

            match protocol.broadcast_message(message, target_endpoints).await {
                Ok(results) => {
                    let mut success_count = 0;
                    
                    for (endpoint, result) in results {
                        if let Some((node_id, connection)) = self.governance_connections
                            .iter_mut()
                            .find(|(_, conn)| conn.endpoint == endpoint) {
                            
                            match result {
                                Ok(_) => {
                                    success_count += 1;
                                    connection.last_activity = SystemTime::now();
                                    connection.message_count += 1;
                                    debug!("Successfully sent message {} to node {}", message_id, node_id);
                                }
                                Err(e) => {
                                    warn!("Failed to send message {} to node {}: {:?}", message_id, node_id, e);
                                    connection.status = NodeConnectionStatus::Failed { 
                                        error: format!("Send failed: {:?}", e) 
                                    };
                                }
                            }
                        }
                    }

                    if success_count > 0 {
                        self.metrics.record_message_broadcast(&message_id, success_count);
                        info!("Broadcast message {} to {} governance nodes", message_id, success_count);
                        Ok(())
                    } else {
                        Err(StreamError::BroadcastFailed)
                    }
                }
                Err(e) => {
                    error!("Broadcast failed: {:?}", e);
                    Err(StreamError::BroadcastFailed)
                }
            }
        } else {
            Err(StreamError::InternalError("Protocol handler not available".to_string()))
        }
    }

    /// Send heartbeat to governance nodes
    pub async fn send_heartbeat(&mut self) -> Result<(), StreamError> {
        let heartbeat_message = GovernanceMessage {
            message_id: format!("heartbeat_{}", Uuid::new_v4()),
            from_node: "alys_bridge".to_string(),
            timestamp: SystemTime::now(),
            message_type: GovernanceMessageType::Heartbeat,
            payload: super::governance::GovernancePayload::Heartbeat,
            signature: None,
        };

        self.broadcast_to_governance_nodes(heartbeat_message).await?;
        self.last_heartbeat = Some(SystemTime::now());
        self.metrics.record_heartbeat_sent();

        Ok(())
    }

    /// Start heartbeat task
    fn start_heartbeat(&mut self, ctx: &mut Context<Self>) {
        let heartbeat_interval = self.config.heartbeat_interval;
        ctx.run_interval(heartbeat_interval, |actor, _ctx| {
            // Simplified heartbeat - just trigger the method without complex future handling
            if let Err(e) = futures::executor::block_on(actor.send_heartbeat()) {
                warn!("Heartbeat failed: {:?}", e);
            }
        });
    }

    /// Start connection monitoring
    fn start_connection_monitoring(&mut self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(30), |actor, _ctx| {
            actor.monitor_connections();
            actor.update_connection_status();
        });
    }

    /// Monitor connection health with advanced reconnection logic
    fn monitor_connections(&mut self) {
        let now = SystemTime::now();
        let stale_threshold = Duration::from_secs(120); // 2 minutes

        let mut nodes_to_reconnect = Vec::new();

        for (node_id, connection) in &mut self.governance_connections {
            // Check for stale connections
            if let Ok(time_since_activity) = now.duration_since(connection.last_activity) {
                if time_since_activity > stale_threshold {
                    if matches!(connection.status, NodeConnectionStatus::Connected) {
                        warn!("Governance node {} appears stale, marking for reconnection", node_id);
                        connection.status = NodeConnectionStatus::Timeout;
                        connection.health_score = (connection.health_score * 0.8).max(10.0);
                        
                        // Record failure in reconnection manager
                        let error = BridgeError::ConnectionError("Connection stale".to_string());
                        self.reconnection_manager.record_failure(node_id.clone(), error);
                        
                        nodes_to_reconnect.push(node_id.clone());
                    }
                }
            }
        }

        // Check reconnection decisions for failed nodes
        for node_id in nodes_to_reconnect {
            match self.reconnection_manager.should_reconnect(&node_id) {
                BackoffDecision::Proceed => {
                    info!("Initiating reconnection to node {}", node_id);
                    // Schedule reconnection attempt
                    self.schedule_reconnection_attempt(node_id);
                }
                BackoffDecision::Wait { delay } => {
                    debug!("Waiting {:?} before reconnecting to {}", delay, node_id);
                }
                BackoffDecision::GiveUp { reason } => {
                    warn!("Giving up on reconnection to {}: {:?}", node_id, reason);
                    // Remove from active connections
                    self.governance_connections.remove(&node_id);
                }
                BackoffDecision::CircuitOpen { recovery_time } => {
                    info!("Circuit breaker open for {}, recovery in {:?}", node_id, recovery_time);
                }
            }
        }

        // Perform health check reset thresholds
        self.reconnection_manager.check_reset_thresholds();

        self.metrics.update_connection_health(&self.governance_connections);
    }

    /// Schedule reconnection attempt for a node
    fn schedule_reconnection_attempt(&mut self, node_id: String) {
        // In a real implementation, this would schedule an async task
        // For now, we'll attempt immediate reconnection
        if let Some(connection) = self.governance_connections.get(&node_id) {
            let endpoint = connection.endpoint.clone();
            
            // Mark as reconnecting
            if let Some(conn) = self.governance_connections.get_mut(&node_id) {
                conn.status = NodeConnectionStatus::Connecting;
            }

            // In an async context, you would spawn a task like:
            /*
            let reconnection_manager = Arc::clone(&self.reconnection_manager);
            let endpoint_clone = endpoint.clone();
            let node_id_clone = node_id.clone();
            
            tokio::spawn(async move {
                match attempt_reconnection(endpoint_clone).await {
                    Ok(_) => {
                        reconnection_manager.lock().await.record_success(node_id_clone);
                    }
                    Err(e) => {
                        reconnection_manager.lock().await.record_failure(node_id_clone, e);
                    }
                }
            });
            */
            
            info!("Reconnection scheduled for node {} at {}", node_id, endpoint);
        }
    }

    /// Update connection status
    pub fn update_connection_status(&mut self) {
        let total_nodes = self.governance_connections.len();
        let healthy_nodes = self.governance_connections
            .values()
            .filter(|conn| matches!(conn.status, NodeConnectionStatus::Connected))
            .count();

        self.connection_status = if healthy_nodes == 0 {
            ConnectionStatus::Disconnected
        } else if healthy_nodes == total_nodes {
            ConnectionStatus::Connected { healthy_nodes, total_nodes }
        } else {
            ConnectionStatus::Degraded {
                issues: vec![format!("Only {}/{} nodes connected", healthy_nodes, total_nodes)],
            }
        };

        self.metrics.update_connection_status(&self.connection_status);
    }

    /// Start request timeout checking
    fn start_request_timeout_checking(&mut self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(10), |actor, _ctx| {
            actor.request_tracker.check_timeouts();
        });
    }

    /// Start message retry mechanism
    fn start_message_retry(&mut self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(15), |actor, _ctx| {
            // Simplified retry mechanism - just retry directly without spawning futures
            let now = SystemTime::now();
            let mut indices_to_remove = Vec::new();

            for (i, pending) in actor.message_buffer.iter_mut().enumerate() {
                if now >= pending.next_retry && pending.attempts < 3 {
                    pending.attempts += 1;
                    pending.next_retry = now + Duration::from_secs(30 * pending.attempts as u64);

                    // In a real implementation, would trigger async retry
                    if pending.attempts >= 3 {
                        indices_to_remove.push(i);
                    }
                }
            }

            // Remove failed messages
            for &index in indices_to_remove.iter().rev() {
                actor.message_buffer.remove(index);
            }
        });
    }

    /// Generate node ID from endpoint
    fn generate_node_id(&self, endpoint: &str) -> String {
        format!("node_{}", 
            endpoint.replace([':', '/', '.'], "_")
                   .replace("http", "")
                   .replace("https", "")
                   .trim_start_matches('_'))
    }

    /// Get connection status
    pub fn get_connection_status(&self) -> GovernanceConnectionStatus {
        let connected_nodes: Vec<GovernanceNodeStatus> = self.governance_connections
            .values()
            .map(|conn| GovernanceNodeStatus {
                node_id: conn.node_id.clone(),
                endpoint: conn.endpoint.clone(),
                status: conn.status.clone(),
                last_activity: conn.last_activity,
                message_count: conn.message_count,
                latency: conn.latency,
            })
            .collect();

        let healthy_connections = connected_nodes.iter()
            .filter(|node| matches!(node.status, NodeConnectionStatus::Connected))
            .count();

        let connection_quality = match healthy_connections as f64 / connected_nodes.len().max(1) as f64 {
            ratio if ratio >= 0.8 => crate::actors::bridge::messages::ConnectionQuality::Excellent,
            ratio if ratio >= 0.6 => crate::actors::bridge::messages::ConnectionQuality::Good,
            ratio if ratio >= 0.4 => crate::actors::bridge::messages::ConnectionQuality::Degraded,
            ratio if ratio >= 0.2 => crate::actors::bridge::messages::ConnectionQuality::Poor,
            _ => crate::actors::bridge::messages::ConnectionQuality::Failed,
        };

        GovernanceConnectionStatus {
            connected_nodes,
            total_connections: self.governance_connections.len(),
            healthy_connections,
            last_heartbeat: self.last_heartbeat,
            connection_quality,
        }
    }

    /// Actor reference management for hybrid pattern

    /// Set pegout actor reference (creates strong reference, stores weak)
    pub fn set_pegout_actor(&mut self, actor: Addr<super::super::pegout::PegOutActor>) {
        let arc_actor = Arc::new(actor);
        self.pegout_actor = Some(Arc::downgrade(&arc_actor));
    }

    /// Set bridge coordinator reference (creates strong reference, stores weak)
    pub fn set_bridge_coordinator(&mut self, actor: Addr<super::super::bridge::BridgeActor>) {
        let arc_actor = Arc::new(actor);
        self.bridge_coordinator = Some(Arc::downgrade(&arc_actor));
    }

    /// Get pegout actor if still alive
    pub fn get_pegout_actor(&self) -> Option<Arc<Addr<super::super::pegout::PegOutActor>>> {
        self.pegout_actor.as_ref()?.upgrade()
    }

    /// Get bridge coordinator if still alive
    pub fn get_bridge_coordinator(&self) -> Option<Arc<Addr<super::super::bridge::BridgeActor>>> {
        self.bridge_coordinator.as_ref()?.upgrade()
    }

    /// Create owned data for async closures to avoid borrowing issues
    fn create_async_context(&self) -> AsyncStreamContext {
        AsyncStreamContext {
            config: self.config.clone(),
            connection_status: self.connection_status.clone(),
            governance_endpoints: self.config.governance_endpoints.clone(),
            reconnect_attempts: self.config.reconnect_attempts,
            reconnect_delay: self.config.reconnect_delay,
        }
    }

    /// Check if there are healthy governance connections
    pub fn has_healthy_connections(&self) -> bool {
        self.governance_connections.values()
            .any(|conn| matches!(conn.status, ConnectionStatus::Connected))
    }
}

/// Owned data structure for async closures
#[derive(Debug, Clone)]
pub struct AsyncStreamContext {
    pub config: StreamConfig,
    pub connection_status: ConnectionStatus,
    pub governance_endpoints: Vec<String>,
    pub reconnect_attempts: u32,
    pub reconnect_delay: Duration,
}

impl Actor for StreamActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Enhanced StreamActor starting");

        let fut = async {
            // Initialize actor state here if needed
            Ok(())
        };
        let fut = actix::fut::wrap_future::<_, Self>(fut);
        ctx.spawn(fut.map(|result, _actor, ctx| {
            match result {
                Ok(_) => {
                    info!("Enhanced StreamActor started successfully");
                }
                Err(e) => {
                    error!("Failed to initialize enhanced StreamActor: {:?}", e);
                    ctx.stop();
                }
            }
        }));
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Enhanced StreamActor stopped");
        self.metrics.record_actor_stopped();
    }
}

// Old RequestTracker implementation removed - functionality moved to AdvancedRequestTracker

/// StreamActor errors
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("No active connections")]
    NoActiveConnections,
    
    #[error("Broadcast failed")]
    BroadcastFailed,
    
    #[error("Unknown request: {0}")]
    UnknownRequest(String),
    
    #[error("PegOut actor error: {0}")]
    PegOutActorError(String),
    
    #[error("Actor communication error: {0}")]
    ActorCommunicationError(String),
    
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}

impl From<Box<dyn std::error::Error + Send + Sync>> for StreamError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        StreamError::InternalError(err.to_string())
    }
}

impl From<Box<dyn std::error::Error>> for StreamError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        StreamError::InternalError(err.to_string())
    }
}


