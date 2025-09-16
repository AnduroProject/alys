//! Mock implementations for external system integration testing
//!
//! This module provides comprehensive mock implementations of external clients
//! and services used in the Alys system, enabling isolated testing of actor
//! interactions without dependencies on real external systems.

use crate::integration::{BitcoinClientExt, ExecutionClientExt};
use crate::types::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock, Mutex};
use uuid::Uuid;

/// Mock governance client for testing
#[derive(Debug, Clone)]
pub struct MockGovernanceClient {
    /// Mock configuration
    config: MockGovernanceConfig,
    
    /// Mock state
    state: Arc<RwLock<MockGovernanceState>>,
    
    /// Response overrides for specific calls
    response_overrides: Arc<RwLock<HashMap<String, MockResponse>>>,
    
    /// Call history for verification
    call_history: Arc<RwLock<Vec<MockCall>>>,
}

/// Mock governance configuration
#[derive(Debug, Clone)]
pub struct MockGovernanceConfig {
    /// Simulate network delays
    pub network_delay: Duration,
    
    /// Failure rate (0.0 to 1.0)
    pub failure_rate: f64,
    
    /// Enable streaming responses
    pub enable_streaming: bool,
    
    /// Maximum concurrent connections
    pub max_connections: u32,
    
    /// Response timeout
    pub response_timeout: Duration,
}

/// Mock governance state
#[derive(Debug, Default)]
pub struct MockGovernanceState {
    /// Current block number
    pub current_block: u64,
    
    /// Governance proposals
    pub proposals: HashMap<String, GovernanceProposal>,
    
    /// Validator set
    pub validators: Vec<ValidatorInfo>,
    
    /// Network status
    pub network_status: NetworkStatus,
    
    /// Connection count
    pub connection_count: u32,
}

/// Governance proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceProposal {
    pub id: String,
    pub title: String,
    pub description: String,
    pub proposer: String,
    pub status: ProposalStatus,
    pub voting_period: VotingPeriod,
    pub votes: HashMap<String, Vote>,
}

/// Proposal status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProposalStatus {
    Draft,
    Active,
    Passed,
    Rejected,
    Cancelled,
    Executed,
}

/// Voting period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VotingPeriod {
    pub start_time: SystemTime,
    pub end_time: SystemTime,
    pub duration: Duration,
}

/// Vote information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub voter: String,
    pub vote_type: VoteType,
    pub power: u64,
    pub timestamp: SystemTime,
}

/// Vote types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoteType {
    Yes,
    No,
    Abstain,
    NoWithVeto,
}

/// Validator information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub address: String,
    pub pub_key: String,
    pub voting_power: u64,
    pub status: ValidatorStatus,
    pub commission: f64,
}

/// Validator status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidatorStatus {
    Active,
    Inactive,
    Jailed,
    Tombstoned,
}

/// Network status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub chain_id: String,
    pub block_height: u64,
    pub block_time: Duration,
    pub peer_count: u32,
    pub syncing: bool,
}

/// Mock Bitcoin client for testing
#[derive(Debug, Clone)]
pub struct MockBitcoinClient {
    /// Mock configuration
    config: MockBitcoinConfig,
    
    /// Mock blockchain state
    blockchain: Arc<RwLock<MockBitcoinBlockchain>>,
    
    /// Mempool state
    mempool: Arc<RwLock<MockMempool>>,
    
    /// Response overrides
    response_overrides: Arc<RwLock<HashMap<String, MockResponse>>>,
    
    /// Call history
    call_history: Arc<RwLock<Vec<MockCall>>>,
}

/// Mock Bitcoin configuration
#[derive(Debug, Clone)]
pub struct MockBitcoinConfig {
    /// Network type (mainnet, testnet, regtest)
    pub network: String,
    
    /// Starting block height
    pub start_block_height: u32,
    
    /// Block generation interval
    pub block_interval: Duration,
    
    /// Transaction fee rate (sat/vB)
    pub fee_rate: u64,
    
    /// Network delay simulation
    pub network_delay: Duration,
    
    /// Failure rate
    pub failure_rate: f64,
}

/// Mock Bitcoin blockchain state
#[derive(Debug, Default)]
pub struct MockBitcoinBlockchain {
    /// Blocks by height
    pub blocks: HashMap<u32, MockBitcoinBlock>,
    
