//! Bridge and two-way peg related types

use crate::types::*;
use serde::{Deserialize, Serialize};

/// Enhanced peg operation with governance integration and comprehensive tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegOperation {
    /// Unique operation identifier
    pub operation_id: uuid::Uuid,
    /// Operation type (peg-in or peg-out)
    pub operation_type: PegOperationType,
    /// Current operation status
    pub status: PegOperationStatus,
    /// Operation workflow state
    pub workflow: PegOperationWorkflow,
    /// Governance integration
    pub governance: GovernanceIntegration,
    /// Actor system metadata
    pub actor_metadata: PegOperationActorMetadata,
    /// Performance tracking
    pub performance: OperationPerformanceMetrics,
    /// Error tracking and recovery
    pub error_tracking: OperationErrorTracking,
    /// Compliance and audit trail
    pub compliance: ComplianceTracking,
    /// Resource allocation
    pub resource_allocation: ResourceAllocation,
}

/// Peg operation types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PegOperationType {
    /// Peg-in from Bitcoin to Alys
    PegIn {
        bitcoin_txid: bitcoin::Txid,
        bitcoin_output_index: u32,
        amount_satoshis: u64,
        recipient_address: Address,
    },
    /// Peg-out from Alys to Bitcoin
    PegOut {
        burn_tx_hash: H256,
        amount_satoshis: u64,
        bitcoin_recipient: bitcoin::Address,
        fee_rate: Option<u64>,
    },
}

/// Enhanced peg operation status with detailed workflow states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PegOperationStatus {
    /// Operation initiated
    Initiated {
        initiated_at: std::time::SystemTime,
        initiator: OperationInitiator,
    },
    /// Validating initial conditions
    Validating {
        validation_started: std::time::SystemTime,
        validations_completed: Vec<ValidationStep>,
        validations_pending: Vec<ValidationStep>,
    },
    /// Waiting for governance approval
    PendingGovernanceApproval {
        submitted_to_governance: std::time::SystemTime,
        governance_id: String,
        required_approvals: u32,
        current_approvals: u32,
        approval_deadline: Option<std::time::SystemTime>,
    },
    /// Governance approved, ready for execution
    Approved {
        approved_at: std::time::SystemTime,
        approved_by: Vec<GovernanceApproval>,
        execution_window: Option<ExecutionWindow>,
    },
    /// Operation in progress
    InProgress {
        started_at: std::time::SystemTime,
        progress_stages: Vec<ProgressStage>,
        current_stage: String,
        estimated_completion: Option<std::time::SystemTime>,
    },
    /// Waiting for confirmations
    AwaitingConfirmations {
        confirmations_started: std::time::SystemTime,
        required_confirmations: u32,
        current_confirmations: u32,
        blockchain: ConfirmationBlockchain,
    },
    /// Operation completed successfully
    Completed {
        completed_at: std::time::SystemTime,
        final_confirmations: u32,
        completion_proof: CompletionProof,
        gas_used: Option<u64>,
    },
    /// Operation failed
    Failed {
        failed_at: std::time::SystemTime,
        failure_reason: FailureReason,
        recovery_possible: bool,
        recovery_options: Vec<RecoveryOption>,
    },
    /// Operation cancelled
    Cancelled {
        cancelled_at: std::time::SystemTime,
        cancelled_by: OperationInitiator,
        cancellation_reason: String,
        refund_status: Option<RefundStatus>,
    },
    /// Operation suspended by governance
    Suspended {
        suspended_at: std::time::SystemTime,
        suspended_by: String, // Governance decision ID
        suspension_reason: String,
        review_deadline: Option<std::time::SystemTime>,
    },
}

/// Operation workflow state machine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegOperationWorkflow {
    /// Current workflow state
    pub current_state: WorkflowState,
    /// State transition history
    pub state_history: Vec<StateTransition>,
    /// Available next states
    pub available_transitions: Vec<WorkflowTransition>,
    /// Workflow configuration
    pub workflow_config: WorkflowConfig,
    /// State timeouts and deadlines
    pub timeouts: WorkflowTimeouts,
}

