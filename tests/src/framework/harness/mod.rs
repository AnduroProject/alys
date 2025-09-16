use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use anyhow::{Result, Context};
use tracing::{info, debug, error};

use crate::config::TestConfig;
use crate::{TestResult, TestError};

pub mod actor;
pub mod sync;
pub mod lighthouse;
pub mod governance;
pub mod network;

pub use actor::ActorTestHarness;
pub use sync::SyncTestHarness;
pub use lighthouse::LighthouseCompatHarness;
pub use governance::GovernanceIntegrationHarness;
pub use network::NetworkTestHarness;

/// Collection of specialized test harnesses for different migration components
/// 
/// Each harness focuses on testing a specific aspect of the Alys V2 migration:
/// - Actor system lifecycle and messaging
/// - Sync engine functionality and resilience
/// - Lighthouse compatibility and consensus
/// - Governance integration workflows
/// - Network communication and P2P protocols
#[derive(Debug)]
pub struct TestHarnesses {
    /// Actor system test harness
    pub actor_harness: ActorTestHarness,
    
    /// Sync engine test harness
    pub sync_harness: SyncTestHarness,
    
    /// Lighthouse compatibility test harness
    pub lighthouse_harness: LighthouseCompatHarness,
    
    /// Governance integration test harness
    pub governance_harness: GovernanceIntegrationHarness,
    
    /// Network communication test harness
    pub network_harness: NetworkTestHarness,
    
    /// Shared runtime for all harnesses
    runtime: Arc<Runtime>,
    
    /// Test configuration
    config: TestConfig,
}

impl TestHarnesses {
    /// Create a new TestHarnesses collection with shared runtime
    /// 
    /// # Arguments
    /// * `config` - Test configuration
    /// * `runtime` - Shared Tokio runtime
    /// 
    /// # Returns
    /// Result containing initialized harnesses or error
    pub fn new(config: TestConfig, runtime: Arc<Runtime>) -> Result<Self> {
        info!("Initializing test harnesses");
        
        // Initialize actor test harness
        let actor_harness = ActorTestHarness::new(
            config.actor_system.clone(),
            runtime.clone(),
        ).context("Failed to initialize actor test harness")?;
        
        // Initialize sync test harness
        let sync_harness = SyncTestHarness::new(
            config.sync.clone(),
            runtime.clone(),
        ).context("Failed to initialize sync test harness")?;
        
        // Initialize lighthouse compatibility harness
        let lighthouse_harness = LighthouseCompatHarness::new(
            config.clone(),
            runtime.clone(),
        ).context("Failed to initialize lighthouse harness")?;
        
        // Initialize governance integration harness
        let governance_harness = GovernanceIntegrationHarness::new(
            config.clone(),
            runtime.clone(),
        ).context("Failed to initialize governance harness")?;
        
        // Initialize network test harness
        let network_harness = NetworkTestHarness::new(
            config.network.clone(),
            runtime.clone(),
        ).context("Failed to initialize network harness")?;
        
        let harnesses = Self {
            actor_harness,
            sync_harness,
            lighthouse_harness,
            governance_harness,
            network_harness,
            runtime,
            config,
        };
        
        info!("All test harnesses initialized successfully");
        Ok(harnesses)
    }
    
    /// Test coordination between harnesses
    /// 
    /// Verifies that all harnesses can communicate and coordinate properly
    pub async fn test_coordination(&self) -> TestResult {
        debug!("Testing harness coordination");
        let start = std::time::Instant::now();
        
        // Test basic harness responsiveness
        let actor_ping = self.actor_harness.health_check().await;
        let sync_ping = self.sync_harness.health_check().await;
        let lighthouse_ping = self.lighthouse_harness.health_check().await;
        let governance_ping = self.governance_harness.health_check().await;
        let network_ping = self.network_harness.health_check().await;
        
        let all_healthy = actor_ping && sync_ping && lighthouse_ping && 
                         governance_ping && network_ping;
        
        let duration = start.elapsed();
        
        TestResult {
            test_name: "harness_coordination".to_string(),
            success: all_healthy,
            duration,
            message: if all_healthy {
                Some("All harnesses responding to coordination test".to_string())
            } else {
                Some("One or more harnesses failed coordination test".to_string())
            },
            metadata: [
                ("actor_health".to_string(), actor_ping.to_string()),
                ("sync_health".to_string(), sync_ping.to_string()),
                ("lighthouse_health".to_string(), lighthouse_ping.to_string()),
                ("governance_health".to_string(), governance_ping.to_string()),
                ("network_health".to_string(), network_ping.to_string()),
            ].iter().cloned().collect(),
        }
    }
    
    /// Get the count of available harnesses
    pub fn count(&self) -> usize {
        5 // actor, sync, lighthouse, governance, network
    }
    
    /// Get shared runtime reference
    pub fn runtime(&self) -> Arc<Runtime> {
        self.runtime.clone()
    }
    
    /// Get configuration reference
    pub fn config(&self) -> &TestConfig {
        &self.config
    }
    
    /// Shutdown all harnesses gracefully
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down test harnesses");
        
        // Shutdown harnesses in reverse dependency order
        self.network_harness.shutdown().await
            .context("Failed to shutdown network harness")?;
        
        self.governance_harness.shutdown().await
            .context("Failed to shutdown governance harness")?;
        
        self.lighthouse_harness.shutdown().await
            .context("Failed to shutdown lighthouse harness")?;
        
        self.sync_harness.shutdown().await
            .context("Failed to shutdown sync harness")?;
        
        self.actor_harness.shutdown().await
            .context("Failed to shutdown actor harness")?;
        
        info!("All test harnesses shut down successfully");
        Ok(())
    }
}

/// Base trait for all test harnesses
/// 
/// Provides common functionality and lifecycle management for test harnesses
pub trait TestHarness: Send + Sync {
    /// Harness name for identification
    fn name(&self) -> &str;
    
    /// Check if harness is healthy and responsive
    async fn health_check(&self) -> bool;
    
    /// Initialize the harness with given configuration
    async fn initialize(&mut self) -> Result<()>;
    
    /// Run all tests associated with this harness
    async fn run_all_tests(&self) -> Vec<TestResult>;
    
    /// Cleanup and shutdown the harness
    async fn shutdown(&self) -> Result<()>;
    
    /// Get harness-specific metrics
    async fn get_metrics(&self) -> serde_json::Value;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TestConfig;
    use std::sync::Arc;
    
    #[tokio::test]
    async fn test_harnesses_initialization() {
        let config = TestConfig::development();
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap()
        );
        
        let harnesses = TestHarnesses::new(config, runtime).unwrap();
        assert_eq!(harnesses.count(), 5);
    }
    
    #[tokio::test]
    async fn test_harness_coordination() {
        let config = TestConfig::development();
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap()
        );
        
        let harnesses = TestHarnesses::new(config, runtime).unwrap();
        let result = harnesses.test_coordination().await;
        
        assert!(result.success);
        assert_eq!(result.test_name, "harness_coordination");
    }
    
    #[tokio::test]
    async fn test_harness_shutdown() {
        let config = TestConfig::development();
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap()
        );
        
        let harnesses = TestHarnesses::new(config, runtime).unwrap();
        let result = harnesses.shutdown().await;
        
        assert!(result.is_ok());
    }
}