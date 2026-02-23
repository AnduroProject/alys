//! Write-Ahead Log for Tendermint consensus safety.
//!
//! The WAL ensures validators never double-vote after crashes by persisting
//! voting decisions before broadcasting them.
//!
//! # Safety Properties
//!
//! 1. **Durability**: Entries are fsync'd before any network broadcast
//! 2. **Atomicity**: Each entry is written atomically (length-prefixed)
//! 3. **Recoverability**: Full state can be reconstructed from WAL on restart
//!
//! # File Format
//!
//! ```text
//! ┌────────────────────────────────────────────────────┐
//! │ Entry 1: [length: u32][crc32: u32][data: bytes]    │
//! │ Entry 2: [length: u32][crc32: u32][data: bytes]    │
//! │ ...                                                 │
//! └────────────────────────────────────────────────────┘
//! ```

use super::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// WAL entry types
///
/// Each entry represents a state transition or action that must survive crashes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WALEntry {
    // ═══════════════════════════════════════════════════════════════════
    // ROUND STATE
    // ═══════════════════════════════════════════════════════════════════
    /// Starting a new round (height or round change)
    ///
    /// Written at the start of each round. On recovery, allows jumping
    /// to the correct (height, round) position.
    NewRound { height: u64, round: u32 },

    // ═══════════════════════════════════════════════════════════════════
    // MESSAGES SENT (Critical for Safety)
    // ═══════════════════════════════════════════════════════════════════
    /// We created and will broadcast a proposal
    ///
    /// Written BEFORE broadcasting the proposal.
    SentProposal {
        height: u64,
        round: u32,
        block_hash: BlockHash,
    },

    /// We created and will broadcast a prevote
    ///
    /// Written BEFORE broadcasting the prevote.
    /// `block_hash = None` indicates a NIL vote.
    SentPrevote {
        height: u64,
        round: u32,
        block_hash: Option<BlockHash>,
    },

    /// We created and will broadcast a precommit
    ///
    /// Written BEFORE broadcasting the precommit.
    SentPrecommit {
        height: u64,
        round: u32,
        block_hash: Option<BlockHash>,
    },

    // ═══════════════════════════════════════════════════════════════════
    // LOCKING STATE
    // ═══════════════════════════════════════════════════════════════════
    /// We locked on a block (after seeing 2/3+ prevotes)
    ///
    /// Critical for ensuring we don't vote for conflicting blocks
    /// across rounds after a restart.
    Locked { round: u32, block_hash: BlockHash },

    /// We unlocked (after seeing valid POL)
    Unlocked { previous_round: u32 },

    // ═══════════════════════════════════════════════════════════════════
    // COMMIT
    // ═══════════════════════════════════════════════════════════════════
    /// Block was committed (2/3+ precommits received)
    ///
    /// On recovery, allows skipping to the next height.
    Commit { height: u64, block_hash: BlockHash },

    // ═══════════════════════════════════════════════════════════════════
    // EVIDENCE
    // ═══════════════════════════════════════════════════════════════════
    /// We detected equivocation and will broadcast evidence
    ///
    /// Written BEFORE broadcasting evidence to prevent re-detection after crash.
    SentEvidence {
        height: u64,
        culprit: ValidatorId,
        evidence_hash: [u8; 32],
    },

    // ═══════════════════════════════════════════════════════════════════
    // LIVENESS STATE
    // ═══════════════════════════════════════════════════════════════════
    /// Blocks without AuxPoW counter update
    ///
    /// Persists the liveness gate counter to survive restarts.
    LivenessUpdate { height: u64, blocks_without_pow: u64 },
}

impl WALEntry {
    /// Get the height this entry pertains to
    pub fn height(&self) -> Option<u64> {
        match self {
            Self::NewRound { height, .. } => Some(*height),
            Self::SentProposal { height, .. } => Some(*height),
            Self::SentPrevote { height, .. } => Some(*height),
            Self::SentPrecommit { height, .. } => Some(*height),
            Self::Commit { height, .. } => Some(*height),
            Self::SentEvidence { height, .. } => Some(*height),
            Self::LivenessUpdate { height, .. } => Some(*height),
            Self::Locked { .. } | Self::Unlocked { .. } => None,
        }
    }

