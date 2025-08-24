//! Error types for the actor system

use std::fmt;
use thiserror::Error;

/// Result type for actor operations
pub type ActorResult<T> = Result<T, ActorError>;

/// Actor system error types with enhanced context preservation and recovery recommendations
#[derive(Debug, Error, Clone)]
pub enum ActorError {
    /// Actor not found in registry
    #[error("Actor not found: {name}")]
    ActorNotFound { name: String },
    
    /// Actor failed to start
    #[error("Actor startup failed: {actor_type} - {reason}")]
    StartupFailed { actor_type: String, reason: String },
    
    /// Actor failed to stop cleanly
    #[error("Actor shutdown failed: {actor_type} - {reason}")]
    ShutdownFailed { actor_type: String, reason: String },
    
    /// Message delivery failed
    #[error("Message delivery failed from {from} to {to}: {reason}")]
    MessageDeliveryFailed { from: String, to: String, reason: String },
    
    /// Message handling failed
    #[error("Message handling failed: {message_type} - {reason}")]
    MessageHandlingFailed { message_type: String, reason: String },
    
    /// Actor supervision failed
    #[error("Supervision failed for {actor_name}: {reason}")]
    SupervisionFailed { actor_name: String, reason: String },
    
    /// Actor restart failed
    #[error("Actor restart failed: {actor_name} - {reason}")]
    RestartFailed { actor_name: String, reason: String },
    
    
    /// Configuration error
    #[error("Configuration error: {parameter} - {reason}")]
    ConfigurationError { parameter: String, reason: String },
    
    /// Permission denied
    #[error("Permission denied: {resource} - {reason}")]
    PermissionDenied { resource: String, reason: String },
    
    /// Invalid state transition
    #[error("Invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },
    
    /// Timeout occurred
    #[error("Operation timed out: {operation} after {timeout:?}")]
    Timeout { operation: String, timeout: std::time::Duration },
    
    /// Deadlock detected
    #[error("Deadlock detected in actor chain: {actors:?}")]
    DeadlockDetected { actors: Vec<String> },
    
    /// Actor mailbox full
    #[error("Mailbox full for actor {actor_name}: {current_size}/{max_size}")]
    MailboxFull { actor_name: String, current_size: usize, max_size: usize },
    
    /// Serialization error
    #[error("Serialization failed: {reason}")]
    SerializationFailed { reason: String },
    
    /// Deserialization error
    #[error("Deserialization failed: {reason}")]
    DeserializationFailed { reason: String },
    
    /// Network error
    #[error("Network error: {reason}")]
    NetworkError { reason: String },
    
    /// Storage error
    #[error("Storage error: {reason}")]
    StorageError { reason: String },
    
    /// Critical system failure
    #[error("Critical system failure: {reason}")]
    SystemFailure { reason: String },
    
    /// Internal error (should not happen in production)
    #[error("Internal error: {reason}")]
    Internal { reason: String },
    
    /// External dependency error
    #[error("External dependency error: {service} - {reason}")]
    ExternalDependency { service: String, reason: String },
    
    /// Rate limit exceeded
    #[error("Rate limit exceeded: {limit} requests per {window:?}")]
    RateLimitExceeded { limit: u32, window: std::time::Duration },
    
    /// Custom error with context
    #[error("Custom error: {message}")]
    Custom { message: String },
    
    /// Resource not found
    #[error("Resource not found: {resource} with id {id}")]
    NotFound { resource: String, id: String },
    
    /// Invalid operation attempted
    #[error("Invalid operation: {operation} - {reason}")]
    InvalidOperation { operation: String, reason: String },
    
    /// Validation failed
    #[error("Validation failed for {field}: {reason}")]
    ValidationFailed { field: String, reason: String },
    
    /// Resource exhausted with details
    #[error("Resource exhausted: {resource} - {details}")]
    ResourceExhausted { resource: String, details: String },
    
    /// Metrics initialization failed
    #[error("Metrics initialization failed: {reason}")]
    MetricsInitializationFailed { reason: String },
    
    /// Metrics export failed
    #[error("Metrics export failed: {reason}")]
    MetricsExportFailed { reason: String },
}

/// Blockchain-specific actor errors
#[derive(Debug, Error, Clone)]
pub enum BlockchainActorError {
    /// Block validation failed
    #[error("Block validation failed: {block_hash} - {reason}")]
    BlockValidationFailed {
        block_hash: String,
        reason: String,
        context: BlockchainErrorContext,
    },
    
    /// Block sync failed
    #[error("Block sync failed from peer {peer_id}: {reason}")]
    BlockSyncFailed {
        peer_id: String,
        reason: String,
        recovery_strategy: SyncRecoveryStrategy,
    },
    