    /// Current block height
    pub best_block_height: u32,
    
    /// Best block hash
    pub best_block_hash: String,
    
    /// Total difficulty
    pub total_difficulty: u64,
}

/// Mock Bitcoin block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockBitcoinBlock {
    pub height: u32,
    pub hash: String,
    pub prev_hash: String,
    pub merkle_root: String,
    pub timestamp: SystemTime,
    pub difficulty: u32,
    pub nonce: u32,
    pub transactions: Vec<MockBitcoinTransaction>,
}

/// Mock Bitcoin transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockBitcoinTransaction {
    pub txid: String,
    pub version: u32,
    pub inputs: Vec<MockTxInput>,
    pub outputs: Vec<MockTxOutput>,
    pub locktime: u32,
    pub size: u32,
    pub weight: u32,
    pub fee: u64,
}

/// Mock transaction input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockTxInput {
    pub prev_txid: String,
    pub vout: u32,
    pub script_sig: String,
    pub sequence: u32,
    pub witness: Vec<String>,
}

/// Mock transaction output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockTxOutput {
    pub value: u64,
    pub script_pubkey: String,
    pub address: Option<String>,
}

/// Mock mempool state
#[derive(Debug, Default)]
pub struct MockMempool {
    /// Pending transactions
    pub transactions: HashMap<String, MockBitcoinTransaction>,
    
    /// Fee estimates
    pub fee_estimates: HashMap<u32, u64>, // blocks -> sat/vB
}

/// Mock execution client for testing
#[derive(Debug, Clone)]
pub struct MockExecutionClient {
    /// Mock configuration
    config: MockExecutionConfig,
    
    /// Mock blockchain state
    blockchain: Arc<RwLock<MockExecutionBlockchain>>,
    
    /// Transaction pool
    tx_pool: Arc<RwLock<MockTxPool>>,
    
    /// Account states
    accounts: Arc<RwLock<HashMap<String, MockAccount>>>,
    
    /// Response overrides
    response_overrides: Arc<RwLock<HashMap<String, MockResponse>>>,
    
    /// Call history
    call_history: Arc<RwLock<Vec<MockCall>>>,
}

/// Mock execution configuration
#[derive(Debug, Clone)]
pub struct MockExecutionConfig {
    /// Chain ID
    pub chain_id: u64,
    
    /// Gas limit per block
    pub gas_limit: u64,
    
    /// Gas price
    pub gas_price: u64,
    
    /// Block time
    pub block_time: Duration,
    
    /// Network delay
    pub network_delay: Duration,
    
    /// Failure rate
    pub failure_rate: f64,
}

/// Mock execution blockchain state
#[derive(Debug, Default)]
pub struct MockExecutionBlockchain {
    /// Blocks by number
    pub blocks: HashMap<u64, MockExecutionBlock>,
    
    /// Current block number
    pub latest_block: u64,
    
    /// Total difficulty
    pub total_difficulty: u128,
    
    /// Gas used
    pub gas_used: u64,
}

/// Mock execution block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockExecutionBlock {
    pub number: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: SystemTime,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub transactions: Vec<MockExecutionTransaction>,
    pub state_root: String,
    pub receipts_root: String,
}

/// Mock execution transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockExecutionTransaction {
    pub hash: String,
    pub from: String,
    pub to: Option<String>,
    pub value: u128,
    pub gas: u64,
    pub gas_price: u64,
    pub data: Vec<u8>,
    pub nonce: u64,
    pub r#type: u8,
}

/// Mock transaction pool
#[derive(Debug, Default)]
pub struct MockTxPool {
    /// Pending transactions
    pub pending: HashMap<String, MockExecutionTransaction>,
    
    /// Queued transactions
    pub queued: HashMap<String, Vec<MockExecutionTransaction>>,
}

/// Mock account state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockAccount {
    pub address: String,
    pub balance: u128,
    pub nonce: u64,
    pub code: Vec<u8>,
    pub storage: HashMap<String, String>,
}

/// Mock response for overriding behavior
#[derive(Debug, Clone)]
pub enum MockResponse {
    Success { data: serde_json::Value },
    Error { code: i32, message: String },
    Timeout,
    NetworkError { message: String },
    Custom { handler: fn() -> Result<serde_json::Value, String> },
}

