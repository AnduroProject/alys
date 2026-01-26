# Implementation Plan: Storage Schema Migration for Tendermint

## Overview

This document provides a comprehensive implementation guide for migrating the StorageActor schema from probabilistic-finality storage (with difficulty tracking, orphans, and fork data) to instant-finality storage (with commit proofs, validator sets, and linear chain progression).

**Estimated Effort**: 1 week
**Dependencies**:
- `01_MESSAGE_TYPES_AND_PROTOCOL_FOUNDATION.md` (Commit type)
- `06_WAL.md` (WAL storage considerations)
- `09_SYNC_ACTOR.md` (Sync storage requirements)
**Files to Modify**:
- `app/src/actors_v2/storage/database.rs`
- `app/src/actors_v2/storage/actor.rs`
- `app/src/actors_v2/storage/handlers/block_handlers.rs`
- `app/src/actors_v2/storage/messages.rs`

---

## 1. Current vs New Schema

### 1.1 Column Family Changes

| Current Column Family | Action | Tendermint Equivalent |
|-----------------------|--------|----------------------|
| `Blocks` | **Keep** | Same (block storage) |
| `BlockHeights` | **Keep** | Same (height → hash index) |
| `State` | **Keep** | Same (EVM state) |
| `Receipts` | **Keep** | Same (transaction receipts) |
| `Logs` | **Keep** | Same (event logs) |
| `Metadata` | **Keep** | Same (chain metadata) |
| `ChainHead` | **Keep** | Same (current tip) |
| `CumulativeDifficulty` | **Remove** | Not needed (no fork choice) |
| `OrphanedBlocks` | **Remove** | Not needed (instant finality) |
| (new) `Commits` | **Add** | Commit proofs per height |
| (new) `ValidatorSets` | **Add** | Epoch-based validator sets |
| (new) `Checkpoints` | **Add** | AuxPoW checkpoint proofs |

### 1.2 Visual Comparison

```
CURRENT STORAGE SCHEMA:

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
├──────────────┼──────────────┼──────────────┼───────────────┤
│     Logs     │   Metadata   │  ChainHead   │   Commits     │ ← NEW
├──────────────┼──────────────┼──────────────┼───────────────┤
│ ValidatorSets│ Checkpoints  │              │               │ ← NEW
└──────────────┴──────────────┴──────────────┴───────────────┘
```

---

## 2. New Column Families

### 2.1 Commits Column Family

Stores Tendermint commit proofs (2/3+ validator signatures) for each height.

```rust
// In storage/database.rs

/// Column family for Tendermint commit proofs
pub const CF_COMMITS: &str = "commits";

/// Key format: height as big-endian u64 (for sorted iteration)
/// Value format: SSZ-encoded Commit

/// Commit storage key
fn commit_key(height: u64) -> [u8; 8] {
    height.to_be_bytes()
}

impl DatabaseManager {
    /// Store a commit proof
    pub fn put_commit(&self, height: u64, commit: &Commit) -> Result<(), StorageError> {
        let key = commit_key(height);
        let value = commit.as_ssz_bytes();

        self.db.put_cf(
            self.cf_handle(CF_COMMITS)?,
            key,
            value,
        )?;

        Ok(())
    }

    /// Get a commit proof by height
    pub fn get_commit(&self, height: u64) -> Result<Option<Commit>, StorageError> {
        let key = commit_key(height);

        match self.db.get_cf(self.cf_handle(CF_COMMITS)?, key)? {
            Some(bytes) => {
                let commit = Commit::from_ssz_bytes(&bytes)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                Ok(Some(commit))
            }
            None => Ok(None),
        }
    }

    /// Check if commit exists for height
    pub fn has_commit(&self, height: u64) -> Result<bool, StorageError> {
        let key = commit_key(height);
        Ok(self.db.get_cf(self.cf_handle(CF_COMMITS)?, key)?.is_some())
    }

    /// Get latest committed height
    pub fn get_latest_committed_height(&self) -> Result<Option<u64>, StorageError> {
        let cf = self.cf_handle(CF_COMMITS)?;
        let mut iter = self.db.raw_iterator_cf(cf);
        iter.seek_to_last();

        if iter.valid() {
            if let Some(key) = iter.key() {
                let height = u64::from_be_bytes(key.try_into().map_err(|_| {
                    StorageError::Corruption("Invalid commit key".to_string())
                })?);
                return Ok(Some(height));
            }
        }

        Ok(None)
    }
}
```

