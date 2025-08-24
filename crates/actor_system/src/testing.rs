//! Testing utilities and harnesses for V2 actor system
//!
//! This module provides comprehensive testing infrastructure for the V2 actor system,
//! including mock services, test harnesses, and integration test utilities.

use crate::{
    error::{ActorError, ActorResult},
    metrics::{MetricsCollector, MetricsSnapshot},
    Actor, ActorContext, AsyncContext, Context, Handler, Message, ResponseFuture,
};
use actix::{dev::ToEnvelope, prelude::*};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Test environment for actor testing
#[derive(Debug, Default)]
pub struct TestEnvironment {
    /// Test instance ID
    pub test_id: String,
    /// Test start time
    pub start_time: Instant,
    /// Test configuration
    pub config: TestConfig,
}

/// Configuration for actor testing
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// Enable verbose logging during tests
    pub verbose_logging: bool,
    /// Test timeout duration
    pub test_timeout: Duration,
    /// Maximum actors for stress testing
    pub max_test_actors: usize,
    /// Mock server ports range
    pub mock_port_range: (u16, u16),
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            verbose_logging: false,
            test_timeout: Duration::from_secs(30),
            max_test_actors: 100,
            mock_port_range: (50000, 50100),
        }
    }
}

impl TestEnvironment {
    pub fn new() -> Self {
        Self {
            test_id: Uuid::new_v4().to_string(),
            start_time: Instant::now(),
            config: TestConfig::default(),
        }
    }

    pub fn with_config(config: TestConfig) -> Self {
        Self {
            test_id: Uuid::new_v4().to_string(),
            start_time: Instant::now(),
            config,
        }
    }
}

/// Mock governance server for testing StreamActor gRPC communication
#[derive(Debug)]
pub struct MockGovernanceServer {
    /// Server address
    pub address: String,
    /// Server state
    state: Arc<RwLock<MockServerState>>,
    /// Connection tracking
    connections: Arc<RwLock<HashMap<String, MockConnection>>>,
    /// Message history
    message_history: Arc<RwLock<Vec<MockMessage>>>,
    /// Server metrics
    metrics: Arc<RwLock<MockServerMetrics>>,
}

/// Mock server internal state
#[derive(Debug, Default)]
struct MockServerState {
    running: bool,
    connected_clients: usize,
    message_count: u64,
    last_heartbeat: Option<SystemTime>,
}

/// Mock connection information
#[derive(Debug, Clone)]
struct MockConnection {
    id: String,
    client_address: String,
    connected_at: SystemTime,
    last_activity: SystemTime,
    authenticated: bool,
    stream_active: bool,
}

/// Mock message for testing
#[derive(Debug, Clone)]
pub struct MockMessage {
    pub id: String,
    pub message_type: String,
    pub payload: serde_json::Value,
    pub timestamp: SystemTime,
    pub connection_id: String,
}

/// Mock server metrics
#[derive(Debug, Default)]
struct MockServerMetrics {
    connections_accepted: u64,
    messages_received: u64,
    messages_sent: u64,
    authentication_attempts: u64,
    stream_sessions: u64,
}

impl MockGovernanceServer {
    /// Create new mock governance server
    pub fn new(port: u16) -> Self {
        Self {
            address: format!("127.0.0.1:{}", port),
            state: Arc::new(RwLock::new(MockServerState::default())),
            connections: Arc::new(RwLock::new(HashMap::new())),
            message_history: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(RwLock::new(MockServerMetrics::default())),
        }
    }

    /// Start the mock server
    pub async fn start(&self) -> ActorResult<()> {
        let mut state = self.state.write().await;
        if state.running {
            return Err(ActorError::InvalidOperation {
                operation: "start".to_string(),
                reason: "Server already running".to_string(),
            });
        }

        state.running = true;
        info!("Mock governance server started on {}", self.address);
        Ok(())
    }

    /// Stop the mock server
    pub async fn stop(&self) -> ActorResult<()> {
        let mut state = self.state.write().await;
        state.running = false;
        info!("Mock governance server stopped");
        Ok(())
    }

    /// Simulate client connection
    pub async fn simulate_connection(&self, client_id: String) -> ActorResult<()> {
        let connection = MockConnection {
            id: client_id.clone(),
            client_address: "127.0.0.1:12345".to_string(),
            connected_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            authenticated: false,
            stream_active: false,
        };

        let mut connections = self.connections.write().await;
        connections.insert(client_id.clone(), connection);

        let mut state = self.state.write().await;
        state.connected_clients = connections.len();

        let mut metrics = self.metrics.write().await;
        metrics.connections_accepted += 1;

        debug!("Simulated connection for client: {}", client_id);
        Ok(())
    }

