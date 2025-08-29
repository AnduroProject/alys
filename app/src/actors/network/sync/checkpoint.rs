//! Checkpoint Management System
//! 
//! Implements blockchain state checkpointing for resilient synchronization
//! with compression, integrity verification, and recovery capabilities.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};
use ethereum_types::H256;
use flate2::write::{GzEncoder, GzDecoder};
use flate2::Compression;
use std::io::Write;
use crate::actors::network::messages::{NetworkError, NetworkResult, CheckpointResponse, RestoreResponse};

/// Checkpoint management system
pub struct CheckpointManager {
    /// Storage directory for checkpoints
    storage_path: PathBuf,
    /// Active checkpoints metadata
    checkpoints: HashMap<String, CheckpointMetadata>,
    /// Compression settings
    compression_level: Compression,
    /// Maximum checkpoints to retain
    max_checkpoints: u32,
    /// Verification enabled
    verify_integrity: bool,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub async fn new(
        storage_path: PathBuf,
        max_checkpoints: u32,
        compression_enabled: bool,
    ) -> NetworkResult<Self> {
        // Create storage directory if it doesn't exist
        if !storage_path.exists() {
            fs::create_dir_all(&storage_path).await.map_err(|e| {
                NetworkError::ProtocolError {
                    message: format!("Failed to create checkpoint directory: {}", e),
                }
            })?;
        }

        let compression_level = if compression_enabled {
            Compression::default()
        } else {
            Compression::none()
        };

        let mut manager = Self {
            storage_path,
            checkpoints: HashMap::new(),
            compression_level,
            max_checkpoints,
            verify_integrity: true,
        };

        // Load existing checkpoints
        manager.load_existing_checkpoints().await?;

        Ok(manager)
    }

