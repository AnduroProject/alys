//! Blockchain-related types and structures

use crate::types::*;
use serde::{Deserialize, Serialize};

/// A complete block in the Alys blockchain with Lighthouse V5 compatibility
/// Enhanced with actor-friendly design and comprehensive metadata tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusBlock {
    /// The block hash of the parent
    pub parent_hash: Hash256,
    /// Aura slot the block was produced in
    pub slot: u64,
    /// Proof of work header, used for finalization. Not every block is expected to have this.
    pub auxpow_header: Option<AuxPowHeader>,
    /// Execution layer payload (from Geth/Reth)
    pub execution_payload: ExecutionPayload,
    /// Transactions that are sending funds to the bridge (Bitcoin txid, block hash)
    pub pegins: Vec<(bitcoin::Txid, bitcoin::BlockHash)>,
    /// Bitcoin payments for pegouts
    pub pegout_payment_proposal: Option<bitcoin::Transaction>,
    /// Finalized bitcoin payments. Only non-empty if there is an auxpow.
    pub finalized_pegouts: Vec<bitcoin::Transaction>,
    /// Lighthouse V5 compatibility fields
    pub lighthouse_metadata: LighthouseMetadata,
    /// Block production timing information
    pub timing: BlockTiming,
    /// Validation status and checkpoints
    pub validation_info: ValidationInfo,
    /// Actor system metadata for tracing and monitoring
    pub actor_metadata: ActorBlockMetadata,
}

/// Auxiliary Proof of Work header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxPowHeader {
    /// The oldest block covered by this AuxPoW
    pub range_start: Hash256,
    /// The newest block covered by this AuxPoW (inclusive)
    pub range_end: Hash256,
    /// The difficulty target in compact form
    pub bits: u32,
    /// The ID of the chain used to isolate the AuxPow merkle branch
    pub chain_id: u32,
    /// The height of the AuxPow, used for difficulty adjustment
    pub height: u64,
    /// The AuxPow itself, only None at genesis
    pub auxpow: Option<AuxPow>,
    /// The miner's EVM address
    pub fee_recipient: Address,
}

/// Auxiliary Proof of Work structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxPow {
    /// The Bitcoin coinbase transaction
    pub coinbase_tx: bitcoin::Transaction,
    /// The merkle branch linking the coinbase tx to the block
    pub merkle_branch: Vec<Hash256>,
    /// The index of the coinbase tx in the merkle tree
    pub merkle_index: u32,
    /// The parent Bitcoin block header
    pub parent_block_header: bitcoin::block::Header,
}

/// Signed consensus block with aggregate approval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedConsensusBlock {
    pub message: ConsensusBlock,
    /// Signed by the authority for that slot, plus the approvals of other authorities
    pub signature: AggregateApproval,
}

/// Aggregate approval signatures from authorities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateApproval {
    /// Bitfield indicating which authorities signed
    pub signers: Vec<bool>,
    /// Aggregated BLS signature
    pub signature: Signature,
}

/// Individual approval from an authority
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndividualApproval {
    pub signature: Signature,
    pub authority_index: u8,
}

/// Block header containing metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub parent_hash: BlockHash,
    pub transactions_root: Hash256,
    pub state_root: Hash256,
    pub receipts_root: Hash256,
    pub logs_bloom: Vec<u8>,
    pub number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub extra_data: Vec<u8>,
    pub base_fee_per_gas: U256,
}

/// Reference to a block (lightweight identifier)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BlockRef {
    pub hash: BlockHash,
    pub number: u64,
    pub parent_hash: BlockHash,
}

/// Transaction structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: H256,
    pub from: Address,
    pub to: Option<Address>,
    pub value: U256,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub data: Vec<u8>,
    pub nonce: u64,
    pub signature: TransactionSignature,
}

/// Transaction signature components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionSignature {
    pub r: U256,
    pub s: U256,
    pub v: u64,
}

/// Consensus signature for blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusSignature {
    pub signature: Signature,
    pub signer: Address,
    pub signature_type: SignatureType,
}

/// Types of signatures used in consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignatureType {
    ECDSA,
    BLS,
    Schnorr,
}

/// Execution payload for EVM compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPayload {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub fee_recipient: Address,
    pub state_root: Hash256,
    pub receipts_root: Hash256,
    pub logs_bloom: Vec<u8>,
    pub prev_randao: Hash256,
    pub block_number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub extra_data: Vec<u8>,
    pub base_fee_per_gas: U256,
    pub transactions: Vec<Vec<u8>>, // Serialized transactions
    pub withdrawals: Option<Vec<Withdrawal>>,
}

