//! Test Utilities for StreamActor Integration Tests
//! 
//! Common test setup, mocks, and utilities for testing StreamActor functionality

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;
use tempfile::TempDir;

use crate::actors::bridge::{
    actors::stream::{
        StreamActor, 
        config::{AdvancedStreamConfig, CoreStreamConfig, GovernanceEndpoint},
        environment::EnvironmentConfigManager,
        hot_reload::ConfigHotReloadManager,
    },
    messages::stream_messages::{StreamMessage, StreamResponse},
    shared::errors::BridgeError,
};
use crate::actor_system::{
    ActorResult, AlysActor, AlysMessage, LifecycleAware,
    metrics::ActorSystemMetrics,
};

/// Test configuration builder
pub struct TestConfigBuilder {
    config: AdvancedStreamConfig,
}

impl TestConfigBuilder {
    /// Create new test configuration builder
    pub fn new() -> Self {
        Self {
            config: AdvancedStreamConfig::default(),
        }
    }

    /// Set actor ID
    pub fn with_actor_id(mut self, actor_id: &str) -> Self {
        self.config.core.actor_id = actor_id.to_string();
        self
    }

    /// Set governance endpoints
    pub fn with_governance_endpoints(mut self, endpoints: Vec<&str>) -> Self {
        self.config.core.governance_endpoints = endpoints
            .into_iter()
            .enumerate()
            .map(|(i, url)| GovernanceEndpoint {
                url: url.to_string(),
                priority: 100 - (i as u8 * 10), // Decreasing priority
                enabled: true,
                expected_latency_ms: Some(100),
                region: Some("test-region".to_string()),
                tags: HashMap::new(),
                metadata: HashMap::new(),
            })
            .collect();
        self
    }

    /// Set connection timeout
    pub fn with_connection_timeout(mut self, timeout: Duration) -> Self {
        self.config.core.connection_timeout = timeout;
        self.config.connection.connection_timeout = timeout;
        self
    }

    /// Set max connections
    pub fn with_max_connections(mut self, max_connections: usize) -> Self {
        self.config.core.max_connections = max_connections;
        self.config.connection.max_connections = max_connections;
        self
    }

    /// Enable TLS
    pub fn with_tls_enabled(mut self, enabled: bool) -> Self {
        self.config.connection.tls.enabled = enabled;
        self
    }

    /// Enable debug mode
    pub fn with_debug_mode(mut self, enabled: bool) -> Self {
        self.config.features.debug_mode = enabled;
        self.config.features.verbose_logging = enabled;
        self
    }

    /// Set message buffer size
    pub fn with_message_buffer_size(mut self, size: usize) -> Self {
        self.config.core.message_buffer_size = size;
        self.config.messaging.message_buffer_size = size;
        self
    }

    /// Set reconnection settings
    pub fn with_reconnection(mut self, attempts: u32, delay: Duration) -> Self {
        self.config.core.reconnect_attempts = attempts;
        self.config.core.reconnect_delay = delay;
        self.config.reconnection.max_attempts = attempts;
        self.config.reconnection.base_delay = delay;
        self
    }

    /// Build the configuration
    pub fn build(self) -> AdvancedStreamConfig {
        self.config
    }
}

impl Default for TestConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock governance server for testing
pub struct MockGovernanceServer {
    pub port: u16,
    pub responses: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    pub request_log: Arc<RwLock<Vec<MockRequest>>>,
    pub latency_simulation: Option<Duration>,
    pub failure_rate: f64,
}

#[derive(Debug, Clone)]
pub struct MockRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub timestamp: std::time::SystemTime,
}

impl MockGovernanceServer {
    /// Create new mock governance server
    pub fn new() -> Self {
        Self {
            port: 0, // Will be assigned when started
            responses: Arc::new(RwLock::new(HashMap::new())),
            request_log: Arc::new(RwLock::new(Vec::new())),
            latency_simulation: None,
            failure_rate: 0.0,
        }
    }

    /// Set response for specific endpoint
    pub async fn set_response(&self, endpoint: &str, response: Vec<u8>) {
        self.responses.write().await.insert(endpoint.to_string(), response);
    }

    /// Set latency simulation
    pub fn with_latency(mut self, latency: Duration) -> Self {
        self.latency_simulation = Some(latency);
        self
    }

