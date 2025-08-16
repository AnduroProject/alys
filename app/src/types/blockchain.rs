//! Blockchain-related types and structures

use crate::types::*;
use serde::{Deserialize, Serialize};

/// A complete block in the Alys blockchain (matches the actual Alys ConsensusBlock)
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
    /// Create a new consensus block
    pub fn new(
        slot: u64,
        execution_payload: ExecutionPayload,
        parent_hash: Hash256,
        auxpow_header: Option<AuxPowHeader>,
        pegins: Vec<(bitcoin::Txid, bitcoin::BlockHash)>,
        pegout_payment_proposal: Option<bitcoin::Transaction>,
        finalized_pegouts: Vec<bitcoin::Transaction>,
    ) -> Self {
        Self {
            slot,
            parent_hash,
            execution_payload,
            auxpow_header,
            pegins,
            pegout_payment_proposal,
            finalized_pegouts,
        }
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
    pub fn add_approval(&mut self, approval: IndividualApproval) -> Result<(), ChainError> {
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
    pub fn add_approval(&mut self, approval: IndividualApproval) -> Result<(), ChainError> {
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