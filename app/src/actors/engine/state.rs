//! Engine State Management
//!
//! This module contains all engine state structures and related implementations
//! for tracking execution state, payload status, and client health.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use crate::types::*;

/// Current execution state of the engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionState {
    /// Engine is starting up and initializing
    Initializing,
    
    /// Syncing with the execution client
    Syncing { 
        /// Sync progress percentage (0.0 to 1.0)
        progress: f64,
        /// Current block height
        current_height: u64,
        /// Target block height
        target_height: u64,
        /// Estimated time remaining
        eta: Option<Duration>,
    },
    
    /// Ready to process blocks and build payloads
    Ready {
        /// Current head block hash
        head_hash: Option<Hash256>,
        /// Current head block height
        head_height: u64,
        /// Last activity timestamp
        last_activity: SystemTime,
    },
    
    /// Degraded state (some functionality limited)
    Degraded {
        /// Reason for degraded state
        reason: String,
        /// Capabilities that are still available
        available_capabilities: Vec<EngineCapability>,
        /// When degraded state was entered
        since: SystemTime,
    },
    
    /// Error state requiring intervention
    Error { 
        /// Error message describing the issue
        message: String,
        /// When the error occurred
        occurred_at: SystemTime,
        /// Whether automatic recovery is possible
        recoverable: bool,
        /// Number of recovery attempts made
        recovery_attempts: u32,
    },
}

/// Engine capabilities that may be available in different states
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineCapability {
    /// Can build new payloads
    PayloadBuilding,
    /// Can execute payloads
    PayloadExecution,
    /// Can update forkchoice
    ForkchoiceUpdate,
    /// Can query blockchain state
    StateQueries,
    /// Can process transactions
    TransactionProcessing,
}

/// Status of a payload being built or executed
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayloadStatus {
    /// Payload is being built
    Building {
        /// When building started
        started_at: SystemTime,
        /// Expected completion time
        expected_completion: Option<SystemTime>,
    },
    
    /// Payload building completed successfully
    Built {
        /// When building completed
        completed_at: SystemTime,
        /// Time taken to build
        build_duration: Duration,
    },
    
    /// Payload is being executed
    Executing {
        /// When execution started
        started_at: SystemTime,
        /// Expected completion time
        expected_completion: Option<SystemTime>,
    },
    
    /// Payload execution completed successfully
    Executed {
        /// When execution completed
        completed_at: SystemTime,
        /// Time taken to execute
        execution_duration: Duration,
        /// Resulting state root
        state_root: Hash256,
    },
    
    /// Payload operation failed
    Failed {
        /// Error message
        error: String,
        /// When the failure occurred
        failed_at: SystemTime,
        /// Whether the operation can be retried
        retryable: bool,
    },
    
    /// Payload operation timed out
    TimedOut {
        /// When the timeout occurred
        timed_out_at: SystemTime,
        /// Duration before timeout
        timeout_duration: Duration,
    },
}

/// Information about a pending payload operation
#[derive(Debug, Clone)]
pub struct PendingPayload {
    /// Unique payload identifier
    pub payload_id: String,
    
    /// The execution payload
    pub payload: ExecutionPayload,
    
    /// Current status of the payload
    pub status: PayloadStatus,
    
    /// When the payload was created
    pub created_at: Instant,
    
    /// Parent block hash
    pub parent_hash: Hash256,
    
    /// Fee recipient address
    pub fee_recipient: Address,
    
    /// Withdrawals included in this payload
    pub withdrawals: Vec<Withdrawal>,
    
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
    
    /// Priority level for processing
    pub priority: PayloadPriority,
    
    /// Number of retry attempts made
    pub retry_attempts: u32,
    
    /// Associated trace context for distributed tracing
    pub trace_context: Option<TraceContext>,
}

/// Priority levels for payload operations
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PayloadPriority {
    /// Low priority background operation
    Background = 0,
    /// Normal priority operation
    Normal = 1,
    /// High priority operation
    High = 2,
    /// Critical operation that must be processed immediately
    Critical = 3,
}

/// Execution client health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientHealthStatus {
    /// Whether the client is reachable
    pub is_reachable: bool,
    
    /// Whether the client is synced
    pub is_synced: bool,
    
    /// Current sync status
    pub sync_status: SyncStatus,
    
    /// Client version information
    pub client_version: Option<String>,
    
    /// Last successful health check
    pub last_healthy: Option<SystemTime>,
    
    /// Consecutive health check failures
    pub consecutive_failures: u32,
    
    /// Average response time for recent requests
    pub average_response_time: Duration,
    
    /// Number of active connections
    pub active_connections: usize,
    
    /// Client-specific capabilities
    pub capabilities: Vec<String>,
}

/// Synchronization status of the execution client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncStatus {
    /// Client is fully synced
    Synced,
    
    /// Client is syncing
    Syncing {
        /// Current block height
        current_block: u64,
        /// Highest known block
        highest_block: u64,
        /// Sync progress percentage
        progress: f64,
    },
    
    /// Sync status unknown
    Unknown,
}

