//! Type definitions for the Lighthouse facade
//!
//! This module provides all types needed for Lighthouse operations, including
//! execution payloads, forkchoice states, and client abstractions.

use serde::{Deserialize, Serialize};
use ethereum_types::{H256, U256, Address};
use std::time::{Duration, SystemTime};
use std::fmt;
use async_trait::async_trait;
use crate::error::FacadeResult;

// Conditional imports for real Lighthouse types
#[cfg(feature = "v4")]
use lighthouse_v4_types as v4_types;
#[cfg(feature = "v7")]  
use lighthouse_v7_types as v7_types;
#[cfg(feature = "v7")]
use lighthouse_v7_execution_layer as v7_execution;

#[cfg(feature = "v4")]
use lighthouse_v4_bls as v4_bls;
#[cfg(feature = "v4")]
use lighthouse_v4_execution_layer as v4_execution;
#[cfg(feature = "v7")]
use lighthouse_v7_bls as v7_bls;

// Type aliases for common Ethereum types
pub type BlockHash = H256;
pub type Hash256 = H256;
pub type PayloadId = u64;
pub type Uint256 = U256;

// Additional commonly needed types
pub type FixedVector<T> = Vec<T>; // Simplified for compatibility
pub type VariableList<T> = Vec<T>; // Simplified for compatibility
pub type Transactions = Vec<Vec<u8>>; // Transaction list
pub type Withdrawals = Vec<Withdrawal>; // Withdrawals list

// BitVector and BitList types for SSZ compatibility
pub type BitVector = Vec<bool>;
pub type BitList = Vec<bool>;

// ExecutionBlockHash - use real Lighthouse types when available
#[cfg(feature = "v7")]
pub use v7_types::ExecutionBlockHash;

#[cfg(all(feature = "v4", not(feature = "v7")))]
pub use v4_types::ExecutionBlockHash;

#[cfg(not(any(feature = "v4", feature = "v7")))]
pub type ExecutionBlockHash = H256;

// Address type - use Lighthouse's Address when available
#[cfg(feature = "v7")]
pub use v7_types::Address as LighthouseAddress;

#[cfg(all(feature = "v4", not(feature = "v7")))]
pub use v4_types::Address as LighthouseAddress;

#[cfg(not(any(feature = "v4", feature = "v7")))]
pub type LighthouseAddress = Address;

// BLS and cryptographic types - use real Lighthouse types when available
#[cfg(all(feature = "v7", not(feature = "v4")))]
pub use v7_bls::{PublicKey, SecretKey, Signature, AggregateSignature, Keypair};

#[cfg(all(feature = "v4", not(feature = "v7")))]
pub use v4_bls::{PublicKey, SecretKey, Signature, AggregateSignature, Keypair};

#[cfg(all(feature = "v4", feature = "v7"))]
pub use v7_bls::{PublicKey, SecretKey, Signature, AggregateSignature, Keypair}; // Default to v7 when both available

// Ethereum specification types - use real Lighthouse types when available
#[cfg(feature = "v7")]
pub use v7_types::{MainnetEthSpec, EthSpec};

#[cfg(all(feature = "v4", not(feature = "v7")))]
pub use v4_types::{MainnetEthSpec, EthSpec};

// ExecutionPayload types - use concrete MainnetEthSpec
#[cfg(feature = "v7")]
pub type ExecutionPayload = v7_types::ExecutionPayload<MainnetEthSpec>;

#[cfg(all(feature = "v4", not(feature = "v7")))]
pub type ExecutionPayload = v4_types::ExecutionPayload<MainnetEthSpec>;

// PayloadStatus types
#[cfg(feature = "v7")]
pub use v7_execution::PayloadStatus;

#[cfg(all(feature = "v4", not(feature = "v7")))]
pub use v4_execution::PayloadStatus;

// ForkchoiceState types  
#[cfg(feature = "v7")]
pub use v7_execution::ForkchoiceState;

#[cfg(all(feature = "v4", not(feature = "v7")))]
pub use v4_execution::ForkchoiceState;

// PayloadAttributes types
#[cfg(feature = "v7")]
pub use v7_execution::PayloadAttributes;

#[cfg(all(feature = "v4", not(feature = "v7")))]
pub use v4_execution::PayloadAttributes;

// Additional execution layer types - use concrete MainnetEthSpec
#[cfg(feature = "v7")]
pub type GetPayloadResponse = v7_execution::GetPayloadResponse<MainnetEthSpec>;
#[cfg(feature = "v7")]
pub use v7_execution::ForkchoiceUpdatedResponse;

