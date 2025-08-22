//! Type definitions for the Lighthouse compatibility layer
//!
//! This module provides unified type definitions that abstract over both Lighthouse v4 and v5,
//! enabling seamless migration and type conversion between versions.

use crate::error::{CompatError, CompatResult};
use ethereum_types::{Address, H256, U256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Unified block hash type
pub type BlockHash = H256;

/// Unified hash type
pub type Hash256 = H256;

/// Unified payload ID type
pub type PayloadId = String;

/// Unified execution payload that works with both v4 and v5
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionPayload {
    /// Parent block hash
    pub parent_hash: BlockHash,
    
    /// Fee recipient address
    pub fee_recipient: Address,
    
    /// State root
    pub state_root: Hash256,
    
    /// Receipts root
    pub receipts_root: Hash256,
    
    /// Logs bloom filter
    pub logs_bloom: Vec<u8>,
    
    /// Previous randao value
    pub prev_randao: Hash256,
    
    /// Block number
    pub block_number: u64,
    
    /// Gas limit
    pub gas_limit: u64,
    
    /// Gas used
    pub gas_used: u64,
    
    /// Block timestamp
    pub timestamp: u64,
    
    /// Extra data
    pub extra_data: Vec<u8>,
    
    /// Base fee per gas
    pub base_fee_per_gas: U256,
    
    /// Block hash
    pub block_hash: BlockHash,
    
    /// Transactions
    pub transactions: Vec<Vec<u8>>,
    
    /// Withdrawals (Capella+)
    pub withdrawals: Option<Vec<Withdrawal>>,
    
    /// Blob gas used (Deneb+, v5 only)
    pub blob_gas_used: Option<u64>,
    
    /// Excess blob gas (Deneb+, v5 only)  
    pub excess_blob_gas: Option<u64>,
    
    /// Parent beacon block root (Deneb+, v5 only)
    pub parent_beacon_block_root: Option<Hash256>,
}

/// Unified withdrawal type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Withdrawal {
    /// Withdrawal index
    pub index: u64,
    
    /// Validator index
    pub validator_index: u64,
    
    /// Withdrawal address
    pub address: Address,
    
    /// Withdrawal amount (in Gwei)
    pub amount: u64,
}

/// Unified forkchoice state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkchoiceState {
    /// Head block hash
    pub head_block_hash: BlockHash,
    
    /// Safe block hash
    pub safe_block_hash: BlockHash,
    
    /// Finalized block hash
    pub finalized_block_hash: BlockHash,
    
    /// Justified block hash (v5 only)
    pub justified_block_hash: Option<BlockHash>,
}

/// Unified payload attributes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PayloadAttributes {
    /// Timestamp
    pub timestamp: u64,
    
    /// Previous randao
    pub prev_randao: Hash256,
    
    /// Suggested fee recipient
    pub suggested_fee_recipient: Address,
    
    /// Withdrawals (Capella+)
    pub withdrawals: Option<Vec<Withdrawal>>,
    
    /// Parent beacon block root (Deneb+, v5 only)
    pub parent_beacon_block_root: Option<Hash256>,
}

/// Unified payload status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PayloadStatus {
    /// Status type
    pub status: PayloadStatusType,
    
    /// Latest valid hash
    pub latest_valid_hash: Option<BlockHash>,
    
    /// Validation error message
    pub validation_error: Option<String>,
}

/// Payload status types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PayloadStatusType {
    /// Valid payload
    Valid,
    
    /// Invalid payload
    Invalid,
    
    /// Syncing
    Syncing,
    
    /// Accepted
    Accepted,
    
    /// Invalid block hash
    InvalidBlockHash,
    
    /// Invalid terminal block
    InvalidTerminalBlock,
}

/// Unified forkchoice updated response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkchoiceUpdatedResponse {
    /// Payload status
    pub payload_status: PayloadStatus,
    
    /// Payload ID for building
    pub payload_id: Option<PayloadId>,
}

/// Unified get payload response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetPayloadResponse {
    /// Execution payload
    pub execution_payload: ExecutionPayload,
    
    /// Block value (v5+)
    pub block_value: Option<U256>,
    
    /// BLS to execution changes (v5+)
    pub bls_to_execution_changes: Option<Vec<BlsToExecutionChange>>,
    
    /// Blob bundle (Deneb+, v5 only)
    pub blob_bundle: Option<BlobBundle>,
    
    /// Should override builder (v5+)
    pub should_override_builder: Option<bool>,
}

