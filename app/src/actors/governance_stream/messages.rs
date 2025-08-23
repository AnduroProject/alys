//! Governance stream message types and protocol definitions
//!
//! This module defines all message types used for communication between the StreamActor
//! and Anduro Governance nodes. It includes both internal actor messages and external
//! gRPC protocol messages following the governance streaming protocol.

use crate::types::*;
use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;

/// Messages handled by StreamActor for establishing connections
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct EstablishConnection {
    /// Governance endpoint URL (e.g., "https://governance.anduro.io:443")
    pub endpoint: String,
    /// Optional authentication token for secure communication
    pub auth_token: Option<String>,
    /// Chain identifier for multi-chain governance
    pub chain_id: String,
    /// Connection priority for load balancing
    pub priority: ConnectionPriority,
}

/// Message to get current connection status
#[derive(Message)]
#[rtype(result = "Result<ConnectionStatus, StreamError>")]
pub struct GetConnectionStatus {
    /// Optional specific connection ID to query
    pub connection_id: Option<String>,
}

/// Message to request signatures from governance
#[derive(Message)]
#[rtype(result = "Result<String, StreamError>")]  // Returns request_id
pub struct RequestSignatures {
    /// Unique request identifier for tracking
    pub request_id: String,
    /// Transaction hex data to be signed
    pub tx_hex: String,
    /// Input indices requiring signatures
    pub input_indices: Vec<usize>,
    /// Input amounts in satoshis for verification
    pub amounts: Vec<u64>,
    /// Type of transaction (pegout, federation change, etc.)
    pub tx_type: TransactionType,
    /// Optional timeout for signature collection
    pub timeout: Option<Duration>,
    /// Request priority for governance processing
    pub priority: RequestPriority,
}

/// Message to notify governance of peg-in operations
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct NotifyPegin {
    /// Bitcoin transaction ID
    pub txid: bitcoin::Txid,
    /// Amount in satoshis
    pub amount: u64,
    /// Recipient EVM address
    pub evm_address: Address,
    /// Bitcoin confirmations
    pub confirmations: u32,
    /// Block hash containing the transaction
    pub block_hash: Option<bitcoin::BlockHash>,
}

/// Message to register node with governance
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct RegisterNode {
    /// Node identifier
    pub node_id: String,
    /// Node's public key for authentication
    pub public_key: PublicKey,
    /// Node capabilities and services
    pub capabilities: NodeCapabilities,
    /// Node endpoint for callbacks
    pub callback_endpoint: Option<String>,
}

/// Message to update federation membership
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct UpdateFederation {
    /// Federation update details
    pub update: FederationUpdate,
    /// Whether to broadcast to all governance nodes
    pub broadcast: bool,
}

/// Internal message for signature responses from governance
#[derive(Message)]
#[rtype(result = "()")]
pub struct SignatureResponse {
    /// Request ID that this response corresponds to
    pub request_id: String,
    /// Collected witness data
    pub witnesses: Vec<WitnessData>,
    /// Status of signature collection
    pub status: SignatureStatus,
    /// Governance node that sent the response
    pub source_node: String,
    /// Timestamp when response was generated
    pub timestamp: SystemTime,
}

/// Internal message for federation updates from governance
#[derive(Message)]
#[rtype(result = "()")]
pub struct FederationUpdateMessage {
    /// Federation configuration version
    pub version: u32,
    /// Updated federation members
    pub members: Vec<FederationMember>,
    /// New signature threshold
    pub threshold: usize,
    /// Updated P2WSH multisig address
    pub p2wsh_address: bitcoin::Address,
    /// Block height when update becomes active
    pub activation_height: Option<u64>,
    /// Governance node that sent the update
    pub source_node: String,
}

/// Internal message for governance proposals
#[derive(Message)]
#[rtype(result = "()")]
pub struct ProposalNotification {
    /// Unique proposal identifier
    pub proposal_id: String,
    /// Type of governance proposal
    pub proposal_type: ProposalType,
    /// Proposal data and parameters
    pub data: serde_json::Value,
    /// Voting deadline
    pub voting_deadline: SystemTime,
    /// Required quorum for decision
    pub required_quorum: u32,
    /// Current vote tally
    pub current_votes: VoteTally,
}