#[cfg(all(feature = "v4", not(feature = "v7")))]
pub type GetPayloadResponse = v4_execution::GetPayloadResponse<MainnetEthSpec>;
#[cfg(all(feature = "v4", not(feature = "v7")))]
pub use v4_execution::ForkchoiceUpdatedResponse;

/// JWT key for authentication - always use our wrapper for serialization
#[derive(Debug, Clone)]  
pub struct JwtKey(pub [u8; 32]);

impl Serialize for JwtKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for JwtKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(JwtKey(<[u8; 32]>::deserialize(deserializer)?))
    }
}

// ==== MOCK TYPES FOR TESTING (only when no features enabled) ====

// Fallback mock types when no features are enabled (for testing)
#[cfg(not(any(feature = "v4", feature = "v7")))]
pub type PublicKey = [u8; 48];
#[cfg(not(any(feature = "v4", feature = "v7")))]
pub type SecretKey = [u8; 32];
#[cfg(not(any(feature = "v4", feature = "v7")))]
pub type Signature = [u8; 96];
#[cfg(not(any(feature = "v4", feature = "v7")))]
pub type AggregateSignature = [u8; 96];

/// Fallback mock Keypair for testing
#[cfg(not(any(feature = "v4", feature = "v7")))]
#[derive(Debug, Clone)]
pub struct Keypair {
    /// Secret key
    pub secret_key: SecretKey,
    /// Public key
    pub public_key: PublicKey,
}

// Fallback mock types when no features are enabled
#[cfg(not(any(feature = "v4", feature = "v7")))]
#[derive(Debug, Clone)]
pub struct MainnetEthSpec;

#[cfg(not(any(feature = "v4", feature = "v7")))]
pub trait EthSpec: Clone + Sync + Send + fmt::Debug + 'static {
    /// Maximum number of validators per committee
    const MAX_VALIDATORS_PER_COMMITTEE: usize = 2048;
    /// Slots per epoch
    const SLOTS_PER_EPOCH: usize = 32;
}

#[cfg(not(any(feature = "v4", feature = "v7")))]
impl EthSpec for MainnetEthSpec {}


#[cfg(not(any(feature = "v4", feature = "v7")))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPayload {
    pub parent_hash: H256,
    pub fee_recipient: Address,
    pub state_root: H256,
    pub receipts_root: H256,
    pub logs_bloom: Vec<u8>,
    pub prev_randao: H256,
    pub block_number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub extra_data: Vec<u8>,
    pub base_fee_per_gas: U256,
    pub block_hash: H256,
    pub transactions: Vec<Vec<u8>>,
    pub withdrawals: Vec<Withdrawal>,
}

// Capella-specific execution payload for compatibility
#[cfg(not(any(feature = "v4", feature = "v7")))]
pub type ExecutionPayloadCapella = ExecutionPayload;

#[cfg(not(any(feature = "v4", feature = "v7")))]
impl ExecutionPayload {
    pub fn default_test_payload() -> Self {
        Self {
            parent_hash: H256::zero(),
            fee_recipient: Address::zero(),
            state_root: H256::zero(),
            receipts_root: H256::zero(),
            logs_bloom: vec![0; 256],
            prev_randao: H256::zero(),
            block_number: 0,
            gas_limit: 30000000,
            gas_used: 0,
            timestamp: 0,
            extra_data: Vec::new(),
            base_fee_per_gas: U256::zero(),
            block_hash: H256::zero(),
            transactions: Vec::new(),
            withdrawals: Vec::new(),
        }
    }
}

#[cfg(not(any(feature = "v4", feature = "v7")))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Withdrawal {
    pub index: u64,
    pub validator_index: u64,
    pub address: Address,
    pub amount: u64,
}

#[cfg(not(any(feature = "v4", feature = "v7")))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadStatus {
    pub status: PayloadStatusKind,
    pub latest_valid_hash: Option<H256>,
    pub validation_error: Option<String>,
}

#[cfg(not(any(feature = "v4", feature = "v7")))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PayloadStatusKind {
    Valid,
    Invalid,
    Syncing,
    Accepted,
}

#[cfg(not(any(feature = "v4", feature = "v7")))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkchoiceState {
    pub head_block_hash: H256,
    pub safe_block_hash: H256,
    pub finalized_block_hash: H256,
}

#[cfg(not(any(feature = "v4", feature = "v7")))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadAttributes {
    pub timestamp: u64,
    pub prev_randao: H256,
    pub suggested_fee_recipient: Address,
    pub withdrawals: Vec<Withdrawal>,
}

#[cfg(not(any(feature = "v4", feature = "v7")))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkchoiceUpdatedResponse {
    pub payload_status: PayloadStatus,
    pub payload_id: Option<PayloadId>,
}