    /// Simulate client authentication
    pub async fn simulate_authentication(&self, client_id: String) -> ActorResult<()> {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.get_mut(&client_id) {
            connection.authenticated = true;
            connection.last_activity = SystemTime::now();

            let mut metrics = self.metrics.write().await;
            metrics.authentication_attempts += 1;

            debug!("Simulated authentication for client: {}", client_id);
            Ok(())
        } else {
            Err(ActorError::NotFound {
                resource: "client connection".to_string(),
                id: client_id,
            })
        }
    }

    /// Simulate starting bi-directional stream
    pub async fn simulate_stream_start(&self, client_id: String) -> ActorResult<()> {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.get_mut(&client_id) {
            if !connection.authenticated {
                return Err(ActorError::PermissionDenied {
                    resource: "stream".to_string(),
                    reason: "Client not authenticated".to_string(),
                });
            }

            connection.stream_active = true;
            connection.last_activity = SystemTime::now();

            let mut metrics = self.metrics.write().await;
            metrics.stream_sessions += 1;

            debug!("Simulated stream start for client: {}", client_id);
            Ok(())
        } else {
            Err(ActorError::NotFound {
                resource: "client connection".to_string(),
                id: client_id,
            })
        }
    }

    /// Simulate receiving message
    pub async fn simulate_receive_message(
        &self,
        client_id: String,
        message_type: String,
        payload: serde_json::Value,
    ) -> ActorResult<()> {
        let message = MockMessage {
            id: Uuid::new_v4().to_string(),
            message_type,
            payload,
            timestamp: SystemTime::now(),
            connection_id: client_id.clone(),
        };

        let mut message_history = self.message_history.write().await;
        message_history.push(message);

        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.get_mut(&client_id) {
            connection.last_activity = SystemTime::now();
        }

        let mut state = self.state.write().await;
        state.message_count += 1;

        let mut metrics = self.metrics.write().await;
        metrics.messages_received += 1;

        debug!("Simulated message received from client: {}", client_id);
        Ok(())
    }

    /// Simulate sending heartbeat
    pub async fn simulate_heartbeat(&self) -> ActorResult<()> {
        let mut state = self.state.write().await;
        state.last_heartbeat = Some(SystemTime::now());

        debug!("Simulated heartbeat sent");
        Ok(())
    }

    /// Get server metrics
    pub async fn get_metrics(&self) -> MockServerMetrics {
        let metrics = self.metrics.read().await;
        MockServerMetrics {
            connections_accepted: metrics.connections_accepted,
            messages_received: metrics.messages_received,
            messages_sent: metrics.messages_sent,
            authentication_attempts: metrics.authentication_attempts,
            stream_sessions: metrics.stream_sessions,
        }
    }

    /// Get message history
    pub async fn get_message_history(&self) -> Vec<MockMessage> {
        let history = self.message_history.read().await;
        history.clone()
    }

    /// Check if server is running
    pub async fn is_running(&self) -> bool {
        let state = self.state.read().await;
        state.running
    }
}

/// Test harness for V2 actors
#[derive(Debug)]
pub struct ActorTestHarness {
    /// Test environment
    pub env: TestEnvironment,
    /// Mock governance servers
    mock_servers: HashMap<String, MockGovernanceServer>,
    /// Test metrics collector
    metrics_collector: Option<Arc<MetricsCollector>>,
    /// Test supervision hierarchy
    test_supervisors: HashMap<String, Addr<TestSupervisor>>,
}

impl ActorTestHarness {
    /// Create new test harness
    pub async fn new() -> Self {
        Self {
            env: TestEnvironment::new(),
            mock_servers: HashMap::new(),
            metrics_collector: None,
            test_supervisors: HashMap::new(),
        }
    }

    /// Create test harness with custom environment
    pub async fn with_environment(env: TestEnvironment) -> Self {
        Self {
            env,
            mock_servers: HashMap::new(),
            metrics_collector: None,
            test_supervisors: HashMap::new(),
        }
    }

    /// Create mock governance server
    pub async fn create_mock_governance_server(&mut self, name: String) -> ActorResult<&MockGovernanceServer> {
        let port = self.allocate_mock_port()?;
        let server = MockGovernanceServer::new(port);
        server.start().await?;
        
        self.mock_servers.insert(name.clone(), server);
        Ok(self.mock_servers.get(&name).unwrap())
    }

    /// Create test supervisor
    pub async fn create_test_supervisor(&mut self) -> Addr<TestSupervisor> {
        let supervisor = TestSupervisor::new();
        let addr = supervisor.start();
        
        let supervisor_id = Uuid::new_v4().to_string();
        self.test_supervisors.insert(supervisor_id, addr.clone());
        
        addr
    }

