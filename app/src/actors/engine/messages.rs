//! Engine Actor Message Definitions
//!
//! This module defines all message types for the EngineActor, including
//! Engine API messages, inter-actor communication messages, and internal
//! coordination messages.

use std::time::{Duration, SystemTime};
use uuid::Uuid;
use actix::prelude::*;
use serde::{Deserialize, Serialize};
use crate::types::*;
use super::state::{ExecutionState, PayloadStatus, TraceContext};

/// Type alias for payload identifier
pub type PayloadId = String;

/// Type alias for message result handling
pub type MessageResult<T> = Result<T, crate::EngineError>;

// ============================================================================
// Engine API Messages (Core Execution Layer Operations)
// ============================================================================

/// Message to build a new execution payload
#[derive(Message, Debug, Clone)]
#[rtype(result = "MessageResult<PayloadId>")]
pub struct BuildPayloadMessage {
    /// Parent block hash for the new payload
    pub parent_hash: Hash256,
    
    /// Timestamp for the new block
    pub timestamp: u64,
    
    /// Fee recipient address
    pub fee_recipient: Address,
    
    /// Withdrawals to include in the payload (peg-ins)
    pub withdrawals: Vec<Withdrawal>,
    
    /// Optional random value for the payload
    pub prev_randao: Option<Hash256>,
    
    /// Gas limit for the block
    pub gas_limit: Option<u64>,
    
    /// Priority level for this payload
    pub priority: super::state::PayloadPriority,
    
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
    
    /// Distributed tracing context
    pub trace_context: Option<TraceContext>,
}

/// Message to retrieve a built payload
#[derive(Message, Debug, Clone)]
#[rtype(result = "MessageResult<ExecutionPayload>")]
pub struct GetPayloadMessage {
    /// Payload ID to retrieve
    pub payload_id: PayloadId,
    
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to execute a payload
#[derive(Message, Debug, Clone)]
#[rtype(result = "MessageResult<PayloadExecutionResult>")]
pub struct ExecutePayloadMessage {
    /// Execution payload to process
    pub payload: ExecutionPayload,
    
    /// Whether to validate the payload before execution
    pub validate: bool,
    
    /// Timeout for execution
    pub timeout: Option<Duration>,
    
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
    
    /// Distributed tracing context
    pub trace_context: Option<TraceContext>,
}

/// Result of payload execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadExecutionResult {
    /// Execution status
    pub status: ExecutionStatus,
    
    /// Latest valid block hash
    pub latest_valid_hash: Option<Hash256>,
    
    /// Validation error if any
    pub validation_error: Option<String>,
    
    /// Gas used during execution
    pub gas_used: Option<u64>,
    
    /// State root after execution
    pub state_root: Option<Hash256>,
    
    /// Transaction receipts
    pub receipts: Vec<TransactionReceipt>,
    
    /// Execution duration
    pub execution_duration: Duration,
}

/// Execution status codes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStatus {
    /// Payload is valid and executed successfully
    Valid,
    
    /// Payload is invalid
    Invalid,
    
    /// Still syncing, cannot execute
    Syncing,
    
    /// Payload accepted but not yet executed
    Accepted,
    
    /// Execution failed due to internal error
    ExecutionFailed,
}

/// Message to update forkchoice state
#[derive(Message, Debug, Clone)]
#[rtype(result = "MessageResult<ForkchoiceUpdateResult>")]
pub struct ForkchoiceUpdatedMessage {
    /// New head block hash
    pub head_block_hash: Hash256,
    
    /// Safe block hash
    pub safe_block_hash: Hash256,
    
    /// Finalized block hash
    pub finalized_block_hash: Hash256,
    
    /// Optional payload attributes for building on this head
    pub payload_attributes: Option<PayloadAttributes>,
    
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Result of forkchoice update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkchoiceUpdateResult {
    /// Status of the forkchoice update
    pub payload_status: PayloadStatusType,
    
    /// Latest valid hash
    pub latest_valid_hash: Option<Hash256>,
    
    /// Validation error if any
    pub validation_error: Option<String>,
    
    /// Payload ID if a new payload was requested
    pub payload_id: Option<PayloadId>,
}

/// Payload status type for forkchoice operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayloadStatusType {
    Valid,
    Invalid,
    Syncing,
    Accepted,
    InvalidBlockHash,
    InvalidTerminalBlock,
}

/// Payload attributes for building new payloads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadAttributes {
    /// Timestamp for the payload
    pub timestamp: u64,
    
    /// Previous randao value
    pub prev_randao: Hash256,
    
    /// Fee recipient address
    pub suggested_fee_recipient: Address,
    
    /// Withdrawals to include
    pub withdrawals: Option<Vec<Withdrawal>>,
    
    /// Parent beacon block root (for future compatibility)
    pub parent_beacon_block_root: Option<Hash256>,
}