/// Withdrawal structure (future use)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Withdrawal {
    pub index: u64,
    pub validator_index: u64,
    pub address: Address,
    pub amount: u64,
}

/// Transaction receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionReceipt {
    pub transaction_hash: H256,
    pub transaction_index: u32,
    pub block_hash: BlockHash,
    pub block_number: u64,
    pub cumulative_gas_used: u64,
    pub gas_used: u64,
    pub contract_address: Option<Address>,
    pub logs: Vec<EventLog>,
    pub logs_bloom: Vec<u8>,
    pub status: TransactionStatus,
}

/// Transaction execution status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionStatus {
    Success,
    Failed { reason: Option<String> },
    Reverted { reason: Option<String> },
}

/// Event log from transaction execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLog {
    pub address: Address,
    pub topics: Vec<H256>,
    pub data: Vec<u8>,
    pub block_hash: BlockHash,
    pub block_number: u64,
    pub transaction_hash: H256,
    pub transaction_index: u32,
    pub log_index: u32,
    pub removed: bool,
}

/// Chain state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainState {
    pub head: BlockRef,
    pub finalized_head: Option<BlockRef>,
    pub genesis_hash: BlockHash,
    pub chain_id: u64,
    pub total_difficulty: U256,
}

/// Pending transaction pool entry
#[derive(Debug, Clone)]
pub struct PendingTransaction {
    pub transaction: Transaction,
    pub added_at: std::time::Instant,
    pub priority: TransactionPriority,
    pub gas_price_priority: U256,
}

/// Transaction priority levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TransactionPriority {
    Low,
    Normal,
    High,
    Critical,
}

/// Account state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountState {
    pub address: Address,
    pub nonce: u64,
    pub balance: U256,
    pub code_hash: Hash256,
    pub storage_root: Hash256,
}

/// Lighthouse V5 compatibility metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LighthouseMetadata {
    /// Beacon block root (for Ethereum compatibility)
    pub beacon_block_root: Option<Hash256>,
    /// State root from beacon chain
    pub beacon_state_root: Option<Hash256>,
    /// Randao reveal for randomness
    pub randao_reveal: Option<Hash256>,
    /// Graffiti from the proposer
    pub graffiti: Option<[u8; 32]>,
    /// Proposer index in the validator set
    pub proposer_index: Option<u64>,
    /// BLS aggregate signature for consensus
    pub bls_aggregate_signature: Option<BLSSignature>,
    /// Sync committee aggregate signature
    pub sync_committee_signature: Option<BLSSignature>,
    /// Sync committee participation bits
    pub sync_committee_bits: Option<Vec<bool>>,
}

/// Block timing information for performance monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTiming {
    /// When block production started
    pub production_started_at: std::time::SystemTime,
    /// When block was finalized by producer
    pub produced_at: std::time::SystemTime,
    /// When block was received by this node
    pub received_at: Option<std::time::SystemTime>,
    /// When block validation started
    pub validation_started_at: Option<std::time::SystemTime>,
    /// When block validation completed
    pub validation_completed_at: Option<std::time::SystemTime>,
    /// When block was added to chain
    pub import_completed_at: Option<std::time::SystemTime>,
    /// Processing time in milliseconds
    pub processing_duration_ms: Option<u64>,
}

/// Block validation information and checkpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationInfo {
    /// Validation status
    pub status: BlockValidationStatus,
    /// Validation errors encountered
    pub validation_errors: Vec<String>,
    /// Checkpoints passed during validation
    pub checkpoints: Vec<ValidationCheckpoint>,
    /// Gas usage validation
    pub gas_validation: GasValidation,
    /// State transition validation
    pub state_validation: StateValidation,
    /// Consensus rules validation
    pub consensus_validation: ConsensusValidation,
}

/// Block validation status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockValidationStatus {
    /// Block is pending validation
    Pending,
    /// Block is currently being validated
    Validating,
    /// Block passed all validations
    Valid,
    /// Block failed validation
    Invalid,
    /// Block validation was skipped (trusted source)
    Skipped,
    /// Block validation timed out
    TimedOut,
}