    /// Initialize metrics collector for testing
    pub fn with_metrics_collector(&mut self, collector: Arc<MetricsCollector>) {
        self.metrics_collector = Some(collector);
    }

    /// Allocate port for mock server
    fn allocate_mock_port(&self) -> ActorResult<u16> {
        let range = self.env.config.mock_port_range;
        for port in range.0..=range.1 {
            // Simple port allocation - in real implementation would check availability
            if !self.mock_servers.values().any(|s| s.address.contains(&port.to_string())) {
                return Ok(port);
            }
        }
        
        Err(ActorError::ResourceExhausted {
            resource: "mock server ports".to_string(),
            details: "All ports in range are allocated".to_string(),
        })
    }

    /// Get mock server by name
    pub fn get_mock_server(&self, name: &str) -> Option<&MockGovernanceServer> {
        self.mock_servers.get(name)
    }

    /// Clean up test resources
    pub async fn cleanup(&mut self) -> ActorResult<()> {
        // Stop all mock servers
        for (_, server) in &self.mock_servers {
            server.stop().await?;
        }
        self.mock_servers.clear();

        // Clean up test supervisors
        self.test_supervisors.clear();

        info!("Test harness cleanup completed for test {}", self.env.test_id);
        Ok(())
    }
}

/// Test supervisor for supervision tree testing
#[derive(Debug)]
pub struct TestSupervisor {
    supervised_actors: HashMap<String, String>, // Store actor IDs instead of actual addresses
    restart_count: u32,
    failure_count: u32,
    supervision_strategy: SupervisionStrategy,
}

/// Supervision strategy for testing
#[derive(Debug, Clone)]
pub enum SupervisionStrategy {
    OneForOne,
    OneForAll,
    RestForOne,
    Custom(String),
}

impl TestSupervisor {
    pub fn new() -> Self {
        Self {
            supervised_actors: HashMap::new(),
            restart_count: 0,
            failure_count: 0,
            supervision_strategy: SupervisionStrategy::OneForOne,
        }
    }

    pub fn with_strategy(strategy: SupervisionStrategy) -> Self {
        Self {
            supervised_actors: HashMap::new(),
            restart_count: 0,
            failure_count: 0,
            supervision_strategy: strategy,
        }
    }
}

impl Actor for TestSupervisor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("Test supervisor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Test supervisor stopped");
    }
}

/// Messages for test supervisor
#[derive(Debug, Message)]
#[rtype(result = "ActorResult<()>")]
pub struct SuperviseActor {
    pub actor_id: String,
    pub actor_type: String, // Just store the type name for tracking
}

#[derive(Debug, Message)]
#[rtype(result = "ActorResult<SupervisionStats>")]
pub struct GetSupervisionStats;

#[derive(Debug)]
pub struct SupervisionStats {
    pub supervised_count: usize,
    pub restart_count: u32,
    pub failure_count: u32,
    pub strategy: SupervisionStrategy,
}

impl Handler<SuperviseActor> for TestSupervisor {
    type Result = ResponseFuture<ActorResult<()>>;

    fn handle(&mut self, msg: SuperviseActor, _ctx: &mut Self::Context) -> Self::Result {
        self.supervised_actors.insert(msg.actor_id.clone(), msg.actor_type);
        debug!("Supervising actor: {}", msg.actor_id);
        
        Box::pin(async move { Ok(()) })
    }
}

impl Handler<GetSupervisionStats> for TestSupervisor {
    type Result = ResponseFuture<ActorResult<SupervisionStats>>;

    fn handle(&mut self, _msg: GetSupervisionStats, _ctx: &mut Self::Context) -> Self::Result {
        let stats = SupervisionStats {
            supervised_count: self.supervised_actors.len(),
            restart_count: self.restart_count,
            failure_count: self.failure_count,
            strategy: self.supervision_strategy.clone(),
        };

        Box::pin(async move { Ok(stats) })
    }
}

/// Test utilities
pub struct TestUtil;

impl TestUtil {
    /// Wait for condition with timeout
    pub async fn wait_for_condition<F, Fut>(
        condition: F,
        timeout: Duration,
        check_interval: Duration,
    ) -> ActorResult<()>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let start = Instant::now();
        
        while start.elapsed() < timeout {
            if condition().await {
                return Ok(());
            }
            tokio::time::sleep(check_interval).await;
        }
        
