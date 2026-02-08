# Implementation Plan: Storage Schema Migration for Tendermint

## Overview

This document provides a comprehensive implementation guide for migrating the StorageActor schema from probabilistic-finality storage (with difficulty tracking, orphans, and fork data) to instant-finality storage (with commit proofs embedded in blocks, validator sets, and linear chain progression).

**Key Design Decision**: Following standard Tendermint/CometBFT architecture, **LastCommit is embedded in the block structure itself**, not stored in a separate column family. Block N contains the commit proof (+2/3 precommit signatures) for Block N-1.

**Estimated Effort**: 1 week
**Dependencies**:
- `01_MESSAGE_TYPES_AND_PROTOCOL_FOUNDATION.md` (Commit type)
- `02_STATE_MACHINE.md` (ValidatorSet, Vote types)
- `06_WAL.md` (WAL storage considerations)
- `07_EL_COORDINATION.md` (EL block execution)
- `09_SYNC_ACTOR.md` (Sync storage requirements)
- `16_AUXPOW_TENDERMINT_INTEGRATION.md` (Optional per-block AuxPoW)
- `17_GOVERNANCE.md` (Parameter change history)
**Files to Modify**:
- `app/src/block.rs` (add LastCommit to ConsensusBlock)
- `app/src/actors_v2/storage/database.rs`
- `app/src/actors_v2/storage/actor.rs`
- `app/src/actors_v2/storage/handlers/block_handlers.rs`
- `app/src/actors_v2/storage/messages.rs`
- `app/src/actors_v2/storage/handlers/tendermint_handlers.rs` (new)

---

## Cross-Document Type References

| Type | Source Document | Usage in This Document |
|------|-----------------|----------------------|
| `Commit`, `CommitSig`, `BlockIDFlag` | `01_MESSAGE_TYPES.md` | Embedded in ConsensusBlock |
| `ValidatorSet`, `ValidatorId` | `02_STATE_MACHINE.md` | ValidatorSets column family |
| `Vote`, `VoteType` | `02_STATE_MACHINE.md` | Commit signature verification |
| `WalWriter`, `WalEntry` | `06_WAL.md` | WAL-storage coordination |
| `SyncStatus`, `BlockRange` | `09_SYNC_ACTOR.md` | Batch storage during sync |
| `AuxPowCheckpoint` | `16_AUXPOW.md` | Checkpoints column family |
| `GovernanceParameter` | `17_GOVERNANCE.md` | Parameter history storage |

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

## 3. Error Types

### 3.1 Tendermint Storage Errors

```rust
/// Storage errors specific to Tendermint consensus data
#[derive(Debug, Clone, thiserror::Error)]
pub enum StorageError {
    // ... existing variants ...

    #[error("Invalid LastCommit: {0}")]
    InvalidLastCommit(String),

    #[error("LastCommit height mismatch: expected {expected}, got {actual}")]
    CommitHeightMismatch { expected: u64, actual: u64 },

    #[error("LastCommit block_hash doesn't match parent_hash at height {0}")]
    CommitHashMismatch(u64),

    #[error("Missing LastCommit for non-genesis block at height {0}")]
    MissingLastCommit(u64),

    #[error("Insufficient commit signatures: need {required}, got {actual}")]
    InsufficientCommitSignatures { required: usize, actual: usize },

    #[error("Missing validator set for height {0}")]
    MissingValidatorSet(u64),

    #[error("Validator set not found at effective height {0}")]
    ValidatorSetNotFound(u64),

    #[error("Checkpoint range overlap: existing {existing_start}-{existing_end}, new {new_start}-{new_end}")]
    CheckpointRangeOverlap {
        existing_start: u64,
        existing_end: u64,
        new_start: u64,
        new_end: u64,
    },

    #[error("Parameter history corruption at height {height}: {details}")]
    ParameterHistoryCorruption { height: u64, details: String },

    #[error("Chain integrity violation at height {height}: {details}")]
    ChainIntegrityViolation { height: u64, details: String },

    #[error("WAL-storage inconsistency: {0}")]
    WalStorageInconsistency(String),

    #[error("Block height gap: expected {expected}, got {actual}")]
    BlockHeightGap { expected: u64, actual: u64 },

    #[error("Batch write failed: {0}")]
    BatchWriteFailed(String),
}
```

---

## 4. Storage Schema Changes

### 4.1 Column Family Changes

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
| ~~`Checkpoints`~~ | **NOT NEEDED** | AuxPoW stored in block.auxpow_header |

### 4.2 Visual Schema Comparison

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

## 5. Commit Retrieval Pattern

### 5.1 GetCommit Implementation

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

### 5.2 Block Storage with Commit Validation

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

## 6. ValidatorSets Column Family

Validator sets are stored by **effective height** (the height at which they become active).

Following standard Tendermint, validator updates at block H take effect at block H+2:
- Updates returned in block H's EndBlock
- Block H+1 has `next_validators_hash` pointing to new set
- Block H+2 uses the new validator set (`validators_hash` = new set)

### 6.1 Implementation

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