/// Mock call record for verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockCall {
    pub call_id: String,
    pub timestamp: SystemTime,
    pub method: String,
    pub parameters: serde_json::Value,
    pub response: MockCallResponse,
    pub duration: Duration,
}

/// Mock call response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MockCallResponse {
    Success,
    Error { message: String },
    Timeout,
}

impl MockGovernanceClient {
    /// Create a new mock governance client
    pub fn new(config: MockGovernanceConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(MockGovernanceState::default())),
            response_overrides: Arc::new(RwLock::new(HashMap::new())),
            call_history: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Set response override for a specific method
    pub async fn set_response_override(&self, method: &str, response: MockResponse) {
        let mut overrides = self.response_overrides.write().await;
        overrides.insert(method.to_string(), response);
    }
    
    /// Get call history
    pub async fn get_call_history(&self) -> Vec<MockCall> {
        self.call_history.read().await.clone()
    }
    
    /// Add a governance proposal
    pub async fn add_proposal(&self, proposal: GovernanceProposal) {
        let mut state = self.state.write().await;
        state.proposals.insert(proposal.id.clone(), proposal);
    }
    
    /// Set network status
    pub async fn set_network_status(&self, status: NetworkStatus) {
        let mut state = self.state.write().await;
        state.network_status = status;
    }
    
    /// Record a mock call
    async fn record_call(&self, method: &str, params: serde_json::Value, response: MockCallResponse, duration: Duration) {
        let call = MockCall {
            call_id: Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            method: method.to_string(),
            parameters: params,
            response,
            duration,
        };
        
        let mut history = self.call_history.write().await;
        history.push(call);
    }
    
    /// Simulate network delay
    async fn simulate_delay(&self) {
        if self.config.network_delay > Duration::from_millis(0) {
            tokio::time::sleep(self.config.network_delay).await;
        }
    }
    
    /// Check if call should fail
    fn should_fail(&self) -> bool {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < self.config.failure_rate
    }
}