#[cfg(not(any(feature = "v4", feature = "v7")))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPayloadResponse {
    pub execution_payload: ExecutionPayload,
    pub block_value: U256,
}

/// Beacon block header structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BeaconBlockHeader {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: H256,
    pub state_root: H256,
    pub body_root: H256,
}

// ==== ADDITIONAL TRAIT DEFINITIONS ====

/// Facade operation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FacadeMode {
    /// V4 only mode
    V4Only,
    /// V7 only mode
    V7Only,
    /// Dual mode with both v4 and v7
    Dual,
    /// Mock mode for testing
    Mock,
    /// Automatic mode selection
    Automatic,
    /// Migration mode
    Migration,
}

impl Default for FacadeMode {
    fn default() -> Self {
        Self::Mock
    }
}

/// Health status for clients
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Overall health
    pub healthy: bool,
    /// Sync status
    pub sync_status: SyncStatus,
    /// Peer count
    pub peer_count: u32,
    /// Last successful operation time
    pub last_success: Option<SystemTime>,
    /// Error details if unhealthy
    pub error_details: Option<String>,
    /// Health metrics
    pub metrics: HealthMetrics,
}

/// Sync status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncStatus {
    /// Fully synced
    Synced,
    /// Syncing
    Syncing,
    /// Stalled
    Stalled,
    /// Error
    Error,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self::Syncing
    }
}

/// Health metrics structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HealthMetrics {
    /// Response time in milliseconds
    pub response_time_ms: u64,
    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,
    /// Success count
    pub success_count: u64,
    /// Error count
    pub error_count: u64,
    /// Request count
    pub request_count: u64,
    /// Memory usage in MB
    pub memory_usage_mb: u64,
    /// CPU usage percentage (0.0 to 100.0)
    pub cpu_usage: f64,
}

/// Client version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientVersion {
    /// V4 client version
    V4 { version: String },
    /// V7 client version
    V7 { version: String },
    /// Mock client version
    Mock { version: String },
}

/// Facade statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FacadeStats {
    /// Total requests processed
    pub total_requests: u64,
    /// Successful requests
    pub successful_requests: u64,
    /// Failed requests
    pub failed_requests: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: u64,
    /// Uptime since start (in seconds)
    pub uptime_seconds: u64,
    /// Client-specific stats
    pub v4_stats: Option<ClientStats>,
    /// Client-specific stats
    pub v7_stats: Option<ClientStats>,
}

impl FacadeStats {
    /// Record a successful operation
    pub fn record_success(&mut self, duration_ms: u64, _version: ClientVersion) {
        self.total_requests += 1;
        self.successful_requests += 1;
        // Update average response time
        if self.total_requests == 1 {
            self.avg_response_time_ms = duration_ms;
        } else {
            self.avg_response_time_ms = 
                (self.avg_response_time_ms * (self.total_requests - 1) + duration_ms) / self.total_requests;
        }
    }
    
    /// Record a failed operation
    pub fn record_failure(&mut self, duration_ms: u64, _version: ClientVersion) {
        self.total_requests += 1;
        self.failed_requests += 1;
        // Update average response time
        if self.total_requests == 1 {
            self.avg_response_time_ms = duration_ms;
        } else {
            self.avg_response_time_ms = 
                (self.avg_response_time_ms * (self.total_requests - 1) + duration_ms) / self.total_requests;
        }
    }
}

/// Per-client statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientStats {
    /// Requests to this client
    pub requests: u64,
    /// Successful responses
    pub successes: u64,
    /// Failed responses
    pub failures: u64,
    /// Average response time
    pub avg_response_time_ms: u64,
    /// Last successful operation
    pub last_success: Option<SystemTime>,
    /// Last error
    pub last_error: Option<String>,
}

/// Context for type conversions between Lighthouse versions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversionContext {
    /// Conversion statistics
    pub stats: ConversionStats,
    /// Conversion options
    pub options: ConversionOptions,
}

impl ConversionContext {
    /// Create a new conversion context
    pub fn new() -> Self {
        Self::default()
    }
}

/// Statistics for conversions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversionStats {
    /// Successful conversions
    pub successes: u64,
    /// Failed conversions
    pub failures: u64,
    /// Total time spent converting
    pub total_time_us: u64,
}

/// Options for conversions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversionOptions {
    /// Strict mode (fail on any conversion error)
    pub strict_mode: bool,
    /// Log conversion errors
    pub log_errors: bool,
    /// Allow lossy conversions
    pub allow_lossy: bool,
    /// Strict validation
    pub strict_validation: bool,
    /// Use default values for missing fields
    pub use_defaults: bool,
    /// Downgrade features when converting to older versions
    pub downgrade_features: bool,
}