    /// Chain reorganization handling failed
    #[error("Chain reorg handling failed at depth {depth}: {reason}")]
    ReorgHandlingFailed {
        depth: u32,
        reason: String,
        affected_blocks: Vec<String>,
    },
    
    /// Consensus mechanism error
    #[error("Consensus error: {consensus_type} - {reason}")]
    ConsensusError {
        consensus_type: String,
        reason: String,
        epoch: Option<u64>,
    },
    
    /// State transition error
    #[error("State transition error: {from_state} -> {to_state} - {reason}")]
    StateTransitionError {
        from_state: String,
        to_state: String,
        reason: String,
        rollback_possible: bool,
    },
}

/// Bridge/Peg operation specific errors
#[derive(Debug, Error, Clone)]
pub enum BridgeActorError {
    /// Peg-in processing failed
    #[error("Peg-in failed for Bitcoin tx {bitcoin_txid}: {reason}")]
    PegInFailed {
        bitcoin_txid: String,
        reason: String,
        retry_possible: bool,
        recovery_actions: Vec<PegRecoveryAction>,
    },
    
    /// Peg-out processing failed
    #[error("Peg-out failed for burn tx {burn_tx_hash}: {reason}")]
    PegOutFailed {
        burn_tx_hash: String,
        reason: String,
        signature_status: SignatureCollectionStatus,
        recovery_deadline: Option<std::time::SystemTime>,
    },
    
    /// Federation signature collection failed
    #[error("Signature collection failed: {collected}/{required} signatures")]
    SignatureCollectionFailed {
        collected: usize,
        required: usize,
        failed_members: Vec<String>,
        timeout: std::time::Duration,
    },
    
    /// Bitcoin node communication error
    #[error("Bitcoin node error: {node_endpoint} - {reason}")]
    BitcoinNodeError {
        node_endpoint: String,
        reason: String,
        fallback_available: bool,
    },
    
    /// Governance approval failed
    #[error("Governance approval failed for operation {operation_id}: {reason}")]
    GovernanceApprovalFailed {
        operation_id: String,
        reason: String,
        appeal_possible: bool,
        required_approvals: u32,
        received_approvals: u32,
    },
}

/// Networking actor specific errors
#[derive(Debug, Error, Clone)]
pub enum NetworkActorError {
    /// Peer connection failed
    #[error("Peer connection failed to {peer_id}: {reason}")]
    PeerConnectionFailed {
        peer_id: String,
        reason: String,
        retry_strategy: PeerRetryStrategy,
    },
    
    /// Message broadcast failed
    #[error("Message broadcast failed: {message_type} - {reason}")]
    BroadcastFailed {
        message_type: String,
        reason: String,
        failed_peers: Vec<String>,
        successful_peers: Vec<String>,
    },
    
    /// DHT operation failed
    #[error("DHT operation failed: {operation} - {reason}")]
    DHTOperationFailed {
        operation: String,
        reason: String,
        retry_with_different_strategy: bool,
    },
    
    /// Protocol version mismatch
    #[error("Protocol version mismatch with {peer_id}: local={local_version}, remote={remote_version}")]
    ProtocolVersionMismatch {
        peer_id: String,
        local_version: String,
        remote_version: String,
        compatibility_possible: bool,
    },
}

/// Mining actor specific errors
#[derive(Debug, Error, Clone)]
pub enum MiningActorError {
    /// Block template creation failed
    #[error("Block template creation failed: {reason}")]
    BlockTemplateCreationFailed {
        reason: String,
        retry_possible: bool,
        fallback_template: Option<String>,
    },
    
    /// Mining hardware communication failed
    #[error("Mining hardware error: {hardware_id} - {reason}")]
    MiningHardwareError {
        hardware_id: String,
        reason: String,
        hardware_status: MiningHardwareStatus,
    },
    
    /// Work distribution failed
    #[error("Work distribution failed to {worker_count} workers: {reason}")]
    WorkDistributionFailed {
        worker_count: usize,
        reason: String,
        affected_workers: Vec<String>,
    },
    
    /// Solution validation failed
    #[error("Solution validation failed: {solution_hash} - {reason}")]
    SolutionValidationFailed {
        solution_hash: String,
        reason: String,
        solution_data: Option<Vec<u8>>,
    },
}

/// Error context structures for specific domains
#[derive(Debug, Clone)]
pub struct BlockchainErrorContext {
    pub block_height: Option<u64>,
    pub chain_tip: Option<String>,
    pub sync_status: Option<String>,
    pub peer_count: Option<usize>,
    pub validation_stage: Option<String>,
}