impl MockBitcoinClient {
    /// Create a new mock Bitcoin client
    pub fn new(config: MockBitcoinConfig) -> Self {
        let mut blockchain = MockBitcoinBlockchain::default();
        blockchain.best_block_height = config.start_block_height;
        blockchain.best_block_hash = "00000000000000000000000000000000000000000000000000000000000000000".to_string();
        
        Self {
            config,
            blockchain: Arc::new(RwLock::new(blockchain)),
            mempool: Arc::new(RwLock::new(MockMempool::default())),
            response_overrides: Arc::new(RwLock::new(HashMap::new())),
            call_history: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Generate a new block
    pub async fn generate_block(&self) -> Result<MockBitcoinBlock, String> {
        let mut blockchain = self.blockchain.write().await;
        let mut mempool = self.mempool.write().await;
        
        let height = blockchain.best_block_height + 1;
        let prev_hash = blockchain.best_block_hash.clone();
        
        // Take transactions from mempool
        let transactions: Vec<MockBitcoinTransaction> = mempool.transactions.values().cloned().collect();
        mempool.transactions.clear();
        
        let block = MockBitcoinBlock {
            height,
            hash: format!("block_hash_{}", height),
            prev_hash,
            merkle_root: format!("merkle_{}", height),
            timestamp: SystemTime::now(),
            difficulty: 1,
            nonce: height,
            transactions,
        };
        
        blockchain.blocks.insert(height, block.clone());
        blockchain.best_block_height = height;
        blockchain.best_block_hash = block.hash.clone();
        
        Ok(block)
    }
    
    /// Add transaction to mempool
    pub async fn add_transaction(&self, tx: MockBitcoinTransaction) {
        let mut mempool = self.mempool.write().await;
        mempool.transactions.insert(tx.txid.clone(), tx);
    }
    
    /// Set response override
    pub async fn set_response_override(&self, method: &str, response: MockResponse) {
        let mut overrides = self.response_overrides.write().await;
        overrides.insert(method.to_string(), response);
    }
    
    /// Get call history
    pub async fn get_call_history(&self) -> Vec<MockCall> {
        self.call_history.read().await.clone()
    }
    
    /// Record a mock call
    async fn record_call(&self, method: &str, params: serde_json::Value, response: MockCallResponse, duration: Duration) {
        let call = MockCall {
            call_id: Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            method: method.to_string(),
            parameters: params,
            response,
            duration,
        };
        
        let mut history = self.call_history.write().await;
        history.push(call);
    }
    
    /// Simulate network delay
    async fn simulate_delay(&self) {
        if self.config.network_delay > Duration::from_millis(0) {
            tokio::time::sleep(self.config.network_delay).await;
        }
    }
    
    /// Check if call should fail
    fn should_fail(&self) -> bool {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < self.config.failure_rate
    }
}

impl MockExecutionClient {
    /// Create a new mock execution client
    pub fn new(config: MockExecutionConfig) -> Self {
        Self {
            config,
            blockchain: Arc::new(RwLock::new(MockExecutionBlockchain::default())),
            tx_pool: Arc::new(RwLock::new(MockTxPool::default())),
            accounts: Arc::new(RwLock::new(HashMap::new())),
            response_overrides: Arc::new(RwLock::new(HashMap::new())),
            call_history: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Create a new block with pending transactions
    pub async fn create_block(&self) -> Result<MockExecutionBlock, String> {
        let mut blockchain = self.blockchain.write().await;
        let mut tx_pool = self.tx_pool.write().await;
        
        let block_number = blockchain.latest_block + 1;
        let parent_hash = if block_number > 0 {
            blockchain.blocks.get(&(block_number - 1))
                .map(|b| b.hash.clone())
                .unwrap_or_else(|| "0x0000000000000000000000000000000000000000000000000000000000000000".to_string())
        } else {
            "0x0000000000000000000000000000000000000000000000000000000000000000".to_string()
        };
        
        // Take transactions from pending pool
        let transactions: Vec<MockExecutionTransaction> = tx_pool.pending.values().cloned().collect();
        tx_pool.pending.clear();
        
        let gas_used = transactions.iter().map(|tx| tx.gas).sum();
        
        let block = MockExecutionBlock {
            number: block_number,
            hash: format!("0x{:064x}", block_number),
            parent_hash,
            timestamp: SystemTime::now(),
            gas_limit: self.config.gas_limit,
            gas_used,
            transactions,
            state_root: format!("0x{:064x}", block_number + 1000),
            receipts_root: format!("0x{:064x}", block_number + 2000),
        };
        
        blockchain.blocks.insert(block_number, block.clone());
        blockchain.latest_block = block_number;
        blockchain.gas_used += gas_used;
        
        Ok(block)
    }
    
    /// Add transaction to pending pool
    pub async fn add_pending_transaction(&self, tx: MockExecutionTransaction) {
        let mut tx_pool = self.tx_pool.write().await;
        tx_pool.pending.insert(tx.hash.clone(), tx);
    }
    
    /// Set account state
    pub async fn set_account(&self, address: String, account: MockAccount) {
        let mut accounts = self.accounts.write().await;
        accounts.insert(address, account);
    }
    
    /// Get account state
    pub async fn get_account(&self, address: &str) -> Option<MockAccount> {
        let accounts = self.accounts.read().await;
        accounts.get(address).cloned()
    }
    
    /// Set response override
    pub async fn set_response_override(&self, method: &str, response: MockResponse) {
        let mut overrides = self.response_overrides.write().await;
        overrides.insert(method.to_string(), response);
    }
    
    /// Get call history
    pub async fn get_call_history(&self) -> Vec<MockCall> {
        self.call_history.read().await.clone()
    }
    
    /// Record a mock call
    async fn record_call(&self, method: &str, params: serde_json::Value, response: MockCallResponse, duration: Duration) {
        let call = MockCall {
            call_id: Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            method: method.to_string(),
            parameters: params,
            response,
            duration,
        };
        
        let mut history = self.call_history.write().await;
        history.push(call);
    }
    
    /// Simulate network delay
    async fn simulate_delay(&self) {
        if self.config.network_delay > Duration::from_millis(0) {
            tokio::time::sleep(self.config.network_delay).await;
        }
    }
    
    /// Check if call should fail
    fn should_fail(&self) -> bool {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < self.config.failure_rate
    }
}

// Default implementations for configurations
impl Default for MockGovernanceConfig {
    fn default() -> Self {
        Self {
            network_delay: Duration::from_millis(50),
            failure_rate: 0.0,
            enable_streaming: true,
            max_connections: 100,
            response_timeout: Duration::from_secs(30),
        }
    }
}

impl Default for MockBitcoinConfig {
    fn default() -> Self {
        Self {
            network: "regtest".to_string(),
            start_block_height: 0,
            block_interval: Duration::from_secs(10),
            fee_rate: 1, // 1 sat/vB
            network_delay: Duration::from_millis(100),
            failure_rate: 0.0,
        }
    }
}

impl Default for MockExecutionConfig {
    fn default() -> Self {
        Self {
            chain_id: 263634, // Alys chain ID
            gas_limit: 30_000_000,
            gas_price: 20_000_000_000, // 20 gwei
            block_time: Duration::from_secs(2),
            network_delay: Duration::from_millis(50),
            failure_rate: 0.0,
        }
    }
}

/// Builder for creating mock test environments
pub struct MockEnvironmentBuilder {
    governance_config: MockGovernanceConfig,
    bitcoin_config: MockBitcoinConfig,
    execution_config: MockExecutionConfig,
}

impl MockEnvironmentBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            governance_config: MockGovernanceConfig::default(),
            bitcoin_config: MockBitcoinConfig::default(),
            execution_config: MockExecutionConfig::default(),
        }
    }
    
    /// Configure governance client
    pub fn with_governance_config(mut self, config: MockGovernanceConfig) -> Self {
        self.governance_config = config;
        self
    }
    
    /// Configure Bitcoin client
    pub fn with_bitcoin_config(mut self, config: MockBitcoinConfig) -> Self {
        self.bitcoin_config = config;
        self
    }
    
    /// Configure execution client
    pub fn with_execution_config(mut self, config: MockExecutionConfig) -> Self {
        self.execution_config = config;
        self
    }
    
    /// Set failure rate for all clients
    pub fn with_failure_rate(mut self, rate: f64) -> Self {
        self.governance_config.failure_rate = rate;
        self.bitcoin_config.failure_rate = rate;
        self.execution_config.failure_rate = rate;
        self
    }
    
    /// Set network delay for all clients
    pub fn with_network_delay(mut self, delay: Duration) -> Self {
        self.governance_config.network_delay = delay;
        self.bitcoin_config.network_delay = delay;
        self.execution_config.network_delay = delay;
        self
    }
    
    /// Build the mock environment
    pub fn build(self) -> MockTestEnvironment {
        MockTestEnvironment {
            governance_client: MockGovernanceClient::new(self.governance_config),
            bitcoin_client: MockBitcoinClient::new(self.bitcoin_config),
            execution_client: MockExecutionClient::new(self.execution_config),
        }
    }
}

impl Default for MockEnvironmentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Complete mock test environment
#[derive(Debug, Clone)]
pub struct MockTestEnvironment {
    pub governance_client: MockGovernanceClient,
    pub bitcoin_client: MockBitcoinClient,
    pub execution_client: MockExecutionClient,
}

impl MockTestEnvironment {
    /// Create a new mock test environment with default configurations
    pub fn new() -> Self {
        MockEnvironmentBuilder::new().build()
    }
    