/// Workflow states
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkflowState {
    /// Initial state after creation
    Created,
    /// Validation phase
    Validating,
    /// Governance review phase
    GovernanceReview,
    /// Execution phase
    Executing,
    /// Confirmation phase
    Confirming,
    /// Final state - completed
    Completed,
    /// Final state - failed
    Failed,
    /// Final state - cancelled
    Cancelled,
    /// Suspended state
    Suspended,
}

/// State transition record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    /// Previous state
    pub from_state: WorkflowState,
    /// New state
    pub to_state: WorkflowState,
    /// When transition occurred
    pub transitioned_at: std::time::SystemTime,
    /// Actor that triggered the transition
    pub triggered_by: Option<String>,
    /// Transition reason/context
    pub reason: String,
    /// Additional metadata
    pub metadata: std::collections::HashMap<String, String>,
}

/// Available workflow transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTransition {
    /// Target state
    pub to_state: WorkflowState,
    /// Transition name/action
    pub action: String,
    /// Required conditions
    pub conditions: Vec<TransitionCondition>,
    /// Estimated time for transition
    pub estimated_duration: Option<std::time::Duration>,
}

/// Conditions required for state transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransitionCondition {
    /// Requires governance approval
    GovernanceApproval { required_votes: u32 },
    /// Requires specific confirmations
    ConfirmationThreshold { confirmations: u32, blockchain: ConfirmationBlockchain },
    /// Requires timeout to expire
    TimeoutExpired { timeout: std::time::Duration },
    /// Requires specific actor action
    ActorAction { actor: String, action: String },
    /// Custom condition
    Custom { condition_id: String, description: String },
}

/// Governance integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceIntegration {
    /// Governance system configuration
    pub governance_config: GovernanceConfig,
    /// Current governance status
    pub governance_status: GovernanceStatus,
    /// Governance history for this operation
    pub governance_history: Vec<GovernanceEvent>,
    /// Required governance actions
    pub required_actions: Vec<RequiredGovernanceAction>,
    /// Governance decision trail
    pub decision_trail: Vec<GovernanceDecision>,
}

/// Governance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
    /// Governance system endpoint
    pub governance_endpoint: String,
    /// Required approval threshold
    pub approval_threshold: u32,
    /// Governance timeout
    pub governance_timeout: std::time::Duration,
    /// Governance categories that apply
    pub applicable_categories: Vec<String>,
    /// Emergency bypass conditions
    pub emergency_bypass: Option<EmergencyBypass>,
}

/// Current governance status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GovernanceStatus {
    /// Not yet submitted to governance
    NotSubmitted,
    /// Submitted and pending review
    PendingReview {
        submitted_at: std::time::SystemTime,
        governance_id: String,
    },
    /// Under active review
    UnderReview {
        review_started: std::time::SystemTime,
        assigned_reviewers: Vec<String>,
    },
    /// Additional information requested
    InformationRequested {
        requested_at: std::time::SystemTime,
        requested_by: String,
        information_needed: String,
        response_deadline: std::time::SystemTime,
    },
    /// Approved by governance
    Approved {
        approved_at: std::time::SystemTime,
        approval_details: GovernanceApprovalDetails,
    },
    /// Rejected by governance
    Rejected {
        rejected_at: std::time::SystemTime,
        rejection_reason: String,
        appeal_possible: bool,
    },
    /// Suspended pending further review
    Suspended {
        suspended_at: std::time::SystemTime,
        suspension_reason: String,
    },
}

/// Governance events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceEvent {
    /// Event type
    pub event_type: GovernanceEventType,
    /// When event occurred
    pub timestamp: std::time::SystemTime,
    /// Event source/actor
    pub source: String,
    /// Event details
    pub details: String,
    /// Related governance ID
    pub governance_id: Option<String>,
}

