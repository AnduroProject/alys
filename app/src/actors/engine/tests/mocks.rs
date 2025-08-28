//! Mock Implementations for Testing
//!
//! Provides mock execution clients, mock engines, and other test doubles
//! for comprehensive testing of the EngineActor.

use std::time::{Duration, Instant, SystemTime};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use async_trait::async_trait;
use tracing::*;

use lighthouse_wrapper::execution_layer::{
    ExecutionPayload, PayloadStatus, PayloadAttributes, ForkchoiceState,
    ForkchoiceUpdatedResponse, ExecutePayloadResponse, NewPayloadResponse,
};
use lighthouse_wrapper::types::{Hash256, Address, MainnetEthSpec};

use crate::types::*;
use super::super::{
    client::{ExecutionClient, HealthCheck, ClientCapabilities},
    engine::Engine,
    EngineError, EngineResult,
};

/// Mock execution client for testing
#[derive(Debug)]
pub struct MockExecutionClient {
    /// Configuration for mock behavior
    pub config: MockClientConfig,
    
    /// Shared state for tracking calls and responses
    pub state: Arc<Mutex<MockClientState>>,
}

/// Configuration for mock client behavior
#[derive(Debug, Clone)]
pub struct MockClientConfig {
    /// Whether the client should be healthy
    pub healthy: bool,
    
    /// Response delay to simulate network latency
    pub response_delay: Duration,
    
    /// Whether to simulate failures
    pub simulate_failures: bool,
    
    /// Failure rate (0.0 to 1.0)
    pub failure_rate: f64,
    
    /// Whether the client is syncing
    pub is_syncing: bool,
    
    /// Current block height
    pub block_height: u64,
    
    /// JWT secret for authentication
    pub jwt_secret: Option<[u8; 32]>,
}

/// Internal state of mock client
#[derive(Debug, Default)]
pub struct MockClientState {
    /// Number of health checks performed
    pub health_checks: u32,
    
    /// Number of payload builds requested
    pub payload_builds: u32,
    
    /// Number of payload executions requested
    pub payload_executions: u32,
    
    /// Number of forkchoice updates requested
    pub forkchoice_updates: u32,
    
    /// Last payload built
    pub last_payload: Option<ExecutionPayload<MainnetEthSpec>>,
    
    /// Current finalized block hash
    pub finalized_hash: Option<Hash256>,
    
    /// Simulated payloads in memory
    pub payloads: HashMap<String, ExecutionPayload<MainnetEthSpec>>,
    
    /// Simulated blocks
    pub blocks: HashMap<Hash256, MockBlock>,
    
    /// Connection attempts
    pub connection_attempts: u32,
}

/// Mock block for testing
#[derive(Debug, Clone)]
pub struct MockBlock {
    /// Block hash
    pub hash: Hash256,
    
    /// Block height
    pub height: u64,
    
    /// Parent hash
    pub parent_hash: Hash256,
    
    /// Timestamp
    pub timestamp: u64,
    
    /// Transaction count
    pub transaction_count: u32,
}

impl Default for MockClientConfig {
    fn default() -> Self {
        Self {
            healthy: true,
            response_delay: Duration::from_millis(10),
            simulate_failures: false,
            failure_rate: 0.0,
            is_syncing: false,
            block_height: 100,
            jwt_secret: Some([0u8; 32]),
        }
    }
}

impl MockExecutionClient {
    /// Create a new mock client with default configuration
    pub fn new() -> Self {
        Self::with_config(MockClientConfig::default())
    }
    
    /// Create a new mock client with custom configuration
    pub fn with_config(config: MockClientConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(MockClientState::default())),
        }
    }
    
    /// Create a failing mock client
    pub fn failing() -> Self {
        Self::with_config(MockClientConfig {
            healthy: false,
            simulate_failures: true,
            failure_rate: 1.0,
            ..Default::default()
        })
    }
    
    /// Create a slow mock client
    pub fn slow() -> Self {
        Self::with_config(MockClientConfig {
            response_delay: Duration::from_millis(500),
            ..Default::default()
        })
    }
    
    /// Create a syncing mock client
    pub fn syncing() -> Self {
        Self::with_config(MockClientConfig {
            is_syncing: true,
            ..Default::default()
        })
    }
    
    /// Get current state statistics
    pub fn get_stats(&self) -> MockClientState {
        self.state.lock().unwrap().clone()
    }
    
    /// Reset mock state
    pub fn reset(&self) {
        *self.state.lock().unwrap() = MockClientState::default();
    }
    
    /// Simulate a failure if configured to do so
    fn should_fail(&self) -> bool {
        if !self.config.simulate_failures {
            return false;
        }
        
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < self.config.failure_rate
    }
    
    /// Add simulated delay
    async fn simulate_delay(&self) {
        if self.config.response_delay > Duration::ZERO {
            tokio::time::sleep(self.config.response_delay).await;
        }
    }
}