/// Recovery strategy for sync failures
#[derive(Debug, Clone)]
pub enum SyncRecoveryStrategy {
    /// Retry with same peer
    RetryWithSamePeer { delay: std::time::Duration },
    /// Try different peer
    TryDifferentPeer { exclude_peers: Vec<String> },
    /// Reset sync state and restart
    ResetAndRestart { checkpoint: Option<u64> },
    /// Perform deep sync validation
    DeepValidation { start_height: u64 },
}

/// Recovery actions for peg operations
#[derive(Debug, Clone)]
pub enum PegRecoveryAction {
    /// Wait for more confirmations
    WaitForConfirmations { current: u32, required: u32 },
    /// Manual intervention required
    ManualIntervention { reason: String, contact: String },
    /// Retry with different federation member
    RetryWithDifferentMember { exclude_members: Vec<String> },
    /// Escalate to governance
    EscalateToGovernance { priority: String },
}

/// Signature collection status
#[derive(Debug, Clone)]
pub enum SignatureCollectionStatus {
    /// Still collecting
    InProgress { collected: usize, required: usize },
    /// Timed out
    TimedOut { collected: usize, required: usize },
    /// Threshold met
    ThresholdMet { collected: usize },
    /// Failed permanently
    Failed { reason: String },
}

/// Peer retry strategy
#[derive(Debug, Clone)]
pub enum PeerRetryStrategy {
    /// Exponential backoff
    ExponentialBackoff { 
        base_delay: std::time::Duration,
        max_delay: std::time::Duration,
        attempt: u32,
    },
    /// Fixed interval
    FixedInterval { interval: std::time::Duration, max_attempts: u32 },
    /// No retry
    NoRetry,
    /// Retry with different network path
    DifferentPath { alternative_addresses: Vec<String> },
}

/// Mining hardware status
#[derive(Debug, Clone)]
pub enum MiningHardwareStatus {
    /// Hardware is operational
    Operational,
    /// Hardware has degraded performance
    Degraded { performance_percentage: f64 },
    /// Hardware is offline
    Offline { last_seen: std::time::SystemTime },
    /// Hardware has errors
    Error { error_count: u32, error_rate: f64 },
}

/// Comprehensive error context with recovery recommendations
#[derive(Debug, Clone)]
pub struct EnhancedErrorContext {
    /// Basic error context
    pub base_context: ErrorContext,
    /// Error correlation ID for distributed tracing
    pub correlation_id: Option<uuid::Uuid>,
    /// Related errors that led to this one
    pub causal_chain: Vec<String>,
    /// Suggested recovery actions
    pub recovery_recommendations: Vec<RecoveryRecommendation>,
    /// Error impact assessment
    pub impact_assessment: ErrorImpactAssessment,
    /// Escalation path
    pub escalation_path: Vec<EscalationLevel>,
    /// Related metrics and measurements
    pub metrics: std::collections::HashMap<String, f64>,
}

/// Recovery recommendation
#[derive(Debug, Clone)]
pub struct RecoveryRecommendation {
    /// Recommended action
    pub action: String,
    /// Priority of this recommendation
    pub priority: RecoveryPriority,
    /// Estimated success probability
    pub success_probability: f64,
    /// Estimated recovery time
    pub estimated_time: std::time::Duration,
    /// Prerequisites for this recovery action
    pub prerequisites: Vec<String>,
    /// Side effects of this action
    pub side_effects: Vec<String>,
}

/// Recovery priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecoveryPriority {
    /// Try as last resort
    Low = 0,
    /// Standard recovery action
    Medium = 1,
    /// High priority recovery action
    High = 2,
    /// Critical recovery action - try first
    Critical = 3,
}

/// Error impact assessment
#[derive(Debug, Clone)]
pub struct ErrorImpactAssessment {
    /// Affected components
    pub affected_components: Vec<String>,
    /// Performance impact (0.0 = no impact, 1.0 = complete failure)
    pub performance_impact: f64,
    /// Data integrity impact
    pub data_integrity_impact: DataIntegrityImpact,
    /// User experience impact
    pub user_experience_impact: UserExperienceImpact,
    /// System availability impact
    pub availability_impact: AvailabilityImpact,
    /// Estimated recovery time
    pub estimated_recovery_time: std::time::Duration,
}

/// Data integrity impact levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataIntegrityImpact {
    /// No data integrity issues
    None,
    /// Minor data inconsistency
    Minor,
    /// Significant data corruption possible
    Significant,
    /// Critical data loss possible
    Critical,
}

/// User experience impact levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserExperienceImpact {
    /// No user impact
    None,
    /// Minor delays or glitches
    Minor,
    /// Significant functionality impaired
    Significant,
    /// Service unavailable
    Severe,
}