    /// Create a new checkpoint at the specified height
    pub async fn create_checkpoint(
        &mut self,
        height: u64,
        chain_state: ChainState,
    ) -> NetworkResult<CheckpointResponse> {
        let start_time = Instant::now();
        let checkpoint_id = generate_checkpoint_id(height);

        tracing::info!("Creating checkpoint {} at height {}", checkpoint_id, height);

        // Serialize chain state
        let serialized_state = bincode::serialize(&chain_state).map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to serialize chain state: {}", e),
            }
        })?;

        // Compress if enabled
        let final_data = if self.compression_level != Compression::none() {
            self.compress_data(&serialized_state)?
        } else {
            serialized_state
        };

        // Calculate integrity hash
        let integrity_hash = self.calculate_hash(&final_data);

        // Write to storage
        let checkpoint_path = self.get_checkpoint_path(&checkpoint_id);
        let mut file = fs::File::create(&checkpoint_path).await.map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to create checkpoint file: {}", e),
            }
        })?;

        file.write_all(&final_data).await.map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to write checkpoint data: {}", e),
            }
        })?;

        // Create metadata
        let metadata = CheckpointMetadata {
            checkpoint_id: checkpoint_id.clone(),
            height,
            state_root: chain_state.state_root,
            created_at: SystemTime::now(),
            size_bytes: final_data.len() as u64,
            compressed: self.compression_level != Compression::none(),
            integrity_hash,
            file_path: checkpoint_path,
            peer_states: chain_state.peer_states.clone(),
        };

        // Save metadata
        self.save_checkpoint_metadata(&metadata).await?;
        self.checkpoints.insert(checkpoint_id.clone(), metadata);

        // Clean up old checkpoints
        self.cleanup_old_checkpoints().await?;

        tracing::info!(
            "Checkpoint {} created successfully in {:?}",
            checkpoint_id,
            start_time.elapsed()
        );

        Ok(CheckpointResponse {
            checkpoint_id,
            height,
            created_at: SystemTime::now(),
            compressed: self.compression_level != Compression::none(),
            size_bytes: final_data.len() as u64,
        })
    }

    /// Restore chain state from a checkpoint
    pub async fn restore_checkpoint(
        &self,
        checkpoint_id: &str,
        verify_integrity: bool,
    ) -> NetworkResult<(ChainState, RestoreResponse)> {
        let start_time = Instant::now();

        tracing::info!("Restoring from checkpoint {}", checkpoint_id);

        // Get checkpoint metadata
        let metadata = self.checkpoints.get(checkpoint_id).ok_or_else(|| {
            NetworkError::ProtocolError {
                message: format!("Checkpoint {} not found", checkpoint_id),
            }
        })?;

        // Read checkpoint data
        let mut file = fs::File::open(&metadata.file_path).await.map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to open checkpoint file: {}", e),
            }
        })?;

        let mut data = Vec::new();
        file.read_to_end(&mut data).await.map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to read checkpoint data: {}", e),
            }
        })?;

        // Verify integrity if requested
        if verify_integrity && self.verify_integrity {
            let calculated_hash = self.calculate_hash(&data);
            if calculated_hash != metadata.integrity_hash {
                return Err(NetworkError::ProtocolError {
                    message: "Checkpoint integrity verification failed".to_string(),
                });
            }
        }

        // Decompress if needed
        let serialized_state = if metadata.compressed {
            self.decompress_data(&data)?
        } else {
            data
        };

        // Deserialize chain state
        let chain_state: ChainState = bincode::deserialize(&serialized_state).map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to deserialize chain state: {}", e),
            }
        })?;

        let restore_response = RestoreResponse {
            restored_height: metadata.height,
            restored_at: SystemTime::now(),
            verified: verify_integrity,
            blocks_restored: chain_state.block_count,
        };

        tracing::info!(
            "Checkpoint {} restored successfully in {:?}",
            checkpoint_id,
            start_time.elapsed()
        );

        Ok((chain_state, restore_response))
    }

    /// List available checkpoints
    pub fn list_checkpoints(&self) -> Vec<CheckpointInfo> {
        self.checkpoints
            .values()
            .map(|metadata| CheckpointInfo {
                checkpoint_id: metadata.checkpoint_id.clone(),
                height: metadata.height,
                state_root: metadata.state_root,
                created_at: metadata.created_at,
                size_bytes: metadata.size_bytes,
                compressed: metadata.compressed,
            })
            .collect()
    }

    /// Get checkpoint metadata
    pub fn get_checkpoint_info(&self, checkpoint_id: &str) -> Option<CheckpointInfo> {
        self.checkpoints.get(checkpoint_id).map(|metadata| CheckpointInfo {
            checkpoint_id: metadata.checkpoint_id.clone(),
            height: metadata.height,
            state_root: metadata.state_root,
            created_at: metadata.created_at,
            size_bytes: metadata.size_bytes,
            compressed: metadata.compressed,
        })
    }

    /// Delete a checkpoint
    pub async fn delete_checkpoint(&mut self, checkpoint_id: &str) -> NetworkResult<()> {
        let metadata = self.checkpoints.remove(checkpoint_id).ok_or_else(|| {
            NetworkError::ProtocolError {
                message: format!("Checkpoint {} not found", checkpoint_id),
            }
        })?;

        // Remove checkpoint file
        if metadata.file_path.exists() {
            fs::remove_file(&metadata.file_path).await.map_err(|e| {
                NetworkError::ProtocolError {
                    message: format!("Failed to delete checkpoint file: {}", e),
                }
            })?;
        }

        // Remove metadata file
        let metadata_path = self.get_metadata_path(checkpoint_id);
        if metadata_path.exists() {
            fs::remove_file(metadata_path).await.map_err(|e| {
                NetworkError::ProtocolError {
                    message: format!("Failed to delete metadata file: {}", e),
                }
            })?;
        }

        tracing::info!("Checkpoint {} deleted successfully", checkpoint_id);
        Ok(())
    }

    /// Load existing checkpoints from storage
    async fn load_existing_checkpoints(&mut self) -> NetworkResult<()> {
        let mut entries = fs::read_dir(&self.storage_path).await.map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to read checkpoint directory: {}", e),
            }
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to read directory entry: {}", e),
            }
        })? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("metadata") {
                if let Ok(metadata) = self.load_checkpoint_metadata(&path).await {
                    self.checkpoints.insert(metadata.checkpoint_id.clone(), metadata);
                }
            }
        }

        tracing::info!("Loaded {} existing checkpoints", self.checkpoints.len());
        Ok(())
    }

    /// Load checkpoint metadata from file
    async fn load_checkpoint_metadata(&self, path: &Path) -> NetworkResult<CheckpointMetadata> {
        let mut file = fs::File::open(path).await.map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to open metadata file: {}", e),
            }
        })?;

        let mut data = Vec::new();
        file.read_to_end(&mut data).await.map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to read metadata: {}", e),
            }
        })?;

        bincode::deserialize(&data).map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to deserialize metadata: {}", e),
            }
        })
    }

    /// Save checkpoint metadata to file
    async fn save_checkpoint_metadata(&self, metadata: &CheckpointMetadata) -> NetworkResult<()> {
        let metadata_path = self.get_metadata_path(&metadata.checkpoint_id);
        let serialized = bincode::serialize(metadata).map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to serialize metadata: {}", e),
            }
        })?;

        let mut file = fs::File::create(metadata_path).await.map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to create metadata file: {}", e),
            }
        })?;

        file.write_all(&serialized).await.map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Failed to write metadata: {}", e),
            }
        })?;

        Ok(())
    }

    /// Clean up old checkpoints beyond retention limit
    async fn cleanup_old_checkpoints(&mut self) -> NetworkResult<()> {
        if self.checkpoints.len() <= self.max_checkpoints as usize {
            return Ok(());
        }

        // Sort checkpoints by creation time (oldest first)
        let mut checkpoints: Vec<_> = self.checkpoints.values().collect();
        checkpoints.sort_by_key(|c| c.created_at);

        // Remove oldest checkpoints
        let to_remove = self.checkpoints.len() - self.max_checkpoints as usize;
        for metadata in checkpoints.iter().take(to_remove) {
            self.delete_checkpoint(&metadata.checkpoint_id).await?;
        }

        tracing::info!("Cleaned up {} old checkpoints", to_remove);
        Ok(())
    }

    /// Compress data using configured compression
    fn compress_data(&self, data: &[u8]) -> NetworkResult<Vec<u8>> {
        let mut encoder = GzEncoder::new(Vec::new(), self.compression_level);
        encoder.write_all(data).map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Compression failed: {}", e),
            }
        })?;

        encoder.finish().map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Compression finalization failed: {}", e),
            }
        })
    }

    /// Decompress data
    fn decompress_data(&self, data: &[u8]) -> NetworkResult<Vec<u8>> {
        let mut decoder = flate2::read::GzDecoder::new(data);
        let mut decompressed = Vec::new();
        
        std::io::Read::read_to_end(&mut decoder, &mut decompressed).map_err(|e| {
            NetworkError::ProtocolError {
                message: format!("Decompression failed: {}", e),
            }
        })?;

        Ok(decompressed)
    }

    /// Calculate integrity hash for data
    fn calculate_hash(&self, data: &[u8]) -> H256 {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(data);
        H256::from_slice(&hasher.finalize())
    }

    /// Get file path for checkpoint data
    fn get_checkpoint_path(&self, checkpoint_id: &str) -> PathBuf {
        self.storage_path.join(format!("{}.checkpoint", checkpoint_id))
    }

    /// Get file path for checkpoint metadata
    fn get_metadata_path(&self, checkpoint_id: &str) -> PathBuf {
        self.storage_path.join(format!("{}.metadata", checkpoint_id))
    }
}