/// Types of governance events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GovernanceEventType {
    /// Submission to governance
    Submitted,
    /// Review assigned
    ReviewAssigned,
    /// Vote cast
    VoteCast,
    /// Information requested
    InformationRequested,
    /// Information provided
    InformationProvided,
    /// Decision made
    DecisionMade,
    /// Appeal filed
    AppealFiled,
    /// Emergency action
    EmergencyAction,
}

/// Actor system metadata for peg operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegOperationActorMetadata {
    /// Processing actor ID
    pub processing_actor: Option<String>,
    /// Actor that initiated the operation
    pub initiating_actor: Option<String>,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<uuid::Uuid>,
    /// Distributed tracing context
    pub trace_context: crate::types::blockchain::TraceContext,
    /// Operation priority
    pub priority: OperationPriority,
    /// Actor performance metrics
    pub actor_metrics: ActorOperationMetrics,
    /// Message routing information
    pub routing_info: OperationRoutingInfo,
}

/// Operation priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OperationPriority {
    /// Low priority background operation
    Low = 0,
    /// Normal priority operation
    Normal = 1,
    /// High priority operation
    High = 2,
    /// Critical priority operation
    Critical = 3,
    /// Emergency operation
    Emergency = 4,
}

/// Actor-specific operation metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorOperationMetrics {
    /// Processing time in actor
    pub processing_time_ms: Option<u64>,
    /// Queue time before processing
    pub queue_time_ms: Option<u64>,
    /// Number of actor hops
    pub actor_hops: u32,
    /// Messages sent during processing
    pub messages_sent: u32,
    /// Messages received during processing
    pub messages_received: u32,
    /// Memory usage during processing
    pub memory_usage_bytes: Option<u64>,
}

/// Operation routing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRoutingInfo {
    /// Route taken through actor system
    pub actor_route: Vec<String>,
    /// Routing decisions made
    pub routing_decisions: Vec<RoutingDecision>,
    /// Load balancing information
    pub load_balancing: Option<LoadBalancingInfo>,
}

/// Routing decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// Decision point
    pub decision_point: String,
    /// Available options
    pub available_options: Vec<String>,
    /// Chosen option
    pub chosen_option: String,
    /// Decision criteria
    pub decision_criteria: String,
    /// Decision timestamp
    pub decided_at: std::time::SystemTime,
}

/// Performance tracking for operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationPerformanceMetrics {
    /// Operation start time
    pub started_at: std::time::SystemTime,
    /// Operation completion time
    pub completed_at: Option<std::time::SystemTime>,
    /// Total processing duration
    pub total_duration: Option<std::time::Duration>,
    /// Time spent in each stage
    pub stage_durations: std::collections::HashMap<String, std::time::Duration>,
    /// Throughput metrics
    pub throughput: ThroughputMetrics,
    /// Resource utilization
    pub resource_utilization: OperationResourceUtilization,
    /// Performance benchmarks
    pub benchmarks: PerformanceBenchmarks,
}

/// Throughput metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputMetrics {
    /// Operations per second
    pub operations_per_second: f64,
    /// Bytes processed per second
    pub bytes_per_second: u64,
    /// Transactions per second
    pub transactions_per_second: f64,
    /// Average latency
    pub average_latency: std::time::Duration,
}

/// Operation-specific resource utilization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResourceUtilization {
    /// CPU usage percentage
    pub cpu_usage: f64,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// Network bandwidth used
    pub network_usage: u64,
    /// Disk I/O operations
    pub disk_io_operations: u64,
    /// Gas usage (for Alys transactions)
    pub gas_used: Option<u64>,
    /// Bitcoin transaction fees
    pub bitcoin_fees_satoshis: Option<u64>,
}

/// Performance benchmarks and comparisons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBenchmarks {
    /// Expected duration for this operation type
    pub expected_duration: std::time::Duration,
    /// Historical average duration
    pub historical_average: Option<std::time::Duration>,
    /// Performance percentile (vs historical operations)
    pub performance_percentile: Option<f64>,
    /// Efficiency score (0.0 to 1.0)
    pub efficiency_score: f64,
}