// ============================================================================
// Inter-Actor Communication Messages
// ============================================================================

/// Message from ChainActor requesting payload building
#[derive(Message, Debug, Clone)]
#[rtype(result = "MessageResult<PayloadId>")]
pub struct ChainRequestPayloadMessage {
    /// Block production context
    pub block_context: BlockProductionContext,
    
    /// Withdrawals from peg-in operations
    pub withdrawals: Vec<Withdrawal>,
    
    /// Timeout for payload building
    pub timeout: Duration,
    
    /// Correlation ID for request tracking
    pub correlation_id: Uuid,
}

/// Block production context from ChainActor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockProductionContext {
    /// Parent block hash
    pub parent_hash: Hash256,
    
    /// Block timestamp
    pub timestamp: u64,
    
    /// Block height
    pub height: u64,
    
    /// Slot number (Aura)
    pub slot: u64,
    
    /// Authority index producing this block
    pub authority_index: u32,
    
    /// Fee recipient for block rewards
    pub fee_recipient: Address,
}

/// Message to BridgeActor about detected burn events
#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct BurnEventDetectedMessage {
    /// Transaction hash containing the burn event
    pub tx_hash: Hash256,
    
    /// Block hash where the transaction was included
    pub block_hash: Hash256,
    
    /// Block height
    pub block_height: u64,
    
    /// Burn event details
    pub burn_event: BurnEvent,
    
    /// When the event was detected
    pub detected_at: SystemTime,
}

/// Burn event details for peg-out operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnEvent {
    /// Address that initiated the burn
    pub from_address: Address,
    
    /// Amount burned (in wei)
    pub amount: U256,
    
    /// Bitcoin address to send to
    pub bitcoin_address: String,
    
    /// Log index in the transaction
    pub log_index: u64,
    
    /// Transaction index in the block
    pub transaction_index: u64,
}

/// Message from BridgeActor requesting transaction validation
#[derive(Message, Debug, Clone)]
#[rtype(result = "MessageResult<TransactionValidationResult>")]
pub struct ValidateTransactionMessage {
    /// Transaction hash to validate
    pub tx_hash: Hash256,
    
    /// Expected transaction details
    pub expected_details: ExpectedTransaction,
    
    /// Correlation ID for tracking
    pub correlation_id: Option<Uuid>,
}

/// Expected transaction details for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedTransaction {
    /// Expected from address
    pub from: Address,
    
    /// Expected value
    pub value: U256,
    
    /// Expected contract address
    pub to: Address,
    
    /// Expected function call data
    pub data: Vec<u8>,
}

/// Result of transaction validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionValidationResult {
    /// Whether the transaction is valid
    pub is_valid: bool,
    
    /// Transaction receipt
    pub receipt: Option<TransactionReceipt>,
    
    /// Validation errors if any
    pub errors: Vec<String>,
    
    /// Gas used by the transaction
    pub gas_used: Option<u64>,
}

/// Message to StorageActor for persisting execution data
#[derive(Message, Debug, Clone)]
#[rtype(result = "MessageResult<()>")]
pub struct StoreExecutionDataMessage {
    /// Block hash for the execution data
    pub block_hash: Hash256,
    
    /// Block height
    pub block_height: u64,
    
    /// Transaction receipts to store
    pub receipts: Vec<TransactionReceipt>,
    
    /// Event logs to store
    pub logs: Vec<EventLog>,
    
    /// State changes to store
    pub state_changes: Vec<StateChange>,
    
    /// Correlation ID for tracking
    pub correlation_id: Option<Uuid>,
}

/// State change record for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    /// Address that changed
    pub address: Address,
    
    /// Storage slot that changed
    pub slot: Hash256,
    
    /// Previous value
    pub previous_value: Hash256,
    
    /// New value
    pub new_value: Hash256,
}

/// Message from NetworkActor for transaction validation
#[derive(Message, Debug, Clone)]
#[rtype(result = "MessageResult<TransactionValidationResult>")]
pub struct ValidateIncomingTransactionMessage {
    /// Raw transaction data
    pub transaction: Vec<u8>,
    
    /// Source peer information
    pub peer_info: PeerInfo,
    
    /// Correlation ID for tracking
    pub correlation_id: Option<Uuid>,
}

/// Peer information for transaction validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Peer ID
    pub peer_id: String,
    
    /// Peer address
    pub peer_address: String,
    
    /// Peer reputation score
    pub reputation: f64,
}

// ============================================================================
// Internal Engine Messages
// ============================================================================

/// Internal message for client health checks
#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct HealthCheckMessage;

/// Internal message for metrics reporting
#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct MetricsReportMessage;

/// Internal message for payload cleanup
#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct CleanupExpiredPayloadsMessage;