/// System availability impact
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailabilityImpact {
    /// System fully available
    None,
    /// Reduced performance
    Degraded,
    /// Partial service outage
    PartialOutage,
    /// Complete service outage
    CompleteOutage,
}

/// Escalation levels
#[derive(Debug, Clone)]
pub enum EscalationLevel {
    /// Handle within actor
    ActorLevel { retry_count: u32, max_retries: u32 },
    /// Escalate to supervisor
    SupervisorLevel { supervisor_name: String },
    /// Escalate to system level
    SystemLevel { system_component: String },
    /// Escalate to operations team
    OperationsLevel { alert_channel: String, severity: String },
    /// Emergency escalation
    EmergencyLevel { contact_list: Vec<String> },
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Low impact, system continues normally
    Minor,
    /// Medium impact, might affect performance
    Moderate,
    /// High impact, requires attention
    Major,
    /// System-threatening, requires immediate action
    Critical,
    /// System failure, emergency shutdown required
    Fatal,
}

/// Error context for better debugging
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub actor_name: String,
    pub actor_type: String,
    pub message_type: Option<String>,
    pub timestamp: std::time::SystemTime,
    pub severity: ErrorSeverity,
    pub metadata: std::collections::HashMap<String, String>,
}

impl ErrorContext {
    /// Create new error context
    pub fn new(actor_name: String, actor_type: String) -> Self {
        Self {
            actor_name,
            actor_type,
            message_type: None,
            timestamp: std::time::SystemTime::now(),
            severity: ErrorSeverity::Moderate,
            metadata: std::collections::HashMap::new(),
        }
    }
    
    /// Set message type
    pub fn with_message_type(mut self, message_type: String) -> Self {
        self.message_type = Some(message_type);
        self
    }
    
    /// Set severity
    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }
    
    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
    
    /// Add multiple metadata entries
    pub fn with_metadata_map(mut self, metadata: std::collections::HashMap<String, String>) -> Self {
        self.metadata.extend(metadata);
        self
    }
}

/// Enhanced error conversion from domain-specific errors to general ActorError
impl From<BlockchainActorError> for ActorError {
    fn from(err: BlockchainActorError) -> Self {
        match err {
            BlockchainActorError::BlockValidationFailed { block_hash, reason, .. } => {
                ActorError::MessageHandlingFailed {
                    message_type: "BlockValidation".to_string(),
                    reason: format!("Block {} validation failed: {}", block_hash, reason),
                }
            }
            BlockchainActorError::BlockSyncFailed { peer_id, reason, .. } => {
                ActorError::ExternalDependency {
                    service: format!("peer_{}", peer_id),
                    reason,
                }
            }
            BlockchainActorError::ReorgHandlingFailed { depth, reason, .. } => {
                ActorError::InvalidStateTransition {
                    from: "stable_chain".to_string(),
                    to: format!("reorg_depth_{}", depth),
                }
            }
            BlockchainActorError::ConsensusError { consensus_type, reason, .. } => {
                ActorError::SystemFailure {
                    reason: format!("{} consensus error: {}", consensus_type, reason),
                }
            }
            BlockchainActorError::StateTransitionError { from_state, to_state, reason, .. } => {
                ActorError::InvalidStateTransition { from: from_state, to: to_state }
            }
        }
    }
}

impl From<BridgeActorError> for ActorError {
    fn from(err: BridgeActorError) -> Self {
        match err {
            BridgeActorError::PegInFailed { bitcoin_txid, reason, .. } => {
                ActorError::MessageHandlingFailed {
                    message_type: "PegIn".to_string(),
                    reason: format!("PegIn failed for {}: {}", bitcoin_txid, reason),
                }
            }
            BridgeActorError::PegOutFailed { burn_tx_hash, reason, .. } => {
                ActorError::MessageHandlingFailed {
                    message_type: "PegOut".to_string(),
                    reason: format!("PegOut failed for {}: {}", burn_tx_hash, reason),
                }
            }
            BridgeActorError::SignatureCollectionFailed { collected, required, .. } => {
                ActorError::Timeout {
                    operation: "signature_collection".to_string(),
                    timeout: std::time::Duration::from_secs(300), // Default timeout
                }
            }
            BridgeActorError::BitcoinNodeError { node_endpoint, reason, .. } => {
                ActorError::ExternalDependency {
                    service: format!("bitcoin_node_{}", node_endpoint),
                    reason,
                }
            }
            BridgeActorError::GovernanceApprovalFailed { operation_id, reason, .. } => {
                ActorError::PermissionDenied {
                    resource: format!("governance_approval_{}", operation_id),
                    reason,
                }
            }
        }
    }
}

