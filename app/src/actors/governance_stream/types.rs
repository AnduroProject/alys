//! Type definitions for governance stream operations
//!
//! This module provides comprehensive type definitions for governance stream
//! operations, including connection management, message handling, and
//! integration with the broader Alys ecosystem.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

/// Re-export common types from the main types module
pub use crate::types::{Address, Hash256, PublicKey, Signature, H256, U256};

/// Stream-specific error type alias
pub use crate::actors::governance_stream::error::StreamError;

/// Connection state for governance streams
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConnectionState {
    /// Not connected
    Disconnected,
    /// Attempting to connect
    Connecting { 
        attempt: u32, 
        next_retry: Instant 
    },
    /// Connected and operational
    Connected { 
        since: Instant 
    },
    /// Reconnecting after disconnection
    Reconnecting { 
        reason: String, 
        attempt: u32 
    },
    /// Connection failed
    Failed { 
        reason: String, 
        permanent: bool 
    },
    /// Connection suspended by governance or system
    Suspended { 
        reason: String 
    },
}

/// Federation update from governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationUpdate {
    /// Update type
    pub update_type: FederationUpdateType,
    /// Federation members
    pub members: Vec<FederationMember>,
    /// Signature threshold
    pub threshold: usize,
    /// Federation epoch/version
    pub epoch: u64,
    /// P2WSH multisig address
    pub p2wsh_address: bitcoin::Address,
    /// Activation block height
    pub activation_height: Option<u64>,
    /// Update timestamp
    pub timestamp: SystemTime,
    /// Update metadata
    pub metadata: HashMap<String, String>,
}

/// Types of federation updates
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FederationUpdateType {
    /// Member added to federation
    MemberAdded,
    /// Member removed from federation
    MemberRemoved,
    /// Threshold changed
    ThresholdChanged,
    /// Epoch transition
    EpochTransition,
    /// Emergency update
    Emergency,
    /// Scheduled update
    Scheduled,
}

/// Federation member information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMember {
    /// Member's Alys address
    pub alys_address: Address,
    /// Member's Bitcoin public key
    pub bitcoin_public_key: bitcoin::PublicKey,
    /// Member's signing weight in the federation
    pub signing_weight: u32,
    /// Whether member is currently active
    pub is_active: bool,
    /// When member joined the federation
    pub joined_at: SystemTime,
    /// Last activity timestamp
    pub last_activity: SystemTime,
    /// Member's reputation score
    pub reputation_score: i32,
    /// Successful signature count
    pub successful_signatures: u64,
    /// Failed signature count
    pub failed_signatures: u64,
    /// Member-specific metadata
    pub metadata: HashMap<String, String>,
}

/// Consensus block representation for governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusBlock {
    /// Block hash
    pub hash: Hash256,
    /// Block number/height
    pub number: u64,
    /// Parent block hash
    pub parent_hash: Hash256,
    /// Block timestamp
    pub timestamp: u64,
    /// Block proposer
    pub proposer: Address,
    /// Transaction count
    pub transaction_count: u32,
    /// Gas used in block
    pub gas_used: u64,
    /// Gas limit
    pub gas_limit: u64,
    /// Block difficulty
    pub difficulty: U256,
    /// Block state root
    pub state_root: Hash256,
    /// Block receipts root
    pub receipts_root: Hash256,
    /// Extra data
    pub extra_data: Vec<u8>,
}

impl ConsensusBlock {
    /// Get block hash as a string
    pub fn hash(&self) -> String {
        format!("{:x}", self.hash)
    }

    /// Get block number
    pub fn number(&self) -> u64 {
        self.number
    }

    /// Get parent hash as a string
    pub fn parent_hash(&self) -> String {
        format!("{:x}", self.parent_hash)
    }
}

/// Attestation for consensus operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    /// Attestation type
    pub attestation_type: AttestationType,
    /// Block hash being attested
    pub block_hash: Hash256,
    /// Block height
    pub block_height: u64,
    /// Attester address
    pub attester: Address,
    /// Attestation signature
    pub signature: Signature,
    /// Attestation timestamp
    pub timestamp: SystemTime,
    /// Additional attestation data
    pub data: AttestationData,
}