### 2.2 ValidatorSets Column Family

Stores validator sets for each epoch (validator set can change between epochs).

```rust
// In storage/database.rs

/// Column family for validator sets by epoch
pub const CF_VALIDATOR_SETS: &str = "validator_sets";

/// Key format: epoch as big-endian u64
/// Value format: SSZ-encoded ValidatorSet

fn validator_set_key(epoch: u64) -> [u8; 8] {
    epoch.to_be_bytes()
}

impl DatabaseManager {
    /// Store a validator set for an epoch
    pub fn put_validator_set(&self, epoch: u64, set: &ValidatorSet) -> Result<(), StorageError> {
        let key = validator_set_key(epoch);
        let value = set.as_ssz_bytes();

        self.db.put_cf(
            self.cf_handle(CF_VALIDATOR_SETS)?,
            key,
            value,
        )?;

        Ok(())
    }

    /// Get validator set for an epoch
    pub fn get_validator_set(&self, epoch: u64) -> Result<Option<ValidatorSet>, StorageError> {
        let key = validator_set_key(epoch);

        match self.db.get_cf(self.cf_handle(CF_VALIDATOR_SETS)?, key)? {
            Some(bytes) => {
                let set = ValidatorSet::from_ssz_bytes(&bytes)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                Ok(Some(set))
            }
            None => Ok(None),
        }
    }

    /// Get validator set for a given height (looks up epoch)
    pub fn get_validator_set_for_height(&self, height: u64) -> Result<Option<ValidatorSet>, StorageError> {
        // Epoch calculation: epoch = height / EPOCH_LENGTH
        let epoch = height / EPOCH_LENGTH;
        self.get_validator_set(epoch)
    }

    /// Get current (latest) validator set
    pub fn get_current_validator_set(&self) -> Result<Option<ValidatorSet>, StorageError> {
        let cf = self.cf_handle(CF_VALIDATOR_SETS)?;
        let mut iter = self.db.raw_iterator_cf(cf);
        iter.seek_to_last();

        if iter.valid() {
            if let Some(value) = iter.value() {
                let set = ValidatorSet::from_ssz_bytes(value)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                return Ok(Some(set));
            }
        }

        Ok(None)
    }
}
```

### 2.3 Checkpoints Column Family (AuxPoW Security Layer)

Stores AuxPoW checkpoint proofs that anchor block ranges to Bitcoin.

```rust
// In storage/database.rs

/// Column family for AuxPoW checkpoints
pub const CF_CHECKPOINTS: &str = "checkpoints";

/// Key format: range_end_height as big-endian u64
/// Value format: SSZ-encoded AuxPowCheckpoint

fn checkpoint_key(range_end_height: u64) -> [u8; 8] {
    range_end_height.to_be_bytes()
}

impl DatabaseManager {
    /// Store an AuxPoW checkpoint
    pub fn put_checkpoint(&self, checkpoint: &AuxPowCheckpoint) -> Result<(), StorageError> {
        let key = checkpoint_key(checkpoint.range_end_height);
        let value = checkpoint.as_ssz_bytes();

        self.db.put_cf(
            self.cf_handle(CF_CHECKPOINTS)?,
            key,
            value,
        )?;

        tracing::info!(
            range_start = checkpoint.range_start_height,
            range_end = checkpoint.range_end_height,
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
                let checkpoint = AuxPowCheckpoint::from_ssz_bytes(value)
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
                let checkpoint = AuxPowCheckpoint::from_ssz_bytes(value)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                return Ok(Some(checkpoint));
            }
        }

        Ok(None)
    }

    /// Get all checkpoints in height range
    pub fn get_checkpoints_in_range(
        &self,
        start_height: u64,
        end_height: u64,
    ) -> Result<Vec<AuxPowCheckpoint>, StorageError> {
        let cf = self.cf_handle(CF_CHECKPOINTS)?;
        let mut iter = self.db.raw_iterator_cf(cf);
        let mut checkpoints = Vec::new();

        iter.seek(checkpoint_key(start_height));

        while iter.valid() {
            if let (Some(key), Some(value)) = (iter.key(), iter.value()) {
                let range_end = u64::from_be_bytes(key.try_into().map_err(|_| {
                    StorageError::Corruption("Invalid checkpoint key".to_string())
                })?);

                if range_end > end_height {
                    break;
                }

                let checkpoint = AuxPowCheckpoint::from_ssz_bytes(value)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                checkpoints.push(checkpoint);
            }
            iter.next();
        }

        Ok(checkpoints)
    }
}
```

