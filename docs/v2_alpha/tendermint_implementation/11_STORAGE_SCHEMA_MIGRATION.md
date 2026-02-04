# Implementation Plan: Storage Schema Migration for Tendermint

## Overview

This document provides a comprehensive implementation guide for migrating the StorageActor schema from probabilistic-finality storage (with difficulty tracking, orphans, and fork data) to instant-finality storage (with commit proofs embedded in blocks, validator sets, and linear chain progression).

**Key Design Decision**: Following standard Tendermint/CometBFT architecture, **LastCommit is embedded in the block structure itself**, not stored in a separate column family. Block N contains the commit proof (+2/3 precommit signatures) for Block N-1.

**Estimated Effort**: 1 week
**Dependencies**:
- `01_MESSAGE_TYPES_AND_PROTOCOL_FOUNDATION.md` (Commit type)
- `06_WAL.md` (WAL storage considerations)
- `09_SYNC_ACTOR.md` (Sync storage requirements)
**Files to Modify**:
- `app/src/block.rs` (add LastCommit to ConsensusBlock)
- `app/src/actors_v2/storage/database.rs`
- `app/src/actors_v2/storage/actor.rs`
- `app/src/actors_v2/storage/handlers/block_handlers.rs`
- `app/src/actors_v2/storage/messages.rs`

---

## 1. Tendermint Block Structure

### 1.1 Standard Tendermint Block Layout

In CometBFT/Tendermint, a block contains the commit proof for the **previous** block:

```
Block N:
├── Header
│   ├── Height: N
│   ├── LastCommitHash: hash(LastCommit)
│   └── ... other header fields
├── Data (transactions)
└── LastCommit: Commit for Block N-1
    ├── Height: N-1
    ├── Round: R
    ├── BlockID: hash of Block N-1
    └── Signatures: [CommitSig, CommitSig, ...]
```

**Key Insight**: `LoadBlockCommit(height)` returns `Block[height+1].LastCommit`

### 1.2 Why LastCommit Trails by One Block

```
Timeline:
  Block 99 produced → Validators precommit Block 99 → Commit(99) created
                                                           ↓
  Block 100 proposed ← includes Commit(99) as LastCommit ←─┘
```

This design ensures:
1. **Atomic persistence**: Block and its proof-of-finality for the previous block are stored together
2. **Proof availability**: Any node with Block N can prove Block N-1 was finalized
3. **Light client efficiency**: Single block fetch provides commit proof

---

## 2. Block Structure Modification

### 2.1 Updated ConsensusBlock

Modify `app/src/block.rs`:

```rust
use crate::actors_v2::chain::tendermint::Commit;

/// Sidechain block containing execution payload and consensus data.
///
/// # Tendermint Integration
///
/// The `last_commit` field contains the Tendermint commit proof (+2/3 precommit
/// signatures) for the **previous** block. This follows standard CometBFT design
/// where Block N proves finality of Block N-1.
///
/// ```text
/// Block N:
/// ├── parent_hash: hash(Block N-1)
/// ├── slot: N
/// ├── last_commit: Commit for Block N-1 (signatures proving N-1 is final)
/// ├── execution_payload: EVM state transition
/// └── ... other fields
/// ```
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ConsensusBlock<T: EthSpec> {
    /// The block hash of the parent
    pub parent_hash: Hash256,

    /// Slot/height the block was produced in
    pub slot: u64,

    /// Tendermint commit proof for the PREVIOUS block (Block N-1).
    ///
    /// - `None` for genesis block (no previous block to commit)
    /// - `Some(Commit)` for all other blocks, containing +2/3 precommit signatures
    ///
    /// This field proves that the parent block reached finality through
    /// Tendermint consensus before this block was proposed.
    pub last_commit: Option<Commit>,

    /// Proof of work checkpoint (for Bitcoin security anchoring).
    /// Repurposed from fork-choice to checkpoint-only role.
    pub auxpow_header: Option<AuxPowHeader>,

    /// Execution layer payload (EVM state transition)
    pub execution_payload: ExecutionPayloadCapella<T>,

    /// Transactions that are sending funds to the bridge
    pub pegins: Vec<(Txid, BlockHash)>,

    /// Bitcoin payments for pegouts
    pub pegout_payment_proposal: Option<BitcoinTransaction>,

    /// Finalized bitcoin payments
    pub finalized_pegouts: Vec<BitcoinTransaction>,
}
```

### 2.2 Commit Structure (from types.rs)

```rust
/// Commit proof - aggregated signatures proving 2/3+ precommits
///
/// This structure proves that a block was finalized by collecting
/// the precommit signatures from validators representing >2/3 of
/// the total voting power.
///
/// # Storage Location
///
/// Commit for Block N is stored in Block N+1's `last_commit` field.
/// This follows the standard Tendermint/CometBFT pattern.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Commit {
    /// Height of the committed block (this commit is FOR this height)
    pub height: Height,

    /// Round in which the block was committed
    pub round: Round,

    /// Hash of the committed block (BlockID in Tendermint terms)
    pub block_hash: BlockHash,

    /// Individual commit signatures from validators
    /// Each CommitSig represents one validator's precommit
    pub signatures: Vec<CommitSig>,
}