        Err(ActorError::Timeout {
            operation: "wait_for_condition".to_string(),
            duration: timeout,
        })
    }

    /// Create test metrics snapshot
    pub fn create_test_metrics_snapshot() -> MetricsSnapshot {
        MetricsSnapshot {
            enabled: true,
            messages_processed: 42,
            messages_failed: 1,
            avg_processing_time: Duration::from_millis(10),
            mailbox_size: 5,
            restarts: 0,
            state_transitions: 3,
            last_activity: SystemTime::now(),
            peak_memory_usage: 1024 * 1024, // 1MB
            total_cpu_time: Duration::from_secs(5),
            error_counts: HashMap::new(),
            custom_counters: HashMap::new(),
            custom_gauges: HashMap::new(),
        }
    }

    /// Assert metrics within expected ranges
    pub fn assert_metrics_valid(snapshot: &MetricsSnapshot) -> ActorResult<()> {
        if !snapshot.enabled {
            return Err(ActorError::ValidationFailed {
                field: "enabled".to_string(),
                reason: "Metrics should be enabled".to_string(),
            });
        }

        if snapshot.messages_processed == 0 && snapshot.messages_failed > 0 {
            return Err(ActorError::ValidationFailed {
                field: "message_counts".to_string(),
                reason: "Cannot have failed messages without processed messages".to_string(),
            });
        }

        if snapshot.avg_processing_time > Duration::from_secs(10) {
            warn!("High average processing time: {:?}", snapshot.avg_processing_time);
        }

        Ok(())
    }

    /// Generate test load for performance testing
    pub async fn generate_test_load<A, M>(
        actor: &Addr<A>,
        message_factory: impl Fn(usize) -> M,
        message_count: usize,
        rate_per_second: u32,
    ) -> ActorResult<Duration>
    where
        A: Actor + Handler<M>,
        M: Message + Send + 'static,
        A::Context: ToEnvelope<A, M>,
    {
        let start_time = Instant::now();
        let interval = Duration::from_millis(1000 / rate_per_second as u64);

        for i in 0..message_count {
            let message = message_factory(i);
            actor.do_send(message);
            
            if i < message_count - 1 {
                tokio::time::sleep(interval).await;
            }
        }

        Ok(start_time.elapsed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_environment_creation() {
        let env = TestEnvironment::new();
        assert!(!env.test_id.is_empty());
        assert!(env.start_time.elapsed().as_millis() < 100);
    }

    #[tokio::test]
    async fn test_mock_governance_server() {
        let server = MockGovernanceServer::new(50051);
        assert!(server.start().await.is_ok());
        assert!(server.is_running().await);
        
        let client_id = "test_client".to_string();
        assert!(server.simulate_connection(client_id.clone()).await.is_ok());
        assert!(server.simulate_authentication(client_id.clone()).await.is_ok());
        assert!(server.simulate_stream_start(client_id.clone()).await.is_ok());
        
        let payload = serde_json::json!({"test": "data"});
        assert!(server.simulate_receive_message(client_id, "test_message".to_string(), payload).await.is_ok());
        
        let metrics = server.get_metrics().await;
        assert_eq!(metrics.connections_accepted, 1);
        assert_eq!(metrics.messages_received, 1);
        assert_eq!(metrics.authentication_attempts, 1);
        assert_eq!(metrics.stream_sessions, 1);
        
        assert!(server.stop().await.is_ok());
    }

    #[tokio::test]
    async fn test_actor_test_harness() {
        let mut harness = ActorTestHarness::new().await;
        
        // Test mock server creation
        assert!(harness.create_mock_governance_server("test_server".to_string()).await.is_ok());
        assert!(harness.get_mock_server("test_server").is_some());
        
        // Test supervisor creation
        let supervisor = harness.create_test_supervisor().await;
        assert!(supervisor.connected());
        
        // Test cleanup
        assert!(harness.cleanup().await.is_ok());
    }

    #[tokio::test]
    async fn test_supervision_stats() {
        let supervisor = TestSupervisor::new();
        let addr = supervisor.start();
        
        let stats = addr.send(GetSupervisionStats).await.unwrap().unwrap();
        assert_eq!(stats.supervised_count, 0);
        assert_eq!(stats.restart_count, 0);
        assert_eq!(stats.failure_count, 0);
    }

    #[tokio::test]
    async fn test_util_wait_for_condition() {
        let mut counter = 0;
        let condition = || async {
            counter += 1;
            counter >= 3
        };
        
        let result = TestUtil::wait_for_condition(
            condition,
            Duration::from_secs(1),
            Duration::from_millis(10),
        ).await;
        
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_metrics_validation() {
        let snapshot = TestUtil::create_test_metrics_snapshot();
        assert!(TestUtil::assert_metrics_valid(&snapshot).is_ok());
        
        // Test invalid case
        let invalid_snapshot = MetricsSnapshot {
            enabled: false,
            ..TestUtil::create_test_metrics_snapshot()
        };
        assert!(TestUtil::assert_metrics_valid(&invalid_snapshot).is_err());
    }
}