/// Types of attestations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttestationType {
    /// Block validity attestation
    BlockValidity,
    /// Transaction inclusion attestation
    TransactionInclusion,
    /// State transition attestation
    StateTransition,
    /// Finality attestation
    Finality,
    /// Custom attestation
    Custom { attestation_type: String },
}

/// Attestation-specific data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationData {
    /// Merkle proof if applicable
    pub merkle_proof: Option<Vec<Hash256>>,
    /// Transaction hashes if applicable
    pub transaction_hashes: Option<Vec<Hash256>>,
    /// State transitions if applicable
    pub state_transitions: Option<Vec<StateTransition>>,
    /// Custom data
    pub custom_data: HashMap<String, serde_json::Value>,
}

/// State transition information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    /// Account address
    pub account: Address,
    /// Previous state hash
    pub previous_state: Hash256,
    /// New state hash
    pub new_state: Hash256,
    /// State change type
    pub change_type: StateChangeType,
}

/// Types of state changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateChangeType {
    /// Balance change
    BalanceChange,
    /// Contract deployment
    ContractDeployment,
    /// Contract state change
    ContractStateChange,
    /// Account creation
    AccountCreation,
    /// Account deletion
    AccountDeletion,
}

/// Chain status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStatus {
    /// Current block height
    pub block_height: u64,
    /// Current block hash
    pub block_hash: Hash256,
    /// Chain ID
    pub chain_id: u64,
    /// Network ID
    pub network_id: u64,
    /// Peer count
    pub peer_count: u32,
    /// Sync status
    pub sync_status: SyncStatus,
    /// Chain health status
    pub health_status: ChainHealthStatus,
    /// Last update timestamp
    pub last_updated: SystemTime,
}

/// Synchronization status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    /// Whether node is syncing
    pub syncing: bool,
    /// Current sync block
    pub current_block: u64,
    /// Highest known block
    pub highest_block: u64,
    /// Sync progress percentage
    pub progress_percentage: f64,
    /// Estimated time to completion
    pub estimated_completion: Option<Duration>,
}

/// Chain health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChainHealthStatus {
    /// Chain is healthy
    Healthy,
    /// Chain has minor issues
    Warning { issues: Vec<String> },
    /// Chain has serious issues
    Critical { issues: Vec<String> },
    /// Chain is not operational
    Down { reason: String },
}

/// Proposal vote for governance decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalVote {
    /// Proposal identifier
    pub proposal_id: String,
    /// Voter address
    pub voter: Address,
    /// Vote type
    pub vote: VoteType,
    /// Vote signature
    pub signature: Signature,
    /// Vote timestamp
    pub timestamp: SystemTime,
    /// Vote weight
    pub weight: u32,
    /// Vote justification
    pub justification: Option<String>,
}

/// Types of votes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VoteType {
    /// Approve the proposal
    Approve,
    /// Reject the proposal
    Reject,
    /// Abstain from voting
    Abstain,
    /// Conditional approval
    ConditionalApprove { conditions: Vec<String> },
}

/// Transaction data for Alys blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Transaction hash
    pub hash: H256,
    /// Transaction nonce
    pub nonce: u64,
    /// Sender address
    pub from: Address,
    /// Recipient address (None for contract creation)
    pub to: Option<Address>,
    /// Transaction value in wei
    pub value: U256,
    /// Gas limit
    pub gas_limit: u64,
    /// Gas price
    pub gas_price: U256,
    /// Transaction data/input
    pub data: Vec<u8>,
    /// Transaction signature
    pub signature: TransactionSignature,
}

/// Transaction signature components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionSignature {
    /// Recovery ID
    pub v: u8,
    /// Signature r component
    pub r: U256,
    /// Signature s component
    pub s: U256,
}

/// Block hash type alias
pub type BlockHash = Hash256;

/// Event log from smart contract execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLog {
    /// Contract address that emitted the log
    pub address: Address,
    /// Log topics (indexed parameters)
    pub topics: Vec<H256>,
    /// Log data (non-indexed parameters)
    pub data: Vec<u8>,
    /// Block hash containing this log
    pub block_hash: BlockHash,
    /// Transaction hash that generated this log
    pub transaction_hash: H256,
    /// Log index within the block
    pub log_index: u32,
    /// Whether this log was removed due to chain reorg
    pub removed: bool,
}