/// Validation checkpoint tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCheckpoint {
    /// Checkpoint name/type
    pub checkpoint: String,
    /// When checkpoint was reached
    pub timestamp: std::time::SystemTime,
    /// Whether checkpoint passed
    pub passed: bool,
    /// Duration to reach this checkpoint
    pub duration_ms: u64,
    /// Additional context
    pub context: std::collections::HashMap<String, String>,
}

/// Gas usage validation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasValidation {
    /// Expected gas limit
    pub expected_gas_limit: u64,
    /// Actual gas used
    pub actual_gas_used: u64,
    /// Gas utilization percentage
    pub utilization_percent: f64,
    /// Whether gas usage is valid
    pub is_valid: bool,
    /// Gas price validation
    pub base_fee_valid: bool,
    /// Priority fee validation
    pub priority_fee_valid: bool,
}

/// State transition validation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateValidation {
    /// Pre-state root
    pub pre_state_root: Hash256,
    /// Post-state root
    pub post_state_root: Hash256,
    /// Expected post-state root
    pub expected_state_root: Hash256,
    /// State root matches expected
    pub state_root_valid: bool,
    /// Storage proofs valid
    pub storage_proofs_valid: bool,
    /// Account state changes
    pub account_changes: u32,
    /// Storage slot changes
    pub storage_changes: u32,
}

/// Consensus validation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusValidation {
    /// Signature validation
    pub signature_valid: bool,
    /// Proposer validation
    pub proposer_valid: bool,
    /// Slot validation
    pub slot_valid: bool,
    /// Parent relationship valid
    pub parent_valid: bool,
    /// Difficulty/target valid (for PoW)
    pub difficulty_valid: bool,
    /// Auxiliary PoW valid
    pub auxpow_valid: Option<bool>,
    /// Committee signatures valid
    pub committee_signatures_valid: bool,
}

/// Actor system metadata for block processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorBlockMetadata {
    /// Processing actor ID
    pub processing_actor: Option<String>,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<uuid::Uuid>,
    /// Trace span information
    pub trace_context: TraceContext,
    /// Processing priority
    pub priority: BlockProcessingPriority,
    /// Retry information
    pub retry_info: RetryInfo,
    /// Actor performance metrics
    pub actor_metrics: ActorProcessingMetrics,
}

/// Distributed tracing context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    /// Trace ID for the entire block processing flow
    pub trace_id: Option<String>,
    /// Span ID for this specific operation
    pub span_id: Option<String>,
    /// Parent span ID
    pub parent_span_id: Option<String>,
    /// Baggage items for context propagation
    pub baggage: std::collections::HashMap<String, String>,
    /// Sampling decision
    pub sampled: bool,
}

/// Block processing priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BlockProcessingPriority {
    /// Low priority background processing
    Low = 0,
    /// Normal priority processing
    Normal = 1,
    /// High priority processing
    High = 2,
    /// Critical priority (chain tip, etc.)
    Critical = 3,
}

/// Retry information for failed operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryInfo {
    /// Current attempt number (0 = first attempt)
    pub attempt: u32,
    /// Maximum retry attempts allowed
    pub max_attempts: u32,
    /// Backoff strategy
    pub backoff_strategy: BackoffStrategy,
    /// Next retry time
    pub next_retry_at: Option<std::time::SystemTime>,
    /// Reason for last failure
    pub last_failure_reason: Option<String>,
}

/// Backoff strategy for retries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackoffStrategy {
    /// Fixed delay between retries
    Fixed { delay_ms: u64 },
    /// Exponential backoff
    Exponential { base_ms: u64, multiplier: f64, max_ms: u64 },
    /// Linear backoff
    Linear { initial_ms: u64, increment_ms: u64 },
}

/// Actor processing performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorProcessingMetrics {
    /// Queue time before processing started
    pub queue_time_ms: Option<u64>,
    /// Processing time in the actor
    pub processing_time_ms: Option<u64>,
    /// Memory usage during processing
    pub memory_usage_bytes: Option<u64>,
    /// CPU time used
    pub cpu_time_ms: Option<u64>,
    /// Number of messages sent during processing
    pub messages_sent: u32,
    /// Number of messages received during processing
    pub messages_received: u32,
}

/// BLS signature for Lighthouse compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BLSSignature {
    /// BLS signature bytes (96 bytes for BLS12-381)
    pub signature: [u8; 96],
    /// Aggregation info (which validators signed)
    pub aggregation_bits: Option<Vec<bool>>,
    /// Message that was signed
    pub message_hash: Option<Hash256>,
}