## 7. AuxPoW Storage (Per-Block Optional)

AuxPoW is optional per block and stored directly in the block structure. See Document 16 for the simplified model.

### 7.1 Design

```
┌─────────────────────────────────────────────────────────────────┐
│                   SIMPLIFIED AUXPOW MODEL                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  • AuxPoW is optional per block                                  │
│  • Stored in ConsensusBlock.auxpow_header field                  │
│  • No checkpoint intervals or ranges                             │
│  • No difficulty thresholds                                      │
│  • No separate CF_CHECKPOINTS column family needed               │
│                                                                  │
│  Block structure:                                                │
│  ConsensusBlock {                                                │
│      ...                                                         │
│      auxpow_header: Option<AuxPowHeader>,  // Optional per block │
│      ...                                                         │
│  }                                                               │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 7.2 Querying Blocks with AuxPoW

To find blocks with AuxPoW, query the block storage and check the `auxpow_header` field:

```rust
impl DatabaseManager {
    /// Get blocks with AuxPoW in a height range (for historical queries)
    pub fn get_blocks_with_auxpow(
        &self,
        start_height: u64,
        end_height: u64,
    ) -> Result<Vec<(u64, AuxPowHeader)>, StorageError> {
        let mut results = Vec::new();

        for height in start_height..=end_height {
            if let Some(block) = self.get_block_by_height(height)? {
                if let Some(auxpow) = block.message.auxpow_header {
                    results.push((height, auxpow));
                }
            }
        }

        Ok(results)
    }

    /// Check if a block has AuxPoW
    pub fn block_has_auxpow(&self, height: u64) -> Result<bool, StorageError> {
        if let Some(block) = self.get_block_by_height(height)? {
            return Ok(block.message.auxpow_header.is_some());
        }
        Ok(false)
    }

    /// Get the latest block with AuxPoW
    pub fn get_latest_block_with_auxpow(&self) -> Result<Option<(u64, AuxPowHeader)>, StorageError> {
        let head = self.get_chain_head()?;
        let Some(head_ref) = head else {
            return Ok(None);
        };

        // Scan backwards from head to find most recent block with AuxPoW
        let mut height = head_ref.height;
        while height > 0 {
            if let Some(block) = self.get_block_by_height(height)? {
                if let Some(auxpow) = block.message.auxpow_header {
                    return Ok(Some((height, auxpow)));
                }
            }
            height -= 1;
        }

        Ok(None)
    }
}
```

**Note**: For production systems with many blocks, consider adding an index (bitmap or secondary column family) to efficiently query blocks with AuxPoW without scanning.

---

## 8. Storage Messages

### 8.1 Updated Message Types

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

/// Get blocks with AuxPoW in a range (for historical queries)
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Vec<(u64, AuxPowHeader)>, StorageError>")]
pub struct GetBlocksWithAuxPowMessage {
    pub start_height: u64,
    pub end_height: u64,
    pub correlation_id: Option<Uuid>,
}

/// Get the latest block with AuxPoW
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Option<(u64, AuxPowHeader)>, StorageError>")]
pub struct GetLatestBlockWithAuxPowMessage {
    pub correlation_id: Option<Uuid>,
}
```

### 8.2 Handler Implementations

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

impl Handler<GetBlocksWithAuxPowMessage> for StorageActor {
    type Result = Result<Vec<(u64, AuxPowHeader)>, StorageError>;

    fn handle(&mut self, msg: GetBlocksWithAuxPowMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.get_blocks_with_auxpow(msg.start_height, msg.end_height)
    }
}

impl Handler<GetLatestBlockWithAuxPowMessage> for StorageActor {
    type Result = Result<Option<(u64, AuxPowHeader)>, StorageError>;

    fn handle(&mut self, _msg: GetLatestBlockWithAuxPowMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.get_latest_block_with_auxpow()
    }
}
```

### 8.3 Parameter History Messages

```rust
/// Store a governance parameter update
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct StoreParameterUpdateMessage {
    /// Parameter identifier
    pub param_id: GovernanceParameterId,
    /// Height at which this value became effective
    pub effective_height: u64,
    /// The parameter value
    pub value: GovernanceParameterValue,
    pub correlation_id: Option<Uuid>,
}

/// Get parameter value at a specific height
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Option<GovernanceParameterValue>, StorageError>")]
pub struct GetParameterAtHeightMessage {
    pub param_id: GovernanceParameterId,
    pub height: u64,
    pub correlation_id: Option<Uuid>,
}

/// Get full parameter history for a parameter
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Vec<(u64, GovernanceParameterValue)>, StorageError>")]
pub struct GetParameterHistoryMessage {
    pub param_id: GovernanceParameterId,
    pub correlation_id: Option<Uuid>,
}

impl Handler<StoreParameterUpdateMessage> for StorageActor {
    type Result = Result<(), StorageError>;

    fn handle(&mut self, msg: StoreParameterUpdateMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.put_parameter_update(msg.param_id, msg.effective_height, &msg.value)
    }
}