/// Engine actor internal state
#[derive(Debug)]
pub struct EngineActorState {
    /// Current execution state
    pub execution_state: ExecutionState,
    
    /// Pending payload operations
    pub pending_payloads: HashMap<String, PendingPayload>,
    
    /// Client health status
    pub client_health: ClientHealthStatus,
    
    /// Performance metrics tracking
    pub metrics: EngineStateMetrics,
    
    /// Configuration reference
    pub config: super::EngineConfig,
    
    /// Last state update timestamp
    pub last_updated: Instant,
    
    /// State change history for debugging
    pub state_history: Vec<StateTransition>,
}

/// Performance metrics tracked in engine state
#[derive(Debug, Default)]
pub struct EngineStateMetrics {
    /// Total number of payloads built
    pub payloads_built: u64,
    
    /// Total number of payloads executed
    pub payloads_executed: u64,
    
    /// Total number of failed operations
    pub failures: u64,
    
    /// Average payload build time
    pub avg_build_time: Duration,
    
    /// Average payload execution time
    pub avg_execution_time: Duration,
    
    /// Peak memory usage
    pub peak_memory_usage: u64,
    
    /// Current active payload count
    pub active_payloads: usize,
    
    /// Client connection uptime percentage
    pub client_uptime: f64,
}

/// State transition for debugging and monitoring
#[derive(Debug, Clone)]
pub struct StateTransition {
    /// Previous state
    pub from_state: String,
    
    /// New state
    pub to_state: String,
    
    /// Reason for transition
    pub reason: String,
    
    /// When the transition occurred
    pub timestamp: SystemTime,
    
    /// Additional context
    pub context: HashMap<String, String>,
}

/// Trace context for distributed tracing
#[derive(Debug, Clone)]
pub struct TraceContext {
    /// Trace ID
    pub trace_id: String,
    
    /// Span ID
    pub span_id: String,
    
    /// Parent span ID
    pub parent_span_id: Option<String>,
    
    /// Trace flags
    pub flags: u8,
}

impl Default for ExecutionState {
    fn default() -> Self {
        ExecutionState::Initializing
    }
}

impl Default for ClientHealthStatus {
    fn default() -> Self {
        Self {
            is_reachable: false,
            is_synced: false,
            sync_status: SyncStatus::Unknown,
            client_version: None,
            last_healthy: None,
            consecutive_failures: 0,
            average_response_time: Duration::from_millis(0),
            active_connections: 0,
            capabilities: vec![],
        }
    }
}

impl ExecutionState {
    /// Check if the engine is ready to process operations
    pub fn is_ready(&self) -> bool {
        matches!(self, ExecutionState::Ready { .. })
    }
    
    /// Check if the engine can build payloads
    pub fn can_build_payloads(&self) -> bool {
        match self {
            ExecutionState::Ready { .. } => true,
            ExecutionState::Degraded { available_capabilities, .. } => {
                available_capabilities.contains(&EngineCapability::PayloadBuilding)
            },
            _ => false,
        }
    }
    
    /// Check if the engine can execute payloads
    pub fn can_execute_payloads(&self) -> bool {
        match self {
            ExecutionState::Ready { .. } => true,
            ExecutionState::Degraded { available_capabilities, .. } => {
                available_capabilities.contains(&EngineCapability::PayloadExecution)
            },
            _ => false,
        }
    }
    
    /// Get a human-readable description of the current state
    pub fn description(&self) -> String {
        match self {
            ExecutionState::Initializing => "Initializing engine".to_string(),
            ExecutionState::Syncing { progress, current_height, target_height, .. } => {
                format!("Syncing: {:.1}% ({}/{})", progress * 100.0, current_height, target_height)
            },
            ExecutionState::Ready { head_height, .. } => {
                format!("Ready at height {}", head_height)
            },
            ExecutionState::Degraded { reason, .. } => {
                format!("Degraded: {}", reason)
            },
            ExecutionState::Error { message, .. } => {
                format!("Error: {}", message)
            },
        }
    }
}

impl PayloadStatus {
    /// Check if the payload operation is complete (success or failure)
    pub fn is_complete(&self) -> bool {
        matches!(self, 
            PayloadStatus::Built { .. } | 
            PayloadStatus::Executed { .. } | 
            PayloadStatus::Failed { .. } | 
            PayloadStatus::TimedOut { .. }
        )
    }
    
    /// Check if the payload operation is in progress
    pub fn is_in_progress(&self) -> bool {
        matches!(self, PayloadStatus::Building { .. } | PayloadStatus::Executing { .. })
    }
    
    /// Check if the operation was successful
    pub fn is_successful(&self) -> bool {
        matches!(self, PayloadStatus::Built { .. } | PayloadStatus::Executed { .. })
    }
    
    /// Get the duration of the operation if completed
    pub fn duration(&self) -> Option<Duration> {
        match self {
            PayloadStatus::Built { build_duration, .. } => Some(*build_duration),
            PayloadStatus::Executed { execution_duration, .. } => Some(*execution_duration),
            _ => None,
        }
    }
}

