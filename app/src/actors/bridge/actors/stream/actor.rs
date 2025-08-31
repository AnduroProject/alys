//! Enhanced StreamActor for Bridge Integration
//! 
//! Bridge-optimized version of StreamActor for governance communication

use actix::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use crate::actors::bridge::{
    config::StreamConfig,
    messages::*,
    shared::*,
};
use crate::types::*;
use super::{governance::*, reconnection::*, metrics::*};

/// Enhanced StreamActor for bridge operations
pub struct StreamActor {
    /// Configuration
    config: StreamConfig,
    
    /// Governance connections
    governance_connections: HashMap<String, GovernanceConnection>,
    
    /// Message handling
    message_buffer: Vec<PendingMessage>,
    request_tracker: RequestTracker,
    
    /// Bridge actor integration
    pegout_actor: Option<Addr<super::super::pegout::PegOutActor>>,
    bridge_coordinator: Option<Addr<super::super::bridge::BridgeActor>>,
    
    /// Connection management
    reconnection_manager: ReconnectionManager,
    
    /// Metrics and monitoring
    metrics: StreamMetrics,
    
    /// State management
    connection_status: ConnectionStatus,
    last_heartbeat: Option<SystemTime>,
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

/// Request tracker for correlation
#[derive(Debug)]
pub struct RequestTracker {
    pending_requests: HashMap<String, PendingRequest>,
    request_timeouts: Vec<(String, SystemTime)>,
}

/// Pending request tracking
#[derive(Debug, Clone)]
pub struct PendingRequest {
    pub request_id: String,
    pub request_type: RequestType,
    pub pegout_id: Option<String>,
    pub created_at: SystemTime,
    pub timeout: SystemTime,
    pub retry_count: u32,
}

/// Request types
#[derive(Debug, Clone)]
pub enum RequestType {
    PegOutSignature,
    FederationUpdate,
    Heartbeat,
}

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
        