impl Handler<GetParameterAtHeightMessage> for StorageActor {
    type Result = Result<Option<GovernanceParameterValue>, StorageError>;

    fn handle(&mut self, msg: GetParameterAtHeightMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.get_parameter_at_height(msg.param_id, msg.height)
    }
}
```

---

## 9. WAL-Storage Coordination

### 9.1 Write Ordering

WAL writes must complete before storage commits to ensure crash safety:

```rust
impl DatabaseManager {
    /// Store a finalized block with WAL coordination
    ///
    /// The WAL entry for this block should already be written by ChainActor
    /// before calling this method. Storage commit is the final step.
    pub fn store_finalized_block_with_wal_check(
        &self,
        block: &SignedConsensusBlock<MainnetEthSpec>,
        expected_wal_height: u64,
    ) -> Result<(), StorageError> {
        let height = block.message.slot;

        // Verify WAL has entry for this block (crash safety check)
        // In practice, ChainActor ensures WAL is written first
        if height != expected_wal_height {
            return Err(StorageError::WalStorageInconsistency(format!(
                "Expected WAL height {}, got block height {}",
                expected_wal_height, height
            )));
        }

        // Store the block
        self.put_block_validated(block)?;

        tracing::debug!(
            height = height,
            "Block stored after WAL confirmation"
        );

        Ok(())
    }
}
```

### 9.2 Crash Recovery: WAL Replay and Storage Verification

```rust
impl DatabaseManager {
    /// Verify storage consistency with WAL after crash recovery
    ///
    /// Called during startup after WAL replay to ensure storage
    /// is consistent with the recovered consensus state.
    pub fn verify_storage_wal_consistency(
        &self,
        wal_last_committed_height: u64,
    ) -> Result<StorageConsistencyResult, StorageError> {
        let storage_height = self.get_chain_height()?;

        match storage_height.cmp(&wal_last_committed_height) {
            std::cmp::Ordering::Equal => {
                // Perfect consistency
                Ok(StorageConsistencyResult::Consistent)
            }
            std::cmp::Ordering::Less => {
                // WAL has commits that weren't persisted to storage
                // This can happen if crash occurred after WAL write but before storage commit
                tracing::warn!(
                    storage_height = storage_height,
                    wal_height = wal_last_committed_height,
                    "Storage behind WAL - blocks need to be re-applied"
                );
                Ok(StorageConsistencyResult::StorageBehind {
                    storage_height,
                    wal_height: wal_last_committed_height,
                })
            }
            std::cmp::Ordering::Greater => {
                // Storage ahead of WAL - shouldn't happen with proper ordering
                tracing::error!(
                    storage_height = storage_height,
                    wal_height = wal_last_committed_height,
                    "Storage ahead of WAL - potential corruption"
                );
                Err(StorageError::WalStorageInconsistency(
                    "Storage height exceeds WAL committed height".to_string()
                ))
            }
        }
    }
}