/// Error tracking and recovery for operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationErrorTracking {
    /// Errors encountered during operation
    pub errors: Vec<OperationError>,
    /// Recovery attempts made
    pub recovery_attempts: Vec<RecoveryAttempt>,
    /// Current recovery strategy
    pub recovery_strategy: Option<RecoveryStrategy>,
    /// Error patterns detected
    pub error_patterns: Vec<ErrorPattern>,
    /// Escalation history
    pub escalation_history: Vec<EscalationEvent>,
}

/// Operation-specific errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationError {
    /// Error type
    pub error_type: OperationErrorType,
    /// Error message
    pub message: String,
    /// When error occurred
    pub occurred_at: std::time::SystemTime,
    /// Error context
    pub context: ErrorContext,
    /// Recovery recommendations
    pub recovery_recommendations: Vec<String>,
    /// Error severity
    pub severity: ErrorSeverity,
}

/// Types of operation errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationErrorType {
    /// Validation errors
    Validation(ValidationErrorType),
    /// Governance errors
    Governance(GovernanceErrorType),
    /// Blockchain errors
    Blockchain(BlockchainErrorType),
    /// Network errors
    Network(NetworkErrorType),
    /// System errors
    System(SystemErrorType),
    /// User errors
    User(UserErrorType),
}

/// Peg-in operation status and tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PegInStatus {
    Detected {
        bitcoin_txid: bitcoin::Txid,
        detected_at: std::time::SystemTime,
        confirmations: u32,
    },
    Confirming {
        bitcoin_txid: bitcoin::Txid,
        current_confirmations: u32,
        required_confirmations: u32,
        estimated_completion: Option<std::time::SystemTime>,
    },
    Confirmed {
        bitcoin_txid: bitcoin::Txid,
        alys_recipient: Address,
        amount_satoshis: u64,
        confirmed_at: std::time::SystemTime,
    },
    Processing {
        bitcoin_txid: bitcoin::Txid,
        alys_recipient: Address,
        amount_satoshis: u64,
        processing_started: std::time::SystemTime,
    },
    Completed {
        bitcoin_txid: bitcoin::Txid,
        alys_tx_hash: H256,
        alys_recipient: Address,
        amount_satoshis: u64,
        completed_at: std::time::SystemTime,
    },
    Failed {
        bitcoin_txid: bitcoin::Txid,
        error_reason: String,
        failed_at: std::time::SystemTime,
        retry_count: u32,
    },
}

/// Peg-out operation status and tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PegOutStatus {
    Initiated {
        burn_tx_hash: H256,
        bitcoin_recipient: bitcoin::Address,
        amount_satoshis: u64,
        initiated_at: std::time::SystemTime,
    },
    ValidatingBurn {
        burn_tx_hash: H256,
        bitcoin_recipient: bitcoin::Address,
        amount_satoshis: u64,
        validation_started: std::time::SystemTime,
    },
    CollectingSignatures {
        burn_tx_hash: H256,
        bitcoin_tx_unsigned: bitcoin::Transaction,
        signatures_collected: usize,
        signatures_required: usize,
        collection_started: std::time::SystemTime,
        deadline: std::time::SystemTime,
    },
    SigningComplete {
        burn_tx_hash: H256,
        bitcoin_tx_signed: bitcoin::Transaction,
        signatures: Vec<FederationSignature>,
        completed_at: std::time::SystemTime,
    },
    Broadcasting {
        burn_tx_hash: H256,
        bitcoin_txid: bitcoin::Txid,
        broadcast_attempts: u32,
        last_attempt: std::time::SystemTime,
    },
    Broadcast {
        burn_tx_hash: H256,
        bitcoin_txid: bitcoin::Txid,
        broadcast_at: std::time::SystemTime,
        confirmations: u32,
    },
    Completed {
        burn_tx_hash: H256,
        bitcoin_txid: bitcoin::Txid,
        amount_satoshis: u64,
        completed_at: std::time::SystemTime,
        final_confirmations: u32,
    },
    Failed {
        burn_tx_hash: H256,
        error_reason: String,
        failed_at: std::time::SystemTime,
        recovery_possible: bool,
    },
}