/// Storage slot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSlot {
    pub address: Address,
    pub slot: U256,
    pub value: U256,
}

/// Block validation context
#[derive(Debug, Clone)]
pub struct ValidationContext {
    pub parent_state_root: Hash256,
    pub current_timestamp: u64,
    pub gas_limit: u64,
    pub base_fee: U256,
}

impl ConsensusBlock {
    /// Create a new consensus block with enhanced metadata
    pub fn new(
        slot: u64,
        execution_payload: ExecutionPayload,
        parent_hash: Hash256,
        auxpow_header: Option<AuxPowHeader>,
        pegins: Vec<(bitcoin::Txid, bitcoin::BlockHash)>,
        pegout_payment_proposal: Option<bitcoin::Transaction>,
        finalized_pegouts: Vec<bitcoin::Transaction>,
    ) -> Self {
        let now = std::time::SystemTime::now();
        
        Self {
            slot,
            parent_hash,
            execution_payload,
            auxpow_header,
            pegins,
            pegout_payment_proposal,
            finalized_pegouts,
            lighthouse_metadata: LighthouseMetadata::default(),
            timing: BlockTiming {
                production_started_at: now,
                produced_at: now,
                received_at: None,
                validation_started_at: None,
                validation_completed_at: None,
                import_completed_at: None,
                processing_duration_ms: None,
            },
            validation_info: ValidationInfo::default(),
            actor_metadata: ActorBlockMetadata::default(),
        }
    }

    /// Create a new consensus block from legacy format (compatibility)
    pub fn from_legacy(
        slot: u64,
        execution_payload: ExecutionPayload,
        parent_hash: Hash256,
        auxpow_header: Option<AuxPowHeader>,
        pegins: Vec<(bitcoin::Txid, bitcoin::BlockHash)>,
        pegout_payment_proposal: Option<bitcoin::Transaction>,
        finalized_pegouts: Vec<bitcoin::Transaction>,
    ) -> Self {
        Self::new(
            slot,
            execution_payload,
            parent_hash,
            auxpow_header,
            pegins,
            pegout_payment_proposal,
            finalized_pegouts,
        )
    }

    /// Calculate the signing root of this block (used for signatures)
    pub fn signing_root(&self) -> Hash256 {
        use sha2::{Digest, Sha256};
        
        // Use the same serialization method as the actual implementation
        let serialized = bincode::serialize(self).unwrap_or_default();
        let hash = Sha256::digest(&serialized);
        Hash256::from_slice(&hash)
    }
    
    /// Calculate the hash of this block
    pub fn hash(&self) -> BlockHash {
        // In Alys, the block hash is the signing root
        self.signing_root()
    }
    
    /// Get the block number from execution payload
    pub fn number(&self) -> u64 {
        self.execution_payload.block_number
    }
    
    /// Get the parent hash
    pub fn parent_hash(&self) -> BlockHash {
        self.parent_hash
    }
    
    /// Get the timestamp from execution payload
    pub fn timestamp(&self) -> u64 {
        self.execution_payload.timestamp
    }
    
    /// Check if this block is the genesis block
    pub fn is_genesis(&self) -> bool {
        self.execution_payload.block_number == 0
    }
    
    /// Get total gas used from execution payload
    pub fn gas_used(&self) -> u64 {
        self.execution_payload.gas_used
    }
    
    /// Get gas limit from execution payload
    pub fn gas_limit(&self) -> u64 {
        self.execution_payload.gas_limit
    }
    
    /// Get gas utilization as a percentage
    pub fn gas_utilization(&self) -> f64 {
        if self.execution_payload.gas_limit == 0 {
            0.0
        } else {
            (self.execution_payload.gas_used as f64) / (self.execution_payload.gas_limit as f64) * 100.0
        }
    }

    /// Check if block has auxiliary proof of work
    pub fn has_auxpow(&self) -> bool {
        self.auxpow_header.is_some()
    }

    /// Get the difficulty bits (if auxpow is present)
    pub fn bits(&self) -> Option<u32> {
        self.auxpow_header.as_ref().map(|header| header.bits)
    }

    /// Get the chain ID (if auxpow is present)
    pub fn chain_id(&self) -> Option<u32> {
        self.auxpow_header.as_ref().map(|header| header.chain_id)
    }

    /// Get the auxpow height (if auxpow is present)
    pub fn auxpow_height(&self) -> Option<u64> {
        self.auxpow_header.as_ref().map(|header| header.height)
    }