#[derive(Debug)]
pub enum StorageConsistencyResult {
    /// Storage and WAL are consistent
    Consistent,
    /// Storage is behind WAL - needs block replay
    StorageBehind { storage_height: u64, wal_height: u64 },
}
```

---

## 10. Batch Write Operations

### 10.1 Atomic Block Finalization

When finalizing a block, multiple storage operations must be atomic:

```rust
impl DatabaseManager {
    /// Atomically store all data for a finalized block
    ///
    /// This uses RocksDB WriteBatch to ensure all-or-nothing semantics.
    /// If any write fails, none are persisted.
    pub fn finalize_block_atomic(
        &self,
        block: &SignedConsensusBlock<MainnetEthSpec>,
        receipts: &[TransactionReceipt],
        state_updates: &StateUpdates,
        validator_set_update: Option<(u64, ValidatorSet)>,
    ) -> Result<(), StorageError> {
        let mut batch = WriteBatch::default();
        let height = block.message.slot;
        let block_hash = block.canonical_root();

        // 1. Block data
        let block_bytes = serde_json::to_vec(block)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        batch.put_cf(
            self.cf_handle(CF_BLOCKS)?,
            block_hash.as_bytes(),
            &block_bytes,
        );

        // 2. Height index
        batch.put_cf(
            self.cf_handle(CF_BLOCK_HEIGHTS)?,
            height.to_be_bytes(),
            block_hash.as_bytes(),
        );

        // 3. Chain head
        batch.put_cf(
            self.cf_handle(CF_CHAIN_HEAD)?,
            b"head",
            &serde_json::to_vec(&(height, block_hash))?,
        );

        // 4. Receipts
        for receipt in receipts {
            let receipt_bytes = serde_json::to_vec(receipt)?;
            batch.put_cf(
                self.cf_handle(CF_RECEIPTS)?,
                receipt.transaction_hash.as_bytes(),
                &receipt_bytes,
            );
        }

        // 5. State updates (simplified - actual implementation would be more complex)
        for (key, value) in state_updates.iter() {
            batch.put_cf(self.cf_handle(CF_STATE)?, key, value);
        }

        // 6. Validator set update (if any)
        if let Some((effective_height, set)) = validator_set_update {
            let set_bytes = serde_json::to_vec(&set)?;
            batch.put_cf(
                self.cf_handle(CF_VALIDATOR_SETS)?,
                effective_height.to_be_bytes(),
                &set_bytes,
            );
        }

        // Execute atomic write
        self.db.write(batch)
            .map_err(|e| StorageError::BatchWriteFailed(e.to_string()))?;

        tracing::debug!(
            height = height,
            hash = ?block_hash,
            "Atomic block finalization complete"
        );

        STORAGE_BATCH_WRITES.inc();

        Ok(())
    }
}
```

### 10.2 Batch Sync Storage

For syncing, store multiple blocks efficiently:

```rust
impl DatabaseManager {
    /// Store a batch of synced blocks atomically
    ///
    /// Used during fast sync to efficiently store multiple blocks.
    /// All blocks must be consecutive in height.
    pub fn store_synced_blocks_batch(
        &self,
        blocks: &[SignedConsensusBlock<MainnetEthSpec>],
    ) -> Result<(), StorageError> {
        if blocks.is_empty() {
            return Ok(());
        }

        // Validate consecutive heights
        let mut expected_height = blocks[0].message.slot;
        for block in blocks {
            if block.message.slot != expected_height {
                return Err(StorageError::BlockHeightGap {
                    expected: expected_height,
                    actual: block.message.slot,
                });
            }
            expected_height += 1;
        }

        let mut batch = WriteBatch::default();

        for block in blocks {
            let height = block.message.slot;
            let block_hash = block.canonical_root();

            // Validate LastCommit
            if height > 0 && block.message.last_commit.is_none() {
                return Err(StorageError::MissingLastCommit(height));
            }

            let block_bytes = serde_json::to_vec(block)?;
            batch.put_cf(
                self.cf_handle(CF_BLOCKS)?,
                block_hash.as_bytes(),
                &block_bytes,
            );
            batch.put_cf(
                self.cf_handle(CF_BLOCK_HEIGHTS)?,
                height.to_be_bytes(),
                block_hash.as_bytes(),
            );
        }

        // Update chain head to last block
        let last_block = blocks.last().unwrap();
        let last_height = last_block.message.slot;
        let last_hash = last_block.canonical_root();
        batch.put_cf(
            self.cf_handle(CF_CHAIN_HEAD)?,
            b"head",
            &serde_json::to_vec(&(last_height, last_hash))?,
        );

        self.db.write(batch)?;

        tracing::info!(
            start_height = blocks[0].message.slot,
            end_height = last_height,
            count = blocks.len(),
            "Batch stored synced blocks"
        );

        STORAGE_SYNC_BATCHES.inc();
        STORAGE_SYNC_BLOCKS.inc_by(blocks.len() as u64);

        Ok(())
    }
}
```

---

## 11. Sync Actor Integration

### 11.1 SyncActor Storage Calls

```rust
// In sync actor, when processing synced blocks:

impl SyncActor {
    /// Store a validated synced block
    async fn store_synced_block(
        &self,
        block: SignedConsensusBlock<MainnetEthSpec>,
        validator_set: &ValidatorSet,
    ) -> Result<(), SyncError> {
        // Verify LastCommit signatures before storage
        if let Some(ref last_commit) = block.message.last_commit {
            self.verify_commit_signatures(last_commit, validator_set)?;
        }

        // Send to storage actor
        self.storage_actor
            .send(StoreBlockMessage {
                block,
                correlation_id: Some(self.correlation_id),
            })
            .await
            .map_err(|e| SyncError::StorageError(e.to_string()))??;

        Ok(())
    }