/// Federation member information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMember {
    pub alys_address: Address,
    pub bitcoin_public_key: bitcoin::PublicKey,
    pub signing_weight: u32,
    pub is_active: bool,
    pub joined_at: std::time::SystemTime,
    pub last_activity: std::time::SystemTime,
    pub reputation_score: i32,
    pub successful_signatures: u64,
    pub failed_signatures: u64,
}

/// Federation signature for multi-sig operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationSignature {
    pub signer_address: Address,
    pub signature_data: Vec<u8>,
    pub public_key: bitcoin::PublicKey,
    pub signature_type: FederationSignatureType,
    pub created_at: std::time::SystemTime,
    pub message_hash: Hash256,
}

/// Types of federation signatures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FederationSignatureType {
    ECDSA,
    Schnorr,
    BLS,
    Threshold,
}

/// Federation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    pub members: Vec<FederationMember>,
    pub threshold: usize,
    pub multisig_address: bitcoin::Address,
    pub emergency_addresses: Vec<bitcoin::Address>,
    pub signing_timeout: std::time::Duration,
    pub minimum_confirmations: u32,
    pub maximum_amount: u64,
    pub fee_rate_sat_per_vbyte: u64,
}

/// Bitcoin UTXO information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoInfo {
    pub outpoint: bitcoin::OutPoint,
    pub value_satoshis: u64,
    pub script_pubkey: bitcoin::ScriptBuf,
    pub confirmations: u32,
    pub is_locked: bool,
    pub locked_until: Option<std::time::SystemTime>,
    pub reserved_for: Option<String>, // Operation ID that reserved this UTXO
}

/// Bitcoin transaction fee estimation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeEstimate {
    pub sat_per_vbyte: u64,
    pub total_fee_satoshis: u64,
    pub confidence_level: f64,
    pub estimated_confirmation_blocks: u32,
    pub estimated_confirmation_time: std::time::Duration,
}

/// Bridge operation metrics and statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeMetrics {
    // Peg-in metrics
    pub total_pegins: u64,
    pub successful_pegins: u64,
    pub failed_pegins: u64,
    pub pending_pegins: u64,
    pub total_pegin_value_satoshis: u64,
    pub average_pegin_time: std::time::Duration,
    
    // Peg-out metrics
    pub total_pegouts: u64,
    pub successful_pegouts: u64,
    pub failed_pegouts: u64,
    pub pending_pegouts: u64,
    pub total_pegout_value_satoshis: u64,
    pub average_pegout_time: std::time::Duration,
    
    // Federation metrics
    pub federation_health_score: f64,
    pub active_federation_members: usize,
    pub successful_signatures_24h: u64,
    pub failed_signatures_24h: u64,
    
    // System metrics
    pub bridge_uptime: std::time::Duration,
    pub last_bitcoin_block_seen: u64,
    pub bitcoin_node_sync_status: bool,
}

/// Bridge configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub bitcoin_network: bitcoin::Network,
    pub bitcoin_node_url: String,
    pub bitcoin_node_auth: BitcoinNodeAuth,
    pub federation_config: FederationConfig,
    pub monitoring_addresses: Vec<MonitoredAddress>,
    pub operation_limits: OperationLimits,
    pub security_params: SecurityParams,
}

/// Bitcoin node authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BitcoinNodeAuth {
    None,
    UserPass { username: String, password: String },
    Cookie { cookie_file: String },
}

/// Monitored Bitcoin address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoredAddress {
    pub address: bitcoin::Address,
    pub purpose: AddressPurpose,
    pub derivation_path: Option<String>,
    pub created_at: std::time::SystemTime,
    pub last_activity: Option<std::time::SystemTime>,
}

/// Purpose of monitored addresses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AddressPurpose {
    PegIn,
    Federation,
    Emergency,
    Change,
    Temporary { expires_at: std::time::SystemTime },
}