#[async_trait]
impl ExecutionClient for MockExecutionClient {
    async fn health_check(&self) -> HealthCheck {
        self.simulate_delay().await;
        
        let mut state = self.state.lock().unwrap();
        state.health_checks += 1;
        
        let start = Instant::now();
        
        if !self.config.healthy || self.should_fail() {
            HealthCheck {
                reachable: false,
                response_time: start.elapsed(),
                error: Some("Mock client configured as unhealthy".to_string()),
            }
        } else {
            HealthCheck {
                reachable: true,
                response_time: start.elapsed(),
                error: None,
            }
        }
    }
    
    async fn get_capabilities(&self) -> EngineResult<ClientCapabilities> {
        self.simulate_delay().await;
        
        if self.should_fail() {
            return Err(EngineError::ClientError(
                super::super::ClientError::ConnectionFailed("Mock failure".to_string())
            ));
        }
        
        Ok(ClientCapabilities {
            client_version: "MockClient/1.0.0".to_string(),
            supported_methods: vec![
                "engine_newPayloadV1".to_string(),
                "engine_executePayloadV1".to_string(),
                "engine_forkchoiceUpdatedV1".to_string(),
            ],
            chain_id: 212121,
            supports_jwt: true,
        })
    }
    
    async fn connect(&self) -> EngineResult<()> {
        self.simulate_delay().await;
        
        let mut state = self.state.lock().unwrap();
        state.connection_attempts += 1;
        
        if !self.config.healthy || self.should_fail() {
            return Err(EngineError::ClientError(
                super::super::ClientError::ConnectionFailed("Mock connection failure".to_string())
            ));
        }
        
        debug!("Mock client connected successfully");
        Ok(())
    }
    
    async fn disconnect(&self) -> EngineResult<()> {
        self.simulate_delay().await;
        debug!("Mock client disconnected");
        Ok(())
    }
    
    async fn reconnect(&self) -> EngineResult<()> {
        self.disconnect().await?;
        self.connect().await?;
        Ok(())
    }
    
    async fn is_connected(&self) -> bool {
        self.config.healthy && !self.should_fail()
    }
}

/// Mock engine for testing
pub struct MockEngine {
    /// Mock client
    pub client: MockExecutionClient,
    
    /// Engine configuration
    pub config: MockEngineConfig,
    
    /// Engine state
    pub state: Arc<Mutex<MockEngineState>>,
}

/// Mock engine configuration
#[derive(Debug, Clone)]
pub struct MockEngineConfig {
    /// Block building time simulation
    pub build_time: Duration,
    
    /// Execution time simulation
    pub execution_time: Duration,
    
    /// Whether to simulate gas estimation failures
    pub fail_gas_estimation: bool,
}

/// Mock engine state
#[derive(Debug, Default)]
pub struct MockEngineState {
    /// Current head block
    pub head_block: Option<Hash256>,
    
    /// Finalized block
    pub finalized_block: Option<Hash256>,
    
    /// Built payloads
    pub built_payloads: HashMap<String, ExecutionPayload<MainnetEthSpec>>,
    
    /// Executed payloads
    pub executed_payloads: Vec<Hash256>,
    
    /// Transaction receipts
    pub receipts: HashMap<Hash256, MockTransactionReceipt>,
}

/// Mock transaction receipt
#[derive(Debug, Clone)]
pub struct MockTransactionReceipt {
    /// Transaction hash
    pub transaction_hash: Hash256,
    
    /// Block hash
    pub block_hash: Hash256,
    
    /// Block height
    pub block_height: u64,
    
    /// Gas used
    pub gas_used: u64,
    
    /// Success status
    pub success: bool,
}

impl Default for MockEngineConfig {
    fn default() -> Self {
        Self {
            build_time: Duration::from_millis(50),
            execution_time: Duration::from_millis(30),
            fail_gas_estimation: false,
        }
    }
}

impl MockEngine {
    /// Create a new mock engine
    pub fn new() -> Self {
        Self {
            client: MockExecutionClient::new(),
            config: MockEngineConfig::default(),
            state: Arc::new(Mutex::new(MockEngineState::default())),
        }
    }
    