    /// Get entry type name for logging
    pub fn entry_type(&self) -> &'static str {
        match self {
            Self::NewRound { .. } => "NewRound",
            Self::SentProposal { .. } => "SentProposal",
            Self::SentPrevote { .. } => "SentPrevote",
            Self::SentPrecommit { .. } => "SentPrecommit",
            Self::Locked { .. } => "Locked",
            Self::Unlocked { .. } => "Unlocked",
            Self::Commit { .. } => "Commit",
            Self::SentEvidence { .. } => "SentEvidence",
            Self::LivenessUpdate { .. } => "LivenessUpdate",
        }
    }
}

/// WAL configuration parameters
#[derive(Debug, Clone)]
pub struct WALConfig {
    /// Directory for WAL file storage
    pub data_dir: PathBuf,

    /// WAL filename (default: "tendermint.wal")
    pub filename: String,

    /// Sync mode for durability vs performance trade-off
    pub sync_mode: SyncMode,

    /// Truncate WAL after this many committed heights
    pub truncate_after_commits: u64,

    /// Maximum WAL file size before forced truncation (optional)
    pub max_size_bytes: Option<u64>,
}

/// Sync mode for WAL writes
#[derive(Debug, Clone, Copy, Default)]
pub enum SyncMode {
    /// Sync after every write (safest, slowest)
    #[default]
    EveryWrite,

    /// Sync after batch of writes (balanced)
    Batched { batch_size: usize },

    /// Sync only on commit (fastest, least safe)
    OnCommitOnly,
}

impl Default for WALConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("/data/alys"),
            filename: "tendermint.wal".to_string(),
            sync_mode: SyncMode::EveryWrite,
            truncate_after_commits: 10,
            max_size_bytes: Some(100 * 1024 * 1024), // 100MB
        }
    }
}

/// Errors that can occur during WAL operations
#[derive(Debug, thiserror::Error)]
pub enum WALError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    #[error("Corrupted entry at offset {offset}")]
    CorruptedEntry { offset: u64 },

    #[error("WAL file not found: {0}")]
    NotFound(PathBuf),

    #[error("WAL file is locked by another process")]
    FileLocked,
}

/// Write-Ahead Log for Tendermint consensus
///
/// # Thread Safety
///
/// WAL is designed to be wrapped in `Arc<RwLock<>>` for async access.
/// Write operations take exclusive locks; reads during recovery are sequential.
pub struct ConsensusWAL {
    /// File handle for writing
    file: BufWriter<File>,

    /// Path to the WAL file
    path: PathBuf,

    /// Configuration
    config: WALConfig,

    /// Current height (for truncation decisions)
    current_height: u64,

    /// Entries written since last sync (for batching)
    pending_entries: usize,
}