    /// Check if block has peg-in transactions
    pub fn has_pegins(&self) -> bool {
        !self.pegins.is_empty()
    }

    /// Check if block has pegout proposals
    pub fn has_pegout_proposal(&self) -> bool {
        self.pegout_payment_proposal.is_some()
    }

    /// Check if block has finalized pegouts
    pub fn has_finalized_pegouts(&self) -> bool {
        !self.finalized_pegouts.is_empty()
    }

    /// Get total number of transactions (execution + peg operations)
    pub fn total_transaction_count(&self) -> usize {
        self.execution_payload.transactions.len() 
            + self.pegins.len() 
            + if self.pegout_payment_proposal.is_some() { 1 } else { 0 }
            + self.finalized_pegouts.len()
    }
}

impl BlockHeader {
    /// Create a new block header
    pub fn new(
        parent_hash: BlockHash,
        number: u64,
        timestamp: u64,
        gas_limit: u64,
    ) -> Self {
        Self {
            parent_hash,
            transactions_root: Hash256::zero(),
            state_root: Hash256::zero(),
            receipts_root: Hash256::zero(),
            logs_bloom: vec![0u8; 256],
            number,
            gas_limit,
            gas_used: 0,
            timestamp,
            extra_data: Vec::new(),
            base_fee_per_gas: U256::zero(),
        }
    }
}

impl Transaction {
    /// Create a new transaction
    pub fn new(
        from: Address,
        to: Option<Address>,
        value: U256,
        gas_limit: u64,
        gas_price: U256,
        data: Vec<u8>,
        nonce: u64,
    ) -> Self {
        let mut tx = Self {
            hash: H256::zero(),
            from,
            to,
            value,
            gas_limit,
            gas_price,
            data,
            nonce,
            signature: TransactionSignature {
                r: U256::zero(),
                s: U256::zero(),
                v: 0,
            },
        };
        
        tx.hash = tx.calculate_hash();
        tx
    }
    
    /// Calculate transaction hash
    pub fn calculate_hash(&self) -> H256 {
        use sha2::{Digest, Sha256};
        
        let serialized = bincode::serialize(self).unwrap_or_default();
        let hash = Sha256::digest(&serialized);
        H256::from_slice(&hash)
    }
    
    /// Check if transaction is contract creation
    pub fn is_contract_creation(&self) -> bool {
        self.to.is_none()
    }
    
    /// Get transaction fee
    pub fn fee(&self) -> U256 {
        U256::from(self.gas_limit) * self.gas_price
    }
    
    /// Get transaction size estimate
    pub fn size_estimate(&self) -> usize {
        // Rough estimate based on fields
        let base_size = 32 + 20 + 20 + 32 + 8 + 32 + 8 + 64; // Fixed fields
        let data_size = self.data.len();
        base_size + data_size
    }
}

impl BlockRef {
    /// Create a new block reference
    pub fn new(hash: BlockHash, number: u64, parent_hash: BlockHash) -> Self {
        Self {
            hash,
            number,
            parent_hash,
        }
    }
    
    /// Create genesis block reference
    pub fn genesis(genesis_hash: BlockHash) -> Self {
        Self {
            hash: genesis_hash,
            number: 0,
            parent_hash: BlockHash::zero(),
        }
    }
}

impl ExecutionPayload {
    /// Create new execution payload
    pub fn new(block_number: u64, parent_hash: BlockHash, timestamp: u64) -> Self {
        Self {
            block_hash: BlockHash::zero(), // Will be calculated
            parent_hash,
            fee_recipient: Address::zero(),
            state_root: Hash256::zero(),
            receipts_root: Hash256::zero(),
            logs_bloom: vec![0u8; 256],
            prev_randao: Hash256::zero(),
            block_number,
            gas_limit: 30_000_000, // Default gas limit
            gas_used: 0,
            timestamp,
            extra_data: Vec::new(),
            base_fee_per_gas: U256::from(1_000_000_000u64), // 1 Gwei
            transactions: Vec::new(),
            withdrawals: None,
        }
    }
}

impl SignedConsensusBlock {
    /// Create new signed consensus block
    pub fn new(message: ConsensusBlock, signature: AggregateApproval) -> Self {
        Self { message, signature }
    }

    /// Verify the aggregate signature against public keys
    pub fn verify_signature(&self, public_keys: &[PublicKey]) -> bool {
        let message = self.message.signing_root();
        self.signature.verify(public_keys, message)
    }