        Ok(Self {
            config,
            governance_connections: HashMap::new(),
            message_buffer: Vec::new(),
            request_tracker: RequestTracker::new(),
            pegout_actor: None,
            bridge_coordinator: None,
            reconnection_manager,
            metrics,
            connection_status: ConnectionStatus::Disconnected,
            last_heartbeat: None,
        })
    }

    /// Initialize StreamActor
    async fn initialize(&mut self, ctx: &mut Context<Self>) -> Result<(), StreamError> {
        info!("Initializing enhanced StreamActor for bridge operations");

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

    /// Establish connections to governance nodes
    async fn establish_governance_connections(&mut self) -> Result<(), StreamError> {
        info!("Establishing connections to {} governance nodes", self.config.governance_endpoints.len());

        for endpoint in &self.config.governance_endpoints {
            let node_id = self.generate_node_id(endpoint);
            
            match self.connect_to_governance_node(endpoint.clone(), node_id.clone()).await {
                Ok(connection) => {
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

        // Track the request
        self.request_tracker.track_request(PendingRequest {
            request_id: request_id.clone(),
            request_type: RequestType::PegOutSignature,
            pegout_id: Some(request.pegout_id.clone()),
            created_at: SystemTime::now(),
            timeout: SystemTime::now() + request.timeout,
            retry_count: 0,
        });

        // Create governance message
        let message = GovernanceMessage {
            message_id: format!("msg_{}", Uuid::new_v4()),
            from_node: "alys_bridge".to_string(),
            timestamp: SystemTime::now(),
            message_type: GovernanceMessageType::ConsensusRequest,
            payload: GovernancePayload::SignatureRequest(request),
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
            if let (Some(pegout_actor), Some(pegout_id)) = (&self.pegout_actor, &request.pegout_id) {
                let msg = PegOutMessage::ApplySignatures {
                    pegout_id: pegout_id.clone(),
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
                warn!("PegOutActor not registered or pegout_id missing");
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

        let mut success_count = 0;
        let message_id = message.message_id.clone();

        for (node_id, _connection) in active_connections {
            // In a real implementation, this would send via gRPC
            debug!("Sending message {} to governance node {}", message_id, node_id);
            
            // Simulate successful send
            success_count += 1;
            
            // Update connection activity
            if let Some(connection) = self.governance_connections.get_mut(node_id) {
                connection.last_activity = SystemTime::now();
                connection.message_count += 1;
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

    /// Send heartbeat to governance nodes
    async fn send_heartbeat(&mut self) -> Result<(), StreamError> {
        let heartbeat_message = GovernanceMessage {
            message_id: format!("heartbeat_{}", Uuid::new_v4()),
            from_node: "alys_bridge".to_string(),
            timestamp: SystemTime::now(),
            message_type: GovernanceMessageType::Heartbeat,
            payload: GovernancePayload::Heartbeat,
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
            let fut = actor.send_heartbeat();
            let fut = actix::fut::wrap_future::<_, Self>(fut);
            ctx.spawn(fut.map(|result, actor, _ctx| {
                if let Err(e) = result {
                    warn!("Heartbeat failed: {:?}", e);
                    actor.metrics.record_heartbeat_failed();
                }
            }));
        });
    }

    /// Start connection monitoring
    fn start_connection_monitoring(&mut self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(30), |actor, _ctx| {
            actor.monitor_connections();
            actor.update_connection_status();
        });
    }

    /// Monitor connection health
    fn monitor_connections(&mut self) {
        let now = SystemTime::now();
        let stale_threshold = Duration::from_secs(120); // 2 minutes

        for (node_id, connection) in &mut self.governance_connections {
            // Check for stale connections
            if let Ok(time_since_activity) = now.duration_since(connection.last_activity) {
                if time_since_activity > stale_threshold {
                    if matches!(connection.status, NodeConnectionStatus::Connected) {
                        warn!("Governance node {} appears stale", node_id);
                        connection.status = NodeConnectionStatus::Timeout;
                        connection.health_score = (connection.health_score * 0.8).max(10.0);
                    }
                }
            }
        }

        self.metrics.update_connection_health(&self.governance_connections);
    }

    /// Update connection status
    fn update_connection_status(&mut self) {
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
            // Retry failed messages
            let now = SystemTime::now();
            let mut messages_to_retry = Vec::new();

            for (i, pending) in actor.message_buffer.iter().enumerate() {
                if now >= pending.next_retry && pending.attempts < 3 {
                    messages_to_retry.push(i);
                }
            }

            // Process retries
            for &index in messages_to_retry.iter().rev() {
                if let Some(mut pending) = actor.message_buffer.get(index).cloned() {
                    pending.attempts += 1;
                    pending.next_retry = now + Duration::from_secs(30 * pending.attempts as u64);
                    
                    let fut = actor.broadcast_to_governance_nodes(pending.message.clone());
                    let fut = actix::fut::wrap_future::<_, Self>(fut);
                    ctx.spawn(fut.map(move |result, actor, _ctx| {
                        if result.is_ok() {
                            actor.message_buffer.remove(index);
                        } else {
                            actor.message_buffer[index] = pending;
                        }
                    }));
                }
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
            ratio if ratio >= 0.8 => ConnectionQuality::Excellent,
            ratio if ratio >= 0.6 => ConnectionQuality::Good,
            ratio if ratio >= 0.4 => ConnectionQuality::Degraded,
            ratio if ratio >= 0.2 => ConnectionQuality::Poor,
            _ => ConnectionQuality::Failed,
        };

        GovernanceConnectionStatus {
            connected_nodes,
            total_connections: self.governance_connections.len(),
            healthy_connections,
            last_heartbeat: self.last_heartbeat,
            connection_quality,
        }
    }
}

impl Actor for StreamActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Enhanced StreamActor starting");
        
        let fut = self.initialize(ctx);
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

impl RequestTracker {
    pub fn new() -> Self {
        Self {
            pending_requests: HashMap::new(),
            request_timeouts: Vec::new(),
        }
    }

    pub fn track_request(&mut self, request: PendingRequest) {
        self.request_timeouts.push((request.request_id.clone(), request.timeout));
        self.pending_requests.insert(request.request_id.clone(), request);
    }

    pub fn has_pending_request(&self, request_id: &str) -> bool {
        self.pending_requests.contains_key(request_id)
    }

    pub fn complete_request(&mut self, request_id: &str) -> Option<PendingRequest> {
        self.pending_requests.remove(request_id)
    }

    pub fn check_timeouts(&mut self) {
        let now = SystemTime::now();
        let mut timed_out = Vec::new();

        for (request_id, timeout) in &self.request_timeouts {
            if now >= *timeout {
                timed_out.push(request_id.clone());
            }
        }

        for request_id in timed_out {
            if let Some(_request) = self.pending_requests.remove(&request_id) {
                warn!("Request {} timed out", request_id);
            }
            self.request_timeouts.retain(|(id, _)| id != &request_id);
        }
    }
}

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