    /// Create a mock environment with specific failure rates
    pub fn with_failure_rate(rate: f64) -> Self {
        MockEnvironmentBuilder::new()
            .with_failure_rate(rate)
            .build()
    }
    
    /// Create a mock environment with network delays
    pub fn with_network_delay(delay: Duration) -> Self {
        MockEnvironmentBuilder::new()
            .with_network_delay(delay)
            .build()
    }
    
    /// Reset all mock states
    pub async fn reset(&self) {
        // Reset governance state
        {
            let mut state = self.governance_client.state.write().await;
            *state = MockGovernanceState::default();
        }
        
        // Reset Bitcoin blockchain
        {
            let mut blockchain = self.bitcoin_client.blockchain.write().await;
            *blockchain = MockBitcoinBlockchain::default();
            blockchain.best_block_height = self.bitcoin_client.config.start_block_height;
        }
        
        // Reset execution blockchain
        {
            let mut blockchain = self.execution_client.blockchain.write().await;
            *blockchain = MockExecutionBlockchain::default();
        }
        
        // Clear call histories
        {
            let mut history = self.governance_client.call_history.write().await;
            history.clear();
        }
        {
            let mut history = self.bitcoin_client.call_history.write().await;
            history.clear();
        }
        {
            let mut history = self.execution_client.call_history.write().await;
            history.clear();
        }
    }
    