    /// Check if block is signed by a specific authority
    pub fn is_signed_by(&self, authority_index: u8) -> bool {
        self.signature.is_signed_by(authority_index)
    }

    /// Get number of approvals
    pub fn num_approvals(&self) -> usize {
        self.signature.num_approvals()
    }

    /// Get the canonical root (same as message signing root)
    pub fn canonical_root(&self) -> Hash256 {
        self.message.signing_root()
    }

    /// Add an individual approval to the aggregate
    pub fn add_approval(&mut self, approval: IndividualApproval) -> Result<(), String> {
        self.signature.add_approval(approval)
    }

    /// Get block reference for storage
    pub fn block_ref(&self) -> BlockRef {
        BlockRef {
            hash: self.canonical_root(),
            number: self.message.execution_payload.block_number,
            parent_hash: self.message.parent_hash,
        }
    }

    /// Create genesis signed block
    pub fn genesis(
        chain_id: u32,
        bits: u32,
        execution_payload: ExecutionPayload,
    ) -> Self {
        if execution_payload.block_number != 0 {
            panic!("Genesis execution payload should start at zero");
        }

        Self {
            message: ConsensusBlock {
                parent_hash: Hash256::zero(),
                slot: 0,
                auxpow_header: Some(AuxPowHeader {
                    range_start: Hash256::zero(),
                    range_end: Hash256::zero(),
                    bits,
                    chain_id,
                    height: 0,
                    auxpow: None,
                    fee_recipient: Address::zero(),
                }),
                execution_payload,
                pegins: vec![],
                pegout_payment_proposal: None,
                finalized_pegouts: vec![],
            },
            signature: AggregateApproval::new(),
        }
    }
}

impl AggregateApproval {
    /// Create new empty aggregate approval
    pub fn new() -> Self {
        Self {
            signers: Vec::new(),
            signature: [0u8; 64],
        }
    }

    /// Verify aggregate signature against public keys and message
    pub fn verify(&self, public_keys: &[PublicKey], message: Hash256) -> bool {
        // TODO: Implement BLS signature verification
        // This would use the BLS library to verify the aggregate signature
        // against the message hash and the public keys of the signers
        true // Placeholder
    }

    /// Check if authority signed
    pub fn is_signed_by(&self, authority_index: u8) -> bool {
        self.signers.get(authority_index as usize).copied().unwrap_or(false)
    }

    /// Get number of approvals
    pub fn num_approvals(&self) -> usize {
        self.signers.iter().filter(|&&signed| signed).count()
    }

    /// Add individual approval
    pub fn add_approval(&mut self, approval: IndividualApproval) -> Result<(), String> {
        let index = approval.authority_index as usize;
        
        // Ensure signers vec is large enough
        if self.signers.len() <= index {
            self.signers.resize(index + 1, false);
        }
        
        // Mark as signed
        self.signers[index] = true;
        
        // TODO: Aggregate the BLS signature
        // This would combine the individual signature with the existing aggregate
        
        Ok(())
    }
}

impl Default for TransactionSignature {
    fn default() -> Self {
        Self {
            r: U256::zero(),
            s: U256::zero(),
            v: 0,
        }
    }
}

impl PendingTransaction {
    /// Create new pending transaction
    pub fn new(transaction: Transaction, priority: TransactionPriority) -> Self {
        let gas_price_priority = transaction.gas_price;
        
        Self {
            transaction,
            added_at: std::time::Instant::now(),
            priority,
            gas_price_priority,
        }
    }
    
    /// Check if transaction has been pending too long
    pub fn is_stale(&self, max_age: std::time::Duration) -> bool {
        self.added_at.elapsed() > max_age
    }
    
    /// Get transaction age
    pub fn age(&self) -> std::time::Duration {
        self.added_at.elapsed()
    }
}

impl AccountState {
    /// Create new account state
    pub fn new(address: Address) -> Self {
        Self {
            address,
            nonce: 0,
            balance: U256::zero(),
            code_hash: Hash256::zero(),
            storage_root: Hash256::zero(),
        }
    }
    
    /// Check if account is empty
    pub fn is_empty(&self) -> bool {
        self.nonce == 0 && self.balance.is_zero() && self.code_hash.is_zero()
    }
    
    /// Check if account is a contract
    pub fn is_contract(&self) -> bool {
        !self.code_hash.is_zero()
    }
}