/// BLS to execution change (v5+)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlsToExecutionChange {
    /// Message
    pub message: BlsToExecutionChangeMessage,
    
    /// Signature
    pub signature: Vec<u8>,
}

/// BLS to execution change message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlsToExecutionChangeMessage {
    /// Validator index
    pub validator_index: u64,
    
    /// From BLS public key
    pub from_bls_pubkey: Vec<u8>,
    
    /// To execution address
    pub to_execution_address: Address,
}

/// Blob bundle (Deneb+, v5 only)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlobBundle {
    /// Commitments
    pub commitments: Vec<Vec<u8>>,
    
    /// Proofs
    pub proofs: Vec<Vec<u8>>,
    
    /// Blobs
    pub blobs: Vec<Vec<u8>>,
}

/// Version-agnostic client interface
#[async_trait::async_trait]
pub trait LighthouseClient: Send + Sync {
    /// Execute new payload
    async fn new_payload(&self, payload: ExecutionPayload) -> CompatResult<PayloadStatus>;
    
    /// Update forkchoice
    async fn forkchoice_updated(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> CompatResult<ForkchoiceUpdatedResponse>;
    
    /// Get payload
    async fn get_payload(&self, payload_id: PayloadId) -> CompatResult<GetPayloadResponse>;
    
    /// Check if client is ready
    async fn is_ready(&self) -> CompatResult<bool>;
    
    /// Get client version
    fn version(&self) -> ClientVersion;
    
    /// Health check
    async fn health_check(&self) -> CompatResult<HealthStatus>;
}

/// Client version information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClientVersion {
    /// Lighthouse v4
    V4 { revision: String },
    
    /// Lighthouse v5
    V5 { version: String },
}

/// Health status information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthStatus {
    /// Overall health
    pub healthy: bool,
    
    /// Sync status
    pub sync_status: SyncStatus,
    
    /// Peer count
    pub peer_count: u32,
    
    /// Last successful request time
    pub last_success: Option<SystemTime>,
    
    /// Error details if unhealthy
    pub error_details: Option<String>,
    
    /// Performance metrics
    pub metrics: HealthMetrics,
}

/// Sync status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncStatus {
    /// Synced
    Synced,
    
    /// Syncing with progress
    Syncing { progress: f64 },
    
    /// Not syncing
    NotSyncing,
    
    /// Sync error
    Error { message: String },
}

/// Health metrics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthMetrics {
    /// Average response time
    pub avg_response_time: Duration,
    
    /// Error rate over last period
    pub error_rate: f64,
    
    /// Request count over last period
    pub request_count: u64,
    
    /// Memory usage (MB)
    pub memory_usage_mb: u64,
    
    /// CPU usage percentage
    pub cpu_usage: f64,
}

/// Migration statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MigrationStats {
    /// Total requests processed
    pub total_requests: u64,
    
    /// V4 requests
    pub v4_requests: u64,
    
    /// V5 requests  
    pub v5_requests: u64,
    
    /// Successful requests
    pub successful_requests: u64,
    
    /// Failed requests
    pub failed_requests: u64,
    
    /// Average response time by version
    pub avg_response_time: HashMap<String, Duration>,
    
    /// Error rates by version
    pub error_rates: HashMap<String, f64>,
    
    /// Result mismatches (for parallel mode)
    pub result_mismatches: u64,
    
    /// Consensus agreement rate
    pub consensus_agreement_rate: f64,
    
    /// Start time
    pub start_time: SystemTime,
    
    /// Last update time
    pub last_update: SystemTime,
}

/// A/B test results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ABTestResults {
    /// Test name
    pub test_name: String,
    
    /// Test duration
    pub duration: Duration,
    
    /// V4 metrics
    pub v4_metrics: TestMetrics,
    
    /// V5 metrics
    pub v5_metrics: TestMetrics,
    
    /// Statistical significance
    pub statistical_significance: StatisticalResult,
    
    /// Confidence intervals
    pub confidence_intervals: ConfidenceIntervals,
    
    /// Recommendation
    pub recommendation: TestRecommendation,
}

/// Test metrics for a specific version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMetrics {
    /// Request count
    pub request_count: u64,
    
    /// Success count
    pub success_count: u64,
    
    /// Success rate
    pub success_rate: f64,
    
    /// Average response time
    pub avg_response_time: Duration,
    
    /// P50 response time
    pub p50_response_time: Duration,
    
    /// P95 response time
    pub p95_response_time: Duration,
    
    /// P99 response time
    pub p99_response_time: Duration,
    
    /// Error distribution
    pub error_distribution: HashMap<String, u64>,
    
    /// Memory usage statistics
    pub memory_stats: MemoryStats,
    
    /// CPU usage statistics
    pub cpu_stats: CpuStats,
}