/// Migration-specific statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrationStats {
    /// V4 client requests
    pub v4_requests: u64,
    /// V7 client requests
    pub v7_requests: u64,
    /// Migration start time
    pub migration_start: Option<SystemTime>,
    /// Migration progress (0.0 to 1.0)
    pub migration_progress: f64,
    /// Rollback count
    pub rollback_count: u32,
}

/// BLS signature set for verification
#[derive(Debug, Clone)]
pub struct SignatureSet {
    /// Public key
    pub public_key: PublicKey,
    /// Message being signed
    pub message: Vec<u8>,
    /// Signature
    pub signature: Signature,
}

/// Unsigned trait for unsigned values
pub trait Unsigned: Clone + fmt::Debug + Serialize + for<'a> Deserialize<'a> {
    /// Convert to u64
    fn to_u64(&self) -> u64;
}

/// Core trait for Lighthouse client operations
#[async_trait::async_trait]
pub trait LighthouseClient: Send + Sync {
    /// Submit a new execution payload
    async fn new_payload(&self, payload: ExecutionPayload) -> FacadeResult<PayloadStatus>;
    
    /// Update forkchoice and optionally trigger block building
    async fn forkchoice_updated(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> FacadeResult<ForkchoiceUpdatedResponse>;
    
    /// Get execution payload by ID
    async fn get_payload(&self, payload_id: PayloadId) -> FacadeResult<GetPayloadResponse>;
    
    /// Check client health
    async fn health_check(&self) -> FacadeResult<HealthStatus>;
    
    /// Check if client is ready
    async fn is_ready(&self) -> FacadeResult<bool>;
    
    /// Get client version
    fn version(&self) -> ClientVersion;
}

impl Unsigned for u64 {
    fn to_u64(&self) -> u64 {
        *self
    }
}

/// Sensitive URL wrapper for authentication
#[derive(Debug, Clone)]
pub struct FacadeSensitiveUrl {
    url: String,
}

impl FacadeSensitiveUrl {
    /// Create from string
    pub fn parse(url: &str) -> Result<Self, String> {
        Ok(Self { url: url.to_string() })
    }
    
    /// Get the URL as string
    pub fn as_str(&self) -> &str {
        &self.url
    }
}

// Mock implementations for when no features are enabled
#[cfg(not(any(feature = "v4", feature = "v7")))]
impl Keypair {
    /// Generate a new random keypair
    pub fn random() -> Self {
        Self {
            secret_key: [0u8; 32], // Mock implementation
            public_key: [0u8; 48], // Mock implementation
        }
    }
    
    /// Get the public key
    pub fn pk(&self) -> PublicKey {
        self.public_key
    }
    
    /// Get the secret key
    pub fn sk(&self) -> &SecretKey {
        &self.secret_key
    }
}

#[cfg(not(any(feature = "v4", feature = "v7")))]
impl JwtKey {
    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        if hex.len() != 64 {
            return Err("Invalid hex length".to_string());
        }
        Ok(Self([0u8; 32])) // Mock implementation
    }
}

// Module re-exports for compatibility
pub mod bls {
    pub use super::{PublicKey, SecretKey, Signature, AggregateSignature, Keypair, SignatureSet};
}

pub mod execution_layer {
    pub use super::{ExecutionPayload, PayloadStatus, ForkchoiceState, PayloadAttributes, ExecutionBlockHash, ForkchoiceUpdatedResponse, GetPayloadResponse};
}

pub mod sensitive_url {
    pub use super::FacadeSensitiveUrl;
}

/// Store module compatibility
pub mod store {
    use super::*;
    use crate::error::{FacadeError, FacadeResult};
    
    /// Re-export MainnetEthSpec for compatibility
    pub use super::MainnetEthSpec;
    
    /// Item store trait for persisting data
    pub trait ItemStore<E: EthSpec>: Send + Sync {
        /// Store an item
        fn put<I: Item>(&self, key: &str, item: &I) -> FacadeResult<()>;
        
        /// Retrieve an item
        fn get<I: Item>(&self, key: &str) -> FacadeResult<Option<I>>;
        
        /// Delete an item
        fn delete<I: Item>(&self, key: &str) -> FacadeResult<()>;
    }
    
    /// Item trait for storable items
    pub trait Item: Serialize + for<'de> Deserialize<'de> + Send + Sync {}
    
    /// Key-value store operation
    pub enum KeyValueStoreOp {
        /// Put operation
        Put(String, Vec<u8>),
        /// Delete operation
        Delete(String),
    }
}