/// Operation limits and constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationLimits {
    pub min_pegin_amount: u64,
    pub max_pegin_amount: u64,
    pub min_pegout_amount: u64,
    pub max_pegout_amount: u64,
    pub daily_volume_limit: u64,
    pub max_pending_operations: usize,
    pub operation_timeout: std::time::Duration,
}

/// Security parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityParams {
    pub required_confirmations_pegin: u32,
    pub required_confirmations_pegout: u32,
    pub reorg_protection_depth: u32,
    pub signature_timeout: std::time::Duration,
    pub emergency_pause_threshold: f64,
    pub max_federation_offline: usize,
}

/// Bitcoin blockchain reorg handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorgInfo {
    pub old_chain_tip: BlockHash,
    pub new_chain_tip: BlockHash,
    pub reorg_depth: u32,
    pub affected_transactions: Vec<bitcoin::Txid>,
    pub detected_at: std::time::SystemTime,
    pub resolved: bool,
}

/// Bridge health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeHealth {
    Healthy,
    Warning { issues: Vec<String> },
    Critical { critical_issues: Vec<String> },
    Emergency { reason: String, paused_at: std::time::SystemTime },
}

/// Bridge operational state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeState {
    Active,
    Paused { reason: String, paused_at: std::time::SystemTime },
    Emergency { reason: String, triggered_at: std::time::SystemTime },
    Maintenance { 
        reason: String, 
        started_at: std::time::SystemTime,
        estimated_duration: std::time::Duration,
    },
}

impl PegInStatus {
    /// Check if peg-in is in a final state
    pub fn is_final(&self) -> bool {
        matches!(self, PegInStatus::Completed { .. } | PegInStatus::Failed { .. })
    }
    
    /// Get current confirmation count
    pub fn confirmations(&self) -> u32 {
        match self {
            PegInStatus::Detected { confirmations, .. } => *confirmations,
            PegInStatus::Confirming { current_confirmations, .. } => *current_confirmations,
            _ => 0,
        }
    }
    
    /// Get estimated completion time if available
    pub fn estimated_completion(&self) -> Option<std::time::SystemTime> {
        match self {
            PegInStatus::Confirming { estimated_completion, .. } => *estimated_completion,
            _ => None,
        }
    }
    
    /// Get processing duration
    pub fn processing_duration(&self) -> Option<std::time::Duration> {
        match self {
            PegInStatus::Completed { detected_at, completed_at, .. } => {
                Some(completed_at.duration_since(*detected_at).unwrap_or_default())
            }
            _ => None,
        }
    }
}

impl PegOutStatus {
    /// Check if peg-out is in a final state
    pub fn is_final(&self) -> bool {
        matches!(self, PegOutStatus::Completed { .. } | PegOutStatus::Failed { .. })
    }
    
    /// Get signature collection progress
    pub fn signature_progress(&self) -> Option<(usize, usize)> {
        match self {
            PegOutStatus::CollectingSignatures { signatures_collected, signatures_required, .. } => {
                Some((*signatures_collected, *signatures_required))
            }
            _ => None,
        }
    }
    
    /// Check if signature collection deadline has passed
    pub fn is_signature_deadline_passed(&self) -> bool {
        match self {
            PegOutStatus::CollectingSignatures { deadline, .. } => {
                std::time::SystemTime::now() > *deadline
            }
            _ => false,
        }
    }
}

impl FederationMember {
    /// Create new federation member
    pub fn new(
        alys_address: Address,
        bitcoin_public_key: bitcoin::PublicKey,
        signing_weight: u32,
    ) -> Self {
        Self {
            alys_address,
            bitcoin_public_key,
            signing_weight,
            is_active: true,
            joined_at: std::time::SystemTime::now(),
            last_activity: std::time::SystemTime::now(),
            reputation_score: 0,
            successful_signatures: 0,
            failed_signatures: 0,
        }
    }
    
    /// Update member activity
    pub fn update_activity(&mut self) {
        self.last_activity = std::time::SystemTime::now();
    }
    
    /// Record successful signature
    pub fn record_successful_signature(&mut self) {
        self.successful_signatures += 1;
        self.reputation_score += 1;
        self.update_activity();
    }
    