/// Individual validator's commit signature
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitSig {
    /// How this validator participated
    pub block_id_flag: BlockIDFlag,

    /// Validator's address/index (None if absent)
    pub validator_address: Option<ValidatorId>,

    /// Timestamp of the vote
    pub timestamp: u64,

    /// BLS signature (None if absent or voted nil)
    pub signature: Option<BLSSignature>,
}

/// Indicates how a validator voted in the commit
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum BlockIDFlag {
    /// Validator was absent (did not vote)
    Absent = 0,
    /// Validator voted for the block
    Commit = 1,
    /// Validator voted nil
    Nil = 2,
}

impl Commit {
    /// Count the number of validators who committed to the block
    pub fn num_commit_signatures(&self) -> usize {
        self.signatures
            .iter()
            .filter(|sig| sig.block_id_flag == BlockIDFlag::Commit)
            .count()
    }

    /// Check if a specific validator committed
    pub fn has_committed(&self, validator: ValidatorId) -> bool {
        self.signatures.iter().any(|sig| {
            sig.validator_address == Some(validator)
                && sig.block_id_flag == BlockIDFlag::Commit
        })
    }

    /// Verify the commit has sufficient signatures (>2/3)
    pub fn has_sufficient_signatures(&self, total_validators: usize) -> bool {
        let threshold = (total_validators * 2 / 3) + 1;
        self.num_commit_signatures() >= threshold
    }

