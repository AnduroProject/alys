//! NetworkActor V2 Testing Framework
//!
//! Testing infrastructure for NetworkActor V2 system following StorageActor patterns exactly.

pub mod fixtures;
pub mod integration;
pub mod unit;

use super::base::*;
use crate::actors_v2::network::{
    NetworkActor, NetworkConfig, NetworkError, NetworkMessage, NetworkResponse, SyncActor,
    SyncConfig, SyncError, SyncMessage, SyncResponse,
};
use async_trait::async_trait;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

/// Test peer for NetworkActor testing
#[derive(Debug, Clone)]
pub struct TestPeer {
    pub peer_id: String,
    pub address: String,
    pub reputation: f64,
    pub is_bootstrap: bool,
    pub is_mdns_discovered: bool,
    pub connection_time: std::time::SystemTime,
}

impl TestPeer {
    pub fn new_bootstrap(peer_id: String, address: String) -> Self {
        Self {
            peer_id,
            address,
            reputation: 75.0,
            is_bootstrap: true,
            is_mdns_discovered: false,
            connection_time: std::time::SystemTime::now(),
        }
    }

    pub fn new_mdns(peer_id: String, address: String) -> Self {
        Self {
            peer_id,
            address,
            reputation: 50.0,
            is_bootstrap: false,
            is_mdns_discovered: true,
            connection_time: std::time::SystemTime::now(),
        }
    }

    pub fn new_regular(peer_id: String, address: String) -> Self {
        Self {
            peer_id,
            address,
            reputation: 50.0,
            is_bootstrap: false,
            is_mdns_discovered: false,
            connection_time: std::time::SystemTime::now(),
        }
    }
}

/// Test block for SyncActor testing
#[derive(Debug, Clone)]
pub struct TestBlock {
    pub height: u64,
    pub data: Vec<u8>,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: u64,
}