/// Message to send heartbeat to governance nodes
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct SendHeartbeat {
    /// Optional specific connection to heartbeat
    pub connection_id: Option<String>,
    /// Include node status in heartbeat
    pub include_status: bool,
}

/// Message to handle connection events
#[derive(Message)]
#[rtype(result = "()")]
pub struct ConnectionEvent {
    /// Connection identifier
    pub connection_id: String,
    /// Type of connection event
    pub event_type: ConnectionEventType,
    /// Event timestamp
    pub timestamp: Instant,
    /// Additional event context
    pub context: HashMap<String, String>,
}

/// Message to handle stream errors and recovery
#[derive(Message)]
#[rtype(result = "()")]
pub struct StreamErrorEvent {
    /// Connection that experienced the error
    pub connection_id: String,
    /// Stream error details
    pub error: StreamError,
    /// Whether automatic recovery should be attempted
    pub auto_recover: bool,
    /// Recovery attempt count
    pub retry_count: u32,
}

/// Message to shutdown connections gracefully
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct ShutdownConnections {
    /// Whether to wait for pending operations
    pub graceful: bool,
    /// Timeout for graceful shutdown
    pub timeout: Option<Duration>,
}

/// Message to get stream metrics and statistics
#[derive(Message)]
#[rtype(result = "StreamMetrics")]
pub struct GetStreamMetrics;

/// Message to update stream configuration
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct UpdateStreamConfig {
    /// Updated configuration
    pub config: StreamConfig,
    /// Whether to restart connections with new config
    pub restart_connections: bool,
}

/// Message to handle emergency governance actions
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct EmergencyAction {
    /// Type of emergency action
    pub action_type: EmergencyActionType,
    /// Action parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Authorization token/signature
    pub authorization: EmergencyAuthorization,
}

// ============================================================================
// Protocol Message Types
// ============================================================================

/// Core stream message for governance communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceStreamMessage {
    /// Message type identifier
    pub message_type: String,
    /// Message payload data
    pub payload: GovernancePayload,
    /// Message timestamp
    pub timestamp: SystemTime,
    /// Sequence number for ordering
    pub sequence_number: u64,
    /// Message priority
    pub priority: MessagePriority,
    /// Optional correlation ID for request/response
    pub correlation_id: Option<String>,
    /// Time-to-live for message expiration
    pub ttl: Option<Duration>,
}

/// Governance message payload variants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GovernancePayload {
    /// Heartbeat request/response
    Heartbeat(HeartbeatData),
    /// Signature request to governance
    SignatureRequest(SignatureRequestData),
    /// Signature response from governance
    SignatureResponse(SignatureResponseData),
    /// Peg-in notification
    PeginNotification(PeginNotificationData),
    /// Federation update
    FederationUpdate(FederationUpdateData),
    /// Proposal notification
    ProposalNotification(ProposalNotificationData),
    /// Node registration
    NodeRegistration(NodeRegistrationData),
    /// Status update
    StatusUpdate(StatusUpdateData),
    /// Error notification
    Error(ErrorData),
    /// Authentication challenge/response
    Authentication(AuthenticationData),
    /// Emergency action
    EmergencyAction(EmergencyActionData),
}

/// Heartbeat message data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatData {
    /// Timestamp when heartbeat was generated
    pub timestamp: i64,
    /// Node identifier
    pub node_id: String,
    /// Optional node status information
    pub status: Option<NodeStatus>,
    /// Round-trip measurement for latency
    pub ping_id: Option<String>,
}

/// Signature request data sent to governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureRequestData {
    /// Unique request identifier
    pub request_id: String,
    /// Target blockchain (always "alys" for our case)
    pub chain: String,
    /// Transaction hex data to sign
    pub tx_hex: String,
    /// Input indices requiring signatures
    pub input_indices: Vec<u32>,
    /// Input amounts for verification
    pub amounts: Vec<u64>,
    /// Transaction type
    pub tx_type: i32, // Maps to governance::TxType enum
    /// Request priority
    pub priority: i32,
    /// Request timeout in seconds
    pub timeout_secs: Option<u64>,
}

/// Signature response data from governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureResponseData {
    /// Request ID this response corresponds to
    pub request_id: String,
    /// Collected witness data
    pub witnesses: Vec<WitnessData>,
    /// Signature collection status
    pub status: SignatureStatusData,
    /// Error message if collection failed
    pub error_message: Option<String>,
    /// Governance decision metadata
    pub metadata: HashMap<String, String>,
}

