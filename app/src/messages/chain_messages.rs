//! Chain consensus and blockchain messages

use crate::types::*;
use actix::prelude::*;

/// Message to process a new block
#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct ProcessBlockMessage {
    pub block: ConsensusBlock,
    pub source: BlockSource,
}

/// Message to get the current chain head
#[derive(Message)]
#[rtype(result = "Option<BlockRef>")]
pub struct GetHeadMessage;

/// Message to produce a new block
#[derive(Message)]
#[rtype(result = "Result<ConsensusBlock, ChainError>")]
pub struct ProduceBlockMessage {
    pub timestamp: u64,
    pub transactions: Vec<Transaction>,
}

/// Message to update the chain head
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateHeadMessage {
    pub new_head: BlockRef,
}

/// Message to validate a block
#[derive(Message)]
#[rtype(result = "Result<ValidationResult, ChainError>")]
pub struct ValidateBlockMessage {
    pub block: ConsensusBlock,
    pub full_validation: bool,
}

/// Message to get block by hash
#[derive(Message)]
#[rtype(result = "Result<Option<ConsensusBlock>, ChainError>")]
pub struct GetBlockMessage {
    pub block_hash: BlockHash,
}

/// Message to get block by number
#[derive(Message)]
#[rtype(result = "Result<Option<ConsensusBlock>, ChainError>")]
pub struct GetBlockByNumberMessage {
    pub block_number: u64,
}

/// Message to get chain status
#[derive(Message)]
#[rtype(result = "ChainStatus")]
pub struct GetChainStatusMessage;

/// Message to register for block notifications
#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct SubscribeBlocksMessage {
    pub subscriber: Recipient<BlockNotification>,
}

/// Message to notify about new blocks
#[derive(Message)]
#[rtype(result = "()")]
pub struct BlockNotification {
    pub block: ConsensusBlock,
    pub is_canonical: bool,
}

/// Message to handle auxiliary PoW submission
#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct AuxPowSubmissionMessage {
    pub aux_pow: AuxiliaryProofOfWork,
    pub block_hash: BlockHash,
}

/// Message to get pending transactions
#[derive(Message)]
#[rtype(result = "Vec<Transaction>")]
pub struct GetPendingTransactionsMessage {
    pub max_count: Option<usize>,
}

/// Message to add transaction to mempool
#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct AddTransactionMessage {
    pub transaction: Transaction,
}

/// Source of a block
#[derive(Debug, Clone)]
pub enum BlockSource {
    Local,
    Peer { peer_id: PeerId },
    Sync,
    Mining,
}

/// Block validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub gas_used: u64,
    pub state_root: Hash256,
}

/// Block validation errors
#[derive(Debug, Clone)]
pub enum ValidationError {
    InvalidParentHash,
    InvalidTimestamp,
    InvalidTransactions { tx_hashes: Vec<H256> },
    InvalidStateRoot,
    InvalidGasUsed,
    InvalidSignature,
    ConsensusError { message: String },
}

/// Current chain status
#[derive(Debug, Clone)]
pub struct ChainStatus {
    pub head: Option<BlockRef>,
    pub best_block_number: u64,
    pub best_block_hash: BlockHash,
    pub pending_transactions: usize,
    pub sync_status: SyncStatus,
    pub validator_status: ValidatorStatus,
    pub pow_status: PoWStatus,
}

/// Validator status
#[derive(Debug, Clone)]
pub enum ValidatorStatus {
    NotValidator,
    Validator {
        address: Address,
        is_active: bool,
        next_slot: Option<u64>,
    },
}

/// Proof of Work status
#[derive(Debug, Clone)]
pub enum PoWStatus {
    Disabled,
    Waiting {
        last_pow_block: u64,
        blocks_since_pow: u64,
        timeout_blocks: u64,
    },
    Active {
        current_target: U256,
        hash_rate: f64,
    },
}

/// Auxiliary Proof of Work
#[derive(Debug, Clone)]
pub struct AuxiliaryProofOfWork {
    pub parent_block: BlockHash,
    pub coinbase_tx: Vec<u8>,
    pub merkle_branch: Vec<Hash256>,
    pub merkle_index: u32,
    pub parent_block_header: Vec<u8>,
}

/// Transaction representation
#[derive(Debug, Clone)]
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

/// Transaction signature
#[derive(Debug, Clone)]
pub struct TransactionSignature {
    pub r: U256,
    pub s: U256,
    pub v: u64,
}