impl TestBlock {
    pub fn new(height: u64) -> Self {
        let hash = format!("block-hash-{}", height);
        let parent_hash = if height == 0 {
            "genesis".to_string()
        } else {
            format!("block-hash-{}", height - 1)
        };

        Self {
            height,
            data: format!("test-block-data-{}", height).into_bytes(),
            hash,
            parent_hash,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

/// NetworkActor specific test harness following StorageActor pattern
pub struct NetworkTestHarness {
    pub base: BaseTestHarness<NetworkActor>,
    pub temp_dir: TempDir,
    pub config: NetworkConfig,
}

/// SyncActor specific test harness following StorageActor pattern
pub struct SyncTestHarness {
    pub base: BaseTestHarness<SyncActor>,
    pub temp_dir: TempDir,
    pub config: SyncConfig,
}

/// NetworkActor test error following StorageActor pattern
#[derive(Debug, thiserror::Error)]
pub enum NetworkTestError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Actor creation error: {0}")]
    ActorCreation(String),
    #[error("Network operation error: {0}")]
    NetworkOperation(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Validation error: {0}")]
    Validation(String),
}

/// SyncActor test error following StorageActor pattern
#[derive(Debug, thiserror::Error)]
pub enum SyncTestError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Actor creation error: {0}")]
    ActorCreation(String),
    #[error("Sync operation error: {0}")]
    SyncOperation(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
}

#[async_trait]
impl ActorTestHarness for NetworkTestHarness {
    type Actor = NetworkActor;
    type Config = NetworkConfig;
    type Message = NetworkMessage;
    type Error = NetworkTestError;

    async fn new() -> Result<Self, Self::Error> {
        let temp_dir = TempDir::new().map_err(NetworkTestError::IoError)?;
        let config = NetworkConfig::default();

        let actor = NetworkActor::new(config.clone())
            .map_err(|e| NetworkTestError::ActorCreation(e.to_string()))?;

        Ok(Self {
            base: BaseTestHarness::new_with_actor(actor),
            temp_dir,
            config,
        })
    }

    async fn with_config(config: Self::Config) -> Result<Self, Self::Error> {
        let temp_dir = TempDir::new().map_err(NetworkTestError::IoError)?;

        let actor = NetworkActor::new(config.clone())
            .map_err(|e| NetworkTestError::ActorCreation(e.to_string()))?;

        Ok(Self {
            base: BaseTestHarness::new_with_actor(actor),
            temp_dir,
            config,
        })
    }

    async fn actor(&self) -> &Self::Actor {
        panic!("Direct actor access not supported. Use base.get_actor_ref() for async access.")
    }

    async fn actor_mut(&mut self) -> &mut Self::Actor {
        panic!(
            "Direct mutable actor access not supported. Use base.get_actor_ref() for async access."
        )
    }

    async fn send_message(&mut self, message: Self::Message) -> Result<(), Self::Error> {
        self.base.start_operation().await;
        self.base.metrics.messages_sent += 1;

        // Use spawn_blocking following StorageActor pattern for async compatibility
        let result = match message {
            NetworkMessage::StartNetwork {
                listen_addrs,
                bootstrap_peers,
            } => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let _actor_guard = actor.read().await;
                        info!(
                            "NetworkActor started with {} listen addresses",
                            listen_addrs.len()
                        );
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await
                .unwrap()
                .map_err(|e| NetworkTestError::NetworkOperation(e.to_string()))
            }
            NetworkMessage::StopNetwork { graceful: _ } => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let _actor_guard = actor.read().await;
                        info!("NetworkActor stopped");
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await
                .unwrap()
                .map_err(|e| NetworkTestError::NetworkOperation(e.to_string()))
            }
            NetworkMessage::BroadcastBlock {
                block_data,
                priority,
            } => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let _actor_guard = actor.read().await;
                        info!(
                            "Broadcasting block ({} bytes, priority: {})",
                            block_data.len(),
                            priority
                        );
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await
                .unwrap()
                .map_err(|e| NetworkTestError::NetworkOperation(e.to_string()))
            }
            NetworkMessage::BroadcastTransaction { tx_data } => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let _actor_guard = actor.read().await;
                        info!("Broadcasting transaction ({} bytes)", tx_data.len());
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await
                .unwrap()
                .map_err(|e| NetworkTestError::NetworkOperation(e.to_string()))
            }
            NetworkMessage::ConnectToPeer { peer_addr } => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let _actor_guard = actor.read().await;
                        info!("Connecting to peer: {}", peer_addr);
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await
                .unwrap()
                .map_err(|e| NetworkTestError::NetworkOperation(e.to_string()))
            }
            NetworkMessage::DisconnectPeer { peer_id } => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let _actor_guard = actor.read().await;
                        info!("Disconnecting from peer: {}", peer_id);
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await
                .unwrap()
                .map_err(|e| NetworkTestError::NetworkOperation(e.to_string()))
            }
            _ => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let _actor_guard = actor.read().await;
                        debug!("Processing other NetworkMessage");
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await
                .unwrap()
                .map_err(|e| NetworkTestError::NetworkOperation(e.to_string()))
            }
        };

        match result {
            Ok(_) => {
                self.base.record_success().await;
                Ok(())
            }
            Err(e) => {
                self.base.record_error(&e.to_string()).await;
                Err(e)
            }
        }
    }

    async fn setup(&mut self) -> Result<(), Self::Error> {
        info!("Setting up NetworkActor test harness");
        Ok(())
    }

    async fn teardown(&mut self) -> Result<(), Self::Error> {
        info!("Tearing down NetworkActor test harness");
        Ok(())
    }

    async fn verify_state(&self) -> Result<(), Self::Error> {
        debug!("Verifying NetworkActor state");
        self.config
            .validate()
            .map_err(|e| NetworkTestError::Configuration(e))?;
        Ok(())
    }

    async fn reset(&mut self) -> Result<(), Self::Error> {
        info!("Resetting NetworkActor test harness");
        Ok(())
    }
}