    /// Get combined call history from all clients
    pub async fn get_all_call_history(&self) -> Vec<MockCall> {
        let mut all_calls = Vec::new();
        
        all_calls.extend(self.governance_client.get_call_history().await);
        all_calls.extend(self.bitcoin_client.get_call_history().await);
        all_calls.extend(self.execution_client.get_call_history().await);
        
        // Sort by timestamp
        all_calls.sort_by_key(|call| call.timestamp);
        all_calls
    }
}

impl Default for MockTestEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility functions for creating test data
pub mod test_data {
    use super::*;
    
    /// Create a sample governance proposal
    pub fn sample_governance_proposal() -> GovernanceProposal {
        GovernanceProposal {
            id: "prop_001".to_string(),
            title: "Test Proposal".to_string(),
            description: "A test governance proposal".to_string(),
            proposer: "test_proposer".to_string(),
            status: ProposalStatus::Active,
            voting_period: VotingPeriod {
                start_time: SystemTime::now(),
                end_time: SystemTime::now() + Duration::from_secs(86400),
                duration: Duration::from_secs(86400),
            },
            votes: HashMap::new(),
        }
    }
    
    /// Create a sample Bitcoin transaction
    pub fn sample_bitcoin_transaction() -> MockBitcoinTransaction {
        MockBitcoinTransaction {
            txid: "tx_001".to_string(),
            version: 1,
            inputs: vec![MockTxInput {
                prev_txid: "prev_tx_001".to_string(),
                vout: 0,
                script_sig: "483045022100...".to_string(),
                sequence: 0xffffffff,
                witness: vec![],
            }],
            outputs: vec![MockTxOutput {
                value: 100000000, // 1 BTC
                script_pubkey: "76a914...88ac".to_string(),
                address: Some("bc1qtest...".to_string()),
            }],
            locktime: 0,
            size: 250,
            weight: 1000,
            fee: 1000, // 1000 sats
        }
    }
    
    /// Create a sample execution transaction
    pub fn sample_execution_transaction() -> MockExecutionTransaction {
        MockExecutionTransaction {
            hash: "0x1234567890abcdef...".to_string(),
            from: "0xabcdefabcdefabcdefabcdefabcdefabcdefabcdef".to_string(),
            to: Some("0x1234567890123456789012345678901234567890".to_string()),
            value: 1000000000000000000u128, // 1 ETH in wei
            gas: 21000,
            gas_price: 20000000000, // 20 gwei
            data: vec![],
            nonce: 1,
            r#type: 2, // EIP-1559
        }
    }
    
    /// Create a sample account
    pub fn sample_account() -> MockAccount {
        MockAccount {
            address: "0xabcdefabcdefabcdefabcdefabcdefabcdefabcdef".to_string(),
            balance: 1000000000000000000u128, // 1 ETH
            nonce: 1,
            code: vec![],
            storage: HashMap::new(),
        }
    }
}

// Trait implementations for the mock clients
use crate::integration::{BitcoinClientExt, ExecutionClientExt};

#[async_trait]
impl BitcoinClientExt for MockBitcoinClient {
    async fn get_best_block_hash(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        self.simulate_delay().await;
        
        if self.should_fail() {
            let response = MockCallResponse::Error { 
                message: "Simulated Bitcoin client failure".to_string() 
            };
            self.record_call("get_best_block_hash", serde_json::Value::Null, response, start.elapsed()).await;
            return Err("Simulated failure".into());
        }
        
        let blockchain = self.blockchain.read().await;
        let hash = blockchain.best_block_hash.clone();
        
        self.record_call("get_best_block_hash", serde_json::Value::Null, MockCallResponse::Success, start.elapsed()).await;
        Ok(hash)
    }
    
    async fn get_block_height(&self) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        self.simulate_delay().await;
        
        if self.should_fail() {
            let response = MockCallResponse::Error { 
                message: "Simulated Bitcoin client failure".to_string() 
            };
            self.record_call("get_block_height", serde_json::Value::Null, response, start.elapsed()).await;
            return Err("Simulated failure".into());
        }
        
        let blockchain = self.blockchain.read().await;
        let height = blockchain.best_block_height;
        
        self.record_call("get_block_height", serde_json::Value::Null, MockCallResponse::Success, start.elapsed()).await;
        Ok(height)
    }
    
    async fn get_raw_transaction(&self, txid: &str) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        self.simulate_delay().await;
        
        let params = serde_json::json!({ "txid": txid });
        
