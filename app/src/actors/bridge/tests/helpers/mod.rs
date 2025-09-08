//! Test Helpers for Bridge Actors
//! 
//! Common utilities and mock implementations for bridge testing

use actix::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use bitcoin::{Address, Network, Txid};
use serde_json::Value;

use crate::types::*;
use crate::actors::bridge::{
    BridgeError, ActorType, BridgeSystemConfig,
    // Import specific message types 
    BridgeCoordinationMessage,
};
use ethereum_types::{H160, H256, U256};

/// Test configuration for bridge actors
#[derive(Debug, Clone)]
pub struct TestBridgeConfig {
    pub bitcoin_network: Network,
    pub federation_size: usize,
    pub confirmation_blocks: u32,
    pub rpc_timeout: Duration,
}

impl Default for TestBridgeConfig {
    fn default() -> Self {
        Self {
            bitcoin_network: Network::Regtest,
            federation_size: 3,
            confirmation_blocks: 6,
            rpc_timeout: Duration::from_secs(30),
        }
    }
}

/// Mock Bitcoin RPC for testing
pub struct MockBitcoinRpc {
    pub network: Network,
    pub mock_responses: Arc<std::sync::Mutex<std::collections::HashMap<String, Value>>>,
}

impl MockBitcoinRpc {
    pub fn new(network: Network) -> Self {
        Self {
            network,
            mock_responses: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub fn set_mock_response(&self, method: String, response: Value) {
        let mut responses = self.mock_responses.lock().unwrap();
        responses.insert(method, response);
    }
}

/// Mock Ethereum client for testing
pub struct MockEthereumClient {
    pub chain_id: u64,
    pub mock_responses: Arc<std::sync::Mutex<std::collections::HashMap<String, Value>>>,
}

impl MockEthereumClient {
    pub fn new(chain_id: u64) -> Self {
        Self {
            chain_id,
            mock_responses: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub fn set_mock_response(&self, method: String, response: Value) {
        let mut responses = self.mock_responses.lock().unwrap();
        responses.insert(method, response);
    }
}

/// Test utilities for creating test data
pub struct TestDataBuilder;

impl TestDataBuilder {
    /// Create a test Bitcoin transaction ID
    pub fn random_txid() -> Txid {
        use bitcoin::hashes::Hash;
        use rand::Rng;
        
        let mut rng = rand::thread_rng();
        let bytes: [u8; 32] = rng.gen();
        Txid::from_byte_array(bytes)
    }

    /// Create a test Bitcoin address
    pub fn test_bitcoin_address() -> Address {
        Address::from_str("bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080").unwrap()
            .require_network(Network::Regtest).unwrap()
    }

    /// Create a test Ethereum address
    pub fn test_ethereum_address() -> H160 {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        H160::from(rng.gen::<[u8; 20]>())
    }

    /// Create a test peg-in request
    pub fn test_pegin_request() -> PegInRequest {
        PegInRequest {
            bitcoin_txid: Self::random_txid(),
            output_index: 0,
            amount: bitcoin::Amount::from_sat(100_000),
            recipient: Self::test_ethereum_address(),
            confirmation_count: 6,
        }
    }

    /// Create a test peg-out request  
    pub fn test_pegout_request() -> PegOutRequest {
        PegOutRequest {
            burn_tx_hash: H256::random(),
            amount: U256::from(100_000),
            recipient: Self::test_bitcoin_address(),
            fee_rate: 10,
        }
    }
}

/// Actor system test harness
pub struct ActorTestHarness {
    pub system: actix::SystemRunner,
}

impl ActorTestHarness {
    pub fn new() -> Self {
        let system = actix::System::new();
        Self { system }
    }

    pub async fn run_test<F, Fut, T>(&self, test_fn: F) -> T 
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        test_fn().await
    }
}

/// Assertion helpers for bridge testing
pub struct BridgeAssertions;

impl BridgeAssertions {
    /// Assert that a peg-in operation succeeded
    pub fn assert_pegin_success(result: &Result<PegInResponse, BridgeError>) {
        match result {
            Ok(response) => {
                assert!(!response.alys_tx_hash.is_zero());
                assert!(response.amount > U256::zero());
            }
            Err(e) => panic!("Peg-in should have succeeded but failed with: {:?}", e),
        }
    }

    /// Assert that a peg-out operation succeeded
    pub fn assert_pegout_success(result: &Result<PegOutResponse, BridgeError>) {
        match result {
            Ok(response) => {
                assert!(!response.bitcoin_txid.to_string().is_empty());
                assert!(response.amount.as_sat() > 0);
            }
            Err(e) => panic!("Peg-out should have succeeded but failed with: {:?}", e),
        }
    }

    /// Assert that an error is of expected type
    pub fn assert_bridge_error_type(result: &Result<(), BridgeError>, expected_type: &str) {
        match result {
            Ok(_) => panic!("Expected error but operation succeeded"),
            Err(e) => {
                let error_str = format!("{:?}", e);
                assert!(error_str.contains(expected_type), 
                       "Expected error type '{}' but got: {:?}", expected_type, e);
            }
        }
    }
}

/// Async test utilities
#[macro_export]
macro_rules! async_test {
    ($test:ident) => {
        #[actix::test]
        async fn $test() {
            $test().await
        }
    };
}

/// Mock bridge configuration for testing
pub fn test_bridge_config() -> BridgeSystemConfig {
    BridgeSystemConfig::default()
}

/// Mock peg-in request for testing
#[derive(Debug, Clone)]
pub struct PegInRequest {
    pub bitcoin_txid: Txid,
    pub output_index: u32,
    pub amount: bitcoin::Amount,
    pub recipient: H160,
    pub confirmation_count: u32,
}

/// Mock peg-in response for testing
#[derive(Debug, Clone)]
pub struct PegInResponse {
    pub alys_tx_hash: H256,
    pub amount: U256,
    pub recipient: H160,
}

/// Mock peg-out request for testing
#[derive(Debug, Clone)]  
pub struct PegOutRequest {
    pub burn_tx_hash: H256,
    pub amount: U256,
    pub recipient: Address,
    pub fee_rate: u64,
}

/// Mock peg-out response for testing
#[derive(Debug, Clone)]
pub struct PegOutResponse {
    pub bitcoin_txid: Txid,
    pub amount: bitcoin::Amount,
    pub recipient: Address,
}

/// Mock governance message for testing
#[derive(Debug, Clone)]
pub struct GovernanceMessage {
    pub msg_type: String,
    pub proposal_id: String,
    pub data: serde_json::Value,
    pub timestamp: std::time::SystemTime,
}

/// Mock consensus message for testing
#[derive(Debug, Clone)]
pub struct ConsensusMessage {
    pub msg_type: String,
    pub block_hash: H256,
    pub block_number: u64,
    pub data: serde_json::Value,
}

/// Mock message enums for testing
pub mod mock_messages {
    use super::*;
    use actix::prelude::*;

    #[derive(Debug, Clone, Message)]
    #[rtype(result = "Result<PegInResponse, BridgeError>")]
    pub enum PegInMessage {
        Initialize,
        ProcessRequest { request: PegInRequest },
        ValidateTransaction { txid: Txid, output_index: u32 },
        CheckConfirmations { txid: Txid, required_confirmations: u32 },
        MintTokens { recipient: H160, amount: U256, bitcoin_txid: Txid },
        GetStatus { pegin_id: String },
        CancelRequest { pegin_id: String, reason: String },
        HandleTimeout { pegin_id: String, timeout_type: String },
        GetMetrics,
        Shutdown,
    }

    #[derive(Debug, Clone, Message)]
    #[rtype(result = "Result<PegOutResponse, BridgeError>")]
    pub enum PegOutMessage {
        Initialize,
        ProcessRequest { request: PegOutRequest },
        ValidateBurnEvent { burn_tx_hash: H256, burn_amount: U256, recipient: Address },
        CreateBitcoinTransaction { recipient: Address, amount: bitcoin::Amount, fee_rate: u64 },
        SignTransaction { tx_bytes: Vec<u8>, input_indices: Vec<u32> },
        BroadcastTransaction { signed_tx_bytes: Vec<u8> },
        GetStatus { pegout_id: String },
        CancelRequest { pegout_id: String, reason: String },
        HandleTimeout { pegout_id: String, timeout_type: String },
        GetMetrics,
        Shutdown,
    }

    #[derive(Debug, Clone, Message)]
    #[rtype(result = "Result<(), BridgeError>")]
    pub enum StreamMessage {
        Initialize,
        EstablishConnection { peer_id: String, endpoint: String },
        SendGovernanceMessage { message: GovernanceMessage, target_peers: Vec<String> },
        ReceiveGovernanceMessage { message: GovernanceMessage, from_peer: String },
        SendConsensusMessage { message: ConsensusMessage, target_peers: Vec<String> },
        ReceiveConsensusMessage { message: ConsensusMessage, from_peer: String },
        SubscribeToEvents { event_types: Vec<String>, callback_addr: Option<actix::Recipient<serde_json::Value>> },
        UnsubscribeFromEvents { event_types: Vec<String> },
        GetConnectionStatus,
        DisconnectPeer { peer_id: String, reason: String },
        HandleConnectionError { peer_id: String, error: String },
        GetMetrics,
        Shutdown,
    }
}

/// Additional BridgeError constructors for testing
impl BridgeError {
    pub fn actor_timeout(actor_type: ActorType, timeout: Duration) -> Self {
        BridgeError::RequestTimeout {
            request_id: format!("{:?}_actor_timeout", actor_type),
            timeout,
        }
    }

    pub fn actor_communication(message: String) -> Self {
        BridgeError::NetworkError(format!("Actor communication failed: {}", message))
    }

    pub fn system_recovery(component: String, issue: String) -> Self {
        BridgeError::InternalError(format!("System recovery needed for {}: {}", component, issue))
    }
}