    /// Get validator set for commit verification during sync
    async fn get_validator_set_for_sync_height(
        &self,
        height: u64,
    ) -> Result<ValidatorSet, SyncError> {
        self.storage_actor
            .send(GetValidatorSetForHeightMessage {
                height,
                correlation_id: Some(self.correlation_id),
            })
            .await
            .map_err(|e| SyncError::StorageError(e.to_string()))?
            .map_err(|e| SyncError::StorageError(e.to_string()))?
            .ok_or(SyncError::MissingValidatorSet(height))
    }
}
```

### 11.2 Commit Signature Verification

```rust
impl DatabaseManager {
    /// Verify LastCommit has sufficient valid signatures
    ///
    /// This is called by SyncActor before storing synced blocks.
    /// ChainActor verifies during consensus, but synced blocks need
    /// verification against the historical validator set.
    pub fn verify_commit_signatures(
        &self,
        commit: &Commit,
        validator_set: &ValidatorSet,
    ) -> Result<(), StorageError> {
        let threshold = validator_set.two_thirds_threshold();
        let mut valid_signatures = 0;

        for sig in &commit.signatures {
            if sig.block_id_flag != BlockIDFlag::Commit {
                continue;
            }

            let validator_id = sig.validator_address
                .ok_or_else(|| StorageError::InvalidLastCommit(
                    "Commit signature missing validator address".to_string()
                ))?;

            let validator = validator_set.get_by_id(validator_id)
                .ok_or_else(|| StorageError::InvalidLastCommit(format!(
                    "Validator {} not in set for height {}",
                    validator_id, commit.height
                )))?;

            // Verify signature (simplified - actual implementation uses BLS)
            if let Some(ref signature) = sig.signature {
                let signing_root = compute_commit_signing_root(commit);
                if validator.public_key.verify(signature, &signing_root) {
                    valid_signatures += 1;
                }
            }
        }

        if valid_signatures < threshold {
            return Err(StorageError::InsufficientCommitSignatures {
                required: threshold,
                actual: valid_signatures,
            });
        }

        Ok(())
    }
}
```

---

## 12. Late-Joiner Bootstrap

### 12.1 Trusted State Bootstrap

New nodes need initial state from a trusted source:

```rust
impl DatabaseManager {
    /// Bootstrap storage from trusted state snapshot
    ///
    /// Used by new nodes joining the network. The snapshot includes:
    /// - Genesis block and initial validator set
    /// - Recent blocks with commits (may optionally include AuxPoW)
    /// - Current validator set
    /// - Parameter change history
    pub fn bootstrap_from_trusted_state(
        &self,
        snapshot: TrustedStateSnapshot,
    ) -> Result<(), StorageError> {
        tracing::info!(
            snapshot_height = snapshot.height,
            validator_count = snapshot.validator_set.len(),
            "Bootstrapping from trusted state"
        );

        // 1. Store genesis validator set
        self.put_validator_set(0, &snapshot.genesis_validator_set)?;

        // 2. Store current validator set
        self.put_validator_set(
            snapshot.validator_set_effective_height,
            &snapshot.validator_set,
        )?;

        // 3. Store recent blocks (with LastCommits, may include AuxPoW)
        for block in &snapshot.recent_blocks {
            self.put_block_validated(block)?;
        }

        // 4. Store parameter history
        for (param_id, history) in &snapshot.parameter_history {
            for (height, value) in history {
                self.put_parameter_update(*param_id, *height, value)?;
            }
        }

        // 5. Update metadata
        self.put_metadata(b"bootstrap_height", &snapshot.height.to_be_bytes())?;
        self.put_metadata(b"bootstrap_timestamp", &snapshot.timestamp.to_be_bytes())?;

        STORAGE_BOOTSTRAP_COMPLETE.inc();

        tracing::info!("Bootstrap complete");

        Ok(())
    }
}

/// Trusted state snapshot for bootstrapping new nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedStateSnapshot {
    /// Height of the snapshot
    pub height: u64,
    /// Timestamp when snapshot was created
    pub timestamp: u64,
    /// Genesis validator set
    pub genesis_validator_set: ValidatorSet,
    /// Current active validator set
    pub validator_set: ValidatorSet,
    /// Height at which current validator set became effective
    pub validator_set_effective_height: u64,
    /// Recent blocks (last N blocks with LastCommits)
    /// Note: Blocks may optionally contain AuxPoW in their auxpow_header field
    pub recent_blocks: Vec<SignedConsensusBlock<MainnetEthSpec>>,
    /// Parameter change history
    pub parameter_history: HashMap<GovernanceParameterId, Vec<(u64, GovernanceParameterValue)>>,
}
```

---

## 13. Pruning Strategy

### 13.1 Validator Set Pruning

```rust
impl DatabaseManager {
    /// Prune old validator sets, keeping minimum history
    ///
    /// Keeps:
    /// - Genesis validator set (always)
    /// - Last N validator set changes
    /// - All sets within MIN_VALIDATOR_SET_HISTORY blocks of current height
    pub fn prune_validator_sets(
        &self,
        current_height: u64,
        keep_last_n_changes: usize,
        min_history_blocks: u64,
    ) -> Result<usize, StorageError> {
        let min_keep_height = current_height.saturating_sub(min_history_blocks);
        let changes = self.list_validator_set_changes()?;

        if changes.len() <= keep_last_n_changes + 1 {
            // +1 for genesis, nothing to prune
            return Ok(0);
        }

        let mut pruned = 0;
        let keep_start_idx = changes.len().saturating_sub(keep_last_n_changes);

        for (i, (effective_height, _)) in changes.iter().enumerate() {
            // Never prune genesis (height 0)
            if *effective_height == 0 {
                continue;
            }

            // Keep recent sets
            if *effective_height >= min_keep_height {
                continue;
            }

            // Keep last N changes
            if i >= keep_start_idx {
                continue;
            }

            // Prune this set
            self.delete_validator_set(*effective_height)?;
            pruned += 1;
        }

        if pruned > 0 {
            tracing::info!(pruned = pruned, "Pruned old validator sets");
            STORAGE_VALIDATOR_SETS_PRUNED.inc_by(pruned as u64);
        }

        Ok(pruned)
    }
}
```

### 13.2 AuxPoW Retention

**Note**: With the simplified AuxPoW model, there is no separate checkpoint storage. AuxPoW is stored as an optional field in the block structure (`auxpow_header`). Block pruning decisions apply to the entire block including any attached AuxPoW.

### 13.3 Parameter History Retention

```rust
impl DatabaseManager {
    /// Parameter history is NEVER pruned
    ///
    /// All parameter changes must be kept forever because:
    /// - Late joiners need to reconstruct historical state
    /// - Audit and compliance requirements
    /// - Verification of historical blocks
    pub fn parameter_history_is_immutable() -> bool {
        true
    }
}
```

---

## 14. Corruption Detection and Recovery

### 14.1 Chain Integrity Verification

```rust
impl DatabaseManager {
    /// Verify chain integrity from height A to B
    ///
    /// Checks:
    /// - No height gaps
    /// - Parent hash linkage
    /// - LastCommit references correct parent
    /// - LastCommit has sufficient signatures
    pub fn verify_chain_integrity(
        &self,
        start_height: u64,
        end_height: u64,
    ) -> Result<ChainIntegrityResult, StorageError> {
        let mut issues = Vec::new();
        let mut prev_hash: Option<Hash256> = None;

        for height in start_height..=end_height {
            let block = match self.get_block_by_height(height)? {
                Some(b) => b,
                None => {
                    issues.push(IntegrityIssue::MissingBlock { height });
                    continue;
                }
            };

            // Check parent linkage
            if let Some(expected_parent) = prev_hash {
                if block.message.parent_hash != expected_parent {
                    issues.push(IntegrityIssue::ParentHashMismatch {
                        height,
                        expected: expected_parent,
                        actual: block.message.parent_hash,
                    });
                }
            }

            // Check LastCommit (skip genesis)
            if height > 0 {
                match &block.message.last_commit {
                    Some(commit) => {
                        if commit.height != height - 1 {
                            issues.push(IntegrityIssue::CommitHeightMismatch {
                                block_height: height,
                                commit_height: commit.height,
                            });
                        }
                        if commit.block_hash != block.message.parent_hash {
                            issues.push(IntegrityIssue::CommitHashMismatch {
                                height,
                            });
                        }
                    }
                    None => {
                        issues.push(IntegrityIssue::MissingCommit { height });
                    }
                }
            }

            prev_hash = Some(block.canonical_root());
        }

        if issues.is_empty() {
            Ok(ChainIntegrityResult::Valid)
        } else {
            Ok(ChainIntegrityResult::Issues(issues))
        }
    }
}

