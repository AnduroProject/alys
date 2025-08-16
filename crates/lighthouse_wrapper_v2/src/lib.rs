//! Lighthouse Wrapper V2
//!
//! Enhanced integration wrapper for Lighthouse Ethereum consensus client with
//! Lighthouse v5 compatibility, improved error handling, and better integration
//! with the Alys V2 actor system architecture.

#![warn(missing_docs)]

pub mod types;
pub mod beacon;
pub mod validator;
pub mod bls;
pub mod sync;
pub mod api;
pub mod config;
pub mod error;

// Re-exports for convenience
pub use types::*;
pub use beacon::*;
pub use validator::*;
pub use bls::*;
pub use sync::*;
pub use api::*;
pub use config::*;
pub use error::*;

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::{
        LighthouseWrapper, LighthouseConfig, LighthouseError, LighthouseResult,
        BeaconClient, BeaconChainInfo, BeaconBlock,
        ValidatorClient, ValidatorInfo, ValidatorDuties,
        BlsKeyManager, BlsSignature, BlsPublicKey,
        SyncStatus, SyncInfo,
        ApiClient, ApiEndpoint, ApiResponse,
    };
    pub use async_trait::async_trait;
    pub use serde::{Deserialize, Serialize};
    pub use std::collections::HashMap;
    pub use std::sync::Arc;
    pub use std::time::{Duration, SystemTime};
    pub use tokio::sync::{mpsc, oneshot, RwLock};
    pub use tracing::{debug, error, info, trace, warn};
}

/// Lighthouse wrapper version
pub const LIGHTHOUSE_WRAPPER_VERSION: &str = "2.0.0";

/// Compatible Lighthouse versions
pub const COMPATIBLE_LIGHTHOUSE_VERSIONS: &[&str] = &["v5.0.0", "v4.6.0", "v4.5.0"];

/// Default configuration
pub fn default_config() -> LighthouseConfig {
    LighthouseConfig::default()
}

/// Main Lighthouse wrapper
pub struct LighthouseWrapper {
    config: LighthouseConfig,
    beacon_client: Arc<BeaconClient>,
    validator_client: Option<Arc<ValidatorClient>>,
    bls_keymanager: Arc<BlsKeyManager>,
    sync_manager: Arc<SyncManager>,
    api_client: Arc<ApiClient>,
}

impl LighthouseWrapper {
    /// Create new Lighthouse wrapper
    pub async fn new(config: LighthouseConfig) -> LighthouseResult<Self> {
        let api_client = Arc::new(ApiClient::new(config.beacon_node.clone()).await?);
        
        let beacon_client = Arc::new(
            BeaconClient::new(config.beacon_node.clone(), api_client.clone()).await?
        );
        
        let validator_client = if config.validator_enabled {
            Some(Arc::new(
                ValidatorClient::new(config.validator.clone(), api_client.clone()).await?
            ))
        } else {
            None
        };
        
        let bls_keymanager = Arc::new(
            BlsKeyManager::new(config.bls.clone()).await?
        );
        
        let sync_manager = Arc::new(
            SyncManager::new(config.sync.clone(), beacon_client.clone()).await?
        );
        
        Ok(Self {
            config,
            beacon_client,
            validator_client,
            bls_keymanager,
            sync_manager,
            api_client,
        })
    }
    
    /// Start the Lighthouse wrapper
    pub async fn start(&self) -> LighthouseResult<()> {
        info!("Starting Lighthouse wrapper v{}", LIGHTHOUSE_WRAPPER_VERSION);
        
        // Check Lighthouse compatibility
        self.check_lighthouse_compatibility().await?;
        
        // Start components
        self.beacon_client.start().await?;
        
        if let Some(validator_client) = &self.validator_client {
            validator_client.start().await?;
        }
        
        self.sync_manager.start().await?;
        
        info!("Lighthouse wrapper started successfully");
        Ok(())
    }
    
    /// Stop the Lighthouse wrapper
    pub async fn stop(&self) -> LighthouseResult<()> {
        info!("Stopping Lighthouse wrapper");
        
        // Stop components in reverse order
        self.sync_manager.stop().await?;
        
        if let Some(validator_client) = &self.validator_client {
            validator_client.stop().await?;
        }
        
        self.beacon_client.stop().await?;
        
        info!("Lighthouse wrapper stopped");
        Ok(())
    }
    