---

## 3. Column Families to Remove

### 3.1 CumulativeDifficulty Removal

```rust
// REMOVE: No longer needed with Tendermint

/// Column family for cumulative difficulty tracking
pub const CF_CUMULATIVE_DIFFICULTY: &str = "cumulative_difficulty";

// Remove all methods:
// - put_cumulative_difficulty()
// - get_cumulative_difficulty()
// - compare_chain_difficulty()
```

### 3.2 OrphanedBlocks Removal

```rust
// REMOVE: No orphan blocks with instant finality

/// Column family for orphaned blocks
pub const CF_ORPHANED_BLOCKS: &str = "orphaned_blocks";

// Remove all methods:
// - put_orphan()
// - get_orphan()
// - get_orphans_by_parent()
// - remove_orphan()
// - clear_old_orphans()
```

---

## 4. New Storage Messages

### 4.1 Commit Messages

```rust
// In storage/messages.rs

/// Store a commit proof
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct StoreCommitMessage {
    /// Block height
    pub height: u64,

    /// Commit proof (2/3+ signatures)
    pub commit: Commit,

    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Get commit proof by height
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Option<Commit>, StorageError>")]
pub struct GetCommitMessage {
    /// Block height
    pub height: u64,

    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Check if commit exists
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<bool, StorageError>")]
pub struct HasCommitMessage {
    pub height: u64,
    pub correlation_id: Option<Uuid>,
}

/// Get block with its commit proof
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Option<CommittedBlock>, StorageError>")]
pub struct GetCommittedBlockMessage {
    /// Block height
    pub height: u64,

    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}
```

### 4.2 Validator Set Messages

```rust
// In storage/messages.rs

/// Store validator set for epoch
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct StoreValidatorSetMessage {
    /// Epoch number
    pub epoch: u64,

    /// Validator set
    pub validator_set: ValidatorSet,

    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Get validator set for epoch
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Option<ValidatorSet>, StorageError>")]
pub struct GetValidatorSetMessage {
    /// Epoch number (None = current)
    pub epoch: Option<u64>,

    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Get validator set for specific height
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Option<ValidatorSet>, StorageError>")]
pub struct GetValidatorSetForHeightMessage {
    pub height: u64,
    pub correlation_id: Option<Uuid>,
}
```

### 4.3 Checkpoint Messages

```rust
// In storage/messages.rs

/// Store AuxPoW checkpoint
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct StoreCheckpointMessage {
    /// Checkpoint data
    pub checkpoint: AuxPowCheckpoint,

    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Get checkpoint covering a height
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

/// Get checkpoints in range
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<Vec<AuxPowCheckpoint>, StorageError>")]
pub struct GetCheckpointsInRangeMessage {
    pub start_height: u64,
    pub end_height: u64,
    pub correlation_id: Option<Uuid>,
}
```

---

## 5. Handler Implementations

### 5.1 Commit Handlers

```rust
// In storage/handlers/tendermint_handlers.rs (new file)

impl Handler<StoreCommitMessage> for StorageActor {
    type Result = Result<(), StorageError>;

    fn handle(&mut self, msg: StoreCommitMessage, _ctx: &mut Context<Self>) -> Self::Result {
        let _span = tracing::debug_span!(
            "store_commit",
            height = msg.height,
            correlation_id = ?msg.correlation_id
        );

        // Validate commit is for correct height
        if msg.commit.height != msg.height {
            return Err(StorageError::InvalidData(format!(
                "Commit height {} doesn't match requested height {}",
                msg.commit.height, msg.height
            )));
        }

        // Store commit
        self.db.put_commit(msg.height, &msg.commit)?;

        // Update metrics
        STORAGE_COMMITS_STORED.inc();

        tracing::debug!(
            height = msg.height,
            signers = msg.commit.num_signers(),
            "Stored commit proof"
        );

        Ok(())
    }
}

impl Handler<GetCommitMessage> for StorageActor {
    type Result = Result<Option<Commit>, StorageError>;

    fn handle(&mut self, msg: GetCommitMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.get_commit(msg.height)
    }
}

impl Handler<GetCommittedBlockMessage> for StorageActor {
    type Result = Result<Option<CommittedBlock>, StorageError>;

    fn handle(&mut self, msg: GetCommittedBlockMessage, _ctx: &mut Context<Self>) -> Self::Result {
        // Get block
        let block = match self.db.get_block_by_height(msg.height)? {
            Some(b) => b,
            None => return Ok(None),
        };

        // Get commit
        let commit = match self.db.get_commit(msg.height)? {
            Some(c) => c,
            None => return Ok(None),
        };

        Ok(Some(CommittedBlock { block, commit }))
    }
}
```