#[derive(Debug)]
pub enum ChainIntegrityResult {
    Valid,
    Issues(Vec<IntegrityIssue>),
}

#[derive(Debug)]
pub enum IntegrityIssue {
    MissingBlock { height: u64 },
    ParentHashMismatch { height: u64, expected: Hash256, actual: Hash256 },
    CommitHeightMismatch { block_height: u64, commit_height: u64 },
    CommitHashMismatch { height: u64 },
    MissingCommit { height: u64 },
}
```

### 14.2 Recovery from Corruption

```rust
impl DatabaseManager {
    /// Attempt to recover from detected corruption
    ///
    /// For missing or corrupt blocks, the node must re-sync from peers.
    /// This method prepares the database for re-sync.
    pub fn prepare_resync_from_height(
        &self,
        height: u64,
    ) -> Result<(), StorageError> {
        tracing::warn!(
            height = height,
            "Preparing database for re-sync due to corruption"
        );

        // Delete all blocks from height onwards
        let chain_height = self.get_chain_height()?;
        for h in height..=chain_height {
            if let Some(block) = self.get_block_by_height(h)? {
                self.delete_block(&block.canonical_root())?;
                self.delete_block_height(h)?;
            }
        }

        // Update chain head to height - 1
        if height > 0 {
            if let Some(block) = self.get_block_by_height(height - 1)? {
                self.put_chain_head(height - 1, &block.canonical_root())?;
            }
        }

        STORAGE_RESYNC_PREPARATIONS.inc();

        tracing::info!(
            new_height = height.saturating_sub(1),
            "Database prepared for re-sync"
        );

        Ok(())
    }
}
```

---

## 15. Metrics

### 15.1 Storage Metrics

```rust
lazy_static! {
    // Block storage metrics
    pub static ref STORAGE_BLOCKS_STORED: IntCounter = IntCounter::new(
        "storage_blocks_stored_total",
        "Total blocks stored"
    ).unwrap();

    pub static ref STORAGE_CHAIN_HEIGHT: IntGauge = IntGauge::new(
        "storage_chain_height",
        "Current chain height in storage"
    ).unwrap();

    // Validator set metrics
    pub static ref STORAGE_VALIDATOR_SET_LOOKUPS: IntCounter = IntCounter::new(
        "storage_validator_set_lookups_total",
        "Total validator set lookups"
    ).unwrap();

    pub static ref STORAGE_VALIDATOR_SETS_STORED: IntCounter = IntCounter::new(
        "storage_validator_sets_stored_total",
        "Total validator sets stored"
    ).unwrap();

    pub static ref STORAGE_VALIDATOR_SETS_PRUNED: IntCounter = IntCounter::new(
        "storage_validator_sets_pruned_total",
        "Total validator sets pruned"
    ).unwrap();

    // AuxPoW query metrics
    pub static ref STORAGE_AUXPOW_QUERIES: IntCounter = IntCounter::new(
        "storage_auxpow_queries_total",
        "Total queries for blocks with AuxPoW"
    ).unwrap();

    // Batch operation metrics
    pub static ref STORAGE_BATCH_WRITES: IntCounter = IntCounter::new(
        "storage_batch_writes_total",
        "Total atomic batch writes"
    ).unwrap();

    pub static ref STORAGE_SYNC_BATCHES: IntCounter = IntCounter::new(
        "storage_sync_batches_total",
        "Total sync block batches stored"
    ).unwrap();

    pub static ref STORAGE_SYNC_BLOCKS: IntCounter = IntCounter::new(
        "storage_sync_blocks_total",
        "Total synced blocks stored"
    ).unwrap();

    // Bootstrap and recovery metrics
    pub static ref STORAGE_BOOTSTRAP_COMPLETE: IntCounter = IntCounter::new(
        "storage_bootstrap_complete_total",
        "Total bootstrap operations completed"
    ).unwrap();

    pub static ref STORAGE_RESYNC_PREPARATIONS: IntCounter = IntCounter::new(
        "storage_resync_preparations_total",
        "Total re-sync preparations due to corruption"
    ).unwrap();

    // Parameter history metrics
    pub static ref STORAGE_PARAMETER_UPDATES: IntCounter = IntCounter::new(
        "storage_parameter_updates_total",
        "Total governance parameter updates stored"
    ).unwrap();

    // Commit verification metrics
    pub static ref STORAGE_COMMIT_VERIFICATIONS: IntCounter = IntCounter::new(
        "storage_commit_verifications_total",
        "Total commit signature verifications"
    ).unwrap();

    pub static ref STORAGE_COMMIT_VERIFICATION_FAILURES: IntCounter = IntCounter::new(
        "storage_commit_verification_failures_total",
        "Total commit signature verification failures"
    ).unwrap();
}
```

---

## 16. Column Families to Remove

### 16.1 CumulativeDifficulty Removal

```rust
// REMOVE: No longer needed with Tendermint instant finality
pub const CF_CUMULATIVE_DIFFICULTY: &str = "cumulative_difficulty";