impl ConsensusWAL {
    /// Create or open a WAL file
    ///
    /// # Arguments
    ///
    /// * `data_dir` - Directory for storing WAL file
    pub fn new(data_dir: &Path) -> Result<Self, WALError> {
        let config = WALConfig {
            data_dir: data_dir.to_path_buf(),
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// Create or open a WAL file with custom configuration
    pub fn with_config(config: WALConfig) -> Result<Self, WALError> {
        let path = config.data_dir.join(&config.filename);

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Open file for appending
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        let file = BufWriter::new(file);

        info!(path = ?path, "Opened WAL file");

        Ok(Self {
            file,
            path,
            config,
            current_height: 0,
            pending_entries: 0,
        })
    }

    /// Write an entry to the WAL
    ///
    /// This method:
    /// 1. Serializes the entry
    /// 2. Computes CRC32 checksum
    /// 3. Writes length-prefixed entry
    /// 4. Calls fsync to ensure durability
    ///
    /// # Safety
    ///
    /// MUST be called BEFORE any network broadcast of the corresponding message.
    pub fn write(&mut self, entry: WALEntry) -> Result<(), WALError> {
        // Serialize entry
        let data =
            bincode::serialize(&entry).map_err(|e| WALError::Serialization(e.to_string()))?;

        // Compute checksum
        let crc = crc32fast::hash(&data);

        // Write length prefix (u32, little-endian)
        let length = data.len() as u32;
        self.file.write_all(&length.to_le_bytes())?;

        // Write checksum
        self.file.write_all(&crc.to_le_bytes())?;

        // Write data
        self.file.write_all(&data)?;

        // Increment pending count
        self.pending_entries += 1;

        // Update current height
        if let Some(height) = entry.height() {
            self.current_height = height;
        }

        debug!(
            entry_type = entry.entry_type(),
            height = ?entry.height(),
            "WAL entry written"
        );

        // Sync based on mode
        match self.config.sync_mode {
            SyncMode::EveryWrite => self.sync()?,
            SyncMode::Batched { batch_size } => {
                if self.pending_entries >= batch_size {
                    self.sync()?;
                }
            }
            SyncMode::OnCommitOnly => {
                if matches!(entry, WALEntry::Commit { .. }) {
                    self.sync()?;
                }
            }
        }

        Ok(())
    }

    /// Flush and sync to disk
    pub fn sync(&mut self) -> Result<(), WALError> {
        self.file.flush()?;
        self.file.get_ref().sync_all()?;
        self.pending_entries = 0;
        Ok(())
    }

    /// Replay all entries from the WAL file
    ///
    /// Used during recovery to reconstruct state after a crash.
    ///
    /// # Returns
    ///
    /// All valid entries in the WAL, in order.
    pub fn replay(&self) -> Result<Vec<WALEntry>, WALError> {
        // Check if file exists
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.path)?;
        let mut reader = BufReader::new(file);
        let mut entries = Vec::new();
        let mut offset: u64 = 0;

        loop {
            // Read length
            let mut length_buf = [0u8; 4];
            match reader.read_exact(&mut length_buf) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
            let length = u32::from_le_bytes(length_buf) as usize;

            // Read checksum - truncated file means corruption
            let mut crc_buf = [0u8; 4];
            match reader.read_exact(&mut crc_buf) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    warn!(offset, "WAL truncated while reading checksum, stopping replay");
                    break;
                }
                Err(e) => return Err(e.into()),
            }
            let expected_crc = u32::from_le_bytes(crc_buf);

            // Read data - truncated file means corruption
            let mut data = vec![0u8; length];
            match reader.read_exact(&mut data) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    warn!(offset, "WAL truncated while reading data, stopping replay");
                    break;
                }
                Err(e) => return Err(e.into()),
            }

            // Verify checksum
            let actual_crc = crc32fast::hash(&data);
            if actual_crc != expected_crc {
                warn!(
                    offset,
                    expected = expected_crc,
                    actual = actual_crc,
                    "WAL checksum mismatch, stopping replay"
                );
                break; // Stop at first corruption (partial write during crash)
            }

            // Deserialize entry
            let entry: WALEntry = bincode::deserialize(&data)
                .map_err(|e| WALError::Deserialization(e.to_string()))?;

            entries.push(entry);
            offset += 4 + 4 + length as u64;
        }

        info!(entries_recovered = entries.len(), "WAL replay completed");

        Ok(entries)
    }

    /// Truncate WAL entries before a given height
    ///
    /// Called after a height is fully committed and no longer needed for recovery.
    /// This prevents unbounded WAL growth.
    pub fn truncate_before(&mut self, height: u64) -> Result<(), WALError> {
        // Read all entries
        let entries = self.replay()?;

        // Filter entries to keep (height >= given height)
        let entries_to_keep: Vec<_> = entries
            .into_iter()
            .filter(|e| e.height().map_or(true, |h| h >= height))
            .collect();

        // Create new WAL file
        let temp_path = self.path.with_extension("wal.tmp");
        let temp_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temp_path)?;

        let mut temp_writer = BufWriter::new(temp_file);

        // Write kept entries
        for entry in &entries_to_keep {
            let data =
                bincode::serialize(entry).map_err(|e| WALError::Serialization(e.to_string()))?;
            let crc = crc32fast::hash(&data);
            let length = data.len() as u32;

            temp_writer.write_all(&length.to_le_bytes())?;
            temp_writer.write_all(&crc.to_le_bytes())?;
            temp_writer.write_all(&data)?;
        }

        temp_writer.flush()?;
        temp_writer.get_ref().sync_all()?;
        drop(temp_writer);

        // Atomic rename
        std::fs::rename(&temp_path, &self.path)?;

        // Reopen file
        let file = OpenOptions::new().append(true).open(&self.path)?;
        self.file = BufWriter::new(file);

        info!(
            truncated_before = height,
            entries_remaining = entries_to_keep.len(),
            "WAL truncated"
        );

        Ok(())
    }

    /// Get the current file size
    pub fn file_size(&self) -> io::Result<u64> {
        Ok(self.path.metadata()?.len())
    }

    /// Get the path to the WAL file
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Recovered state from WAL replay
#[derive(Debug, Default, Clone)]
pub struct RecoveredState {
    /// Last committed height (start consensus at height + 1)
    pub last_committed_height: Option<u64>,