/// Peg-in notification data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeginNotificationData {
    /// Bitcoin transaction ID
    pub bitcoin_txid: String,
    /// Amount in satoshis
    pub amount_satoshis: u64,
    /// Recipient EVM address
    pub evm_address: String,
    /// Current Bitcoin confirmations
    pub confirmations: u32,
    /// Block hash containing transaction
    pub block_hash: Option<String>,
    /// Block height
    pub block_height: Option<u64>,
}

/// Federation update data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationUpdateData {
    /// Update type
    pub update_type: String,
    /// Federation version/epoch
    pub version: u32,
    /// Updated member list
    pub members: Vec<FederationMemberData>,
    /// New signature threshold
    pub threshold: u32,
    /// Updated multisig address
    pub multisig_address: String,
    /// Activation block height
    pub activation_height: Option<u64>,
}

/// Federation member data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMemberData {
    /// Member's Alys address
    pub alys_address: String,
    /// Member's Bitcoin public key
    pub bitcoin_pubkey: String,
    /// Member's signing weight
    pub weight: u32,
    /// Whether member is currently active
    pub active: bool,
}

/// Node registration data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRegistrationData {
    /// Node identifier
    pub node_id: String,
    /// Node public key for authentication
    pub public_key: String,
    /// Node capabilities
    pub capabilities: Vec<String>,
    /// Node endpoint for callbacks
    pub endpoint: Option<String>,
    /// Node version information
    pub version: String,
}

/// Authentication data for secure communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationData {
    /// Authentication type
    pub auth_type: AuthenticationType,
    /// Authentication challenge or response
    pub challenge: Option<String>,
    /// Token or signature
    pub credential: String,
    /// Expiration timestamp
    pub expires_at: Option<i64>,
}

// ============================================================================
// Supporting Types
// ============================================================================

/// Connection status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatus {
    /// Whether connection is active
    pub connected: bool,
    /// Governance endpoint
    pub endpoint: String,
    /// Last heartbeat timestamp
    pub last_heartbeat: Option<Instant>,
    /// Messages sent count
    pub messages_sent: u64,
    /// Messages received count
    pub messages_received: u64,
    /// Connection uptime
    pub connection_uptime: Duration,
    /// Reconnection attempt count
    pub reconnect_count: u32,
    /// Current connection state
    pub state: ConnectionState,
    /// Authentication status
    pub authenticated: bool,
    /// Last error if any
    pub last_error: Option<String>,
}

/// Connection priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConnectionPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Transaction types for signature requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionType {
    /// Peg-out transaction
    Pegout,
    /// Federation configuration change
    FederationChange,
    /// Emergency action transaction
    Emergency,
    /// Regular consensus transaction
    Consensus,
}

/// Request priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RequestPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Urgent = 3,
}

/// Message priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Node capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    /// Supported signature types
    pub signature_types: Vec<String>,
    /// Supported protocols
    pub protocols: Vec<String>,
    /// Maximum concurrent operations
    pub max_concurrent_ops: u32,
    /// Node role in federation
    pub role: NodeRole,
}

/// Node roles in the federation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeRole {
    /// Full federation member
    Member,
    /// Observer node
    Observer,
    /// Gateway node
    Gateway,
    /// Sentry node for security
    Sentry,
}

/// Witness data for Bitcoin transactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitnessData {
    /// Input index this witness applies to
    pub input_index: usize,
    /// Witness stack data
    pub witness: Vec<u8>,
    /// Signature type used
    pub signature_type: Option<String>,
}

/// Signature collection status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignatureStatus {
    /// Request is pending
    Pending,
    /// Collection in progress
    InProgress { 
        collected: usize, 
        required: usize,
        estimated_completion: Option<SystemTime>,
    },
    /// Collection completed successfully
    Complete,
    /// Collection failed
    Failed { reason: String },
    /// Collection timed out
    Timeout,
    /// Request was rejected by governance
    Rejected { reason: String },
}

/// Signature status in protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureStatusData {
    /// Status type
    pub status: String,
    /// Collected signature count
    pub collected: u32,
    /// Required signature count
    pub required: u32,
    /// Completion percentage
    pub completion_percentage: f64,
    /// Estimated completion time
    pub estimated_completion: Option<i64>,
}