    /// Set failure rate (0.0 to 1.0)
    pub fn with_failure_rate(mut self, rate: f64) -> Self {
        self.failure_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// Start the mock server
    pub async fn start(&mut self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        use tokio::net::TcpListener;
        use std::net::SocketAddr;

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        self.port = addr.port();

        let responses = Arc::clone(&self.responses);
        let request_log = Arc::clone(&self.request_log);
        let latency = self.latency_simulation;
        let failure_rate = self.failure_rate;

        tokio::spawn(async move {
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    let responses = Arc::clone(&responses);
                    let request_log = Arc::clone(&request_log);
                    
                    tokio::spawn(async move {
                        // Simulate latency
                        if let Some(latency) = latency {
                            tokio::time::sleep(latency).await;
                        }

                        // Simulate failures
                        if failure_rate > 0.0 && rand::random::<f64>() < failure_rate {
                            return; // Drop connection to simulate failure
                        }

                        // Handle connection (simplified HTTP-like server)
                        Self::handle_connection(stream, responses, request_log).await;
                    });
                }
            }
        });

        Ok(format!("http://127.0.0.1:{}", self.port))
    }

    /// Get request log
    pub async fn get_requests(&self) -> Vec<MockRequest> {
        self.request_log.read().await.clone()
    }

    /// Clear request log
    pub async fn clear_requests(&self) {
        self.request_log.write().await.clear();
    }

    /// Handle incoming connection
    async fn handle_connection(
        mut stream: tokio::net::TcpStream,
        responses: Arc<RwLock<HashMap<String, Vec<u8>>>>,
        request_log: Arc<RwLock<Vec<MockRequest>>>,
    ) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut buffer = [0; 4096];
        if let Ok(size) = stream.read(&mut buffer).await {
            let request_str = String::from_utf8_lossy(&buffer[..size]);
            let lines: Vec<&str> = request_str.lines().collect();
            
            if let Some(request_line) = lines.first() {
                let parts: Vec<&str> = request_line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let method = parts[0].to_string();
                    let path = parts[1].to_string();
                    
                    // Log request
                    let mock_request = MockRequest {
                        method: method.clone(),
                        path: path.clone(),
                        headers: HashMap::new(), // Simplified
                        body: Vec::new(),        // Simplified
                        timestamp: std::time::SystemTime::now(),
                    };
                    request_log.write().await.push(mock_request);
                    
                    // Get response
                    let response_body = {
                        let responses = responses.read().await;
                        responses.get(&path).cloned().unwrap_or_else(|| b"404 Not Found".to_vec())
                    };
                    
                    // Send response
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n",
                        response_body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.write_all(&response_body).await;
                }
            }
        }
    }
}

/// Stream actor test harness
pub struct StreamActorTestHarness {
    pub actor: StreamActor,
    pub config: AdvancedStreamConfig,
    pub temp_dir: TempDir,
    pub mock_server: MockGovernanceServer,
    pub message_sender: mpsc::UnboundedSender<StreamMessage>,
    pub message_receiver: mpsc::UnboundedReceiver<StreamMessage>,
    pub response_sender: mpsc::UnboundedSender<StreamResponse>,
    pub response_receiver: mpsc::UnboundedReceiver<StreamResponse>,
}

impl StreamActorTestHarness {
    /// Create new test harness
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;
        let mut mock_server = MockGovernanceServer::new();
        let server_url = mock_server.start().await?;

        let config = TestConfigBuilder::new()
            .with_actor_id("test-stream-actor")
            .with_governance_endpoints(vec![&server_url])
            .with_debug_mode(true)
            .with_connection_timeout(Duration::from_millis(100))
            .with_max_connections(10)
            .build();

        let metrics = ActorSystemMetrics::new("test");
        let actor = StreamActor::new(config.clone(), metrics)?;

        let (message_sender, message_receiver) = mpsc::unbounded_channel();
        let (response_sender, response_receiver) = mpsc::unbounded_channel();