impl Default for LighthouseMetadata {
    fn default() -> Self {
        Self {
            beacon_block_root: None,
            beacon_state_root: None,
            randao_reveal: None,
            graffiti: None,
            proposer_index: None,
            bls_aggregate_signature: None,
            sync_committee_signature: None,
            sync_committee_bits: None,
        }
    }
}

impl Default for ValidationInfo {
    fn default() -> Self {
        Self {
            status: BlockValidationStatus::Pending,
            validation_errors: Vec::new(),
            checkpoints: Vec::new(),
            gas_validation: GasValidation::default(),
            state_validation: StateValidation::default(),
            consensus_validation: ConsensusValidation::default(),
        }
    }
}

impl Default for GasValidation {
    fn default() -> Self {
        Self {
            expected_gas_limit: 0,
            actual_gas_used: 0,
            utilization_percent: 0.0,
            is_valid: true,
            base_fee_valid: true,
            priority_fee_valid: true,
        }
    }
}

impl Default for StateValidation {
    fn default() -> Self {
        Self {
            pre_state_root: Hash256::zero(),
            post_state_root: Hash256::zero(),
            expected_state_root: Hash256::zero(),
            state_root_valid: true,
            storage_proofs_valid: true,
            account_changes: 0,
            storage_changes: 0,
        }
    }
}

impl Default for ConsensusValidation {
    fn default() -> Self {
        Self {
            signature_valid: true,
            proposer_valid: true,
            slot_valid: true,
            parent_valid: true,
            difficulty_valid: true,
            auxpow_valid: None,
            committee_signatures_valid: true,
        }
    }
}

impl Default for ActorBlockMetadata {
    fn default() -> Self {
        Self {
            processing_actor: None,
            correlation_id: None,
            trace_context: TraceContext::default(),
            priority: BlockProcessingPriority::Normal,
            retry_info: RetryInfo::default(),
            actor_metrics: ActorProcessingMetrics::default(),
        }
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self {
            trace_id: None,
            span_id: None,
            parent_span_id: None,
            baggage: std::collections::HashMap::new(),
            sampled: false,
        }
    }
}

impl Default for RetryInfo {
    fn default() -> Self {
        Self {
            attempt: 0,
            max_attempts: 3,
            backoff_strategy: BackoffStrategy::Exponential {
                base_ms: 1000,
                multiplier: 2.0,
                max_ms: 30000,
            },
            next_retry_at: None,
            last_failure_reason: None,
        }
    }
}

impl Default for ActorProcessingMetrics {
    fn default() -> Self {
        Self {
            queue_time_ms: None,
            processing_time_ms: None,
            memory_usage_bytes: None,
            cpu_time_ms: None,
            messages_sent: 0,
            messages_received: 0,
        }
    }
}

impl LighthouseMetadata {
    /// Set Lighthouse V5 beacon metadata
    pub fn set_beacon_metadata(
        &mut self,
        beacon_block_root: Hash256,
        beacon_state_root: Hash256,
        proposer_index: u64,
    ) {
        self.beacon_block_root = Some(beacon_block_root);
        self.beacon_state_root = Some(beacon_state_root);
        self.proposer_index = Some(proposer_index);
    }

    /// Set BLS signatures for consensus
    pub fn set_consensus_signatures(
        &mut self,
        aggregate_signature: BLSSignature,
        sync_committee_signature: Option<BLSSignature>,
    ) {
        self.bls_aggregate_signature = Some(aggregate_signature);
        self.sync_committee_signature = sync_committee_signature;
    }

    /// Check if block has Lighthouse V5 compatibility
    pub fn is_lighthouse_compatible(&self) -> bool {
        self.beacon_block_root.is_some() && self.beacon_state_root.is_some()
    }
}

impl BlockTiming {
    /// Record when block was received
    pub fn mark_received(&mut self) {
        self.received_at = Some(std::time::SystemTime::now());
    }

    /// Record when validation started
    pub fn mark_validation_started(&mut self) {
        self.validation_started_at = Some(std::time::SystemTime::now());
    }

    /// Record when validation completed
    pub fn mark_validation_completed(&mut self) {
        self.validation_completed_at = Some(std::time::SystemTime::now());
        self.calculate_processing_duration();
    }

    /// Record when import completed
    pub fn mark_import_completed(&mut self) {
        self.import_completed_at = Some(std::time::SystemTime::now());
        self.calculate_processing_duration();
    }