/// Chain state for checkpointing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainState {
    /// Current blockchain height
    pub height: u64,
    /// State root hash
    pub state_root: H256,
    /// Block hashes for recent blocks
    pub block_hashes: Vec<(u64, H256)>,
    /// Peer synchronization states
    pub peer_states: HashMap<String, PeerCheckpointState>,
    /// Federation state
    pub federation_state: FederationCheckpointState,
    /// Block count for metrics
    pub block_count: u64,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Per-peer state in checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCheckpointState {
    pub peer_id: String,
    pub last_known_height: u64,
    pub reliability_score: f64,
    pub last_activity: SystemTime,
}

/// Federation state in checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationCheckpointState {
    pub current_authorities: Vec<String>,
    pub current_slot: u64,
    pub last_finalized_block: u64,
    pub emergency_mode: bool,
}

/// Internal checkpoint metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CheckpointMetadata {
    checkpoint_id: String,
    height: u64,
    state_root: H256,
    created_at: SystemTime,
    size_bytes: u64,
    compressed: bool,
    integrity_hash: H256,
    file_path: PathBuf,
    peer_states: HashMap<String, PeerCheckpointState>,
}

/// Public checkpoint information
#[derive(Debug, Clone)]
pub struct CheckpointInfo {
    pub checkpoint_id: String,
    pub height: u64,
    pub state_root: H256,
    pub created_at: SystemTime,
    pub size_bytes: u64,
    pub compressed: bool,
}

