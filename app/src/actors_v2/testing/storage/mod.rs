pub mod unit;
pub mod integration;
pub mod property;
pub mod chaos;
pub mod fixtures;

use super::base::*;
use crate::actors_v2::storage::actor::{StorageActor, StorageConfig, AlysConsensusBlock, StorageError};
use crate::actors_v2::storage::messages::*;
use crate::actors_v2::common::StorageMessage;
use crate::auxpow_miner::BlockIndex;
use crate::block::ConvertBlockHash;
use async_trait::async_trait;
use tempfile::TempDir;
use uuid::Uuid;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, debug};

/// Storage Actor specific test harness
pub struct StorageTestHarness {
    pub base: BaseTestHarness<StorageActor>,
    pub temp_dir: TempDir,
    pub config: StorageConfig,
    pub test_blocks: Vec<AlysConsensusBlock>,
}

#[async_trait]
impl ActorTestHarness for StorageTestHarness {
    type Actor = StorageActor;
    type Config = StorageConfig;
    type Message = StorageMessage;
    type Error = StorageTestError;

    async fn new() -> Result<Self, Self::Error> {
        let temp_dir = TempDir::new().map_err(StorageTestError::IoError)?;
        let mut config = StorageConfig::default();
        config.database.main_path = temp_dir.path().join("test_storage").to_string_lossy().to_string();

        let actor = StorageActor::new(config.clone()).await
            .map_err(|e| StorageTestError::ActorCreation(e.to_string()))?;

        Ok(Self {
            base: BaseTestHarness::new_with_actor(actor),
            temp_dir,
            config,
            test_blocks: Vec::new(),
        })
    }

    async fn with_config(config: Self::Config) -> Result<Self, Self::Error> {
        let temp_dir = TempDir::new().map_err(StorageTestError::IoError)?;
        let mut test_config = config;
        test_config.database.main_path = temp_dir.path().join("test_storage").to_string_lossy().to_string();

        let actor = StorageActor::new(test_config.clone()).await
            .map_err(|e| StorageTestError::ActorCreation(e.to_string()))?;

        Ok(Self {
            base: BaseTestHarness::new_with_actor(actor),
            temp_dir,
            config: test_config,
            test_blocks: Vec::new(),
        })
    }

    async fn actor(&self) -> &Self::Actor {
        // For async access, we need to use the RwLock
        // This is a simplified interface - in practice you'd work with the Arc<RwLock<T>>
        panic!("Direct actor access not supported. Use base.get_actor_ref() for async access.")
    }

    async fn actor_mut(&mut self) -> &mut Self::Actor {
        panic!("Direct mutable actor access not supported. Use base.get_actor_ref() for async access.")
    }

    async fn send_message(&mut self, message: Self::Message) -> Result<(), Self::Error> {
        self.base.start_operation().await;
        self.base.metrics.messages_sent += 1;

        // Use spawn_blocking to avoid Send issues with RocksDB types
        let result = match message {
            StorageMessage::StoreBlock(msg) => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let mut actor_guard = actor.write().await;
                        actor_guard.store_block(msg.block, msg.canonical).await
                    })
                }).await.unwrap().map_err(|e| StorageTestError::StorageOperation(e.to_string()))
            },
            StorageMessage::GetBlock(msg) => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let mut actor_guard = actor.write().await;
                        actor_guard.get_block(&msg.block_hash).await
                    })
                }).await.unwrap().map(|_| ()).map_err(|e| StorageTestError::StorageOperation(e.to_string()))
            },
            StorageMessage::GetBlockByHeight(msg) => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let actor_guard = actor.read().await;
                        actor_guard.database.get_block_by_height(msg.height).await
                    })
                }).await.unwrap().map(|_| ()).map_err(|e| StorageTestError::StorageOperation(e.to_string()))
            },
            StorageMessage::BlockExists(msg) => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let actor_guard = actor.read().await;
                        actor_guard.database.get_block(&msg.block_hash).await
                    })
                }).await.unwrap().map(|block| block.is_some()).map(|_| ()).map_err(|e| StorageTestError::StorageOperation(e.to_string()))
            },
            StorageMessage::UpdateState(msg) => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let mut actor_guard = actor.write().await;
                        actor_guard.database.put_state(&msg.key, &msg.value).await
                    })
                }).await.unwrap().map_err(|e| StorageTestError::StorageOperation(e.to_string()))
            },
            StorageMessage::GetState(msg) => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let actor_guard = actor.read().await;
                        actor_guard.database.get_state(&msg.key).await
                    })
                }).await.unwrap().map(|_| ()).map_err(|e| StorageTestError::StorageOperation(e.to_string()))
            },
            StorageMessage::GetChainHead(_msg) => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let actor_guard = actor.read().await;
                        actor_guard.database.get_chain_head().await
                    })
                }).await.unwrap().map(|_| ()).map_err(|e| StorageTestError::StorageOperation(e.to_string()))
            },
        };

        match result {
            Ok(()) => {
                self.base.record_success().await;
                Ok(())
            },
            Err(e) => {
                self.base.record_error(&e.to_string()).await;
                Err(e)
            }
        }
    }

    async fn setup(&mut self) -> Result<(), Self::Error> {
        info!("Setting up storage test harness");

        // Initialize test data
        self.test_blocks = fixtures::create_test_block_sequence(10);

        // Setup metrics collection
        self.base.measure_memory().await;

        // Set test name in context
        self.base.set_test_name("storage_test".to_string()).await;

        debug!("Created {} test blocks for testing", self.test_blocks.len());

        Ok(())
    }

    async fn teardown(&mut self) -> Result<(), Self::Error> {
        info!("Tearing down storage test harness");

        // Metrics are automatically collected
        let metrics = self.base.get_metrics();
        debug!("Test completed with {} messages sent, {} errors",
               metrics.messages_sent, metrics.errors_encountered);

        // Cleanup is automatic with TempDir drop
        Ok(())
    }

    async fn verify_state(&self) -> Result<(), Self::Error> {
        debug!("Verifying storage actor state");

        let actor = self.base.get_actor_ref().await;
        let actor_guard = actor.read().await;

        // Verify database is accessible
        let _health = actor_guard.database.get_chain_head().await
            .map_err(|e| StorageTestError::StateVerification(format!("Database not accessible: {}", e)))?;

        // Verify cache is functioning
        // Note: This would require actual cache verification methods

        // Verify metrics are being collected
        if actor_guard.metrics.blocks_stored == 0 && self.base.get_metrics().messages_sent > 0 {
            return Err(StorageTestError::StateVerification("Metrics not being updated".to_string()));
        }

        debug!("Storage actor state verification passed");
        Ok(())
    }

    async fn reset(&mut self) -> Result<(), Self::Error> {
        info!("Resetting storage test harness");

        // Create fresh actor instance with same config
        let actor = StorageActor::new(self.config.clone()).await
            .map_err(|e| StorageTestError::ActorCreation(e.to_string()))?;

        self.base.actor = Arc::new(RwLock::new(actor));
        self.base.metrics = TestMetrics::default();

        // Reset test data
        self.test_blocks.clear();

        debug!("Storage test harness reset completed");
        Ok(())
    }
}