// Remove all methods:
// - put_cumulative_difficulty()
// - get_cumulative_difficulty()
// - compare_chain_difficulty()
```

### 16.2 OrphanedBlocks Removal

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

## 17. Database Initialization

### 17.1 Column Family Configuration

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
    CF_PARAMETER_HISTORY,   // NEW: governance parameter change history (Doc 17)
    // REMOVED: CF_CUMULATIVE_DIFFICULTY
    // REMOVED: CF_ORPHANED_BLOCKS
    // NOT NEEDED: CF_COMMITS (commits are in blocks)
    // NOT NEEDED: CF_CHECKPOINTS (AuxPoW is in block.auxpow_header)
];

/// Column family for governance parameter change history
/// Enables late-joiners to reconstruct parameter state at any height
/// Key: [param_id (2 bytes)][effective_height (8 bytes BE)]
/// Value: serialized parameter value
pub const CF_PARAMETER_HISTORY: &str = "parameter_history";
```

### 17.2 Migration Script

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
    let new_cfs = [CF_VALIDATOR_SETS, CF_PARAMETER_HISTORY];
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

## 18. Block Validation Changes

### 18.1 Finalized-Only Storage

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

## 19. Testing Strategy

### 19.1 Unit Tests

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

## 20. Checklist

### Block Structure Changes
- [ ] Add `last_commit: Option<Commit>` to `ConsensusBlock`
- [ ] Add `Commit`, `CommitSig`, `BlockIDFlag` types
- [ ] Update `Default` impl for `ConsensusBlock`
- [ ] Update `genesis()` method
- [ ] Update block serialization

### Error Types
- [ ] Add `StorageError::InvalidLastCommit`
- [ ] Add `StorageError::CommitHeightMismatch`
- [ ] Add `StorageError::CommitHashMismatch`
- [ ] Add `StorageError::MissingLastCommit`
- [ ] Add `StorageError::InsufficientCommitSignatures`
- [ ] Add `StorageError::MissingValidatorSet`
- [ ] ~~Add `StorageError::CheckpointRangeOverlap`~~ (not needed - no checkpoint ranges)
- [ ] Add `StorageError::WalStorageInconsistency`
- [ ] Add `StorageError::BatchWriteFailed`
- [ ] Add `StorageError::ChainIntegrityViolation`

### Storage Schema
- [ ] Add `CF_VALIDATOR_SETS` column family
- [ ] Add `CF_PARAMETER_HISTORY` column family
- [ ] Remove `CF_CUMULATIVE_DIFFICULTY` column family
- [ ] Remove `CF_ORPHANED_BLOCKS` column family
- [ ] **DO NOT add `CF_COMMITS`** (commits are in blocks)
- [ ] **DO NOT add `CF_CHECKPOINTS`** (AuxPoW is in block.auxpow_header)