/// Node capabilities for registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    /// Supported signature algorithms
    pub signature_algorithms: Vec<SignatureAlgorithm>,
    /// Supported consensus protocols
    pub consensus_protocols: Vec<String>,
    /// Maximum concurrent operations
    pub max_concurrent_operations: u32,
    /// Node role in the network
    pub node_role: NodeRole,
    /// Supported features
    pub features: Vec<NodeFeature>,
    /// Network protocols supported
    pub network_protocols: Vec<NetworkProtocol>,
}

/// Signature algorithms supported
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignatureAlgorithm {
    /// ECDSA with secp256k1
    EcdsaSecp256k1,
    /// Schnorr signatures
    Schnorr,
    /// BLS signatures
    Bls,
    /// Ed25519 signatures
    Ed25519,
    /// RSA signatures
    Rsa,
}

/// Node roles in the network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeRole {
    /// Full federation member
    FederationMember,
    /// Validator node
    Validator,
    /// Observer node (read-only)
    Observer,
    /// Gateway node
    Gateway,
    /// Archive node
    Archive,
    /// Light client
    LightClient,
}

/// Node features and capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeFeature {
    /// Supports fast sync
    FastSync,
    /// Supports state pruning
    StatePruning,
    /// Supports transaction indexing
    TransactionIndexing,
    /// Supports event log filtering
    EventLogFiltering,
    /// Supports trace API
    TraceApi,
    /// Custom feature
    Custom { name: String, version: String },
}

/// Network protocols supported
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkProtocol {
    /// libp2p gossipsub
    Libp2pGossipsub,
    /// Ethereum devp2p
    Devp2p,
    /// Custom protocol
    Custom { name: String, version: String },
}

/// Stream configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Maximum number of governance connections
    pub max_governance_connections: usize,
    /// Message buffer size per connection
    pub buffer_size: usize,
    /// Heartbeat interval
    pub heartbeat_interval: Duration,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Governance endpoint URLs
    pub governance_endpoints: Vec<String>,
    /// Authentication token
    pub auth_token: Option<String>,
    /// Request timeout
    pub request_timeout: Duration,
    /// Maximum pending requests
    pub max_pending_requests: usize,
    /// Enable compression
    pub enable_compression: bool,
    /// Compression threshold in bytes
    pub compression_threshold: usize,
}

/// Load balancing strategies for multiple endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoadBalancingStrategy {
    /// Round-robin selection
    RoundRobin,
    /// Random selection
    Random,
    /// Latency-based selection
    LatencyBased,
    /// Priority-based selection
    Priority,
    /// Least connections
    LeastConnections,
    /// Weighted selection
    Weighted { weights: HashMap<String, u32> },
}

/// Message routing strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutingStrategy {
    /// Broadcast to all targets
    Broadcast,
    /// Route to single target
    SingleTarget,
    /// Route based on message content
    ContentBased,
    /// Route based on priority
    PriorityBased,
    /// Custom routing logic
    Custom { handler: String },
}

/// Performance metrics for stream operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamPerformanceMetrics {
    /// Messages per second throughput
    pub messages_per_second: f64,
    /// Average message latency in milliseconds
    pub average_latency_ms: f64,
    /// Peak messages per second
    pub peak_messages_per_second: f64,
    /// 95th percentile latency
    pub p95_latency_ms: f64,
    /// 99th percentile latency
    pub p99_latency_ms: f64,
    /// Error rate percentage
    pub error_rate_percent: f64,
    /// Connection success rate
    pub connection_success_rate: f64,
    /// Buffer utilization percentage
    pub buffer_utilization_percent: f64,
}

/// Message validation results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether validation passed
    pub valid: bool,
    /// Validation errors if any
    pub errors: Vec<ValidationError>,
    /// Validation warnings
    pub warnings: Vec<ValidationWarning>,
    /// Validation timestamp
    pub validated_at: SystemTime,
}

/// Validation error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Field that caused the error
    pub field: Option<String>,
    /// Error severity
    pub severity: ValidationSeverity,
}

/// Validation warning information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    /// Warning code
    pub code: String,
    /// Warning message
    pub message: String,
    /// Field that caused the warning
    pub field: Option<String>,
}