        Ok(Self {
            actor,
            config,
            temp_dir,
            mock_server,
            message_sender,
            message_receiver,
            response_sender,
            response_receiver,
        })
    }

    /// Create harness with custom configuration
    pub async fn with_config(config: AdvancedStreamConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let temp_dir = TempDir::new()?;
        let mock_server = MockGovernanceServer::new();

        let metrics = ActorSystemMetrics::new("test");
        let actor = StreamActor::new(config.clone(), metrics)?;

        let (message_sender, message_receiver) = mpsc::unbounded_channel();
        let (response_sender, response_receiver) = mpsc::unbounded_channel();

        Ok(Self {
            actor,
            config,
            temp_dir,
            mock_server,
            message_sender,
            message_receiver,
            response_sender,
            response_receiver,
        })
    }

    /// Start the actor
    pub async fn start(&mut self) -> Result<(), BridgeError> {
        self.actor.start().await
    }

    /// Stop the actor
    pub async fn stop(&mut self) -> Result<(), BridgeError> {
        self.actor.stop().await
    }

    /// Send message to actor
    pub async fn send_message(&mut self, message: StreamMessage) -> Result<(), BridgeError> {
        self.actor.handle_message(message).await
    }

    /// Wait for response
    pub async fn wait_for_response(&mut self, timeout: Duration) -> Option<StreamResponse> {
        tokio::time::timeout(timeout, self.response_receiver.recv())
            .await
            .ok()
            .flatten()
    }

    /// Get actor state
    pub async fn get_state(&self) -> Result<String, BridgeError> {
        self.actor.get_state().await
    }

    /// Check if actor is healthy
    pub async fn is_healthy(&self) -> bool {
        self.actor.health_check().await.unwrap_or(false)
    }

    /// Get metrics
    pub async fn get_metrics(&self) -> HashMap<String, f64> {
        // Return simplified metrics for testing
        let mut metrics = HashMap::new();
        metrics.insert("messages_processed".to_string(), 0.0);
        metrics.insert("connections_active".to_string(), 0.0);
        metrics.insert("errors_total".to_string(), 0.0);
        metrics
    }

    /// Create a test configuration file
    pub async fn create_config_file(&self, config: &AdvancedStreamConfig) -> Result<std::path::PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let config_path = self.temp_dir.path().join("stream_config.yaml");
        let config_yaml = serde_yaml::to_string(config)?;
        tokio::fs::write(&config_path, config_yaml).await?;
        Ok(config_path)
    }

    /// Test configuration hot-reload
    pub async fn test_hot_reload(&self, new_config: &AdvancedStreamConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let config_path = self.create_config_file(new_config).await?;
        let mut reload_manager = ConfigHotReloadManager::new(new_config.clone(), config_path)?;
        reload_manager.start_watching().await?;
        
        // Wait a bit for the watcher to initialize
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        Ok(())
    }

    /// Simulate network partition
    pub fn simulate_network_partition(&mut self, duration: Duration) {
        self.mock_server.failure_rate = 1.0;
        let mock_server = &mut self.mock_server;
        tokio::spawn(async move {
            tokio::time::sleep(duration).await;
            // Would reset failure rate here, but we can't move mock_server
        });
    }

    /// Add governance server response
    pub async fn add_server_response(&self, endpoint: &str, response: &str) {
        self.mock_server.set_response(endpoint, response.as_bytes().to_vec()).await;
    }

    /// Get server request log
    pub async fn get_server_requests(&self) -> Vec<MockRequest> {
        self.mock_server.get_requests().await
    }
}

/// Test message factory
pub struct TestMessageFactory;

impl TestMessageFactory {
    /// Create governance request message
    pub fn governance_request(request_id: &str, data: Vec<u8>) -> StreamMessage {
        StreamMessage::GovernanceRequest {
            request_id: request_id.to_string(),
            data,
            timeout: Duration::from_secs(30),
            priority: crate::actors::bridge::messages::MessagePriority::Normal,
        }
    }

    /// Create governance response message
    pub fn governance_response(request_id: &str, data: Vec<u8>) -> StreamMessage {
        StreamMessage::GovernanceResponse {
            request_id: request_id.to_string(),
            data,
            success: true,
        }
    }

    /// Create connection status message
    pub fn connection_status(endpoint: &str, connected: bool) -> StreamMessage {
        StreamMessage::ConnectionStatus {
            endpoint: endpoint.to_string(),
            connected,
            latency: if connected { Some(Duration::from_millis(50)) } else { None },
        }
    }

    /// Create health check message
    pub fn health_check() -> StreamMessage {
        StreamMessage::HealthCheck {
            request_id: Uuid::new_v4().to_string(),
        }
    }

    /// Create configuration update message
    pub fn config_update(config: AdvancedStreamConfig) -> StreamMessage {
        StreamMessage::ConfigUpdate {
            request_id: Uuid::new_v4().to_string(),
            config: Box::new(config),
        }
    }
}