impl From<NetworkActorError> for ActorError {
    fn from(err: NetworkActorError) -> Self {
        match err {
            NetworkActorError::PeerConnectionFailed { peer_id, reason, .. } => {
                ActorError::ExternalDependency {
                    service: format!("peer_{}", peer_id),
                    reason,
                }
            }
            NetworkActorError::BroadcastFailed { message_type, reason, .. } => {
                ActorError::MessageDeliveryFailed {
                    from: "broadcaster".to_string(),
                    to: "network".to_string(),
                    reason: format!("{} broadcast failed: {}", message_type, reason),
                }
            }
            NetworkActorError::DHTOperationFailed { operation, reason, .. } => {
                ActorError::ExternalDependency {
                    service: "dht".to_string(),
                    reason: format!("{} operation failed: {}", operation, reason),
                }
            }
            NetworkActorError::ProtocolVersionMismatch { peer_id, local_version, remote_version, .. } => {
                ActorError::ConfigurationError {
                    parameter: "protocol_version".to_string(),
                    reason: format!("Mismatch with {}: local={}, remote={}", peer_id, local_version, remote_version),
                }
            }
        }
    }
}

impl From<MiningActorError> for ActorError {
    fn from(err: MiningActorError) -> Self {
        match err {
            MiningActorError::BlockTemplateCreationFailed { reason, .. } => {
                ActorError::MessageHandlingFailed {
                    message_type: "BlockTemplate".to_string(),
                    reason,
                }
            }
            MiningActorError::MiningHardwareError { hardware_id, reason, .. } => {
                ActorError::ExternalDependency {
                    service: format!("mining_hardware_{}", hardware_id),
                    reason,
                }
            }
            MiningActorError::WorkDistributionFailed { worker_count, reason, .. } => {
                ActorError::MessageDeliveryFailed {
                    from: "mining_coordinator".to_string(),
                    to: format!("{}_workers", worker_count),
                    reason,
                }
            }
            MiningActorError::SolutionValidationFailed { solution_hash, reason, .. } => {
                ActorError::MessageHandlingFailed {
                    message_type: "SolutionValidation".to_string(),
                    reason: format!("Solution {} validation failed: {}", solution_hash, reason),
                }
            }
        }
    }
}