/// Validation severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum ValidationSeverity {
    /// Low severity - advisory only
    Low,
    /// Medium severity - potential issue
    Medium,
    /// High severity - definite issue
    High,
    /// Critical severity - blocking issue
    Critical,
}

/// Governance approval information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceApproval {
    /// Approver address
    pub approver: Address,
    /// Approval signature
    pub signature: Signature,
    /// Approval timestamp
    pub timestamp: SystemTime,
    /// Approval conditions
    pub conditions: Vec<String>,
    /// Approval metadata
    pub metadata: HashMap<String, String>,
}

/// Execution window for approved operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionWindow {
    /// Window start time
    pub start_time: SystemTime,
    /// Window end time
    pub end_time: SystemTime,
    /// Maximum operations in window
    pub max_operations: Option<u32>,
    /// Window conditions
    pub conditions: Vec<String>,
}

/// Progress stage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressStage {
    /// Stage name
    pub name: String,
    /// Stage description
    pub description: String,
    /// Stage start time
    pub started_at: SystemTime,
    /// Stage completion time
    pub completed_at: Option<SystemTime>,
    /// Stage progress percentage
    pub progress_percent: f64,
    /// Stage status
    pub status: StageStatus,
}

/// Stage status enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StageStatus {
    /// Stage is pending
    Pending,
    /// Stage is in progress
    InProgress,
    /// Stage completed successfully
    Completed,
    /// Stage failed
    Failed,
    /// Stage was skipped
    Skipped,
}

/// Blockchain confirmation tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfirmationBlockchain {
    /// Bitcoin confirmations
    Bitcoin,
    /// Alys confirmations
    Alys,
    /// Ethereum confirmations
    Ethereum,
    /// Custom blockchain
    Custom { name: String },
}

/// Operation completion proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionProof {
    /// Proof type
    pub proof_type: ProofType,
    /// Proof data
    pub proof_data: Vec<u8>,
    /// Verification instructions
    pub verification: VerificationInstructions,
    /// Proof timestamp
    pub timestamp: SystemTime,
}

/// Types of completion proofs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProofType {
    /// Merkle proof
    MerkleProof,
    /// Transaction inclusion proof
    TransactionInclusion,
    /// State proof
    StateProof,
    /// Signature proof
    SignatureProof,
    /// Custom proof
    Custom { proof_type: String },
}

/// Verification instructions for proofs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationInstructions {
    /// Verification algorithm
    pub algorithm: String,
    /// Required parameters
    pub parameters: HashMap<String, String>,
    /// Verification steps
    pub steps: Vec<VerificationStep>,
}

/// Individual verification step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStep {
    /// Step description
    pub description: String,
    /// Required inputs
    pub inputs: Vec<String>,
    /// Expected outputs
    pub outputs: Vec<String>,
}

/// Operation failure reasons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailureReason {
    /// Network connectivity issues
    NetworkError { details: String },
    /// Authentication failure
    AuthenticationFailure,
    /// Insufficient funds
    InsufficientFunds,
    /// Invalid parameters
    InvalidParameters { field: String, reason: String },
    /// Timeout occurred
    Timeout { duration: Duration },
    /// System error
    SystemError { error_code: String, message: String },
    /// Governance rejection
    GovernanceRejection { reason: String },
    /// Custom failure reason
    Custom { reason: String },
}

/// Recovery options for failed operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryOption {
    /// Retry with same parameters
    Retry,
    /// Retry with modified parameters
    RetryWithModification { modifications: HashMap<String, String> },
    /// Manual intervention required
    ManualIntervention { instructions: String },
    /// Escalate to governance
    EscalateToGovernance,
    /// Cancel operation
    Cancel,
    /// Custom recovery option
    Custom { option: String, parameters: HashMap<String, String> },
}

/// Refund status for cancelled operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundStatus {
    /// Whether refund is available
    pub available: bool,
    /// Refund amount
    pub amount: u64,
    /// Refund processing status
    pub status: RefundProcessingStatus,
    /// Refund transaction hash if processed
    pub refund_txid: Option<String>,
    /// Estimated processing time
    pub estimated_processing_time: Option<Duration>,
}

/// Refund processing status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RefundProcessingStatus {
    /// Refund is pending
    Pending,
    /// Refund is being processed
    Processing,
    /// Refund completed
    Completed,
    /// Refund failed
    Failed { reason: String },
}