    /// Create a mock engine with custom client
    pub fn with_client(client: MockExecutionClient) -> Self {
        Self {
            client,
            config: MockEngineConfig::default(),
            state: Arc::new(Mutex::new(MockEngineState::default())),
        }
    }
    
    /// Get engine statistics
    pub fn get_stats(&self) -> (MockClientState, MockEngineState) {
        (
            self.client.get_stats(),
            self.state.lock().unwrap().clone()
        )
    }
    
    /// Create a mock payload for testing
    pub fn create_mock_payload(&self, parent_hash: Hash256) -> ExecutionPayload<MainnetEthSpec> {
        ExecutionPayload {
            parent_hash,
            fee_recipient: Address::zero(),
            state_root: Hash256::random(),
            receipts_root: Hash256::random(),
            logs_bloom: vec![0u8; 256],
            prev_randao: Hash256::random(),
            block_number: self.client.config.block_height,
            gas_limit: 30_000_000,
            gas_used: 21_000,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            extra_data: vec![],
            base_fee_per_gas: 1_000_000_000u64.into(), // 1 gwei
            block_hash: Hash256::random(),
            transactions: vec![],
            withdrawals: None,
            blob_gas_used: None,
            excess_blob_gas: None,
        }
    }
}

/// Mock payload builder for testing payload building operations
pub struct MockPayloadBuilder {
    /// Configuration
    pub config: MockClientConfig,
    
    /// Built payloads
    pub payloads: Arc<Mutex<HashMap<String, ExecutionPayload<MainnetEthSpec>>>>,
}

impl MockPayloadBuilder {
    pub fn new() -> Self {
        Self {
            config: MockClientConfig::default(),
            payloads: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Build a payload with given attributes
    pub async fn build_payload(
        &self,
        parent_hash: Hash256,
        attributes: PayloadAttributes,
    ) -> EngineResult<(String, ExecutionPayload<MainnetEthSpec>)> {
        // Simulate build time
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        let payload_id = format!("mock_payload_{}", rand::random::<u64>());
        let payload = ExecutionPayload {
            parent_hash,
            fee_recipient: attributes.suggested_fee_recipient,
            state_root: Hash256::random(),
            receipts_root: Hash256::random(),
            logs_bloom: vec![0u8; 256],
            prev_randao: attributes.prev_randao,
            block_number: self.config.block_height + 1,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: attributes.timestamp,
            extra_data: vec![],
            base_fee_per_gas: 1_000_000_000u64.into(),
            block_hash: Hash256::random(),
            transactions: vec![],
            withdrawals: attributes.withdrawals.map(|w| w.into_iter().map(Into::into).collect()),
            blob_gas_used: None,
            excess_blob_gas: None,
        };
        
        self.payloads.lock().unwrap().insert(payload_id.clone(), payload.clone());
        
        Ok((payload_id, payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_mock_client_healthy() {
        let client = MockExecutionClient::new();
        let health = client.health_check().await;
        
        assert!(health.reachable);
        assert!(health.error.is_none());
        
        let stats = client.get_stats();
        assert_eq!(stats.health_checks, 1);
    }
    
    #[tokio::test]
    async fn test_mock_client_failing() {
        let client = MockExecutionClient::failing();
        let health = client.health_check().await;
        
        assert!(!health.reachable);
        assert!(health.error.is_some());
    }
    
    #[tokio::test]
    async fn test_mock_client_connection() {
        let client = MockExecutionClient::new();
        
        // Test successful connection
        let result = client.connect().await;
        assert!(result.is_ok());
        
        let stats = client.get_stats();
        assert_eq!(stats.connection_attempts, 1);
    }
    
    #[tokio::test]
    async fn test_mock_engine_creation() {
        let engine = MockEngine::new();
        let (client_stats, engine_stats) = engine.get_stats();
        
        assert_eq!(client_stats.health_checks, 0);
        assert!(engine_stats.built_payloads.is_empty());
    }
    
    #[tokio::test]
    async fn test_mock_payload_builder() {
        let builder = MockPayloadBuilder::new();
        
        let parent_hash = Hash256::random();
        let attributes = PayloadAttributes::new(
            1234567890,
            Hash256::random(),
            Address::zero(),
            None,
        );
        
        let result = builder.build_payload(parent_hash, attributes).await;
        assert!(result.is_ok());
        
        let (payload_id, payload) = result.unwrap();
        assert!(!payload_id.is_empty());
        assert_eq!(payload.parent_hash, parent_hash);
    }
}