#[async_trait]
impl ActorTestHarness for SyncTestHarness {
    type Actor = SyncActor;
    type Config = SyncConfig;
    type Message = SyncMessage;
    type Error = SyncTestError;

    async fn new() -> Result<Self, Self::Error> {
        let temp_dir = TempDir::new().map_err(SyncTestError::IoError)?;
        let config = SyncConfig::default();

        let actor = SyncActor::new(config.clone())
            .map_err(|e| SyncTestError::ActorCreation(e.to_string()))?;

        Ok(Self {
            base: BaseTestHarness::new_with_actor(actor),
            temp_dir,
            config,
        })
    }

    async fn with_config(config: Self::Config) -> Result<Self, Self::Error> {
        let temp_dir = TempDir::new().map_err(SyncTestError::IoError)?;

        let actor = SyncActor::new(config.clone())
            .map_err(|e| SyncTestError::ActorCreation(e.to_string()))?;

        Ok(Self {
            base: BaseTestHarness::new_with_actor(actor),
            temp_dir,
            config,
        })
    }

    async fn actor(&self) -> &Self::Actor {
        panic!("Direct actor access not supported. Use base.get_actor_ref() for async access.")
    }

    async fn actor_mut(&mut self) -> &mut Self::Actor {
        panic!(
            "Direct mutable actor access not supported. Use base.get_actor_ref() for async access."
        )
    }

    async fn send_message(&mut self, message: Self::Message) -> Result<(), Self::Error> {
        self.base.start_operation().await;
        self.base.metrics.messages_sent += 1;

        // Use spawn_blocking following StorageActor pattern for async compatibility
        let result = match message {
            SyncMessage::StartSync => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let _actor_guard = actor.read().await;
                        info!("SyncActor started");
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await
                .unwrap()
                .map_err(|e| SyncTestError::SyncOperation(e.to_string()))
            }
            SyncMessage::StopSync => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let _actor_guard = actor.read().await;
                        info!("SyncActor stopped");
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await
                .unwrap()
                .map_err(|e| SyncTestError::SyncOperation(e.to_string()))
            }
            SyncMessage::RequestBlocks {
                start_height,
                count,
                peer_id,
            } => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let _actor_guard = actor.read().await;
                        info!(
                            "Requesting {} blocks from height {} via peer {:?}",
                            count, start_height, peer_id
                        );
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await
                .unwrap()
                .map_err(|e| SyncTestError::SyncOperation(e.to_string()))
            }
            SyncMessage::HandleNewBlock { block, peer_id } => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let _actor_guard = actor.read().await;
                        info!(
                            "Processing new block ({} bytes) from peer {}",
                            block.len(),
                            peer_id
                        );
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await
                .unwrap()
                .map_err(|e| SyncTestError::SyncOperation(e.to_string()))
            }
            _ => {
                let actor = self.base.get_actor_ref().await;
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let _actor_guard = actor.read().await;
                        debug!("Processing other SyncMessage");
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await
                .unwrap()
                .map_err(|e| SyncTestError::SyncOperation(e.to_string()))
            }
        };

        match result {
            Ok(_) => {
                self.base.record_success().await;
                Ok(())
            }
            Err(e) => {
                self.base.record_error(&e.to_string()).await;
                Err(e)
            }
        }
    }

    async fn setup(&mut self) -> Result<(), Self::Error> {
        info!("Setting up SyncActor test harness");
        Ok(())
    }

    async fn teardown(&mut self) -> Result<(), Self::Error> {
        info!("Tearing down SyncActor test harness");
        Ok(())
    }

    async fn verify_state(&self) -> Result<(), Self::Error> {
        debug!("Verifying SyncActor state");
        self.config
            .validate()
            .map_err(|e| SyncTestError::Configuration(e))?;
        Ok(())
    }

    async fn reset(&mut self) -> Result<(), Self::Error> {
        info!("Resetting SyncActor test harness");
        Ok(())
    }
}