/// Connection state enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConnectionState {
    /// Not connected
    Disconnected,
    /// Attempting to connect
    Connecting { attempt: u32, next_retry: Instant },
    /// Connected and authenticated
    Connected { since: Instant },
    /// Reconnecting after disconnection
    Reconnecting { reason: String, attempt: u32 },
    /// Connection failed permanently
    Failed { reason: String, permanent: bool },
    /// Connection suspended by governance
    Suspended { reason: String },
}

/// Connection event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionEventType {
    /// Connection established
    Connected,
    /// Connection lost
    Disconnected,
    /// Authentication completed
    Authenticated,
    /// Heartbeat received
    HeartbeatReceived,
    /// Error occurred
    Error,
    /// Connection suspended
    Suspended,
    /// Connection resumed
    Resumed,
}

/// Emergency action types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EmergencyActionType {
    /// Pause all operations
    PauseOperations,
    /// Resume operations
    ResumeOperations,
    /// Force federation update
    ForceFederationUpdate,
    /// Emergency signature override
    EmergencySignature,
    /// Initiate emergency recovery
    InitiateRecovery,
}

/// Emergency authorization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyAuthorization {
    /// Authorization type
    pub auth_type: String,
    /// Digital signature or token
    pub signature: String,
    /// Authorizing entity
    pub authority: String,
    /// Expiration time
    pub expires_at: SystemTime,
}

/// Proposal types for governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProposalType {
    /// Federation membership change
    FederationChange,
    /// Protocol parameter update
    ParameterUpdate,
    /// Emergency action proposal
    EmergencyAction,
    /// Software upgrade proposal
    SoftwareUpgrade,
    /// Bridge configuration change
    BridgeConfig,
}

/// Vote tally for proposals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteTally {
    /// Approve votes
    pub approve: u32,
    /// Reject votes
    pub reject: u32,
    /// Abstain votes
    pub abstain: u32,
    /// Total voting weight
    pub total_weight: u32,
}

/// Node status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    /// Node health status
    pub health: String,
    /// Current blockchain height
    pub block_height: u64,
    /// Synchronization status
    pub sync_status: bool,
    /// Active connections count
    pub connections: u32,
    /// Memory usage
    pub memory_usage: u64,
    /// CPU usage percentage
    pub cpu_usage: f64,
}

/// Stream metrics and performance data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMetrics {
    /// Total connections established
    pub total_connections: u64,
    /// Currently active connections
    pub active_connections: u32,
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Messages dropped due to buffer overflow
    pub messages_dropped: u64,
    /// Total bytes transferred
    pub bytes_transferred: u64,
    /// Average message latency
    pub avg_latency_ms: f64,
    /// Reconnection attempts
    pub reconnection_attempts: u64,
    /// Error count by type
    pub error_counts: HashMap<String, u64>,
    /// Stream uptime
    pub uptime: Duration,
    /// Performance metrics
    pub performance: StreamPerformanceMetrics,
}

/// Stream performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamPerformanceMetrics {
    /// Messages per second throughput
    pub messages_per_second: f64,
    /// Bytes per second throughput
    pub bytes_per_second: f64,
    /// Connection success rate
    pub connection_success_rate: f64,
    /// Average reconnection time
    pub avg_reconnection_time_ms: f64,
    /// Buffer utilization percentage
    pub buffer_utilization: f64,
}

/// Authentication types supported
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthenticationType {
    /// Bearer token authentication
    Bearer,
    /// Mutual TLS authentication
    MutualTls,
    /// Digital signature authentication
    Signature,
    /// API key authentication
    ApiKey,
}

/// Status update data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusUpdateData {
    /// Update type
    pub update_type: String,
    /// Node status
    pub node_status: NodeStatus,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Error data for protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorData {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Error details
    pub details: Option<String>,
    /// Whether error is recoverable
    pub recoverable: bool,
}

/// Emergency action data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyActionData {
    /// Action type
    pub action_type: String,
    /// Action parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Authorization information
    pub authorization: EmergencyAuthorization,
    /// Execution timestamp
    pub execute_at: Option<SystemTime>,
}