/// Generate unique checkpoint ID
fn generate_checkpoint_id(height: u64) -> String {
    format!("checkpoint_{}_{}", height, SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_manager() -> (CheckpointManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::new(
            temp_dir.path().to_path_buf(),
            5,
            true,
        ).await.unwrap();
        (manager, temp_dir)
    }

    fn create_test_chain_state(height: u64) -> ChainState {
        ChainState {
            height,
            state_root: H256::random(),
            block_hashes: vec![(height, H256::random())],
            peer_states: HashMap::new(),
            federation_state: FederationCheckpointState {
                current_authorities: vec!["authority1".to_string()],
                current_slot: height / 2,
                last_finalized_block: height - 1,
                emergency_mode: false,
            },
            block_count: height,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn checkpoint_creation_and_restoration() {
        let (mut manager, _temp_dir) = create_test_manager().await;
        let chain_state = create_test_chain_state(100);
        let original_state_root = chain_state.state_root;

        // Create checkpoint
        let response = manager.create_checkpoint(100, chain_state).await.unwrap();
        assert_eq!(response.height, 100);
        assert!(response.size_bytes > 0);

        // Restore checkpoint
        let (restored_state, restore_response) = manager
            .restore_checkpoint(&response.checkpoint_id, true)
            .await
            .unwrap();

        assert_eq!(restored_state.height, 100);
        assert_eq!(restored_state.state_root, original_state_root);
        assert_eq!(restore_response.restored_height, 100);
        assert!(restore_response.verified);
    }

    #[tokio::test]
    async fn checkpoint_listing() {
        let (mut manager, _temp_dir) = create_test_manager().await;

        // Create multiple checkpoints
        for height in [100, 200, 300] {
            let chain_state = create_test_chain_state(height);
            manager.create_checkpoint(height, chain_state).await.unwrap();
        }

        let checkpoints = manager.list_checkpoints();
        assert_eq!(checkpoints.len(), 3);

        let heights: Vec<u64> = checkpoints.iter().map(|c| c.height).collect();
        assert!(heights.contains(&100));
        assert!(heights.contains(&200));
        assert!(heights.contains(&300));
    }

    #[tokio::test]
    async fn checkpoint_cleanup() {
        let (mut manager, _temp_dir) = create_test_manager().await;

        // Create more checkpoints than retention limit (5)
        for height in 100..=800 {
            if height % 100 == 0 {
                let chain_state = create_test_chain_state(height);
                manager.create_checkpoint(height, chain_state).await.unwrap();
            }
        }

        // Should have cleaned up to retention limit
        let checkpoints = manager.list_checkpoints();
        assert_eq!(checkpoints.len(), 5);
    }

    #[test]
    fn checkpoint_id_generation() {
        let id1 = generate_checkpoint_id(100);
        let id2 = generate_checkpoint_id(100);
        
        // IDs should be unique even for same height
        assert_ne!(id1, id2);
        assert!(id1.starts_with("checkpoint_100_"));
        assert!(id2.starts_with("checkpoint_100_"));
    }
}