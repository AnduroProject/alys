//! Tendermint RPC response types
//!
//! These types are used for the new `tendermint_*` RPC endpoints that expose
//! consensus state, validator information, and governance parameters.
//!
//! Note: Timestamps are formatted as RFC3339 strings for RPC compatibility.

use serde::{Deserialize, Serialize};

/// Response for `tendermint_consensusState`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusStateResponse {
    /// Current consensus height
    pub height: u64,
    /// Current consensus round
    pub round: u32,
    /// Current step: "Propose" | "Prevote" | "Precommit" | "Commit"
    pub step: String,
    /// When the current height started (RFC3339 formatted)
    pub start_time: String,
    /// Hash of the current proposal block (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal_block_hash: Option<String>,
    /// Hash of the locked block (if locked)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_block_hash: Option<String>,
    /// Round at which block was locked
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_round: Option<u32>,
    /// Hash of the valid block (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_block_hash: Option<String>,
    /// Round at which valid block was seen
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_round: Option<u32>,
    /// Current vote information
    pub votes: VotesInfo,
}

/// Vote tracking information for current round
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VotesInfo {
    /// List of prevotes received
    pub prevotes: Vec<VoteInfo>,
    /// List of precommits received
    pub precommits: Vec<VoteInfo>,
    /// Bit array representation of prevotes (e.g., "BA{4:xx__}")
    pub prevotes_bit_array: String,
    /// Bit array representation of precommits
    pub precommits_bit_array: String,
}

/// Individual vote information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteInfo {
    /// Index of the validator in the set
    pub validator_index: u32,
    /// Hex-encoded validator address
    pub validator_address: String,
    /// Block hash voted for (None = NIL vote)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_hash: Option<String>,
    /// When the vote was cast (RFC3339 formatted)
    pub timestamp: String,
    /// Base64-encoded signature
    pub signature: String,
}

/// Request for `tendermint_validators`
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidatorsRequest {
    /// Height to query (None = current)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u64>,
    /// Page number (1-indexed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Results per page
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
}

/// Response for `tendermint_validators`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorsResponse {
    /// Height at which this validator set is active
    pub block_height: u64,
    /// List of validators (paginated)
    pub validators: Vec<ValidatorInfo>,
    /// Number of validators in this page
    pub count: u32,
    /// Total number of validators
    pub total: u32,
}

/// Individual validator information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    /// Hex-encoded validator address
    pub address: String,
    /// Public key information
    pub pub_key: PubKeyInfo,
    /// Voting power
    pub voting_power: u64,
    /// Proposer priority (for round-robin selection)
    pub proposer_priority: i64,
}

/// Public key information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubKeyInfo {
    /// Key type: "ed25519" | "bls12-381"
    #[serde(rename = "type")]
    pub type_: String,
    /// Base64-encoded public key
    pub value: String,
}

/// Request for `tendermint_commit`
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommitRequest {
    /// Height to query (None = latest)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u64>,
}

/// Response for `tendermint_commit`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitResponse {
    /// Signed header with commit proof
    pub signed_header: SignedHeader,
    /// Whether this is the canonical commit
    pub canonical: bool,
}

/// Signed block header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedHeader {
    /// Block header information
    pub header: BlockHeaderInfo,
    /// Commit proof (signatures)
    pub commit: CommitInfo,
}

/// Block header information for RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeaderInfo {
    /// Block height
    pub height: u64,
    /// Block hash
    pub hash: String,
    /// Parent block hash
    pub parent_hash: String,
    /// Block timestamp (RFC3339 formatted)
    pub timestamp: String,
    /// Proposer address
    pub proposer_address: String,
    /// Hash of the last commit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_commit_hash: Option<String>,
    /// Hash of the validator set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validators_hash: Option<String>,
}

/// Commit proof information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    /// Height of the committed block
    pub height: u64,
    /// Round at which consensus was reached
    pub round: u32,
    /// Block ID that was committed
    pub block_id: BlockIdInfo,
    /// Validator signatures
    pub signatures: Vec<CommitSigInfo>,
}

/// Block ID information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockIdInfo {
    /// Block hash
    pub hash: String,
    /// Part set header (for compatibility)
    pub parts: PartsInfo,
}

/// Part set information (minimal for compatibility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartsInfo {
    /// Total parts
    pub total: u32,
    /// Hash of parts
    pub hash: String,
}

/// Individual commit signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitSigInfo {
    /// Vote flag: "Commit" | "Nil" | "Absent"
    pub block_id_flag: String,
    /// Validator address (if present)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validator_address: Option<String>,
    /// Timestamp (if present, RFC3339 formatted)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Base64-encoded signature (if present)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Response for `tendermint_params`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamsResponse {
    /// Height at which these params are active
    pub block_height: u64,
    /// Consensus parameters
    pub consensus_params: ConsensusParams,
    /// Governance parameters
    pub governance_params: GovernanceParams,
}

/// Consensus parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusParams {
    /// Block size parameters
    pub block: BlockParams,
    /// Evidence parameters
    pub evidence: EvidenceParams,
    /// Validator parameters
    pub validator: ValidatorParams,
}

/// Block size parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockParams {
    /// Maximum block size in bytes
    pub max_bytes: u64,
    /// Maximum gas per block
    pub max_gas: i64,
}

/// Evidence parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceParams {
    /// Maximum age in blocks
    pub max_age_num_blocks: u64,
    /// Maximum age in milliseconds
    pub max_age_duration_ms: u64,
    /// Maximum evidence bytes
    pub max_bytes: u64,
}

/// Validator parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorParams {
    /// Supported public key types
    pub pub_key_types: Vec<String>,
}

/// Governance parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceParams {
    /// Minimum peg-in amount in satoshis
    pub pegin_minimum_satoshis: u64,
    /// Required Bitcoin confirmations for peg-in
    pub pegin_confirmation_depth: u32,
    /// Bridge fee rate in basis points
    pub bridge_fee_rate_bps: u32,
    /// Whether emergency pause is enabled
    pub emergency_pause_enabled: bool,
}

/// Response for `tendermint_pendingGovernanceUpdates`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingUpdatesResponse {
    /// List of pending updates
    pub updates: Vec<PendingUpdate>,
}

/// Pending governance update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingUpdate {
    /// Update type: "Validator" | "Parameter" | "Emergency"
    pub update_type: String,
    /// Height at which update activates
    pub activation_height: u64,
    /// Update details (varies by type)
    pub details: serde_json::Value,
    /// Height at which update was proposed
    pub proposed_at_height: u64,
    /// Address of the proposer
    pub proposed_by: String,
}

/// Tendermint-specific error codes
pub mod error_codes {
    /// Consensus not initialized or still syncing
    pub const CONSENSUS_NOT_READY: i32 = -32050;
    /// Requested height doesn't exist
    pub const HEIGHT_NOT_FOUND: i32 = -32051;
    /// Commit not yet produced for height
    pub const COMMIT_NOT_AVAILABLE: i32 = -32052;
    /// Validator address not in current set
    pub const VALIDATOR_NOT_FOUND: i32 = -32053;
    /// Query during governance transition
    pub const GOVERNANCE_UPDATE_PENDING: i32 = -32054;
}