    /// Last committed block hash
    pub last_committed_block: Option<BlockHash>,

    /// Current round (if crashed mid-height)
    pub current_round: Option<u32>,

    /// Votes already sent at current height
    pub sent_prevotes: HashMap<u32, Option<BlockHash>>, // round -> block_hash
    pub sent_precommits: HashMap<u32, Option<BlockHash>>,

    /// Lock state
    pub locked_round: Option<u32>,
    pub locked_block: Option<BlockHash>,

    /// Evidence already sent (to avoid re-broadcasting)
    pub sent_evidence: HashSet<[u8; 32]>, // evidence_hash

    /// Liveness counter (blocks without AuxPoW)
    pub blocks_without_pow: u64,
}

impl RecoveredState {
    /// Recover state from WAL entries
    pub fn from_wal_entries(entries: Vec<WALEntry>) -> Self {
        let mut state = Self::default();
        let mut current_height: Option<u64> = None;

        for entry in entries {
            match entry {
                WALEntry::NewRound { height, round } => {
                    if current_height != Some(height) {
                        // New height - reset round-specific state
                        current_height = Some(height);
                        state.current_round = Some(round);
                        state.sent_prevotes.clear();
                        state.sent_precommits.clear();
                    } else {
                        state.current_round = Some(round);
                    }
                }

                WALEntry::SentPrevote {
                    height,
                    round,
                    block_hash,
                } => {
                    if current_height == Some(height) {
                        state.sent_prevotes.insert(round, block_hash);
                    }
                }

                WALEntry::SentPrecommit {
                    height,
                    round,
                    block_hash,
                } => {
                    if current_height == Some(height) {
                        state.sent_precommits.insert(round, block_hash);
                    }
                }

                WALEntry::Locked { round, block_hash } => {
                    state.locked_round = Some(round);
                    state.locked_block = Some(block_hash);
                }

                WALEntry::Unlocked { .. } => {
                    state.locked_round = None;
                    state.locked_block = None;
                }

                WALEntry::Commit { height, block_hash } => {
                    state.last_committed_height = Some(height);
                    state.last_committed_block = Some(block_hash);
                    // Clear height-specific state
                    state.sent_prevotes.clear();
                    state.sent_precommits.clear();
                    state.locked_round = None;
                    state.locked_block = None;
                    current_height = None;
                }

                WALEntry::SentProposal { .. } => {
                    // Proposals don't need special recovery handling
                    // (proposer selection is deterministic)
                }

                WALEntry::SentEvidence { evidence_hash, .. } => {
                    state.sent_evidence.insert(evidence_hash);
                }

                WALEntry::LivenessUpdate {
                    blocks_without_pow, ..
                } => {
                    state.blocks_without_pow = blocks_without_pow;
                }
            }
        }

        state
    }