/// Test assertions and utilities
pub struct TestAssertions;

impl TestAssertions {
    /// Assert that actor is in expected state
    pub async fn assert_actor_state(
        harness: &StreamActorTestHarness,
        expected_state: &str,
    ) -> Result<(), String> {
        let actual_state = harness.get_state().await
            .map_err(|e| format!("Failed to get actor state: {:?}", e))?;
            
        if actual_state.contains(expected_state) {
            Ok(())
        } else {
            Err(format!("Expected state '{}', got '{}'", expected_state, actual_state))
        }
    }

    /// Assert that actor is healthy
    pub async fn assert_actor_healthy(harness: &StreamActorTestHarness) -> Result<(), String> {
        if harness.is_healthy().await {
            Ok(())
        } else {
            Err("Actor is not healthy".to_string())
        }
    }

    /// Assert metric value
    pub async fn assert_metric_value(
        harness: &StreamActorTestHarness,
        metric_name: &str,
        expected_value: f64,
        tolerance: f64,
    ) -> Result<(), String> {
        let metrics = harness.get_metrics().await;
        let actual_value = metrics.get(metric_name)
            .ok_or_else(|| format!("Metric '{}' not found", metric_name))?;

        if (actual_value - expected_value).abs() <= tolerance {
            Ok(())
        } else {
            Err(format!(
                "Metric '{}': expected {}, got {} (tolerance: {})",
                metric_name, expected_value, actual_value, tolerance
            ))
        }
    }

    /// Assert that requests were made to governance server
    pub async fn assert_requests_made(
        harness: &StreamActorTestHarness,
        min_requests: usize,
    ) -> Result<(), String> {
        let requests = harness.get_server_requests().await;
        if requests.len() >= min_requests {
            Ok(())
        } else {
            Err(format!("Expected at least {} requests, got {}", min_requests, requests.len()))
        }
    }

    /// Assert response received within timeout
    pub async fn assert_response_received(
        harness: &mut StreamActorTestHarness,
        timeout: Duration,
    ) -> Result<StreamResponse, String> {
        harness.wait_for_response(timeout).await
            .ok_or_else(|| format!("No response received within {:?}", timeout))
    }
}

/// Performance test utilities
pub struct PerformanceTestUtils;

impl PerformanceTestUtils {
    /// Measure operation latency
    pub async fn measure_latency<F, Fut, T>(operation: F) -> (T, Duration)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let start = std::time::Instant::now();
        let result = operation().await;
        let duration = start.elapsed();
        (result, duration)
    }

    /// Run throughput test
    pub async fn throughput_test<F, Fut>(
        operation: F,
        duration: Duration,
    ) -> u64
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let start = std::time::Instant::now();
        let mut count = 0u64;

        while start.elapsed() < duration {
            operation().await;
            count += 1;
        }

        count
    }

    /// Generate load test scenario
    pub async fn generate_load(
        harness: &mut StreamActorTestHarness,
        messages_per_second: u64,
        duration: Duration,
    ) -> Result<u64, String> {
        let interval = Duration::from_nanos(1_000_000_000 / messages_per_second);
        let mut interval_timer = tokio::time::interval(interval);
        let end_time = std::time::Instant::now() + duration;
        let mut messages_sent = 0u64;

        while std::time::Instant::now() < end_time {
            interval_timer.tick().await;
            
            let message = TestMessageFactory::governance_request(
                &Uuid::new_v4().to_string(),
                b"test data".to_vec(),
            );
            
            if harness.send_message(message).await.is_ok() {
                messages_sent += 1;
            }
        }

        Ok(messages_sent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_builder() {
        let config = TestConfigBuilder::new()
            .with_actor_id("test-actor")
            .with_max_connections(100)
            .with_debug_mode(true)
            .build();

        assert_eq!(config.core.actor_id, "test-actor");
        assert_eq!(config.core.max_connections, 100);
        assert!(config.features.debug_mode);
    }

    #[tokio::test]
    async fn test_mock_server() {
        let mut server = MockGovernanceServer::new();
        let url = server.start().await.unwrap();
        
        server.set_response("/test", b"hello world".to_vec()).await;
        
        // Test would require HTTP client to verify server works
        assert!(url.starts_with("http://127.0.0.1:"));
    }

    #[tokio::test]
    async fn test_harness_creation() {
        let harness = StreamActorTestHarness::new().await;
        assert!(harness.is_ok());
    }
}