/// Message to query engine status
#[derive(Message, Debug, Clone)]
#[rtype(result = "MessageResult<EngineStatusResponse>")]
pub struct GetEngineStatusMessage {
    /// Include detailed metrics in response
    pub include_metrics: bool,
    
    /// Include pending payload information
    pub include_payloads: bool,
}

/// Engine status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatusResponse {
    /// Current execution state
    pub execution_state: ExecutionState,
    
    /// Client health status
    pub client_healthy: bool,
    
    /// Number of pending payloads
    pub pending_payloads: usize,
    
    /// Performance metrics (if requested)
    pub metrics: Option<EnginePerformanceMetrics>,
    
    /// Pending payload details (if requested)
    pub payload_details: Option<Vec<PayloadDetails>>,
    
    /// Engine uptime
    pub uptime: Duration,
}

/// Performance metrics for status reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnginePerformanceMetrics {
    /// Total payloads built
    pub payloads_built: u64,
    
    /// Total payloads executed
    pub payloads_executed: u64,
    
    /// Total failures
    pub failures: u64,
    
    /// Average build time
    pub avg_build_time_ms: u64,
    
    /// Average execution time
    pub avg_execution_time_ms: u64,
    
    /// Success rate percentage
    pub success_rate: f64,
    
    /// Client uptime percentage
    pub client_uptime: f64,
}

/// Payload details for status reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadDetails {
    /// Payload ID
    pub payload_id: String,
    
    /// Current status
    pub status: PayloadStatus,
    
    /// Age of the payload
    pub age_ms: u64,
    
    /// Priority level
    pub priority: super::state::PayloadPriority,
    
    /// Retry attempts made
    pub retry_attempts: u32,
}

// ============================================================================
// System Messages
// ============================================================================

/// Message to gracefully shutdown the engine
#[derive(Message, Debug, Clone)]
#[rtype(result = "MessageResult<()>")]
pub struct ShutdownEngineMessage {
    /// Timeout for graceful shutdown
    pub timeout: Duration,
    
    /// Whether to wait for pending payloads to complete
    pub wait_for_pending: bool,
}

/// Message to restart the engine actor
#[derive(Message, Debug, Clone)]
#[rtype(result = "MessageResult<()>")]
pub struct RestartEngineMessage {
    /// Reason for restart
    pub reason: String,
    
    /// Whether to preserve pending payloads
    pub preserve_state: bool,
}

/// Message to update engine configuration
#[derive(Message, Debug, Clone)]
#[rtype(result = "MessageResult<()>")]
pub struct UpdateConfigMessage {
    /// New configuration
    pub config: super::EngineConfig,
    
    /// Whether to restart with new config
    pub restart_if_needed: bool,
}

// ============================================================================
// Message Implementations
// ============================================================================

impl BuildPayloadMessage {
    /// Create a new payload build request with default priority
    pub fn new(
        parent_hash: Hash256,
        timestamp: u64,
        fee_recipient: Address,
        withdrawals: Vec<Withdrawal>,
    ) -> Self {
        Self {
            parent_hash,
            timestamp,
            fee_recipient,
            withdrawals,
            prev_randao: None,
            gas_limit: None,
            priority: super::state::PayloadPriority::Normal,
            correlation_id: Some(Uuid::new_v4()),
            trace_context: None,
        }
    }
    
    /// Set high priority for urgent payload building
    pub fn with_high_priority(mut self) -> Self {
        self.priority = super::state::PayloadPriority::High;
        self
    }
    
    /// Set critical priority for time-sensitive operations
    pub fn with_critical_priority(mut self) -> Self {
        self.priority = super::state::PayloadPriority::Critical;
        self
    }
    
    /// Add trace context for distributed tracing
    pub fn with_trace_context(mut self, trace_context: TraceContext) -> Self {
        self.trace_context = Some(trace_context);
        self
    }
}

impl ExecutePayloadMessage {
    /// Create a new payload execution request
    pub fn new(payload: ExecutionPayload) -> Self {
        Self {
            payload,
            validate: true,
            timeout: None,
            correlation_id: Some(Uuid::new_v4()),
            trace_context: None,
        }
    }
    
    /// Skip validation for trusted payloads
    pub fn skip_validation(mut self) -> Self {
        self.validate = false;
        self
    }
    
    /// Set custom execution timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

impl ForkchoiceUpdatedMessage {
    /// Create a new forkchoice update message
    pub fn new(
        head_block_hash: Hash256,
        safe_block_hash: Hash256,
        finalized_block_hash: Hash256,
    ) -> Self {
        Self {
            head_block_hash,
            safe_block_hash,
            finalized_block_hash,
            payload_attributes: None,
            correlation_id: Some(Uuid::new_v4()),
        }
    }
    
    /// Add payload attributes to request a new payload
    pub fn with_payload_attributes(mut self, attrs: PayloadAttributes) -> Self {
        self.payload_attributes = Some(attrs);
        self
    }
}