    /// Get the height to start consensus at
    pub fn start_height(&self) -> u64 {
        self.last_committed_height.map_or(1, |h| h + 1)
    }

    /// Check if we already voted prevote in a given round
    pub fn has_prevoted(&self, round: u32) -> Option<Option<BlockHash>> {
        self.sent_prevotes.get(&round).copied()
    }

    /// Check if we already voted precommit in a given round
    pub fn has_precommitted(&self, round: u32) -> Option<Option<BlockHash>> {
        self.sent_precommits.get(&round).copied()
    }

    /// Check if we are locked on a block
    pub fn is_locked(&self) -> bool {
        self.locked_block.is_some()
    }

    /// Get the current step based on recovered state
    pub fn current_step(&self) -> TendermintStep {
        // Determine step based on what votes we've sent
        if let Some(round) = self.current_round {
            if self.sent_precommits.contains_key(&round) {
                return TendermintStep::Precommit;
            }
            if self.sent_prevotes.contains_key(&round) {
                return TendermintStep::Prevote;
            }
        }
        TendermintStep::Propose
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_wal_write_and_replay() {
        let dir = tempdir().unwrap();
        let mut wal = ConsensusWAL::new(dir.path()).unwrap();

        // Write entries
        wal.write(WALEntry::NewRound {
            height: 100,
            round: 0,
        })
        .unwrap();
        wal.write(WALEntry::SentPrevote {
            height: 100,
            round: 0,
            block_hash: Some(BlockHash::repeat_byte(0xAB)),
        })
        .unwrap();
        wal.write(WALEntry::Commit {
            height: 100,
            block_hash: BlockHash::repeat_byte(0xAB),
        })
        .unwrap();

        // Replay
        let entries = wal.replay().unwrap();
        assert_eq!(entries.len(), 3);

        match &entries[0] {
            WALEntry::NewRound { height, round } => {
                assert_eq!(*height, 100);
                assert_eq!(*round, 0);
            }
            _ => panic!("Wrong entry type"),
        }
    }

    #[test]
    fn test_recovery_state() {
        let entries = vec![
            WALEntry::NewRound {
                height: 100,
                round: 0,
            },
            WALEntry::SentPrevote {
                height: 100,
                round: 0,
                block_hash: Some(BlockHash::repeat_byte(0xAB)),
            },
            WALEntry::Locked {
                round: 0,
                block_hash: BlockHash::repeat_byte(0xAB),
            },
            WALEntry::Commit {
                height: 100,
                block_hash: BlockHash::repeat_byte(0xAB),
            },
        ];

        let recovered = RecoveredState::from_wal_entries(entries);

        assert_eq!(recovered.last_committed_height, Some(100));
        assert_eq!(recovered.start_height(), 101);
        // Lock state cleared after commit
        assert!(recovered.locked_block.is_none());
    }

    #[test]
    fn test_truncation() {
        let dir = tempdir().unwrap();
        let mut wal = ConsensusWAL::new(dir.path()).unwrap();

        // Write entries for multiple heights
        for height in 100..105 {
            wal.write(WALEntry::NewRound { height, round: 0 })
                .unwrap();
            wal.write(WALEntry::Commit {
                height,
                block_hash: BlockHash::repeat_byte(height as u8),
            })
            .unwrap();
        }

        // Truncate before height 103
        wal.truncate_before(103).unwrap();

        // Verify only heights >= 103 remain
        let entries = wal.replay().unwrap();
        let heights: Vec<_> = entries.iter().filter_map(|e| e.height()).collect();

        assert!(heights.iter().all(|&h| h >= 103));
    }

    #[test]
    fn test_recovery_after_crash_during_prevote() {
        let dir = tempdir().unwrap();
        let mut wal = ConsensusWAL::new(dir.path()).unwrap();

        // Simulate: Started round, sent prevote, then crashed
        wal.write(WALEntry::NewRound {
            height: 100,
            round: 0,
        })
        .unwrap();
        wal.write(WALEntry::SentPrevote {
            height: 100,
            round: 0,
            block_hash: Some(BlockHash::repeat_byte(0xAB)),
        })
        .unwrap();
        // CRASH - no commit written

        // Recovery
        let entries = wal.replay().unwrap();
        let recovered = RecoveredState::from_wal_entries(entries);

        // Should know we already voted
        assert!(recovered.has_prevoted(0).is_some());
        assert_eq!(
            recovered.has_prevoted(0).unwrap(),
            Some(BlockHash::repeat_byte(0xAB))
        );

        // Should NOT have a committed height
        assert!(recovered.last_committed_height.is_none());
    }

    #[test]
    fn test_recovery_with_corrupted_entry() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tendermint.wal");

        // Write valid entries
        {
            let mut wal = ConsensusWAL::new(dir.path()).unwrap();
            wal.write(WALEntry::NewRound {
                height: 100,
                round: 0,
            })
            .unwrap();
            wal.write(WALEntry::Commit {
                height: 100,
                block_hash: BlockHash::repeat_byte(0xAB),
            })
            .unwrap();
        }

        // Append garbage (simulates partial write during crash)
        {
            let mut file = OpenOptions::new().append(true).open(&path).unwrap();
            file.write_all(&[0xFF, 0xFF, 0xFF, 0xFF]).unwrap();
        }

        // Recovery should stop at corruption but return valid entries
        let wal = ConsensusWAL::new(dir.path()).unwrap();
        let entries = wal.replay().unwrap();

        // Should have the two valid entries
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_evidence_recovery() {
        let entries = vec![
            WALEntry::NewRound {
                height: 100,
                round: 0,
            },
            WALEntry::SentEvidence {
                height: 100,
                culprit: ValidatorId(5),
                evidence_hash: [0xAB; 32],
            },
        ];

        let recovered = RecoveredState::from_wal_entries(entries);

        assert!(recovered.sent_evidence.contains(&[0xAB; 32]));
    }

    #[test]
    fn test_liveness_counter_recovery() {
        let entries = vec![
            WALEntry::Commit {
                height: 99,
                block_hash: BlockHash::repeat_byte(0x99),
            },
            WALEntry::NewRound {
                height: 100,
                round: 0,
            },
            WALEntry::LivenessUpdate {
                height: 100,
                blocks_without_pow: 42,
            },
        ];

        let recovered = RecoveredState::from_wal_entries(entries);

        assert_eq!(recovered.blocks_without_pow, 42);
    }

    #[test]
    fn test_wal_entry_height() {
        let new_round = WALEntry::NewRound {
            height: 100,
            round: 0,
        };
        assert_eq!(new_round.height(), Some(100));

        let locked = WALEntry::Locked {
            round: 0,
            block_hash: BlockHash::zero(),
        };
        assert_eq!(locked.height(), None);
    }

    #[test]
    fn test_wal_entry_type() {
        let entry = WALEntry::SentPrevote {
            height: 100,
            round: 0,
            block_hash: None,
        };
        assert_eq!(entry.entry_type(), "SentPrevote");
    }

    #[test]
    fn test_recovered_state_current_step() {
        // No votes -> Propose
        let state = RecoveredState::default();
        assert_eq!(state.current_step(), TendermintStep::Propose);

        // With prevote -> Prevote
        let entries = vec![
            WALEntry::NewRound {
                height: 100,
                round: 0,
            },
            WALEntry::SentPrevote {
                height: 100,
                round: 0,
                block_hash: Some(BlockHash::zero()),
            },
        ];
        let state = RecoveredState::from_wal_entries(entries);
        assert_eq!(state.current_step(), TendermintStep::Prevote);

        // With precommit -> Precommit
        let entries = vec![
            WALEntry::NewRound {
                height: 100,
                round: 0,
            },
            WALEntry::SentPrecommit {
                height: 100,
                round: 0,
                block_hash: Some(BlockHash::zero()),
            },
        ];
        let state = RecoveredState::from_wal_entries(entries);
        assert_eq!(state.current_step(), TendermintStep::Precommit);
    }
}