/// Operation initiator information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationInitiator {
    /// Initiator type
    pub initiator_type: InitiatorType,
    /// Initiator address
    pub address: Address,
    /// Initiator metadata
    pub metadata: HashMap<String, String>,
}

/// Types of operation initiators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InitiatorType {
    /// User-initiated operation
    User,
    /// System-initiated operation
    System,
    /// Governance-initiated operation
    Governance,
    /// Scheduled operation
    Scheduled,
    /// Emergency operation
    Emergency,
}

/// Validation step for operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationStep {
    /// Validation step name
    pub name: String,
    /// Validation description
    pub description: String,
    /// Validation status
    pub status: ValidationStepStatus,
    /// Validation result
    pub result: Option<ValidationResult>,
    /// Validation timestamp
    pub validated_at: Option<SystemTime>,
}

/// Validation step status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationStepStatus {
    /// Validation pending
    Pending,
    /// Validation in progress
    InProgress,
    /// Validation passed
    Passed,
    /// Validation failed
    Failed,
    /// Validation skipped
    Skipped,
}

// Default implementations for commonly used types
impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            max_governance_connections: 10,
            buffer_size: 1000,
            heartbeat_interval: Duration::from_secs(30),
            connection_timeout: Duration::from_secs(300),
            governance_endpoints: Vec::new(),
            auth_token: None,
            request_timeout: Duration::from_secs(60),
            max_pending_requests: 100,
            enable_compression: true,
            compression_threshold: 1024,
        }
    }
}

impl Default for StreamPerformanceMetrics {
    fn default() -> Self {
        Self {
            messages_per_second: 0.0,
            average_latency_ms: 0.0,
            peak_messages_per_second: 0.0,
            p95_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            error_rate_percent: 0.0,
            connection_success_rate: 1.0,
            buffer_utilization_percent: 0.0,
        }
    }
}

impl Default for NodeCapabilities {
    fn default() -> Self {
        Self {
            signature_algorithms: vec![SignatureAlgorithm::EcdsaSecp256k1],
            consensus_protocols: vec!["aura".to_string()],
            max_concurrent_operations: 100,
            node_role: NodeRole::Observer,
            features: Vec::new(),
            network_protocols: vec![NetworkProtocol::Libp2pGossipsub],
        }
    }
}

impl ConnectionState {
    /// Check if connection is active
    pub fn is_active(&self) -> bool {
        matches!(self, ConnectionState::Connected { .. })
    }

    /// Check if connection is attempting to connect
    pub fn is_connecting(&self) -> bool {
        matches!(
            self,
            ConnectionState::Connecting { .. } | ConnectionState::Reconnecting { .. }
        )
    }

    /// Check if connection has permanently failed
    pub fn is_permanently_failed(&self) -> bool {
        matches!(self, ConnectionState::Failed { permanent: true, .. })
    }
}

impl VoteType {
    /// Check if vote is positive (approve or conditional approve)
    pub fn is_positive(&self) -> bool {
        matches!(self, VoteType::Approve | VoteType::ConditionalApprove { .. })
    }

    /// Check if vote is negative (reject)
    pub fn is_negative(&self) -> bool {
        matches!(self, VoteType::Reject)
    }
}

impl ValidationSeverity {
    /// Check if severity is blocking
    pub fn is_blocking(&self) -> bool {
        matches!(self, ValidationSeverity::Critical)
    }
}

impl StageStatus {
    /// Check if stage is complete (either successfully or failed)
    pub fn is_complete(&self) -> bool {
        matches!(
            self,
            StageStatus::Completed | StageStatus::Failed | StageStatus::Skipped
        )
    }
}

impl ServiceHealthStatus {
    /// Check if service is operational
    pub fn is_operational(&self) -> bool {
        matches!(self, ServiceHealthStatus::Healthy | ServiceHealthStatus::Degraded)
    }
}

// Conversion implementations for interoperability
impl From<bitcoin::Address> for String {
    fn from(addr: bitcoin::Address) -> Self {
        addr.to_string()
    }
}

impl From<bitcoin::PublicKey> for String {
    fn from(pk: bitcoin::PublicKey) -> Self {
        pk.to_string()
    }
}