### 5.2 Validator Set Handlers

```rust
impl Handler<StoreValidatorSetMessage> for StorageActor {
    type Result = Result<(), StorageError>;

    fn handle(&mut self, msg: StoreValidatorSetMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.put_validator_set(msg.epoch, &msg.validator_set)?;

        tracing::info!(
            epoch = msg.epoch,
            validator_count = msg.validator_set.len(),
            total_power = msg.validator_set.total_power(),
            "Stored validator set"
        );

        Ok(())
    }
}

impl Handler<GetValidatorSetMessage> for StorageActor {
    type Result = Result<Option<ValidatorSet>, StorageError>;

    fn handle(&mut self, msg: GetValidatorSetMessage, _ctx: &mut Context<Self>) -> Self::Result {
        match msg.epoch {
            Some(epoch) => self.db.get_validator_set(epoch),
            None => self.db.get_current_validator_set(),
        }
    }
}

impl Handler<GetValidatorSetForHeightMessage> for StorageActor {
    type Result = Result<Option<ValidatorSet>, StorageError>;

    fn handle(&mut self, msg: GetValidatorSetForHeightMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.get_validator_set_for_height(msg.height)
    }
}
```

### 5.3 Checkpoint Handlers

```rust
impl Handler<StoreCheckpointMessage> for StorageActor {
    type Result = Result<(), StorageError>;

    fn handle(&mut self, msg: StoreCheckpointMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.put_checkpoint(&msg.checkpoint)?;

        // Update metrics
        STORAGE_CHECKPOINTS_STORED.inc();
        STORAGE_LATEST_CHECKPOINT_HEIGHT.set(msg.checkpoint.range_end_height as i64);

        Ok(())
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

    fn handle(&mut self, msg: GetLatestCheckpointMessage, _ctx: &mut Context<Self>) -> Self::Result {
        self.db.get_latest_checkpoint()
    }
}
```

---

## 6. Database Initialization

### 6.1 Column Family Configuration

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
    CF_COMMITS,           // NEW
    CF_VALIDATOR_SETS,    // NEW
    CF_CHECKPOINTS,       // NEW
    // REMOVED: CF_CUMULATIVE_DIFFICULTY
    // REMOVED: CF_ORPHANED_BLOCKS
];

impl DatabaseManager {
    pub fn new(path: &Path, config: &DatabaseConfig) -> Result<Self, StorageError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Configure compression
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        // Configure cache
        let cache = Cache::new_lru_cache(config.cache_size_mb * 1024 * 1024);
        let mut block_opts = BlockBasedOptions::default();
        block_opts.set_block_cache(&cache);
        opts.set_block_based_table_factory(&block_opts);

        // Create column family descriptors
        let cf_descriptors: Vec<_> = COLUMN_FAMILIES
            .iter()
            .map(|name| {
                let mut cf_opts = Options::default();
                // Column-specific optimizations
                match *name {
                    CF_COMMITS | CF_CHECKPOINTS => {
                        // These are append-only and accessed by key
                        cf_opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
                    }
                    CF_VALIDATOR_SETS => {
                        // Small, rarely changes
                        cf_opts.set_compression_type(rocksdb::DBCompressionType::Zstd);
                    }
                    _ => {}
                }
                ColumnFamilyDescriptor::new(*name, cf_opts)
            })
            .collect();

        let db = DB::open_cf_descriptors(&opts, path, cf_descriptors)?;

        Ok(Self { db, path: path.to_path_buf() })
    }
}
```

### 6.2 Migration from Old Schema

```rust
// In storage/migration.rs (new file)