/// Storage test error types
#[derive(Debug, thiserror::Error)]
pub enum StorageTestError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Actor creation failed: {0}")]
    ActorCreation(String),
    #[error("Storage operation failed: {0}")]
    StorageOperation(String),
    #[error("State verification failed: {0}")]
    StateVerification(String),
    #[error("Test configuration error: {0}")]
    Configuration(String),
}

impl StorageTestHarness {
    /// Convenience method to create test storage with custom database path
    pub async fn with_temp_storage() -> Result<Self, StorageTestError> {
        Self::new().await
    }

    /// Convenience method to get storage actor metrics
    pub async fn get_storage_metrics(&self) -> Result<crate::actors_v2::storage::metrics::StorageActorMetrics, StorageTestError> {
        let actor = self.base.get_actor_ref().await;
        let actor_guard = actor.read().await;
        Ok(actor_guard.metrics.clone())
    }

    /// Add a test block to the test data set
    pub fn add_test_block(&mut self, block: AlysConsensusBlock) {
        self.test_blocks.push(block);
    }

    /// Get the number of test blocks available
    pub fn test_block_count(&self) -> usize {
        self.test_blocks.len()
    }

    /// Create a message to store a specific test block
    pub fn create_store_message(&self, index: usize, canonical: bool) -> Result<StorageMessage, StorageTestError> {
        if index >= self.test_blocks.len() {
            return Err(StorageTestError::Configuration(format!("Block index {} out of range", index)));
        }

        Ok(StorageMessage::StoreBlock(StoreBlockMessage {
            block: self.test_blocks[index].clone(),
            canonical,
            correlation_id: Some(Uuid::new_v4()),
        }))
    }

    /// Create a message to get a specific test block by hash
    pub fn create_get_message(&self, index: usize) -> Result<StorageMessage, StorageTestError> {
        if index >= self.test_blocks.len() {
            return Err(StorageTestError::Configuration(format!("Block index {} out of range", index)));
        }

        use crate::block::ConvertBlockHash;
        let block_hash = self.test_blocks[index].message.block_hash().to_block_hash();

        Ok(StorageMessage::GetBlock(GetBlockMessage {
            block_hash,
            correlation_id: Some(Uuid::new_v4()),
        }))
    }

    /// Store all test blocks in sequence
    pub async fn store_all_test_blocks(&mut self) -> Result<(), StorageTestError> {
        for (i, _) in self.test_blocks.clone().into_iter().enumerate() {
            let message = self.create_store_message(i, true)?;
            self.send_message(message).await?;
        }
        Ok(())
    }

    /// Verify that all stored blocks can be retrieved
    pub async fn verify_all_blocks_retrievable(&mut self) -> Result<(), StorageTestError> {
        for (i, _) in self.test_blocks.clone().into_iter().enumerate() {
            let message = self.create_get_message(i)?;
            self.send_message(message).await?;
        }
        Ok(())
    }
}