impl EngineActorState {
    /// Create new engine actor state with the given configuration
    pub fn new(config: super::EngineConfig) -> Self {
        Self {
            execution_state: ExecutionState::default(),
            pending_payloads: HashMap::new(),
            client_health: ClientHealthStatus::default(),
            metrics: EngineStateMetrics::default(),
            config,
            last_updated: Instant::now(),
            state_history: Vec::new(),
        }
    }
    
    /// Transition to a new execution state
    pub fn transition_state(&mut self, new_state: ExecutionState, reason: String) {
        let old_state = std::mem::replace(&mut self.execution_state, new_state);
        
        let transition = StateTransition {
            from_state: format!("{:?}", old_state),
            to_state: format!("{:?}", self.execution_state),
            reason,
            timestamp: SystemTime::now(),
            context: HashMap::new(),
        };
        
        self.state_history.push(transition);
        self.last_updated = Instant::now();
        
        // Keep only recent history (last 100 transitions)
        if self.state_history.len() > 100 {
            self.state_history.remove(0);
        }
    }
    
    /// Add a new pending payload
    pub fn add_pending_payload(&mut self, payload: PendingPayload) {
        self.pending_payloads.insert(payload.payload_id.clone(), payload);
        self.metrics.active_payloads = self.pending_payloads.len();
    }
    
    /// Remove a pending payload and update metrics
    pub fn remove_pending_payload(&mut self, payload_id: &str) -> Option<PendingPayload> {
        let payload = self.pending_payloads.remove(payload_id);
        self.metrics.active_payloads = self.pending_payloads.len();
        
        // Update metrics if payload was completed
        if let Some(ref payload) = payload {
            match &payload.status {
                PayloadStatus::Built { build_duration, .. } => {
                    self.metrics.payloads_built += 1;
                    self.update_avg_build_time(*build_duration);
                },
                PayloadStatus::Executed { execution_duration, .. } => {
                    self.metrics.payloads_executed += 1;
                    self.update_avg_execution_time(*execution_duration);
                },
                PayloadStatus::Failed { .. } | PayloadStatus::TimedOut { .. } => {
                    self.metrics.failures += 1;
                },
                _ => {},
            }
        }
        
        payload
    }
    
    /// Update average build time with exponential moving average
    fn update_avg_build_time(&mut self, new_duration: Duration) {
        if self.metrics.avg_build_time == Duration::ZERO {
            self.metrics.avg_build_time = new_duration;
        } else {
            let alpha = 0.1; // Smoothing factor
            let current_ms = self.metrics.avg_build_time.as_millis() as f64;
            let new_ms = new_duration.as_millis() as f64;
            let updated_ms = current_ms * (1.0 - alpha) + new_ms * alpha;
            self.metrics.avg_build_time = Duration::from_millis(updated_ms as u64);
        }
    }
    
    /// Update average execution time with exponential moving average
    fn update_avg_execution_time(&mut self, new_duration: Duration) {
        if self.metrics.avg_execution_time == Duration::ZERO {
            self.metrics.avg_execution_time = new_duration;
        } else {
            let alpha = 0.1; // Smoothing factor
            let current_ms = self.metrics.avg_execution_time.as_millis() as f64;
            let new_ms = new_duration.as_millis() as f64;
            let updated_ms = current_ms * (1.0 - alpha) + new_ms * alpha;
            self.metrics.avg_execution_time = Duration::from_millis(updated_ms as u64);
        }
    }
    
    /// Clean up old pending payloads that have timed out
    pub fn cleanup_expired_payloads(&mut self, max_age: Duration) {
        let now = Instant::now();
        let expired_payloads: Vec<String> = self.pending_payloads
            .iter()
            .filter(|(_, payload)| now.duration_since(payload.created_at) > max_age)
            .map(|(id, _)| id.clone())
            .collect();
        
        for payload_id in expired_payloads {
            if let Some(mut payload) = self.pending_payloads.remove(&payload_id) {
                payload.status = PayloadStatus::TimedOut {
                    timed_out_at: SystemTime::now(),
                    timeout_duration: max_age,
                };
                self.metrics.failures += 1;
            }
        }
        
        self.metrics.active_payloads = self.pending_payloads.len();
    }
    
    /// Get current state summary for monitoring
    pub fn state_summary(&self) -> HashMap<String, String> {
        let mut summary = HashMap::new();
        
        summary.insert("execution_state".to_string(), self.execution_state.description());
        summary.insert("pending_payloads".to_string(), self.pending_payloads.len().to_string());
        summary.insert("client_healthy".to_string(), self.client_health.is_reachable.to_string());
        summary.insert("client_synced".to_string(), self.client_health.is_synced.to_string());
        summary.insert("payloads_built".to_string(), self.metrics.payloads_built.to_string());
        summary.insert("payloads_executed".to_string(), self.metrics.payloads_executed.to_string());
        summary.insert("failures".to_string(), self.metrics.failures.to_string());
        summary.insert("avg_build_time_ms".to_string(), self.metrics.avg_build_time.as_millis().to_string());
        summary.insert("avg_execution_time_ms".to_string(), self.metrics.avg_execution_time.as_millis().to_string());
        
        summary
    }
}