        if self.should_fail() {
            let response = MockCallResponse::Error { 
                message: "Simulated Bitcoin client failure".to_string() 
            };
            self.record_call("get_raw_transaction", params, response, start.elapsed()).await;
            return Err("Simulated failure".into());
        }
        
        // Check mempool first
        let mempool = self.mempool.read().await;
        if let Some(tx) = mempool.transactions.get(txid) {
            let result = serde_json::to_value(tx).unwrap_or_default();
            self.record_call("get_raw_transaction", params, MockCallResponse::Success, start.elapsed()).await;
            return Ok(result);
        }
        
        // Then check blockchain
        let blockchain = self.blockchain.read().await;
        for block in blockchain.blocks.values() {
            if let Some(tx) = block.transactions.iter().find(|tx| tx.txid == txid) {
                let result = serde_json::to_value(tx).unwrap_or_default();
                self.record_call("get_raw_transaction", params, MockCallResponse::Success, start.elapsed()).await;
                return Ok(result);
            }
        }
        
        let response = MockCallResponse::Error { 
            message: "Transaction not found".to_string() 
        };
        self.record_call("get_raw_transaction", params, response, start.elapsed()).await;
        Err("Transaction not found".into())
    }
    
    async fn send_raw_transaction(&self, tx_hex: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        self.simulate_delay().await;
        
        let params = serde_json::json!({ "tx_hex": tx_hex });
        
        if self.should_fail() {
            let response = MockCallResponse::Error { 
                message: "Simulated Bitcoin client failure".to_string() 
            };
            self.record_call("send_raw_transaction", params, response, start.elapsed()).await;
            return Err("Simulated failure".into());
        }
        
        // Create a mock transaction
        let txid = format!("mock_tx_{}", uuid::Uuid::new_v4());
        let tx = MockBitcoinTransaction {
            txid: txid.clone(),
            version: 1,
            inputs: vec![],
            outputs: vec![],
            locktime: 0,
            size: tx_hex.len() as u32 / 2,
            weight: tx_hex.len() as u32,
            fee: 1000,
        };
        
        // Add to mempool
        let mut mempool = self.mempool.write().await;
        mempool.transactions.insert(txid.clone(), tx);
        
        self.record_call("send_raw_transaction", params, MockCallResponse::Success, start.elapsed()).await;
        Ok(txid)
    }
    
    async fn estimate_smart_fee(&self, conf_target: u16) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        self.simulate_delay().await;
        
        let params = serde_json::json!({ "conf_target": conf_target });
        
        if self.should_fail() {
            let response = MockCallResponse::Error { 
                message: "Simulated Bitcoin client failure".to_string() 
            };
            self.record_call("estimate_smart_fee", params, response, start.elapsed()).await;
            return Err("Simulated failure".into());
        }
        
        // Return mock fee rate based on confirmation target
        let fee_rate = match conf_target {
            1..=2 => 50.0,   // High priority
            3..=6 => 20.0,   // Medium priority
            _ => 10.0,       // Low priority
        };
        
        self.record_call("estimate_smart_fee", params, MockCallResponse::Success, start.elapsed()).await;
        Ok(fee_rate)
    }
}

#[async_trait]
impl ExecutionClientExt for MockExecutionClient {
    async fn get_block_number(&self) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        self.simulate_delay().await;
        
        if self.should_fail() {
            let response = MockCallResponse::Error { 
                message: "Simulated execution client failure".to_string() 
            };
            self.record_call("get_block_number", serde_json::Value::Null, response, start.elapsed()).await;
            return Err("Simulated failure".into());
        }
        
        let blockchain = self.blockchain.read().await;
        let block_number = blockchain.latest_block;
        