impl ActorError {
    /// Get error severity
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            ActorError::SystemFailure { .. } => ErrorSeverity::Fatal,
            ActorError::DeadlockDetected { .. } => ErrorSeverity::Critical,
            ActorError::ResourceExhausted { .. } => ErrorSeverity::Critical,
            ActorError::StartupFailed { .. } => ErrorSeverity::Major,
            ActorError::ShutdownFailed { .. } => ErrorSeverity::Major,
            ActorError::SupervisionFailed { .. } => ErrorSeverity::Major,
            ActorError::RestartFailed { .. } => ErrorSeverity::Major,
            ActorError::MessageDeliveryFailed { .. } => ErrorSeverity::Moderate,
            ActorError::MessageHandlingFailed { .. } => ErrorSeverity::Moderate,
            ActorError::MailboxFull { .. } => ErrorSeverity::Moderate,
            ActorError::Timeout { .. } => ErrorSeverity::Moderate,
            ActorError::InvalidStateTransition { .. } => ErrorSeverity::Moderate,
            ActorError::ConfigurationError { .. } => ErrorSeverity::Major,
            ActorError::PermissionDenied { .. } => ErrorSeverity::Moderate,
            ActorError::SerializationFailed { .. } => ErrorSeverity::Minor,
            ActorError::DeserializationFailed { .. } => ErrorSeverity::Minor,
            ActorError::NetworkError { .. } => ErrorSeverity::Moderate,
            ActorError::StorageError { .. } => ErrorSeverity::Major,
            ActorError::ExternalDependency { .. } => ErrorSeverity::Moderate,
            ActorError::RateLimitExceeded { .. } => ErrorSeverity::Minor,
            ActorError::ActorNotFound { .. } => ErrorSeverity::Minor,
            ActorError::Internal { .. } => ErrorSeverity::Critical,
            ActorError::Custom { .. } => ErrorSeverity::Moderate,
            ActorError::NotFound { .. } => ErrorSeverity::Minor,
            ActorError::InvalidOperation { .. } => ErrorSeverity::Moderate,
            ActorError::ValidationFailed { .. } => ErrorSeverity::Moderate,
            ActorError::MetricsInitializationFailed { .. } => ErrorSeverity::Moderate,
            ActorError::MetricsExportFailed { .. } => ErrorSeverity::Minor,
        }
    }
    
    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self.severity() {
            ErrorSeverity::Fatal | ErrorSeverity::Critical => false,
            _ => true,
        }
    }
    
    /// Check if error should trigger actor restart
    pub fn should_restart_actor(&self) -> bool {
        match self {
            ActorError::MessageHandlingFailed { .. } => true,
            ActorError::InvalidStateTransition { .. } => true,
            ActorError::Internal { .. } => true,
            _ => false,
        }
    }
    
    /// Check if error should escalate to supervisor
    pub fn should_escalate(&self) -> bool {
        match self.severity() {
            ErrorSeverity::Critical | ErrorSeverity::Fatal => true,
            ErrorSeverity::Major => true,
            _ => false,
        }
    }
    
    /// Get error category for metrics
    pub fn category(&self) -> &'static str {
        match self {
            ActorError::ActorNotFound { .. } => "actor_lifecycle",
            ActorError::StartupFailed { .. } => "actor_lifecycle",
            ActorError::ShutdownFailed { .. } => "actor_lifecycle",
            ActorError::RestartFailed { .. } => "actor_lifecycle",
            ActorError::MessageDeliveryFailed { .. } => "messaging",
            ActorError::MessageHandlingFailed { .. } => "messaging",
            ActorError::MailboxFull { .. } => "messaging",
            ActorError::SupervisionFailed { .. } => "supervision",
            ActorError::ResourceExhausted { .. } => "resources",
            ActorError::ConfigurationError { .. } => "configuration",
            ActorError::PermissionDenied { .. } => "security",
            ActorError::InvalidStateTransition { .. } => "state_management",
            ActorError::Timeout { .. } => "performance",
            ActorError::DeadlockDetected { .. } => "deadlock",
            ActorError::SerializationFailed { .. } => "serialization",
            ActorError::DeserializationFailed { .. } => "serialization",
            ActorError::NetworkError { .. } => "network",
            ActorError::StorageError { .. } => "storage",
            ActorError::SystemFailure { .. } => "system",
            ActorError::Internal { .. } => "internal",
            ActorError::ExternalDependency { .. } => "external",
            ActorError::RateLimitExceeded { .. } => "rate_limiting",
            ActorError::Custom { .. } => "custom",
            ActorError::NotFound { .. } => "resource_management",
            ActorError::InvalidOperation { .. } => "operations",
            ActorError::ValidationFailed { .. } => "validation",
            ActorError::MetricsInitializationFailed { .. } => "metrics",
            ActorError::MetricsExportFailed { .. } => "metrics",
        }
    }
    
    /// Create enhanced error context with recovery recommendations
    pub fn create_enhanced_context(
        &self,
        actor_name: String,
        actor_type: String,
    ) -> EnhancedErrorContext {
        let base_context = ErrorContext::new(actor_name.clone(), actor_type.clone())
            .with_severity(self.severity());
        
        let recovery_recommendations = self.generate_recovery_recommendations();
        let impact_assessment = self.assess_impact();
        let escalation_path = self.determine_escalation_path(&actor_type);
        
        EnhancedErrorContext {
            base_context,
            correlation_id: Some(uuid::Uuid::new_v4()),
            causal_chain: Vec::new(),
            recovery_recommendations,
            impact_assessment,
            escalation_path,
            metrics: std::collections::HashMap::new(),
        }
    }
    
    /// Generate recovery recommendations based on error type
    fn generate_recovery_recommendations(&self) -> Vec<RecoveryRecommendation> {
        match self {
            ActorError::MessageHandlingFailed { .. } => vec![
                RecoveryRecommendation {
                    action: "Restart actor with clean state".to_string(),
                    priority: RecoveryPriority::High,
                    success_probability: 0.8,
                    estimated_time: std::time::Duration::from_secs(5),
                    prerequisites: vec!["Actor supervision enabled".to_string()],
                    side_effects: vec!["Message queue will be cleared".to_string()],
                },
                RecoveryRecommendation {
                    action: "Retry message with exponential backoff".to_string(),
                    priority: RecoveryPriority::Medium,
                    success_probability: 0.6,
                    estimated_time: std::time::Duration::from_secs(30),
                    prerequisites: vec!["Message is retryable".to_string()],
                    side_effects: vec!["Increased latency".to_string()],
                },
            ],
            ActorError::NetworkError { .. } => vec![
                RecoveryRecommendation {
                    action: "Retry with different network peer".to_string(),
                    priority: RecoveryPriority::High,
                    success_probability: 0.7,
                    estimated_time: std::time::Duration::from_secs(10),
                    prerequisites: vec!["Alternative peers available".to_string()],
                    side_effects: vec!["May cause temporary data inconsistency".to_string()],
                },
            ],
            ActorError::ResourceExhausted { .. } => vec![
                RecoveryRecommendation {
                    action: "Trigger garbage collection".to_string(),
                    priority: RecoveryPriority::Critical,
                    success_probability: 0.5,
                    estimated_time: std::time::Duration::from_secs(2),
                    prerequisites: vec![],
                    side_effects: vec!["Temporary performance degradation".to_string()],
                },
                RecoveryRecommendation {
                    action: "Scale up resources".to_string(),
                    priority: RecoveryPriority::Medium,
                    success_probability: 0.9,
                    estimated_time: std::time::Duration::from_secs(60),
                    prerequisites: vec!["Auto-scaling enabled".to_string()],
                    side_effects: vec!["Increased resource costs".to_string()],
                },
            ],
            _ => vec![],
        }
    }
    
    /// Assess the impact of this error
    fn assess_impact(&self) -> ErrorImpactAssessment {
        match self.severity() {
            ErrorSeverity::Fatal => ErrorImpactAssessment {
                affected_components: vec!["entire_system".to_string()],
                performance_impact: 1.0,
                data_integrity_impact: DataIntegrityImpact::Critical,
                user_experience_impact: UserExperienceImpact::Severe,
                availability_impact: AvailabilityImpact::CompleteOutage,
                estimated_recovery_time: std::time::Duration::from_secs(300),
            },
            ErrorSeverity::Critical => ErrorImpactAssessment {
                affected_components: vec!["core_components".to_string()],
                performance_impact: 0.8,
                data_integrity_impact: DataIntegrityImpact::Significant,
                user_experience_impact: UserExperienceImpact::Significant,
                availability_impact: AvailabilityImpact::PartialOutage,
                estimated_recovery_time: std::time::Duration::from_secs(120),
            },
            ErrorSeverity::Major => ErrorImpactAssessment {
                affected_components: vec!["single_component".to_string()],
                performance_impact: 0.4,
                data_integrity_impact: DataIntegrityImpact::Minor,
                user_experience_impact: UserExperienceImpact::Minor,
                availability_impact: AvailabilityImpact::Degraded,
                estimated_recovery_time: std::time::Duration::from_secs(30),
            },
            _ => ErrorImpactAssessment {
                affected_components: vec![],
                performance_impact: 0.1,
                data_integrity_impact: DataIntegrityImpact::None,
                user_experience_impact: UserExperienceImpact::None,
                availability_impact: AvailabilityImpact::None,
                estimated_recovery_time: std::time::Duration::from_secs(5),
            },
        }
    }
    
    /// Determine escalation path based on error and actor type
    fn determine_escalation_path(&self, actor_type: &str) -> Vec<EscalationLevel> {
        let mut path = vec![
            EscalationLevel::ActorLevel { retry_count: 0, max_retries: 3 },
        ];
        
        if self.should_escalate() {
            path.push(EscalationLevel::SupervisorLevel {
                supervisor_name: format!("{}_supervisor", actor_type),
            });
        }
        
        if self.severity() >= ErrorSeverity::Critical {
            path.push(EscalationLevel::SystemLevel {
                system_component: "actor_system_manager".to_string(),
            });
            
            if self.severity() == ErrorSeverity::Fatal {
                path.push(EscalationLevel::EmergencyLevel {
                    contact_list: vec!["oncall@example.com".to_string()],
                });
            }
        }
        
        path
    }
}