### Commit Retrieval Methods
- [ ] Implement `get_commit_for_height()` (fetches from next block)
- [ ] Implement `has_commit_for_height()`
- [ ] Implement `get_block_with_commit()`
- [ ] Implement `put_block_validated()` with LastCommit validation
- [ ] Implement `verify_commit_signatures()` for sync verification

### Validator Set Methods
- [ ] Implement `put_validator_set()`
- [ ] Implement `get_validator_set_for_height()`
- [ ] Implement `get_validator_set_at_height()`
- [ ] Implement `get_current_validator_set()`
- [ ] Implement `list_validator_set_changes()`
- [ ] Implement `prune_validator_sets()`

### AuxPoW Query Methods
- [ ] Implement `get_blocks_with_auxpow()` (scan block range for AuxPoW)
- [ ] Implement `get_latest_block_with_auxpow()`
- [ ] Implement `block_has_auxpow()`

### Parameter History Methods
- [ ] Implement `put_parameter_update()`
- [ ] Implement `get_parameter_at_height()`
- [ ] Implement `get_parameter_history()`

### WAL-Storage Coordination
- [ ] Implement `store_finalized_block_with_wal_check()`
- [ ] Implement `verify_storage_wal_consistency()`
- [ ] Define `StorageConsistencyResult` enum

### Batch Write Operations
- [ ] Implement `finalize_block_atomic()` with WriteBatch
- [ ] Implement `store_synced_blocks_batch()`

### Sync Actor Integration
- [ ] Update SyncActor to use `GetValidatorSetForHeightMessage`
- [ ] Update SyncActor to verify commit signatures before storage
- [ ] Implement batch storage for synced block ranges

### Late-Joiner Bootstrap
- [ ] Define `TrustedStateSnapshot` struct
- [ ] Implement `bootstrap_from_trusted_state()`
- [ ] Add bootstrap metadata storage

### Corruption Detection and Recovery
- [ ] Define `ChainIntegrityResult` and `IntegrityIssue` enums
- [ ] Implement `verify_chain_integrity()`
- [ ] Implement `prepare_resync_from_height()`

### Messages and Handlers
- [ ] Add `GetCommitForHeightMessage`
- [ ] Add `GetBlockWithCommitMessage`
- [ ] Add `StoreValidatorSetMessage`
- [ ] Add `GetValidatorSetForHeightMessage`
- [ ] Add `GetCurrentValidatorSetMessage`
- [ ] Add `ListValidatorSetChangesMessage`
- [ ] Add `GetBlocksWithAuxPowMessage`
- [ ] Add `GetLatestBlockWithAuxPowMessage`
- [ ] Add `StoreParameterUpdateMessage`
- [ ] Add `GetParameterAtHeightMessage`
- [ ] Add `GetParameterHistoryMessage`
- [ ] Implement all handlers

### Metrics
- [ ] Add `STORAGE_BLOCKS_STORED` counter
- [ ] Add `STORAGE_CHAIN_HEIGHT` gauge
- [ ] Add `STORAGE_VALIDATOR_SET_LOOKUPS` counter
- [ ] Add `STORAGE_VALIDATOR_SETS_STORED` counter
- [ ] Add `STORAGE_VALIDATOR_SETS_PRUNED` counter
- [ ] Add `STORAGE_AUXPOW_QUERIES` counter
- [ ] Add `STORAGE_BATCH_WRITES` counter
- [ ] Add `STORAGE_SYNC_BATCHES` counter
- [ ] Add `STORAGE_SYNC_BLOCKS` counter
- [ ] Add `STORAGE_BOOTSTRAP_COMPLETE` counter
- [ ] Add `STORAGE_RESYNC_PREPARATIONS` counter
- [ ] Add `STORAGE_PARAMETER_UPDATES` counter
- [ ] Add `STORAGE_COMMIT_VERIFICATIONS` counter
- [ ] Add `STORAGE_COMMIT_VERIFICATION_FAILURES` counter

### Migration
- [ ] Implement `migrate_to_tendermint_schema()`
- [ ] Handle existing blocks without LastCommit
- [ ] Add schema version metadata

### Testing
- [ ] Unit tests for commit retrieval pattern
- [ ] Unit tests for validator sets (height-based lookup)
- [ ] Unit tests for checkpoints
- [ ] Unit tests for block validation
- [ ] Unit tests for parameter history
- [ ] Unit tests for WAL-storage consistency
- [ ] Unit tests for batch writes
- [ ] Unit tests for chain integrity verification
- [ ] Unit tests for commit signature verification
- [ ] Migration tests
- [ ] Bootstrap tests

---

## 21. Summary

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

*Implementation Plan Version: 3.1*
*Last Updated: February 2026*
*Changes in v3.1: Simplified to per-block optional AuxPoW model. Removed CF_CHECKPOINTS, checkpoint ranges, and related methods. AuxPoW stored in block.auxpow_header field.*
*Changes in v3.0: Added error types, WAL coordination, batch writes, sync integration, bootstrap, pruning, corruption detection, and metrics*
