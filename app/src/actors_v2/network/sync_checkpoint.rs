//! SyncActor Checkpoint/Resume Capability
//!
//! Provides persistence for sync progress to survive node restarts.
//! Checkpoints are saved periodically during sync and loaded on startup.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::SystemTime;
use tokio::fs;

/// Sync progress checkpoint for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCheckpoint {
    /// Current blockchain height
    pub current_height: u64,

    /// Target height to sync to
    pub target_height: u64,

    /// When sync started
    pub sync_start_time: SystemTime,

    /// Total blocks synced in this session
    pub blocks_synced: u64,

    /// When this checkpoint was last saved
    pub last_checkpoint_time: SystemTime,

    /// Checkpoint version for future compatibility
    pub version: u32,
}

impl SyncCheckpoint {
    /// Checkpoint file name
    const CHECKPOINT_FILE: &'static str = "sync_checkpoint.json";

    /// Current checkpoint version
    const VERSION: u32 = 1;

    /// Create a new checkpoint
    pub fn new(current_height: u64, target_height: u64, blocks_synced: u64) -> Self {
        let now = SystemTime::now();
        Self {
            current_height,
            target_height,
            sync_start_time: now,
            blocks_synced,
            last_checkpoint_time: now,
            version: Self::VERSION,
        }
    }

    /// Save checkpoint to disk
    ///
    /// # Arguments
    /// * `data_dir` - Directory to save checkpoint file in
    ///
    /// # Errors
    /// Returns error if file write fails or JSON serialization fails
    pub async fn save(&self, data_dir: &Path) -> Result<()> {
        // Ensure data directory exists
        if !data_dir.exists() {
            tokio::fs::create_dir_all(data_dir).await?;
        }

        let checkpoint_path = data_dir.join(Self::CHECKPOINT_FILE);
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| anyhow!("Failed to serialize checkpoint: {}", e))?;

        fs::write(&checkpoint_path, json)
            .await
            .map_err(|e| anyhow!("Failed to write checkpoint to {:?}: {}", checkpoint_path, e))?;

        tracing::debug!(
            path = ?checkpoint_path,
            current_height = self.current_height,
            target_height = self.target_height,
            "Checkpoint saved"
        );

        Ok(())
    }

    /// Load checkpoint from disk
    ///
    /// # Arguments
    /// * `data_dir` - Directory containing checkpoint file
    ///
    /// # Returns
    /// * `Ok(Some(checkpoint))` - Checkpoint loaded successfully
    /// * `Ok(None)` - No checkpoint file exists
    /// * `Err(...)` - File read or parse error
    pub async fn load(data_dir: &Path) -> Result<Option<Self>> {
        let checkpoint_path = data_dir.join(Self::CHECKPOINT_FILE);

        // Check if checkpoint exists
        if !checkpoint_path.exists() {
            tracing::debug!("No sync checkpoint found");
            return Ok(None);
        }

        // Read and parse checkpoint
        let json = fs::read_to_string(&checkpoint_path)
            .await
            .map_err(|e| anyhow!("Failed to read checkpoint from {:?}: {}", checkpoint_path, e))?;

        let checkpoint: SyncCheckpoint = serde_json::from_str(&json)
            .map_err(|e| anyhow!("Failed to parse checkpoint: {}", e))?;

        // Validate checkpoint version
        if checkpoint.version != Self::VERSION {
            tracing::warn!(
                found_version = checkpoint.version,
                expected_version = Self::VERSION,
                "Checkpoint version mismatch, ignoring old checkpoint"
            );
            return Ok(None);
        }

        // Calculate checkpoint age
        let age = checkpoint
            .last_checkpoint_time
            .elapsed()
            .unwrap_or(std::time::Duration::from_secs(0));

        tracing::info!(
            current_height = checkpoint.current_height,
            target_height = checkpoint.target_height,
            blocks_synced = checkpoint.blocks_synced,
            age_secs = age.as_secs(),
            "Loaded sync checkpoint"
        );

        Ok(Some(checkpoint))
    }

    /// Delete checkpoint file
    ///
    /// Called when sync completes successfully.
    ///
    /// # Arguments
    /// * `data_dir` - Directory containing checkpoint file
    pub async fn delete(data_dir: &Path) -> Result<()> {
        let checkpoint_path = data_dir.join(Self::CHECKPOINT_FILE);

        if checkpoint_path.exists() {
            fs::remove_file(&checkpoint_path)
                .await
                .map_err(|e| anyhow!("Failed to delete checkpoint: {}", e))?;

            tracing::info!("Checkpoint deleted after sync completion");
        }

        Ok(())
    }

    /// Check if checkpoint is stale (older than threshold)
    ///
    /// Stale checkpoints may indicate an incomplete or failed sync.
    pub fn is_stale(&self, threshold: std::time::Duration) -> bool {
        self.last_checkpoint_time
            .elapsed()
            .map(|age| age > threshold)
            .unwrap_or(true)
    }

    /// Update checkpoint with new progress
    pub fn update(&mut self, current_height: u64, blocks_synced: u64) {
        self.current_height = current_height;
        self.blocks_synced = blocks_synced;
        self.last_checkpoint_time = SystemTime::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_checkpoint_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint = SyncCheckpoint::new(1000, 5000, 1000);

        // Save checkpoint
        checkpoint.save(temp_dir.path()).await.unwrap();

        // Load checkpoint
        let loaded = SyncCheckpoint::load(temp_dir.path()).await.unwrap();
        assert!(loaded.is_some());

        let loaded = loaded.unwrap();
        assert_eq!(loaded.current_height, 1000);
        assert_eq!(loaded.target_height, 5000);
        assert_eq!(loaded.blocks_synced, 1000);
    }

    #[tokio::test]
    async fn test_checkpoint_load_nonexistent() {
        let temp_dir = TempDir::new().unwrap();

        // Try to load from empty directory
        let loaded = SyncCheckpoint::load(temp_dir.path()).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_checkpoint_delete() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint = SyncCheckpoint::new(1000, 5000, 1000);

        // Save and verify exists
        checkpoint.save(temp_dir.path()).await.unwrap();
        let checkpoint_path = temp_dir.path().join(SyncCheckpoint::CHECKPOINT_FILE);
        assert!(checkpoint_path.exists());

        // Delete
        SyncCheckpoint::delete(temp_dir.path()).await.unwrap();
        assert!(!checkpoint_path.exists());
    }

    #[tokio::test]
    async fn test_checkpoint_update() {
        let mut checkpoint = SyncCheckpoint::new(1000, 5000, 1000);
        let original_time = checkpoint.last_checkpoint_time;

        // Wait a bit to ensure time changes
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Update checkpoint
        checkpoint.update(2000, 2000);

        assert_eq!(checkpoint.current_height, 2000);
        assert_eq!(checkpoint.blocks_synced, 2000);
        assert!(checkpoint.last_checkpoint_time > original_time);
    }

    #[tokio::test]
    async fn test_checkpoint_is_stale() {
        let checkpoint = SyncCheckpoint::new(1000, 5000, 1000);

        // Fresh checkpoint should not be stale
        assert!(!checkpoint.is_stale(Duration::from_secs(60)));

        // Wait and check staleness
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(checkpoint.is_stale(Duration::from_millis(50)));
    }
}