    /// Get beacon client
    pub fn beacon_client(&self) -> &BeaconClient {
        &self.beacon_client
    }
    
    /// Get validator client
    pub fn validator_client(&self) -> Option<&ValidatorClient> {
        self.validator_client.as_ref().map(|v| v.as_ref())
    }
    
    /// Get BLS key manager
    pub fn bls_keymanager(&self) -> &BlsKeyManager {
        &self.bls_keymanager
    }
    
    /// Get sync manager
    pub fn sync_manager(&self) -> &SyncManager {
        &self.sync_manager
    }
    
    /// Get API client
    pub fn api_client(&self) -> &ApiClient {
        &self.api_client
    }
    
    /// Check if Lighthouse is synced
    pub async fn is_synced(&self) -> LighthouseResult<bool> {
        let sync_status = self.sync_manager.get_sync_status().await?;
        Ok(matches!(sync_status.status, crate::SyncStatusType::Synced))
    }
    
    /// Get current head block
    pub async fn get_head_block(&self) -> LighthouseResult<BeaconBlock> {
        self.beacon_client.get_head_block().await
    }
    
    /// Get finalized block
    pub async fn get_finalized_block(&self) -> LighthouseResult<BeaconBlock> {
        self.beacon_client.get_finalized_block().await
    }
    
    /// Get chain info
    pub async fn get_chain_info(&self) -> LighthouseResult<BeaconChainInfo> {
        self.beacon_client.get_chain_info().await
    }
    
    /// Submit block
    pub async fn submit_block(&self, block: BeaconBlock) -> LighthouseResult<()> {
        self.beacon_client.submit_block(block).await
    }
    
    /// Get validator duties
    pub async fn get_validator_duties(&self, epoch: u64) -> LighthouseResult<Vec<ValidatorDuties>> {
        if let Some(validator_client) = &self.validator_client {
            validator_client.get_duties(epoch).await
        } else {
            Err(LighthouseError::Configuration {
                parameter: "validator_client".to_string(),
                reason: "Validator client not enabled".to_string(),
            })
        }
    }
    
    /// Sign message with BLS
    pub async fn bls_sign(&self, message: &[u8], public_key: &BlsPublicKey) -> LighthouseResult<BlsSignature> {
        self.bls_keymanager.sign(message, public_key).await
    }
    
    /// Verify BLS signature
    pub async fn bls_verify(
        &self, 
        message: &[u8], 
        signature: &BlsSignature, 
        public_key: &BlsPublicKey
    ) -> LighthouseResult<bool> {
        self.bls_keymanager.verify(message, signature, public_key).await
    }
    
    async fn check_lighthouse_compatibility(&self) -> LighthouseResult<()> {
        let version_info = self.api_client.get_version().await?;
        
        let is_compatible = COMPATIBLE_LIGHTHOUSE_VERSIONS.iter()
            .any(|v| version_info.version.contains(v));
        
        if !is_compatible {
            warn!(
                lighthouse_version = %version_info.version,
                compatible_versions = ?COMPATIBLE_LIGHTHOUSE_VERSIONS,
                "Lighthouse version may not be fully compatible"
            );
        } else {
            info!(
                lighthouse_version = %version_info.version,
                "Lighthouse version compatibility verified"
            );
        }
        
        Ok(())
    }
}

/// Version information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VersionInfo {
    /// Lighthouse version
    pub version: String,
    /// Commit hash
    pub commit: Option<String>,
    /// Build date
    pub build_date: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_lighthouse_wrapper_creation() {
        let config = LighthouseConfig::default();
        
        // This would fail in actual test without running Lighthouse
        // but shows the intended API
        match LighthouseWrapper::new(config).await {
            Ok(_wrapper) => {
                // Success case
            }
            Err(e) => {
                // Expected in test environment
                println!("Expected error in test: {}", e);
            }
        }
    }
    
    #[test]
    fn test_version_constants() {
        assert!(!LIGHTHOUSE_WRAPPER_VERSION.is_empty());
        assert!(!COMPATIBLE_LIGHTHOUSE_VERSIONS.is_empty());
    }
}