    /// Record failed signature
    pub fn record_failed_signature(&mut self) {
        self.failed_signatures += 1;
        self.reputation_score -= 2;
        self.update_activity();
    }
    
    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.successful_signatures + self.failed_signatures;
        if total == 0 {
            1.0
        } else {
            self.successful_signatures as f64 / total as f64
        }
    }
    
    /// Check if member is considered reliable
    pub fn is_reliable(&self) -> bool {
        self.reputation_score > -10 && self.success_rate() > 0.8
    }
    
    /// Check if member has been active recently
    pub fn is_recently_active(&self, threshold: std::time::Duration) -> bool {
        std::time::SystemTime::now()
            .duration_since(self.last_activity)
            .unwrap_or_default() < threshold
    }
}

impl FederationConfig {
    /// Check if threshold is met with active members
    pub fn has_sufficient_active_members(&self) -> bool {
        let active_count = self.members.iter().filter(|m| m.is_active).count();
        active_count >= self.threshold
    }
    
    /// Get active members
    pub fn active_members(&self) -> Vec<&FederationMember> {
        self.members.iter().filter(|m| m.is_active).collect()
    }
    
    /// Get total voting weight of active members
    pub fn total_active_weight(&self) -> u32 {
        self.active_members()
            .iter()
            .map(|m| m.signing_weight)
            .sum()
    }
    
    /// Check if enough signatures are collected
    pub fn is_threshold_met(&self, signatures: &[FederationSignature]) -> bool {
        let collected_weight: u32 = signatures
            .iter()
            .filter_map(|sig| {
                self.members
                    .iter()
                    .find(|m| m.alys_address == sig.signer_address)
                    .map(|m| m.signing_weight)
            })
            .sum();
            
        let required_weight: u32 = self.total_active_weight() * self.threshold as u32 / self.members.len() as u32;
        collected_weight >= required_weight
    }
}

impl BridgeMetrics {
    /// Create new bridge metrics
    pub fn new() -> Self {
        Self {
            total_pegins: 0,
            successful_pegins: 0,
            failed_pegins: 0,
            pending_pegins: 0,
            total_pegin_value_satoshis: 0,
            average_pegin_time: std::time::Duration::from_secs(0),
            total_pegouts: 0,
            successful_pegouts: 0,
            failed_pegouts: 0,
            pending_pegouts: 0,
            total_pegout_value_satoshis: 0,
            average_pegout_time: std::time::Duration::from_secs(0),
            federation_health_score: 1.0,
            active_federation_members: 0,
            successful_signatures_24h: 0,
            failed_signatures_24h: 0,
            bridge_uptime: std::time::Duration::from_secs(0),
            last_bitcoin_block_seen: 0,
            bitcoin_node_sync_status: false,
        }
    }
    
    /// Get peg-in success rate
    pub fn pegin_success_rate(&self) -> f64 {
        if self.total_pegins == 0 {
            0.0
        } else {
            self.successful_pegins as f64 / self.total_pegins as f64
        }
    }
    
    /// Get peg-out success rate
    pub fn pegout_success_rate(&self) -> f64 {
        if self.total_pegouts == 0 {
            0.0
        } else {
            self.successful_pegouts as f64 / self.total_pegouts as f64
        }
    }
    
    /// Get federation signature success rate
    pub fn federation_signature_success_rate(&self) -> f64 {
        let total_signatures = self.successful_signatures_24h + self.failed_signatures_24h;
        if total_signatures == 0 {
            1.0
        } else {
            self.successful_signatures_24h as f64 / total_signatures as f64
        }
    }
    
    /// Check if bridge is performing well
    pub fn is_healthy(&self) -> bool {
        self.pegin_success_rate() > 0.95
            && self.pegout_success_rate() > 0.95
            && self.federation_health_score > 0.8
            && self.bitcoin_node_sync_status
    }
}

impl Default for BridgeMetrics {
    fn default() -> Self {
        Self::new()
    }
}