/// Memory usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    /// Average memory usage (MB)
    pub avg_usage_mb: u64,
    
    /// Peak memory usage (MB)
    pub peak_usage_mb: u64,
    
    /// Memory growth rate (MB/hour)
    pub growth_rate_mb_per_hour: f64,
}

/// CPU usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuStats {
    /// Average CPU usage percentage
    pub avg_usage_percent: f64,
    
    /// Peak CPU usage percentage
    pub peak_usage_percent: f64,
}

/// Statistical test results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticalResult {
    /// P-value
    pub p_value: f64,
    
    /// Is statistically significant
    pub is_significant: bool,
    
    /// Effect size
    pub effect_size: f64,
    
    /// Statistical power
    pub power: f64,
    
    /// Test type used
    pub test_type: String,
}

/// Confidence intervals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceIntervals {
    /// Response time difference confidence interval
    pub response_time_diff: (f64, f64),
    
    /// Success rate difference confidence interval
    pub success_rate_diff: (f64, f64),
    
    /// Confidence level
    pub confidence_level: f64,
}

/// Test recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestRecommendation {
    /// Continue with v4
    StayWithV4 { reason: String },
    
    /// Migrate to v5
    MigrateToV5 { reason: String },
    
    /// Extend testing
    ExtendTesting { reason: String, duration: Duration },
    
    /// Inconclusive results
    Inconclusive { reason: String },
}

/// Rollback information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackInfo {
    /// Rollback trigger
    pub trigger: RollbackTrigger,
    
    /// Rollback start time
    pub start_time: SystemTime,
    
    /// Rollback completion time
    pub completion_time: Option<SystemTime>,
    
    /// Rollback status
    pub status: RollbackStatus,
    
    /// Pre-rollback state
    pub pre_rollback_state: Option<SystemState>,
    
    /// Post-rollback state
    pub post_rollback_state: Option<SystemState>,
    
    /// Rollback duration
    pub duration: Option<Duration>,
    
    /// Success metrics after rollback
    pub success_metrics: Option<HealthMetrics>,
}

/// Rollback trigger reasons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RollbackTrigger {
    /// High error rate
    HighErrorRate { rate: f64, threshold: f64 },
    
    /// High latency
    HighLatency { latency: Duration, threshold: Duration },
    
    /// Memory exhaustion
    MemoryExhaustion { usage_mb: u64, limit_mb: u64 },
    
    /// Consecutive failures
    ConsecutiveFailures { count: u32, threshold: u32 },
    
    /// Health check failure
    HealthCheckFailure { check_name: String },
    
    /// Manual trigger
    Manual { operator: String, reason: String },
    
    /// Custom condition
    Custom { condition: String },
}

/// Rollback status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RollbackStatus {
    /// Rollback in progress
    InProgress,
    
    /// Rollback completed successfully
    Completed,
    
    /// Rollback failed
    Failed { error: String },
    
    /// Rollback verification in progress
    Verifying,
    
    /// Rollback verified
    Verified,
}

/// System state snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemState {
    /// Timestamp
    pub timestamp: SystemTime,
    
    /// Migration mode
    pub migration_mode: String,
    
    /// Active version percentages
    pub version_percentages: HashMap<String, u8>,
    
    /// Health status
    pub health_status: HealthStatus,
    
    /// Performance metrics
    pub performance_metrics: HealthMetrics,
    
    /// Configuration snapshot
    pub config_snapshot: HashMap<String, serde_json::Value>,
}

// Utility functions for type conversion
impl ExecutionPayload {
    /// Check if this payload uses v5-only features
    pub fn uses_v5_features(&self) -> bool {
        self.blob_gas_used.is_some() 
            || self.excess_blob_gas.is_some() 
            || self.parent_beacon_block_root.is_some()
    }
    
    /// Check if this payload is compatible with v4
    pub fn is_v4_compatible(&self) -> bool {
        !self.uses_v5_features()
    }
    
    /// Create a default payload for testing
    pub fn default_test_payload() -> Self {
        Self {
            parent_hash: H256::zero(),
            fee_recipient: Address::zero(),
            state_root: H256::zero(),
            receipts_root: H256::zero(),
            logs_bloom: vec![0; 256],
            prev_randao: H256::zero(),
            block_number: 1,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            extra_data: vec![],
            base_fee_per_gas: U256::from(1_000_000_000u64), // 1 gwei
            block_hash: H256::zero(),
            transactions: vec![],
            withdrawals: None,
            blob_gas_used: None,
            excess_blob_gas: None,
            parent_beacon_block_root: None,
        }
    }
}