/// Conversion from common error types
impl From<tokio::time::error::Elapsed> for ActorError {
    fn from(err: tokio::time::error::Elapsed) -> Self {
        ActorError::Timeout {
            operation: "tokio_timeout".to_string(),
            timeout: std::time::Duration::from_millis(0), // Unknown timeout duration
        }
    }
}

impl From<serde_json::Error> for ActorError {
    fn from(err: serde_json::Error) -> Self {
        if err.is_io() {
            ActorError::SerializationFailed {
                reason: format!("JSON I/O error: {}", err),
            }
        } else if err.is_syntax() {
            ActorError::DeserializationFailed {
                reason: format!("JSON syntax error: {}", err),
            }
        } else {
            ActorError::SerializationFailed {
                reason: format!("JSON error: {}", err),
            }
        }
    }
}

impl From<std::io::Error> for ActorError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => ActorError::ActorNotFound {
                name: "unknown".to_string(),
            },
            std::io::ErrorKind::PermissionDenied => ActorError::PermissionDenied {
                resource: "io_operation".to_string(),
                reason: "Permission denied".to_string(),
            },
            std::io::ErrorKind::TimedOut => ActorError::Timeout {
                operation: "io_operation".to_string(),
                timeout: std::time::Duration::from_millis(0),
            },
            _ => ActorError::SystemFailure {
                reason: format!("I/O error: {}", err),
            },
        }
    }
}

/// Error reporting and metrics
pub struct ErrorReporter {
    error_counts: dashmap::DashMap<String, std::sync::atomic::AtomicU64>,
}