    /// Calculate total processing duration
    fn calculate_processing_duration(&mut self) {
        if let Some(started) = self.validation_started_at {
            if let Some(completed) = self.validation_completed_at.or(self.import_completed_at) {
                if let Ok(duration) = completed.duration_since(started) {
                    self.processing_duration_ms = Some(duration.as_millis() as u64);
                }
            }
        }
    }

    /// Get total processing time
    pub fn total_processing_time(&self) -> Option<std::time::Duration> {
        self.processing_duration_ms
            .map(|ms| std::time::Duration::from_millis(ms))
    }

    /// Get time from production to import
    pub fn end_to_end_time(&self) -> Option<std::time::Duration> {
        if let Some(import_time) = self.import_completed_at {
            if let Ok(duration) = import_time.duration_since(self.production_started_at) {
                return Some(duration);
            }
        }
        None
    }
}

impl ValidationInfo {
    /// Add validation checkpoint
    pub fn add_checkpoint(&mut self, checkpoint: String, passed: bool) {
        let now = std::time::SystemTime::now();
        let duration_ms = if let Some(last) = self.checkpoints.last() {
            now.duration_since(last.timestamp)
                .unwrap_or_default()
                .as_millis() as u64
        } else {
            0
        };

        self.checkpoints.push(ValidationCheckpoint {
            checkpoint,
            timestamp: now,
            passed,
            duration_ms,
            context: std::collections::HashMap::new(),
        });

        if !passed {
            self.status = BlockValidationStatus::Invalid;
        }
    }

    /// Add validation error
    pub fn add_error(&mut self, error: String) {
        self.validation_errors.push(error);
        self.status = BlockValidationStatus::Invalid;
    }

    /// Mark validation as complete
    pub fn mark_complete(&mut self, valid: bool) {
        self.status = if valid {
            BlockValidationStatus::Valid
        } else {
            BlockValidationStatus::Invalid
        };
    }

    /// Check if all validations passed
    pub fn all_validations_passed(&self) -> bool {
        self.status == BlockValidationStatus::Valid
            && self.validation_errors.is_empty()
            && self.checkpoints.iter().all(|c| c.passed)
    }
}

impl ActorBlockMetadata {
    /// Set processing actor
    pub fn set_processing_actor(&mut self, actor_id: String) {
        self.processing_actor = Some(actor_id);
    }

    /// Set correlation ID for distributed tracing
    pub fn set_correlation_id(&mut self, correlation_id: uuid::Uuid) {
        self.correlation_id = Some(correlation_id);
    }

    /// Set trace context
    pub fn set_trace_context(&mut self, trace_id: String, span_id: String) {
        self.trace_context.trace_id = Some(trace_id);
        self.trace_context.span_id = Some(span_id);
        self.trace_context.sampled = true;
    }

    /// Record retry attempt
    pub fn record_retry(&mut self, reason: String) {
        self.retry_info.attempt += 1;
        self.retry_info.last_failure_reason = Some(reason);
        
        // Calculate next retry time based on backoff strategy
        let delay_ms = match &self.retry_info.backoff_strategy {
            BackoffStrategy::Fixed { delay_ms } => *delay_ms,
            BackoffStrategy::Exponential { base_ms, multiplier, max_ms } => {
                let delay = (*base_ms as f64) * multiplier.powi(self.retry_info.attempt as i32);
                (delay as u64).min(*max_ms)
            }
            BackoffStrategy::Linear { initial_ms, increment_ms } => {
                initial_ms + (increment_ms * self.retry_info.attempt as u64)
            }
        };
        
        self.retry_info.next_retry_at = Some(
            std::time::SystemTime::now() + std::time::Duration::from_millis(delay_ms)
        );
    }

    /// Check if retry should be attempted
    pub fn should_retry(&self) -> bool {
        self.retry_info.attempt < self.retry_info.max_attempts
            && self.retry_info.next_retry_at
                .map(|time| std::time::SystemTime::now() >= time)
                .unwrap_or(false)
    }
}

impl BLSSignature {
    /// Create new BLS signature
    pub fn new(signature: [u8; 96], message_hash: Option<Hash256>) -> Self {
        Self {
            signature,
            aggregation_bits: None,
            message_hash,
        }
    }

    /// Set aggregation info
    pub fn set_aggregation_bits(&mut self, bits: Vec<bool>) {
        self.aggregation_bits = Some(bits);
    }

    /// Check if signature is aggregated
    pub fn is_aggregated(&self) -> bool {
        self.aggregation_bits.is_some()
    }
}