/// Migrate database from Aura schema to Tendermint schema
pub async fn migrate_to_tendermint_schema(db_path: &Path) -> Result<(), StorageError> {
    tracing::info!("Starting storage schema migration to Tendermint");

    let db = DB::open_default(db_path)?;

    // 1. Check if migration is needed
    if db.cf_handle(CF_COMMITS).is_some() {
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
    let new_cfs = [CF_COMMITS, CF_VALIDATOR_SETS, CF_CHECKPOINTS];
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

    tracing::info!("Storage schema migration completed successfully");

    Ok(())
}

/// Check if database needs migration
pub fn needs_migration(db_path: &Path) -> Result<bool, StorageError> {
    let db = DB::open_for_read_only(&Options::default(), db_path, false)?;

    // Check for old column families
    let has_old = db.cf_handle("cumulative_difficulty").is_some()
        || db.cf_handle("orphaned_blocks").is_some();

    // Check for new column families
    let has_new = db.cf_handle(CF_COMMITS).is_some();

    Ok(has_old && !has_new)
}
```

---

## 7. Block Storage Changes

### 7.1 Finalized-Only Storage

```rust
// In storage/handlers/block_handlers.rs

impl Handler<StoreBlockMessage> for StorageActor {
    type Result = Result<(), StorageError>;

    fn handle(&mut self, msg: StoreBlockMessage, _ctx: &mut Context<Self>) -> Self::Result {
        // With Tendermint, all stored blocks are finalized
        // No need for canonical/finalized flags

        let block_hash = msg.block.hash();
        let height = msg.block.message.slot;

        // Store block
        self.db.put_block(&block_hash, &msg.block)?;

        // Store height index (always canonical with Tendermint)
        self.db.put_block_height(height, &block_hash)?;

        // Update chain head
        self.db.put_chain_head(height, &block_hash)?;

        // Update cache
        self.cache.put_block(block_hash, msg.block.clone());

        // Metrics
        STORAGE_BLOCKS_STORED.inc();
        STORAGE_CHAIN_HEIGHT.set(height as i64);

        tracing::debug!(
            height = height,
            hash = ?block_hash,
            "Stored finalized block"
        );

        Ok(())
    }
}
```

### 7.2 No Fork Handling

```rust
// REMOVE: All fork-related methods

// These methods are no longer needed:
impl StorageActor {
    // REMOVE
    fn handle_potential_fork(&self, ...) { ... }

    // REMOVE
    fn update_canonical_chain(&self, ...) { ... }

    // REMOVE
    fn mark_blocks_orphaned(&self, ...) { ... }

    // REMOVE
    fn get_cumulative_difficulty(&self, ...) { ... }
}
```

---

## 8. Cache Updates

### 8.1 New Cache Entries

```rust
// In storage/cache.rs

pub struct StorageCache {
    /// Block cache (existing)
    blocks: LruCache<BlockHash, SignedConsensusBlock>,

    /// State cache (existing)
    state: LruCache<Hash256, StateSnapshot>,

    /// Commit cache (NEW)
    commits: LruCache<u64, Commit>,

    /// Validator set cache (NEW)
    validator_sets: LruCache<u64, ValidatorSet>,

    /// Latest checkpoint cache (NEW)
    latest_checkpoint: Option<AuxPowCheckpoint>,
}

impl StorageCache {
    pub fn new(config: &CacheConfig) -> Self {
        Self {
            blocks: LruCache::new(NonZeroUsize::new(config.block_cache_size).unwrap()),
            state: LruCache::new(NonZeroUsize::new(config.state_cache_size).unwrap()),
            commits: LruCache::new(NonZeroUsize::new(config.commit_cache_size).unwrap()),
            validator_sets: LruCache::new(NonZeroUsize::new(16).unwrap()),  // Few epochs
            latest_checkpoint: None,
        }
    }

    pub fn put_commit(&mut self, height: u64, commit: Commit) {
        self.commits.put(height, commit);
    }

    pub fn get_commit(&mut self, height: u64) -> Option<&Commit> {
        self.commits.get(&height)
    }

    pub fn put_validator_set(&mut self, epoch: u64, set: ValidatorSet) {
        self.validator_sets.put(epoch, set);
    }

    pub fn get_validator_set(&mut self, epoch: u64) -> Option<&ValidatorSet> {
        self.validator_sets.get(&epoch)
    }

    pub fn set_latest_checkpoint(&mut self, checkpoint: AuxPowCheckpoint) {
        self.latest_checkpoint = Some(checkpoint);
    }

    pub fn get_latest_checkpoint(&self) -> Option<&AuxPowCheckpoint> {
        self.latest_checkpoint.as_ref()
    }
}
```

---

## 9. Metrics

```rust
lazy_static! {
    /// Commits stored
    pub static ref STORAGE_COMMITS_STORED: IntCounter = IntCounter::new(
        "storage_commits_stored_total",
        "Total commit proofs stored"
    ).unwrap();

    /// Latest committed height
    pub static ref STORAGE_LATEST_COMMIT_HEIGHT: IntGauge = IntGauge::new(
        "storage_latest_commit_height",
        "Height of latest stored commit"
    ).unwrap();

    /// Checkpoints stored
    pub static ref STORAGE_CHECKPOINTS_STORED: IntCounter = IntCounter::new(
        "storage_checkpoints_stored_total",
        "Total AuxPoW checkpoints stored"
    ).unwrap();

    /// Latest checkpoint height
    pub static ref STORAGE_LATEST_CHECKPOINT_HEIGHT: IntGauge = IntGauge::new(
        "storage_latest_checkpoint_height",
        "End height of latest checkpoint"
    ).unwrap();

    /// Validator set updates
    pub static ref STORAGE_VALIDATOR_SET_UPDATES: IntCounter = IntCounter::new(
        "storage_validator_set_updates_total",
        "Total validator set updates stored"
    ).unwrap();
}
```

---

## 10. Testing Strategy

### 10.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_commit_storage() {
        let db = create_test_db();
        let commit = create_test_commit(100, 11);  // Height 100, 11 signers

        // Store
        db.put_commit(100, &commit).unwrap();

        // Retrieve
        let retrieved = db.get_commit(100).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().height, 100);
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
    async fn test_validator_set_epoch() {
        let db = create_test_db();

        let set = create_test_validator_set(15);
        db.put_validator_set(0, &set).unwrap();

        // Height 0-999 should use epoch 0
        let retrieved = db.get_validator_set_for_height(500).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().len(), 15);
    }

    #[tokio::test]
    async fn test_migration() {
        let temp_dir = tempdir::TempDir::new("migration_test").unwrap();

        // Create old-style database
        create_old_schema_db(temp_dir.path());

        // Run migration
        migrate_to_tendermint_schema(temp_dir.path()).await.unwrap();

        // Verify new column families exist
        let db = DB::open_default(temp_dir.path()).unwrap();
        assert!(db.cf_handle(CF_COMMITS).is_some());
        assert!(db.cf_handle(CF_VALIDATOR_SETS).is_some());
        assert!(db.cf_handle(CF_CHECKPOINTS).is_some());

        // Verify old column families removed
        assert!(db.cf_handle("cumulative_difficulty").is_none());
        assert!(db.cf_handle("orphaned_blocks").is_none());
    }
}
```

---

## 11. Checklist

- [ ] Add `CF_COMMITS` column family
- [ ] Add `CF_VALIDATOR_SETS` column family
- [ ] Add `CF_CHECKPOINTS` column family
- [ ] Implement `put_commit`, `get_commit` methods
- [ ] Implement `put_validator_set`, `get_validator_set` methods
- [ ] Implement `put_checkpoint`, `get_checkpoint_for_height` methods
- [ ] Add `StoreCommitMessage` and handler
- [ ] Add `GetCommitMessage` and handler
- [ ] Add `StoreValidatorSetMessage` and handler
- [ ] Add `GetValidatorSetMessage` and handler
- [ ] Add `StoreCheckpointMessage` and handler
- [ ] Add `GetCheckpointForHeightMessage` and handler
- [ ] Remove `CF_CUMULATIVE_DIFFICULTY` column family
- [ ] Remove `CF_ORPHANED_BLOCKS` column family
- [ ] Remove all difficulty-related methods
- [ ] Remove all orphan-related methods
- [ ] Update cache with commit/validator set entries
- [ ] Implement database migration script
- [ ] Update block storage for finalized-only model
- [ ] Add new metrics
- [ ] Write unit tests for new storage methods
- [ ] Write migration tests

---

*Implementation Plan Version: 1.0*
*Last Updated: January 2026*
