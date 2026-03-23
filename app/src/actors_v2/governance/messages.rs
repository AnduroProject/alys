//! Messages for GovernanceClientActor.

use actix::prelude::*;
use bitcoin::{BlockHash, Txid};
use ethereum_types::Address;
use uuid::Uuid;

use crate::actors_v2::chain::tendermint::GovernanceUpdate;
use crate::actors_v2::chain::ChainActor;

/// Messages sent to GovernanceClientActor.
#[derive(Debug, Message)]
#[rtype(result = "Result<GovernanceResponse, GovernanceError>")]
pub enum GovernanceMessage {
    /// Connect to governance service
    Connect,

    /// Verify a peg-in transaction (blocking - waits for response)
    VerifyPegin(VerifyPegin),

    /// Set ChainActor address for forwarding governance updates
    SetChainActor { addr: Addr<ChainActor> },

    /// Internal: Send heartbeat
    SendHeartbeat,

    /// Internal: Attempt reconnection after disconnect
    Reconnect,
}

/// Request to verify a peg-in transaction.
#[derive(Debug, Clone)]
pub struct VerifyPegin {
    /// Bitcoin transaction ID
    pub txid: Txid,
    /// Bitcoin block hash containing the transaction
    pub block_hash: BlockHash,
    /// EVM account to receive the pegged-in funds
    pub evm_account: Address,
    /// Amount in satoshis
    pub amount: u64,
    /// Required number of Bitcoin confirmations
    pub required_confirmations: u32,
    /// Correlation ID for request/response matching
    pub correlation_id: Uuid,
}

/// Result of peg-in verification.
#[derive(Debug, Clone)]
pub struct PeginVerificationResult {
    /// Whether the peg-in was verified
    pub verified: bool,
    /// Reason for rejection (if not verified)
    pub reason: String,
    /// Number of confirmations observed
    pub confirmations: u32,
    /// Correlation ID for request matching
    pub correlation_id: Uuid,
}

/// Response from GovernanceClientActor.
#[derive(Debug, Clone)]
pub enum GovernanceResponse {
    /// Successfully connected to governance
    Connected,

    /// Peg-in verification result
    PeginVerified(PeginVerificationResult),

    /// ChainActor set successfully
    ChainActorSet,

    /// Heartbeat sent
    HeartbeatSent,

    /// Reconnection scheduled
    ReconnectScheduled,
}

/// Governance update received from the governance service.
/// Forwarded to ChainActor for processing.
#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub struct GovernanceUpdateReceived {
    /// The governance update
    pub update: GovernanceUpdate,
    /// Correlation ID for tracing
    pub correlation_id: Uuid,
}

/// Errors from governance operations.
#[derive(Debug, Clone)]
pub enum GovernanceError {
    /// Not connected to governance service
    NotConnected,
    /// Connection failed
    ConnectionFailed(String),
    /// Request timed out
    Timeout,
    /// Request failed
    RequestFailed(String),
    /// Invalid response from governance
    InvalidResponse(String),
    /// ChainActor not set
    ChainActorNotSet,
}

impl std::fmt::Display for GovernanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GovernanceError::NotConnected => write!(f, "Not connected to governance service"),
            GovernanceError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            GovernanceError::Timeout => write!(f, "Request timed out"),
            GovernanceError::RequestFailed(msg) => write!(f, "Request failed: {}", msg),
            GovernanceError::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
            GovernanceError::ChainActorNotSet => write!(f, "ChainActor not set"),
        }
    }
}

impl std::error::Error for GovernanceError {}