impl ForkchoiceState {
    /// Check if this state uses v5-only features
    pub fn uses_v5_features(&self) -> bool {
        self.justified_block_hash.is_some()
    }
    
    /// Convert to v4-compatible state
    pub fn to_v4_compatible(&self) -> Self {
        Self {
            head_block_hash: self.head_block_hash,
            safe_block_hash: self.safe_block_hash,
            finalized_block_hash: self.finalized_block_hash,
            justified_block_hash: None,
        }
    }
    
    /// Create default forkchoice state for testing
    pub fn default_test_state() -> Self {
        Self {
            head_block_hash: H256::zero(),
            safe_block_hash: H256::zero(),
            finalized_block_hash: H256::zero(),
            justified_block_hash: None,
        }
    }
}

impl PayloadAttributes {
    /// Check if this attributes use v5-only features
    pub fn uses_v5_features(&self) -> bool {
        self.parent_beacon_block_root.is_some()
    }
    
    /// Convert to v4-compatible attributes
    pub fn to_v4_compatible(&self) -> Self {
        Self {
            timestamp: self.timestamp,
            prev_randao: self.prev_randao,
            suggested_fee_recipient: self.suggested_fee_recipient,
            withdrawals: self.withdrawals.clone(),
            parent_beacon_block_root: None,
        }
    }
    
    /// Create default attributes for testing
    pub fn default_test_attributes() -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            prev_randao: H256::zero(),
            suggested_fee_recipient: Address::zero(),
            withdrawals: None,
            parent_beacon_block_root: None,
        }
    }
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            healthy: true,
            sync_status: SyncStatus::Synced,
            peer_count: 0,
            last_success: Some(SystemTime::now()),
            error_details: None,
            metrics: HealthMetrics::default(),
        }
    }
}

impl Default for HealthMetrics {
    fn default() -> Self {
        Self {
            avg_response_time: Duration::from_millis(50),
            error_rate: 0.0,
            request_count: 0,
            memory_usage_mb: 100,
            cpu_usage: 10.0,
        }
    }
}

impl MigrationStats {
    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.successful_requests as f64 / self.total_requests as f64
        }
    }
    
    /// Calculate error rate
    pub fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.failed_requests as f64 / self.total_requests as f64
        }
    }
    
    /// Calculate v5 traffic percentage
    pub fn v5_traffic_percentage(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            (self.v5_requests as f64 / self.total_requests as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_execution_payload_v5_features() {
        let mut payload = ExecutionPayload::default_test_payload();
        assert!(!payload.uses_v5_features());
        assert!(payload.is_v4_compatible());
        
        payload.blob_gas_used = Some(100);
        assert!(payload.uses_v5_features());
        assert!(!payload.is_v4_compatible());
    }
    
    #[test]
    fn test_forkchoice_state_compatibility() {
        let mut state = ForkchoiceState::default_test_state();
        assert!(!state.uses_v5_features());
        
        state.justified_block_hash = Some(H256::from_low_u64_be(1));
        assert!(state.uses_v5_features());
        
        let v4_compatible = state.to_v4_compatible();
        assert!(!v4_compatible.uses_v5_features());
        assert!(v4_compatible.justified_block_hash.is_none());
    }
    
    #[test]
    fn test_migration_stats() {
        let mut stats = MigrationStats::default();
        stats.total_requests = 100;
        stats.successful_requests = 95;
        stats.failed_requests = 5;
        stats.v5_requests = 30;
        
        assert_eq!(stats.success_rate(), 0.95);
        assert_eq!(stats.error_rate(), 0.05);
        assert_eq!(stats.v5_traffic_percentage(), 30.0);
    }
    
    #[test]
    fn test_health_metrics_default() {
        let metrics = HealthMetrics::default();
        assert_eq!(metrics.error_rate, 0.0);
        assert_eq!(metrics.request_count, 0);
        assert!(metrics.avg_response_time < Duration::from_millis(100));
    }
    
    #[test]
    fn test_payload_attributes_v5_features() {
        let mut attrs = PayloadAttributes::default_test_attributes();
        assert!(!attrs.uses_v5_features());
        
        attrs.parent_beacon_block_root = Some(H256::from_low_u64_be(1));
        assert!(attrs.uses_v5_features());
        
        let v4_compatible = attrs.to_v4_compatible();
        assert!(!v4_compatible.uses_v5_features());
    }
}