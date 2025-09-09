//! Execution layer types and utilities
//! 
//! This module provides execution layer abstractions that work across different
//! Lighthouse versions or fall back to mock implementations when no features are enabled.

use serde::{Deserialize, Serialize};
use ethereum_types::{H256, U256, Address};
use std::fmt;
use crate::types::{ExecutionPayload, MainnetEthSpec};
use crate::error::{FacadeError, FacadeResult};

// Re-export from v7 when available
#[cfg(feature = "v7")]
pub use lighthouse_v7_execution_layer::*;

// Re-export from v4 when v7 not available
#[cfg(all(feature = "v4", not(feature = "v7")))]
pub use lighthouse_v4_execution_layer::*;

// Fallback types when no features enabled
#[cfg(not(any(feature = "v4", feature = "v7")))]
pub use crate::execution_layer::fallback::*;

#[cfg(not(any(feature = "v4", feature = "v7")))]
pub mod fallback {
    use super::*;
    
    pub type Hash256 = H256;
    pub type Uint256 = U256;
    
    /// Default execution endpoint
    pub const DEFAULT_EXECUTION_ENDPOINT: &str = "http://localhost:8551";
    
    /// Latest block tag
    pub const LATEST_TAG: &str = "latest";
    
    /// JWT authentication key
    #[derive(Debug, Clone)]
    pub struct JwtKey(pub [u8; 32]);
    
    impl JwtKey {
        pub fn from_slice(slice: &[u8]) -> Result<Self, FacadeError> {
            if slice.len() != 32 {
                return Err(FacadeError::Internal {
                    reason: format!("Invalid JWT key length: expected 32, got {}", slice.len()),
                });
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(slice);
            Ok(JwtKey(key))
        }
        
        pub fn random() -> Self {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            use std::time::SystemTime;
            
            let mut hasher = DefaultHasher::new();
            SystemTime::now().hash(&mut hasher);
            let hash = hasher.finish();
            
            let mut key = [0u8; 32];
            key[..8].copy_from_slice(&hash.to_le_bytes());
            
            JwtKey(key)
        }
    }
    
    /// Authentication module
    pub mod auth {
        pub use super::JwtKey;
        
        /// Authentication configuration
        #[derive(Debug, Clone)]
        pub struct Auth {
            pub jwt_key: Option<JwtKey>,
            pub endpoint: String,
        }
        
        impl Default for Auth {
            fn default() -> Self {
                Self {
                    jwt_key: None,
                    endpoint: "http://localhost:8551".to_string(),
                }
            }
        }
    }
    
    /// Execution layer error
    #[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
    pub enum Error {
        #[error("Request failed: {message}")]
        RequestFailed { message: String },
        
        #[error("Invalid response: {message}")]
        InvalidResponse { message: String },
        
        #[error("Connection failed: {message}")]
        ConnectionFailed { message: String },
        
        #[error("Authentication failed")]
        AuthenticationFailed,
        
        #[error("Payload invalid: {message}")]
        PayloadInvalid { message: String },
        
        #[error("Missing latest valid hash")]
        MissingLatestValidHash,
    }
    
    /// Payload status response
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PayloadStatus {
        pub status: PayloadStatusEnum,
        pub latest_valid_hash: Option<H256>,
        pub validation_error: Option<String>,
    }
    
    /// Payload status enumeration
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum PayloadStatusEnum {
        Valid,
        Invalid,
        Syncing,
        Accepted,
    }
    
    /// Forkchoice state
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ForkchoiceState {
        pub head_block_hash: H256,
        pub safe_block_hash: H256,
        pub finalized_block_hash: H256,
    }
    
    /// Payload attributes
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PayloadAttributes {
        pub timestamp: u64,
        pub prev_randao: H256,
        pub suggested_fee_recipient: Address,
        pub withdrawals: Vec<super::super::types::Withdrawal>,
    }
    
    /// Get payload response
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetPayloadResponse {
        pub execution_payload: super::super::types::ExecutionPayload,
        pub block_value: U256,
    }
    
    /// Forkchoice updated response
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ForkchoiceUpdatedResponse {
        pub payload_status: PayloadStatus,
        pub payload_id: Option<u64>,
    }
    
    /// Execute payload response
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExecutePayloadResponse {
        pub status: PayloadStatus,
        pub latest_valid_hash: Option<H256>,
        pub validation_error: Option<String>,
    }
    
    /// New payload response
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct NewPayloadResponse {
        pub status: PayloadStatus,
        pub latest_valid_hash: Option<H256>,
        pub validation_error: Option<String>,
    }
    
    /// Block by number query
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BlockByNumberQuery {
        pub block_number: String,
        pub full_transactions: bool,
    }
    
    /// HTTP JSON-RPC client
    #[derive(Debug, Clone)]
    pub struct HttpJsonRpc {
        endpoint: String,
        auth_token: Option<JwtKey>,
    }
    
    impl HttpJsonRpc {
        pub fn new(endpoint: String, auth_token: Option<JwtKey>) -> FacadeResult<Self> {
            Ok(Self {
                endpoint,
                auth_token,
            })
        }
        
        pub async fn upcheck(&self) -> FacadeResult<()> {
            // Mock implementation - always returns ok
            Ok(())
        }
    }
    
    /// Sensitive URL wrapper
    #[derive(Debug, Clone)]
    pub struct SensitiveUrl(pub String);
    
    impl SensitiveUrl {
        pub fn new(url: String) -> FacadeResult<Self> {
            Ok(SensitiveUrl(url))
        }
        
        pub fn full_url(&self) -> &str {
            &self.0
        }
    }
    
    impl std::fmt::Display for SensitiveUrl {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "<REDACTED_URL>")
        }
    }
    
    /// Database store abstraction
    pub trait Store: Send + Sync + 'static {
        type Error: std::error::Error + Send + Sync + 'static;
        
        fn get(&self, column: &str, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error>;
        fn put(&self, column: &str, key: &[u8], value: &[u8]) -> Result<(), Self::Error>;
        fn delete(&self, column: &str, key: &[u8]) -> Result<(), Self::Error>;
    }
    
    /// LevelDB store implementation (mock)
    #[derive(Debug)]
    pub struct LevelDB;
    
    impl LevelDB {
        pub fn open<P: AsRef<std::path::Path>>(path: P) -> FacadeResult<Self> {
            let _ = path.as_ref(); // Use the path parameter
            Ok(LevelDB)
        }
    }
    
    /// Memory store implementation (mock)
    #[derive(Debug, Default)]
    pub struct MemoryStore;
    
    impl MemoryStore {
        pub fn new() -> Self {
            Self::default()
        }
    }
    
    /// Get key for column (utility function)
    pub fn get_key_for_col(column: &str, key: &[u8]) -> Vec<u8> {
        let mut result = column.as_bytes().to_vec();
        result.extend_from_slice(key);
        result
    }
    
    /// Execution block with transactions
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExecutionBlockWithTransactions {
        pub hash: H256,
        pub parent_hash: H256,
        pub number: u64,
        pub timestamp: u64,
        pub transactions: Vec<ExecutionTransaction>,
    }
    
    /// Execution transaction
    #[derive(Debug, Clone, Serialize, Deserialize)]  
    pub struct ExecutionTransaction {
        pub hash: H256,
        pub from: Address,
        pub to: Option<Address>,
        pub value: U256,
        pub gas: u64,
        pub gas_price: U256,
    }
}