/// Stream configuration updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Message buffer size per connection
    pub buffer_size: usize,
    /// Heartbeat interval
    pub heartbeat_interval: Duration,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Governance endpoints
    pub governance_endpoints: Vec<String>,
    /// Request timeout
    pub request_timeout: Duration,
    /// Maximum pending requests
    pub max_pending_requests: usize,
    /// Message TTL
    pub message_ttl: Duration,
    /// Reconnection configuration
    pub reconnect_config: ReconnectConfig,
    /// Authentication configuration
    pub auth_config: Option<AuthConfig>,
}

/// Reconnection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectConfig {
    /// Initial delay between reconnection attempts
    pub initial_delay: Duration,
    /// Maximum delay between attempts
    pub max_delay: Duration,
    /// Backoff multiplier
    pub multiplier: f64,
    /// Maximum number of attempts before giving up
    pub max_attempts: Option<u32>,
    /// Whether to add jitter to avoid thundering herd
    pub use_jitter: bool,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Authentication type
    pub auth_type: AuthenticationType,
    /// Token or credential
    pub credential: String,
    /// Token refresh interval
    pub refresh_interval: Option<Duration>,
    /// Additional auth parameters
    pub parameters: HashMap<String, String>,
}

// ============================================================================
// Default Implementations
// ============================================================================

impl Default for ConnectionPriority {
    fn default() -> Self {
        ConnectionPriority::Normal
    }
}

impl Default for RequestPriority {
    fn default() -> Self {
        RequestPriority::Normal
    }
}

impl Default for MessagePriority {
    fn default() -> Self {
        MessagePriority::Normal
    }
}

impl Default for StreamMetrics {
    fn default() -> Self {
        Self {
            total_connections: 0,
            active_connections: 0,
            messages_sent: 0,
            messages_received: 0,
            messages_dropped: 0,
            bytes_transferred: 0,
            avg_latency_ms: 0.0,
            reconnection_attempts: 0,
            error_counts: HashMap::new(),
            uptime: Duration::from_secs(0),
            performance: StreamPerformanceMetrics::default(),
        }
    }
}

impl Default for StreamPerformanceMetrics {
    fn default() -> Self {
        Self {
            messages_per_second: 0.0,
            bytes_per_second: 0.0,
            connection_success_rate: 1.0,
            avg_reconnection_time_ms: 0.0,
            buffer_utilization: 0.0,
        }
    }
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        use crate::actors::governance_stream::*;
        Self {
            initial_delay: Duration::from_millis(DEFAULT_RECONNECT_INITIAL_DELAY_MS),
            max_delay: Duration::from_secs(DEFAULT_RECONNECT_MAX_DELAY_SECS),
            multiplier: DEFAULT_RECONNECT_MULTIPLIER,
            max_attempts: Some(100),
            use_jitter: true,
        }
    }
}

impl TransactionType {
    /// Convert to governance protocol integer representation
    pub fn to_protocol_value(&self) -> i32 {
        match self {
            TransactionType::Pegout => 0,
            TransactionType::FederationChange => 1,
            TransactionType::Emergency => 2,
            TransactionType::Consensus => 3,
        }
    }

    /// Create from governance protocol integer representation
    pub fn from_protocol_value(value: i32) -> Option<Self> {
        match value {
            0 => Some(TransactionType::Pegout),
            1 => Some(TransactionType::FederationChange),
            2 => Some(TransactionType::Emergency),
            3 => Some(TransactionType::Consensus),
            _ => None,
        }
    }
}

impl SignatureStatus {
    /// Check if signature collection is in a final state
    pub fn is_final(&self) -> bool {
        matches!(
            self,
            SignatureStatus::Complete
                | SignatureStatus::Failed { .. }
                | SignatureStatus::Timeout
                | SignatureStatus::Rejected { .. }
        )
    }

    /// Get completion percentage if available
    pub fn completion_percentage(&self) -> Option<f64> {
        match self {
            SignatureStatus::InProgress { collected, required, .. } => {
                if *required > 0 {
                    Some((*collected as f64 / *required as f64) * 100.0)
                } else {
                    None
                }
            }
            SignatureStatus::Complete => Some(100.0),
            _ => None,
        }
    }
}

impl ConnectionState {
    /// Check if connection is in an active state
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

    /// Check if connection has failed
    pub fn is_failed(&self) -> bool {
        matches!(self, ConnectionState::Failed { .. })
    }
}