    /// Get all public keys that signed this commit (for aggregate verification)
    pub fn get_signer_indices(&self) -> Vec<u8> {
        self.signatures
            .iter()
            .filter_map(|sig| {
                if sig.block_id_flag == BlockIDFlag::Commit {
                    sig.validator_address.map(|v| v.index())
                } else {
                    None
                }
            })
            .collect()
    }
}
```

### 2.3 Updated Default Implementation

```rust
impl Default for ConsensusBlock<MainnetEthSpec> {
    fn default() -> Self {
        Self {
            parent_hash: Hash256::zero(),
            slot: 0,
            last_commit: None,  // Genesis has no previous commit
            auxpow_header: None,
            execution_payload: ExecutionPayloadCapella {
                // ... existing defaults
            },
            pegins: vec![],
            pegout_payment_proposal: None,
            finalized_pegouts: vec![],
        }
    }
}
```

### 2.4 Genesis Block Handling

```rust
impl SignedConsensusBlock<MainnetEthSpec> {
    pub fn genesis(
        chain_spec: ChainSpec,
        execution_payload: ExecutionPayloadCapella<MainnetEthSpec>,
        initial_validator_set: &ValidatorSet,
    ) -> Self {
        if execution_payload.block_number != 0 {
            panic!("Execution payload should start at zero");
        }

        Self {
            message: ConsensusBlock {
                parent_hash: Hash256::zero(),
                slot: 0,
                last_commit: None,  // Genesis has no LastCommit
                auxpow_header: Some(AuxPowHeader {
                    range_start: Hash256::zero(),
                    range_end: Hash256::zero(),
                    bits: chain_spec.bits,
                    chain_id: chain_spec.chain_id,
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
```

---

## 3. Storage Schema Changes

### 3.1 Column Family Changes

| Current Column Family | Action | Notes |
|-----------------------|--------|-------|
| `Blocks` | **Keep** | Now includes LastCommit in block data |
| `BlockHeights` | **Keep** | Height → hash index |
| `State` | **Keep** | EVM state |
| `Receipts` | **Keep** | Transaction receipts |
| `Logs` | **Keep** | Event logs |
| `Metadata` | **Keep** | Chain metadata |
| `ChainHead` | **Keep** | Current tip |
| `CumulativeDifficulty` | **Remove** | Not needed (no fork choice) |
| `OrphanedBlocks` | **Remove** | Not needed (instant finality) |
| ~~`Commits`~~ | **NOT NEEDED** | Commits are in blocks |
| (new) `ValidatorSets` | **Add** | Height-based validator sets (H+2 rule) |
| (new) `Checkpoints` | **Add** | AuxPoW checkpoint proofs |

### 3.2 Visual Schema Comparison

```
CURRENT STORAGE SCHEMA (Aura):

┌─────────────────────────────────────────────────────────────┐
│                      RocksDB                                │
├──────────────┬──────────────┬──────────────┬───────────────┤
│   Blocks     │ BlockHeights │    State     │   Receipts    │
├──────────────┼──────────────┼──────────────┼───────────────┤
│     Logs     │   Metadata   │  ChainHead   │ CumulativeDiff│ ← REMOVE
├──────────────┼──────────────┼──────────────┼───────────────┤
│ OrphanedBlks │              │              │               │ ← REMOVE
└──────────────┴──────────────┴──────────────┴───────────────┘


TENDERMINT STORAGE SCHEMA:

┌─────────────────────────────────────────────────────────────┐
│                      RocksDB                                │
├──────────────┬──────────────┬──────────────┬───────────────┤
│   Blocks     │ BlockHeights │    State     │   Receipts    │
│ (w/LastCommit)                                              │
├──────────────┼──────────────┼──────────────┼───────────────┤
│     Logs     │   Metadata   │  ChainHead   │ValidatorSets  │ ← NEW
├──────────────┼──────────────┼──────────────┼───────────────┤
│ Checkpoints  │              │              │               │ ← NEW
└──────────────┴──────────────┴──────────────┴───────────────┘

Note: NO separate Commits column family - commits are embedded in Blocks
```

---

## 4. Commit Retrieval Pattern

### 4.1 GetCommit Implementation

Since commits are embedded in blocks, retrieving a commit requires fetching the **next** block:

```rust
impl DatabaseManager {
    /// Get the commit that finalized a block at the given height.
    ///
    /// # Important
    ///
    /// This returns `Block[height+1].last_commit`, following the Tendermint
    /// pattern where Block N contains the commit for Block N-1.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(commit))` - Commit found in next block
    /// - `Ok(None)` - Next block doesn't exist yet (tip of chain)
    /// - `Err(_)` - Storage error
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Get the commit that finalized block 100
    /// let commit = db.get_commit_for_height(100)?;
    /// // This actually fetches block 101 and returns its last_commit
    /// assert_eq!(commit.unwrap().height, 100);
    /// ```
    pub fn get_commit_for_height(&self, height: u64) -> Result<Option<Commit>, StorageError> {
        // Genesis has no commit
        if height == 0 {
            return Ok(None);
        }

        // Get the NEXT block which contains the commit for this height
        let next_block = self.get_block_by_height(height + 1)?;

        match next_block {
            Some(block) => Ok(block.message.last_commit),
            None => Ok(None), // Next block doesn't exist yet
        }
    }

    /// Check if a commit exists for the given height.
    ///
    /// A commit exists if height+1 block exists and has a last_commit.
    pub fn has_commit_for_height(&self, height: u64) -> Result<bool, StorageError> {
        if height == 0 {
            return Ok(false); // Genesis has no commit
        }

        match self.get_block_by_height(height + 1)? {
            Some(block) => Ok(block.message.last_commit.is_some()),
            None => Ok(false),
        }
    }

    /// Get block along with its commit proof (if available).
    ///
    /// Returns the block at `height` and the commit that finalized it
    /// (from block at `height+1`).
    pub fn get_block_with_commit(
        &self,
        height: u64
    ) -> Result<Option<(SignedConsensusBlock<MainnetEthSpec>, Option<Commit>)>, StorageError> {
        let block = match self.get_block_by_height(height)? {
            Some(b) => b,
            None => return Ok(None),
        };

        let commit = self.get_commit_for_height(height)?;

        Ok(Some((block, commit)))
    }
}
```

### 4.2 Block Storage with Commit Validation

```rust
impl DatabaseManager {
    /// Store a block, validating its LastCommit references the correct parent.
    pub fn put_block_validated(
        &self,
        block: &SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<(), StorageError> {
        let height = block.message.slot;

        // Validate LastCommit (skip for genesis)
        if height > 0 {
            if let Some(ref last_commit) = block.message.last_commit {
                // LastCommit must be for height - 1
                if last_commit.height != height - 1 {
                    return Err(StorageError::InvalidData(format!(
                        "Block {} LastCommit height mismatch: expected {}, got {}",
                        height,
                        height - 1,
                        last_commit.height
                    )));
                }

                // LastCommit block_hash must match parent_hash
                if last_commit.block_hash != block.message.parent_hash {
                    return Err(StorageError::InvalidData(format!(
                        "Block {} LastCommit block_hash doesn't match parent_hash",
                        height
                    )));
                }
            } else {
                return Err(StorageError::InvalidData(format!(
                    "Block {} missing required LastCommit",
                    height
                )));
            }
        }

        // Store the block
        self.put_block(&block.canonical_root(), block)
    }
}
```

---

## 5. ValidatorSets Column Family

Validator sets are stored by **effective height** (the height at which they become active).

Following standard Tendermint, validator updates at block H take effect at block H+2:
- Updates returned in block H's EndBlock
- Block H+1 has `next_validators_hash` pointing to new set
- Block H+2 uses the new validator set (`validators_hash` = new set)

### 5.1 Implementation

```rust
/// Column family for validator sets by effective height
pub const CF_VALIDATOR_SETS: &str = "validator_sets";

/// Key format: effective_height as big-endian u64
/// Value format: Serialized ValidatorSet
///
/// Example entries:
///   0 -> Genesis validator set
///   102 -> Validator set effective at height 102 (update returned at height 100)
///   500 -> Validator set effective at height 500 (update returned at height 498)

fn validator_set_key(effective_height: u64) -> [u8; 8] {
    effective_height.to_be_bytes()
}

impl DatabaseManager {
    /// Store a validator set for an effective height
    ///
    /// The effective_height is when this set becomes active.
    /// For genesis, effective_height = 0.
    /// For updates, effective_height = update_height + 2 (H+2 rule).
    pub fn put_validator_set(
        &self,
        effective_height: u64,
        set: &ValidatorSet,
    ) -> Result<(), StorageError> {
        let key = validator_set_key(effective_height);
        let value = serde_json::to_vec(set)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        self.db.put_cf(
            self.cf_handle(CF_VALIDATOR_SETS)?,
            key,
            value,
        )?;

        tracing::info!(
            effective_height = effective_height,
            validator_count = set.len(),
            total_power = set.total_power(),
            "Stored validator set"
        );

        Ok(())
    }

    /// Get validator set that is active at a given height
    ///
    /// Finds the most recent validator set with effective_height <= height.
    /// This handles the case where validator sets don't change every block.
    pub fn get_validator_set_for_height(&self, height: u64) -> Result<Option<ValidatorSet>, StorageError> {
        let cf = self.cf_handle(CF_VALIDATOR_SETS)?;
        let mut iter = self.db.raw_iterator_cf(cf);

        // Seek to the first key > height, then go back one
        let search_key = validator_set_key(height + 1);
        iter.seek_for_prev(&search_key);

        // Now we're at the largest key <= height (or invalid if none exist)
        if iter.valid() {
            if let Some(value) = iter.value() {
                let set = serde_json::from_slice(value)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                return Ok(Some(set));
            }
        }

        Ok(None)
    }

    /// Get validator set at exact effective height (if exists)
    pub fn get_validator_set_at_height(&self, effective_height: u64) -> Result<Option<ValidatorSet>, StorageError> {
        let key = validator_set_key(effective_height);

        match self.db.get_cf(self.cf_handle(CF_VALIDATOR_SETS)?, key)? {
            Some(bytes) => {
                let set = serde_json::from_slice(&bytes)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                Ok(Some(set))
            }
            None => Ok(None),
        }
    }

    /// Get current (latest) validator set
    pub fn get_current_validator_set(&self) -> Result<Option<ValidatorSet>, StorageError> {
        let cf = self.cf_handle(CF_VALIDATOR_SETS)?;
        let mut iter = self.db.raw_iterator_cf(cf);
        iter.seek_to_last();

        if iter.valid() {
            if let Some(value) = iter.value() {
                let set = serde_json::from_slice(value)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                return Ok(Some(set));
            }
        }

        Ok(None)
    }

    /// List all validator set changes (for debugging/admin)
    pub fn list_validator_set_changes(&self) -> Result<Vec<(u64, ValidatorSet)>, StorageError> {
        let cf = self.cf_handle(CF_VALIDATOR_SETS)?;
        let mut iter = self.db.raw_iterator_cf(cf);
        iter.seek_to_first();

        let mut changes = Vec::new();
        while iter.valid() {
            if let (Some(key), Some(value)) = (iter.key(), iter.value()) {
                let effective_height = u64::from_be_bytes(key.try_into().unwrap());
                let set: ValidatorSet = serde_json::from_slice(value)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                changes.push((effective_height, set));
            }
            iter.next();
        }

        Ok(changes)
    }
}
```

---

## 6. Checkpoints Column Family

AuxPoW checkpoints anchor block ranges to Bitcoin for additional security.

### 6.1 Implementation

```rust
/// Column family for AuxPoW checkpoints
pub const CF_CHECKPOINTS: &str = "checkpoints";

/// Key format: range_end_height as big-endian u64
/// Value format: Serialized AuxPowCheckpoint

fn checkpoint_key(range_end_height: u64) -> [u8; 8] {
    range_end_height.to_be_bytes()
}

/// AuxPoW checkpoint that anchors a range of Tendermint blocks to Bitcoin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxPowCheckpoint {
    /// First block height covered by this checkpoint
    pub range_start_height: u64,

    /// Last block height covered by this checkpoint (inclusive)
    pub range_end_height: u64,

    /// Hash of the block at range_start
    pub range_start_hash: Hash256,

    /// Hash of the block at range_end
    pub range_end_hash: Hash256,

    /// The AuxPoW proof anchoring to Bitcoin
    pub auxpow: AuxPow,

    /// Bitcoin block height where checkpoint was mined
    pub bitcoin_height: u64,

    /// Timestamp of checkpoint creation
    pub timestamp: u64,
}

impl DatabaseManager {
    /// Store an AuxPoW checkpoint
    pub fn put_checkpoint(&self, checkpoint: &AuxPowCheckpoint) -> Result<(), StorageError> {
        let key = checkpoint_key(checkpoint.range_end_height);
        let value = serde_json::to_vec(checkpoint)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        self.db.put_cf(
            self.cf_handle(CF_CHECKPOINTS)?,
            key,
            value,
        )?;

        tracing::info!(
            range_start = checkpoint.range_start_height,
            range_end = checkpoint.range_end_height,
            bitcoin_height = checkpoint.bitcoin_height,
            "Stored AuxPoW checkpoint"
        );

        Ok(())
    }

    /// Get checkpoint that covers a given height
    pub fn get_checkpoint_for_height(&self, height: u64) -> Result<Option<AuxPowCheckpoint>, StorageError> {
        let cf = self.cf_handle(CF_CHECKPOINTS)?;
        let mut iter = self.db.raw_iterator_cf(cf);

        // Seek to first checkpoint with range_end >= height
        iter.seek(checkpoint_key(height));

        while iter.valid() {
            if let Some(value) = iter.value() {
                let checkpoint: AuxPowCheckpoint = serde_json::from_slice(value)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;

                // Check if this checkpoint covers the height
                if checkpoint.range_start_height <= height && checkpoint.range_end_height >= height {
                    return Ok(Some(checkpoint));
                }

                // If range_start > height, no checkpoint covers this height
                if checkpoint.range_start_height > height {
                    break;
                }
            }
            iter.next();
        }

        Ok(None)
    }

    /// Get latest checkpoint
    pub fn get_latest_checkpoint(&self) -> Result<Option<AuxPowCheckpoint>, StorageError> {
        let cf = self.cf_handle(CF_CHECKPOINTS)?;
        let mut iter = self.db.raw_iterator_cf(cf);
        iter.seek_to_last();

        if iter.valid() {
            if let Some(value) = iter.value() {
                let checkpoint = serde_json::from_slice(value)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                return Ok(Some(checkpoint));
            }
        }

        Ok(None)
    }
}
```

---

## 7. Storage Messages

### 7.1 Updated Message Types

```rust
// In storage/messages.rs

/// Get commit for a specific height (fetches from next block)
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Option<Commit>, StorageError>")]
pub struct GetCommitForHeightMessage {
    /// Block height to get commit for
    pub height: u64,

    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Get block with its commit proof
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Option<BlockWithCommit>, StorageError>")]
pub struct GetBlockWithCommitMessage {
    /// Block height
    pub height: u64,

    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Block paired with its commit proof
#[derive(Debug, Clone)]
pub struct BlockWithCommit {
    /// The block
    pub block: SignedConsensusBlock<MainnetEthSpec>,

    /// Commit that finalized this block (from next block's LastCommit)
    /// None if this is the chain tip (next block doesn't exist yet)
    pub commit: Option<Commit>,
}

/// Store validator set at effective height
///
/// Following standard Tendermint H+2 rule:
/// - Updates returned at block H take effect at block H+2
/// - effective_height = H + 2 for updates
/// - effective_height = 0 for genesis
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct StoreValidatorSetMessage {
    /// Height at which this validator set becomes active
    pub effective_height: u64,
    pub validator_set: ValidatorSet,
    pub correlation_id: Option<Uuid>,
}

/// Get current (latest) validator set
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Option<ValidatorSet>, StorageError>")]
pub struct GetCurrentValidatorSetMessage {
    pub correlation_id: Option<Uuid>,
}

/// Get validator set active at specific height
///
/// Returns the validator set with the highest effective_height <= requested height.
/// This is the set that was active when block at `height` was produced.
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Option<ValidatorSet>, StorageError>")]
pub struct GetValidatorSetForHeightMessage {
    pub height: u64,
    pub correlation_id: Option<Uuid>,
}

/// List all validator set changes (for debugging/admin)
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Vec<(u64, ValidatorSet)>, StorageError>")]
pub struct ListValidatorSetChangesMessage {
    pub correlation_id: Option<Uuid>,
}

/// Store AuxPoW checkpoint
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct StoreCheckpointMessage {
    pub checkpoint: AuxPowCheckpoint,
    pub correlation_id: Option<Uuid>,
}

/// Get checkpoint for height
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Option<AuxPowCheckpoint>, StorageError>")]
pub struct GetCheckpointForHeightMessage {
    pub height: u64,
    pub correlation_id: Option<Uuid>,
}

/// Get latest checkpoint
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Option<AuxPowCheckpoint>, StorageError>")]
pub struct GetLatestCheckpointMessage {
    pub correlation_id: Option<Uuid>,
}
```

### 7.2 Handler Implementations

```rust
// In storage/handlers/tendermint_handlers.rs

impl Handler<GetCommitForHeightMessage> for StorageActor {
    type Result = Result<Option<Commit>, StorageError>;

    fn handle(&mut self, msg: GetCommitForHeightMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.get_commit_for_height(msg.height)
    }
}

impl Handler<GetBlockWithCommitMessage> for StorageActor {
    type Result = Result<Option<BlockWithCommit>, StorageError>;

    fn handle(&mut self, msg: GetBlockWithCommitMessage, _ctx: &mut Context<Self>) -> Self::Result {
        match self.db.get_block_with_commit(msg.height)? {
            Some((block, commit)) => Ok(Some(BlockWithCommit { block, commit })),
            None => Ok(None),
        }
    }
}

impl Handler<StoreValidatorSetMessage> for StorageActor {
    type Result = Result<(), StorageError>;

    fn handle(&mut self, msg: StoreValidatorSetMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.put_validator_set(msg.effective_height, &msg.validator_set)
    }
}

impl Handler<GetCurrentValidatorSetMessage> for StorageActor {
    type Result = Result<Option<ValidatorSet>, StorageError>;

    fn handle(&mut self, _msg: GetCurrentValidatorSetMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.get_current_validator_set()
    }
}

impl Handler<GetValidatorSetForHeightMessage> for StorageActor {
    type Result = Result<Option<ValidatorSet>, StorageError>;

    fn handle(&mut self, msg: GetValidatorSetForHeightMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.get_validator_set_for_height(msg.height)
    }
}

impl Handler<ListValidatorSetChangesMessage> for StorageActor {
    type Result = Result<Vec<(u64, ValidatorSet)>, StorageError>;

    fn handle(&mut self, _msg: ListValidatorSetChangesMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.list_validator_set_changes()
    }
}

impl Handler<StoreCheckpointMessage> for StorageActor {
    type Result = Result<(), StorageError>;

    fn handle(&mut self, msg: StoreCheckpointMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.put_checkpoint(&msg.checkpoint)
    }
}

impl Handler<GetCheckpointForHeightMessage> for StorageActor {
    type Result = Result<Option<AuxPowCheckpoint>, StorageError>;

    fn handle(&mut self, msg: GetCheckpointForHeightMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.get_checkpoint_for_height(msg.height)
    }
}

impl Handler<GetLatestCheckpointMessage> for StorageActor {
    type Result = Result<Option<AuxPowCheckpoint>, StorageError>;

    fn handle(&mut self, _msg: GetLatestCheckpointMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.get_latest_checkpoint()
    }
}
```

---

## 8. Column Families to Remove

### 8.1 CumulativeDifficulty Removal

```rust
// REMOVE: No longer needed with Tendermint instant finality
pub const CF_CUMULATIVE_DIFFICULTY: &str = "cumulative_difficulty";

// Remove all methods:
// - put_cumulative_difficulty()
// - get_cumulative_difficulty()
// - compare_chain_difficulty()
```

### 8.2 OrphanedBlocks Removal

```rust
// REMOVE: No orphan blocks with instant finality
pub const CF_ORPHANED_BLOCKS: &str = "orphaned_blocks";

// Remove all methods:
// - put_orphan()
// - get_orphan()
// - get_orphans_by_parent()
// - remove_orphan()
// - clear_old_orphans()
```

---

## 9. Database Initialization

### 9.1 Column Family Configuration

```rust
// In storage/database.rs

/// All column families for Tendermint storage
pub const COLUMN_FAMILIES: &[&str] = &[
    CF_BLOCKS,
    CF_BLOCK_HEIGHTS,
    CF_STATE,
    CF_RECEIPTS,
    CF_LOGS,
    CF_METADATA,
    CF_CHAIN_HEAD,
    CF_VALIDATOR_SETS,      // NEW: height-based validator sets
    CF_CHECKPOINTS,         // NEW: AuxPoW checkpoints
    CF_PARAMETER_HISTORY,   // NEW: governance parameter change history (Doc 17)
    // REMOVED: CF_CUMULATIVE_DIFFICULTY
    // REMOVED: CF_ORPHANED_BLOCKS
    // NOT NEEDED: CF_COMMITS (commits are in blocks)
];

/// Column family for governance parameter change history
/// Enables late-joiners to reconstruct parameter state at any height
/// Key: [param_id (2 bytes)][effective_height (8 bytes BE)]
/// Value: serialized parameter value
pub const CF_PARAMETER_HISTORY: &str = "parameter_history";
```

### 9.2 Migration Script

```rust
// In storage/migration.rs

/// Migrate database from Aura schema to Tendermint schema
pub async fn migrate_to_tendermint_schema(db_path: &Path) -> Result<(), StorageError> {
    tracing::info!("Starting storage schema migration to Tendermint");

    let db = DB::open_default(db_path)?;

    // 1. Check if migration is needed
    if db.cf_handle(CF_VALIDATOR_SETS).is_some() {
        tracing::info!("Database already migrated to Tendermint schema");
        return Ok(());
    }

    // 2. Drop obsolete column families
    let obsolete_cfs = ["cumulative_difficulty", "orphaned_blocks"];
    for cf_name in &obsolete_cfs {
        if db.cf_handle(cf_name).is_some() {
            tracing::info!(cf = cf_name, "Dropping obsolete column family");
            db.drop_cf(cf_name)?;
        }
    }

    // 3. Create new column families
    let new_cfs = [CF_VALIDATOR_SETS, CF_CHECKPOINTS, CF_PARAMETER_HISTORY];
    for cf_name in &new_cfs {
        if db.cf_handle(cf_name).is_none() {
            tracing::info!(cf = cf_name, "Creating new column family");
            let opts = Options::default();
            db.create_cf(cf_name, &opts)?;
        }
    }

    // 4. Update metadata
    let metadata_cf = db.cf_handle(CF_METADATA)
        .ok_or(StorageError::Corruption("Metadata CF missing".to_string()))?;

    db.put_cf(metadata_cf, b"schema_version", b"tendermint-1.0")?;
    db.put_cf(metadata_cf, b"migrated_at", chrono::Utc::now().to_rfc3339().as_bytes())?;

    // 5. Note: Existing blocks don't have LastCommit - they will be
    //    treated as pre-Tendermint blocks during sync/validation

    tracing::info!("Storage schema migration completed successfully");

    Ok(())
}
```

---

## 10. Block Validation Changes

### 10.1 Finalized-Only Storage

With Tendermint, all stored blocks are finalized. The storage layer should validate this:

```rust
impl Handler<StoreBlockMessage> for StorageActor {
    type Result = Result<(), StorageError>;

    fn handle(&mut self, msg: StoreBlockMessage, _ctx: &mut Context<Self>) -> Self::Result {
        let block = &msg.block;
        let height = block.message.slot;
        let block_hash = block.canonical_root();

        // Validate LastCommit for non-genesis blocks
        if height > 0 {
            let last_commit = block.message.last_commit.as_ref()
                .ok_or_else(|| StorageError::InvalidData(
                    format!("Block {} missing LastCommit", height)
                ))?;

            // LastCommit must reference previous block
            if last_commit.height != height - 1 {
                return Err(StorageError::InvalidData(format!(
                    "Block {} LastCommit height {} doesn't match expected {}",
                    height, last_commit.height, height - 1
                )));
            }

            if last_commit.block_hash != block.message.parent_hash {
                return Err(StorageError::InvalidData(format!(
                    "Block {} LastCommit block_hash doesn't match parent_hash",
                    height
                )));
            }
        }

        // Store block
        self.db.put_block(&block_hash, block)?;

        // Store height index
        self.db.put_block_height(height, &block_hash)?;

        // Update chain head
        self.db.put_chain_head(height, &block_hash)?;

        // Update cache
        self.cache.put_block(block_hash, block.clone());

        // Metrics
        STORAGE_BLOCKS_STORED.inc();
        STORAGE_CHAIN_HEIGHT.set(height as i64);

        tracing::debug!(
            height = height,
            hash = ?block_hash,
            last_commit_height = block.message.last_commit.as_ref().map(|c| c.height),
            "Stored finalized block"
        );

        Ok(())
    }
}
```

---

## 11. Testing Strategy

### 11.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_commit_in_block_retrieval() {
        let db = create_test_db();

        // Create genesis (no LastCommit)
        let genesis = create_genesis_block();
        db.put_block_validated(&genesis).unwrap();

        // Create block 1 with LastCommit for genesis
        let commit_for_genesis = create_commit(0, genesis.canonical_root());
        let block1 = create_block_with_commit(1, Some(commit_for_genesis.clone()));
        db.put_block_validated(&block1).unwrap();

        // Retrieve commit for genesis via block 1
        let retrieved = db.get_commit_for_height(0).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().height, 0);

        // Chain tip has no commit yet
        let tip_commit = db.get_commit_for_height(1).unwrap();
        assert!(tip_commit.is_none());
    }

    #[tokio::test]
    async fn test_block_with_commit() {
        let db = create_test_db();

        // Setup chain: genesis -> block1 -> block2
        let genesis = create_genesis_block();
        db.put_block_validated(&genesis).unwrap();

        let commit0 = create_commit(0, genesis.canonical_root());
        let block1 = create_block_with_commit(1, Some(commit0));
        db.put_block_validated(&block1).unwrap();

        let commit1 = create_commit(1, block1.canonical_root());
        let block2 = create_block_with_commit(2, Some(commit1));
        db.put_block_validated(&block2).unwrap();

        // Get block 1 with its commit
        let result = db.get_block_with_commit(1).unwrap().unwrap();
        assert_eq!(result.block.message.slot, 1);
        assert!(result.commit.is_some());
        assert_eq!(result.commit.unwrap().height, 1);
    }

    #[tokio::test]
    async fn test_validator_set_height_based() {
        let db = create_test_db();

        // Genesis validator set (effective from height 0)
        let genesis_set = create_test_validator_set(15);
        db.put_validator_set(0, &genesis_set).unwrap();

        // Update at block 100 takes effect at height 102 (H+2 rule)
        let updated_set = create_test_validator_set(14);  // One removed
        db.put_validator_set(102, &updated_set).unwrap();

        // Heights 0-101 should use genesis set
        let set_at_50 = db.get_validator_set_for_height(50).unwrap();
        assert!(set_at_50.is_some());
        assert_eq!(set_at_50.unwrap().len(), 15);

        let set_at_101 = db.get_validator_set_for_height(101).unwrap();
        assert_eq!(set_at_101.unwrap().len(), 15);

        // Height 102+ should use updated set
        let set_at_102 = db.get_validator_set_for_height(102).unwrap();
        assert_eq!(set_at_102.unwrap().len(), 14);

        let set_at_500 = db.get_validator_set_for_height(500).unwrap();
        assert_eq!(set_at_500.unwrap().len(), 14);
    }

    #[tokio::test]
    async fn test_checkpoint_coverage() {
        let db = create_test_db();

        // Checkpoint covers heights 1-100
        let checkpoint = create_test_checkpoint(1, 100);
        db.put_checkpoint(&checkpoint).unwrap();

        // Height 50 should be covered
        let found = db.get_checkpoint_for_height(50).unwrap();
        assert!(found.is_some());

        // Height 150 should not be covered
        let not_found = db.get_checkpoint_for_height(150).unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_block_validation_rejects_missing_commit() {
        let db = create_test_db();

        // Genesis is OK without LastCommit
        let genesis = create_genesis_block();
        db.put_block_validated(&genesis).unwrap();

        // Block 1 without LastCommit should fail
        let block1_no_commit = create_block_with_commit(1, None);
        let result = db.put_block_validated(&block1_no_commit);
        assert!(result.is_err());
    }
}
```

---

## 12. Checklist

### Block Structure Changes
- [ ] Add `last_commit: Option<Commit>` to `ConsensusBlock`
- [ ] Add `Commit`, `CommitSig`, `BlockIDFlag` types
- [ ] Update `Default` impl for `ConsensusBlock`
- [ ] Update `genesis()` method
- [ ] Update block serialization

### Storage Schema
- [ ] Add `CF_VALIDATOR_SETS` column family
- [ ] Add `CF_CHECKPOINTS` column family
- [ ] Add `CF_PARAMETER_HISTORY` column family (governance parameter changes)
- [ ] Remove `CF_CUMULATIVE_DIFFICULTY` column family
- [ ] Remove `CF_ORPHANED_BLOCKS` column family
- [ ] **DO NOT add `CF_COMMITS`** (commits are in blocks)

### Database Methods
- [ ] Implement `get_commit_for_height()` (fetches from next block)
- [ ] Implement `has_commit_for_height()`
- [ ] Implement `get_block_with_commit()`
- [ ] Implement `put_block_validated()` with LastCommit validation
- [ ] Implement validator set methods
- [ ] Implement checkpoint methods

### Messages and Handlers
- [ ] Add `GetCommitForHeightMessage`
- [ ] Add `GetBlockWithCommitMessage`
- [ ] Add `StoreValidatorSetMessage`, `GetValidatorSetMessage`
- [ ] Add `StoreCheckpointMessage`, `GetCheckpointMessage`
- [ ] Add `StoreParameterUpdateMessage`, `GetParameterAtHeightMessage` (governance params)
- [ ] Implement all handlers

### Migration
- [ ] Implement `migrate_to_tendermint_schema()`
- [ ] Handle existing blocks without LastCommit

### Testing
- [ ] Unit tests for commit retrieval pattern
- [ ] Unit tests for validator sets
- [ ] Unit tests for checkpoints
- [ ] Unit tests for block validation
- [ ] Migration tests

---

## 13. Summary

**Key Design Decision**: LastCommit is embedded in the block, not stored separately.

| Aspect | Implementation |
|--------|---------------|
| Commit storage | `Block[N].last_commit` contains commit for Block N-1 |
| Commit retrieval | `get_commit(height)` → `Block[height+1].last_commit` |
| Genesis | `last_commit: None` (no previous block) |
| Chain tip | No commit available until next block is produced |
| Column families | **No CF_COMMITS** - commits are in CF_BLOCKS |

This follows the standard Tendermint/CometBFT pattern and ensures atomic persistence of blocks with their finality proofs.

---

*Implementation Plan Version: 2.0*
*Last Updated: February 2026*
*Change: Embedded LastCommit in block structure per Tendermint standard*