impl ErrorReporter {
    /// Create new error reporter
    pub fn new() -> Self {
        Self {
            error_counts: dashmap::DashMap::new(),
        }
    }
    
    /// Report an error
    pub fn report_error(&self, error: &ActorError, context: Option<&ErrorContext>) {
        let category = error.category();
        
        // Increment error count
        let counter = self.error_counts
            .entry(category.to_string())
            .or_insert_with(|| std::sync::atomic::AtomicU64::new(0));
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // Log error
        match error.severity() {
            ErrorSeverity::Fatal => {
                tracing::error!(
                    error = %error,
                    category = category,
                    context = ?context,
                    "FATAL error occurred"
                );
            }
            ErrorSeverity::Critical => {
                tracing::error!(
                    error = %error,
                    category = category,
                    context = ?context,
                    "CRITICAL error occurred"
                );
            }
            ErrorSeverity::Major => {
                tracing::error!(
                    error = %error,
                    category = category,
                    context = ?context,
                    "MAJOR error occurred"
                );
            }
            ErrorSeverity::Moderate => {
                tracing::warn!(
                    error = %error,
                    category = category,
                    context = ?context,
                    "MODERATE error occurred"
                );
            }
            ErrorSeverity::Minor => {
                tracing::debug!(
                    error = %error,
                    category = category,
                    context = ?context,
                    "MINOR error occurred"
                );
            }
        }
    }
    
    /// Get error counts by category
    pub fn get_error_counts(&self) -> std::collections::HashMap<String, u64> {
        self.error_counts
            .iter()
            .map(|entry| {
                let key = entry.key().clone();
                let value = entry.value().load(std::sync::atomic::Ordering::Relaxed);
                (key, value)
            })
            .collect()
    }
    
    /// Reset error counts
    pub fn reset_counts(&self) {
        for mut entry in self.error_counts.iter_mut() {
            entry.value_mut().store(0, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

impl Default for ErrorReporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Default implementations for error context structures
impl Default for BlockchainErrorContext {
    fn default() -> Self {
        Self {
            block_height: None,
            chain_tip: None,
            sync_status: None,
            peer_count: None,
            validation_stage: None,
        }
    }
}

impl Default for EnhancedErrorContext {
    fn default() -> Self {
        Self {
            base_context: ErrorContext::new("unknown".to_string(), "Unknown".to_string()),
            correlation_id: None,
            causal_chain: Vec::new(),
            recovery_recommendations: Vec::new(),
            impact_assessment: ErrorImpactAssessment {
                affected_components: Vec::new(),
                performance_impact: 0.0,
                data_integrity_impact: DataIntegrityImpact::None,
                user_experience_impact: UserExperienceImpact::None,
                availability_impact: AvailabilityImpact::None,
                estimated_recovery_time: std::time::Duration::from_secs(0),
            },
            escalation_path: Vec::new(),
            metrics: std::collections::HashMap::new(),
        }
    }
}

/// Global error reporter instance
static ERROR_REPORTER: once_cell::sync::Lazy<ErrorReporter> = 
    once_cell::sync::Lazy::new(ErrorReporter::new);

/// Report error globally
pub fn report_error(error: &ActorError, context: Option<&ErrorContext>) {
    ERROR_REPORTER.report_error(error, context);
}

/// Get global error counts
pub fn get_global_error_counts() -> std::collections::HashMap<String, u64> {
    ERROR_REPORTER.get_error_counts()
}

/// Reset global error counts
pub fn reset_global_error_counts() {
    ERROR_REPORTER.reset_counts();
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_severity() {
        let error = ActorError::SystemFailure { reason: "test".to_string() };
        assert_eq!(error.severity(), ErrorSeverity::Fatal);
        assert!(!error.is_recoverable());
        assert!(error.should_escalate());
    }
    
    #[test]
    fn test_error_context() {
        let context = ErrorContext::new("test_actor".to_string(), "TestActor".to_string())
            .with_message_type("TestMessage".to_string())
            .with_severity(ErrorSeverity::Major)
            .with_metadata("key".to_string(), "value".to_string());
        
        assert_eq!(context.actor_name, "test_actor");
        assert_eq!(context.message_type, Some("TestMessage".to_string()));
        assert_eq!(context.severity, ErrorSeverity::Major);
        assert_eq!(context.metadata.get("key"), Some(&"value".to_string()));
    }
    
    #[test]
    fn test_error_reporter() {
        let reporter = ErrorReporter::new();
        let error = ActorError::MessageHandlingFailed {
            message_type: "test".to_string(),
            reason: "test".to_string(),
        };
        
        reporter.report_error(&error, None);
        let counts = reporter.get_error_counts();
        assert_eq!(counts.get("messaging"), Some(&1));
    }
}