        self.record_call("get_block_number", serde_json::Value::Null, MockCallResponse::Success, start.elapsed()).await;
        Ok(block_number)
    }
    
    async fn get_balance(&self, address: &str, block_number: Option<u64>) -> Result<u128, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        self.simulate_delay().await;
        
        let params = serde_json::json!({ 
            "address": address, 
            "block_number": block_number 
        });
        
        if self.should_fail() {
            let response = MockCallResponse::Error { 
                message: "Simulated execution client failure".to_string() 
            };
            self.record_call("get_balance", params, response, start.elapsed()).await;
            return Err("Simulated failure".into());
        }
        
        let accounts = self.accounts.read().await;
        let balance = accounts.get(address)
            .map(|account| account.balance)
            .unwrap_or(0);
        
        self.record_call("get_balance", params, MockCallResponse::Success, start.elapsed()).await;
        Ok(balance)
    }
    
    async fn send_transaction(&self, tx_data: serde_json::Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        self.simulate_delay().await;
        
        if self.should_fail() {
            let response = MockCallResponse::Error { 
                message: "Simulated execution client failure".to_string() 
            };
            self.record_call("send_transaction", tx_data, response, start.elapsed()).await;
            return Err("Simulated failure".into());
        }
        
        // Create a mock transaction hash
        let tx_hash = format!("0x{:064x}", uuid::Uuid::new_v4().as_u128());
        
        // Create mock transaction
        let mock_tx = MockExecutionTransaction {
            hash: tx_hash.clone(),
            from: tx_data["from"].as_str().unwrap_or("0x0000000000000000000000000000000000000000").to_string(),
            to: tx_data["to"].as_str().map(|s| s.to_string()),
            value: tx_data["value"].as_str()
                .and_then(|s| s.strip_prefix("0x"))
                .and_then(|s| u128::from_str_radix(s, 16).ok())
                .unwrap_or(0),
            gas: tx_data["gas"].as_str()
                .and_then(|s| s.strip_prefix("0x"))
                .and_then(|s| u64::from_str_radix(s, 16).ok())
                .unwrap_or(21000),
            gas_price: tx_data["gasPrice"].as_str()
                .and_then(|s| s.strip_prefix("0x"))
                .and_then(|s| u64::from_str_radix(s, 16).ok())
                .unwrap_or(self.config.gas_price),
            data: tx_data["data"].as_str()
                .and_then(|s| s.strip_prefix("0x"))
                .and_then(|s| hex::decode(s).ok())
                .unwrap_or_default(),
            nonce: tx_data["nonce"].as_str()
                .and_then(|s| s.strip_prefix("0x"))
                .and_then(|s| u64::from_str_radix(s, 16).ok())
                .unwrap_or(0),
            r#type: 2, // EIP-1559
        };
        
        // Add to pending pool
        let mut tx_pool = self.tx_pool.write().await;
        tx_pool.pending.insert(tx_hash.clone(), mock_tx);
        
        self.record_call("send_transaction", tx_data, MockCallResponse::Success, start.elapsed()).await;
        Ok(tx_hash)
    }
    
    async fn get_transaction_receipt(&self, tx_hash: &str) -> Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        self.simulate_delay().await;
        
        let params = serde_json::json!({ "tx_hash": tx_hash });
        
        if self.should_fail() {
            let response = MockCallResponse::Error { 
                message: "Simulated execution client failure".to_string() 
            };
            self.record_call("get_transaction_receipt", params, response, start.elapsed()).await;
            return Err("Simulated failure".into());
        }
        
        // Check if transaction exists in blocks
        let blockchain = self.blockchain.read().await;
        for block in blockchain.blocks.values() {
            if let Some(tx) = block.transactions.iter().find(|tx| tx.hash == tx_hash) {
                let receipt = serde_json::json!({
                    "transactionHash": tx.hash,
                    "blockNumber": format!("0x{:x}", block.number),
                    "blockHash": block.hash,
                    "gasUsed": format!("0x{:x}", tx.gas),
                    "status": "0x1", // Success
                    "logs": []
                });
                
                self.record_call("get_transaction_receipt", params, MockCallResponse::Success, start.elapsed()).await;
                return Ok(Some(receipt));
            }
        }
        
        // Transaction not mined yet
        self.record_call("get_transaction_receipt", params, MockCallResponse::Success, start.elapsed()).await;
        Ok(None)
    }
    
    async fn call_contract(&self, call_data: serde_json::Value) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        self.simulate_delay().await;
        
        if self.should_fail() {
            let response = MockCallResponse::Error { 
                message: "Simulated execution client failure".to_string() 
            };
            self.record_call("call_contract", call_data, response, start.elapsed()).await;
            return Err("Simulated failure".into());
        }
        
        // Return mock call result
        let result = serde_json::json!("0x0000000000000000000000000000000000000000000000000000000000000001");
        
        self.record_call("call_contract", call_data, MockCallResponse::Success, start.elapsed()).await;
        Ok(result)
    }
}