use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use std::path::PathBuf;
use std::collections::HashMap;
use tokio::runtime::Runtime;
use anyhow::{Result, Context};
use tracing::{info, debug, error, warn};

pub mod harness;
pub mod validators;
pub mod generators;
pub mod chaos;
pub mod performance;
pub mod metrics;
pub mod config;

pub use config::TestConfig;
pub use harness::TestHarnesses;
pub use validators::Validators;
pub use metrics::MetricsCollector;

/// Master test framework for migration testing
/// 
/// Central orchestrator for all testing activities during the V2 migration process.
/// Manages runtime, configuration, test harnesses, validators, and metrics collection.
pub struct MigrationTestFramework {
    /// Shared Tokio runtime for all test operations
    runtime: Arc<Runtime>,
    /// Test configuration settings
    config: TestConfig,
    /// Collection of specialized test harnesses
    harnesses: TestHarnesses,
    /// Test result validators
    validators: Validators,
    /// Metrics collection and reporting system
    metrics: MetricsCollector,
    /// Framework start time for duration tracking
    start_time: SystemTime,
}

/// Migration phases that can be validated
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MigrationPhase {
    Foundation,
    ActorCore,
    SyncImprovement,
    LighthouseMigration,
    GovernanceIntegration,
}

/// Validation result for a migration phase
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub phase: MigrationPhase,
    pub success: bool,
    pub duration: Duration,
    pub test_results: Vec<TestResult>,
    pub metrics: TestMetrics,
    pub errors: Vec<TestError>,
}

/// Individual test result
#[derive(Debug, Clone)]
pub struct TestResult {
    pub test_name: String,
    pub success: bool,
    pub duration: Duration,
    pub message: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// Test metrics collected during execution
#[derive(Debug, Clone)]
pub struct TestMetrics {
    pub total_tests: u32,
    pub passed_tests: u32,
    pub failed_tests: u32,
    pub total_duration: Duration,
    pub average_duration: Duration,
    pub memory_usage: u64,
    pub cpu_usage: f64,
}

/// Test execution errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum TestError {
    #[error("Runtime initialization failed: {0}")]
    RuntimeInit(String),
    #[error("Harness setup failed: {0}")]
    HarnessSetup(String),
    #[error("Test execution failed: {message}")]
    TestExecution { message: String },
    #[error("Validation failed: {message}")]
    ValidationFailed { message: String },
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Resource allocation failed: {0}")]
    ResourceAllocation(String),
}

impl MigrationTestFramework {
    /// Create a new MigrationTestFramework instance
    /// 
    /// # Arguments
    /// * `config` - Test configuration settings
    /// 
    /// # Returns
    /// Result containing the initialized framework or an error
    pub fn new(config: TestConfig) -> Result<Self> {
        info!("Initializing MigrationTestFramework");
        
        // Create multi-threaded Tokio runtime with 8 worker threads
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(8)
                .thread_name("migration-test")
                .enable_all()
                .build()
                .context("Failed to initialize Tokio runtime")?
        );
        
        debug!("Tokio runtime initialized with 8 worker threads");
        
        // Initialize harnesses with shared runtime
        let harnesses = TestHarnesses::new(config.clone(), runtime.clone())
            .context("Failed to initialize test harnesses")?;
        
        // Initialize validators
        let validators = Validators::new()
            .context("Failed to initialize validators")?;
        
        // Initialize metrics collector
        let metrics = MetricsCollector::new(config.clone())
            .context("Failed to initialize metrics collector")?;
        
        let framework = Self {
            runtime,
            config,
            harnesses,
            validators,
            metrics,
            start_time: SystemTime::now(),
        };
        
        info!("MigrationTestFramework initialized successfully");
        Ok(framework)
    }
    
    /// Run validation for a specific migration phase
    /// 
    /// # Arguments
    /// * `phase` - The migration phase to validate
    /// 
    /// # Returns
    /// ValidationResult containing test results and metrics
    pub async fn run_phase_validation(&self, phase: MigrationPhase) -> ValidationResult {
        let start = Instant::now();
        info!("Starting validation for phase: {:?}", phase);
        
        // Record phase validation start
        self.metrics.record_phase_start(phase.clone()).await;
        
        // Run tests specific to migration phase
        let results = match phase {
            MigrationPhase::Foundation => self.validate_foundation().await,
            MigrationPhase::ActorCore => self.validate_actor_core().await,
            MigrationPhase::SyncImprovement => self.validate_sync().await,
            MigrationPhase::LighthouseMigration => self.validate_lighthouse().await,
            MigrationPhase::GovernanceIntegration => self.validate_governance().await,
        };
        
        let duration = start.elapsed();
        
        // Collect metrics for this phase
        let phase_metrics = self.metrics.collect_phase_metrics(&phase).await;
        
        // Record phase validation completion
        self.metrics.record_phase_completion(phase.clone(), duration, &results).await;
        
        info!("Phase {:?} validation completed in {:?}", phase, duration);
        
        ValidationResult {
            phase: phase.clone(),
            success: results.iter().all(|r| r.success),
            duration,
            test_results: results,
            metrics: phase_metrics,
            errors: vec![], // TODO: Collect actual errors during execution
        }
    }
    
    /// Validate foundation infrastructure
    async fn validate_foundation(&self) -> Vec<TestResult> {
        info!("Validating foundation infrastructure");
        let mut results = Vec::new();
        
        // Test framework initialization
        results.push(TestResult {
            test_name: "framework_initialization".to_string(),
            success: true,
            duration: Duration::from_millis(10),
            message: Some("Framework initialized successfully".to_string()),
            metadata: HashMap::new(),
        });
        
        // Test configuration validation
        results.push(TestResult {
            test_name: "configuration_validation".to_string(),
            success: self.config.validate(),
            duration: Duration::from_millis(5),
            message: Some("Configuration validated".to_string()),
            metadata: HashMap::new(),
        });
        
        // Test harness coordination
        results.push(self.harnesses.test_coordination().await);
        
        // Test metrics collection
        results.push(self.metrics.test_collection().await);
        
        results
    }
    
    /// Validate actor core system
    async fn validate_actor_core(&self) -> Vec<TestResult> {
        info!("Validating actor core system");
        let mut results = Vec::new();
        
        // Run actor lifecycle tests
        results.extend(
            self.harnesses
                .actor_harness
                .run_lifecycle_tests()
                .await
        );
        
        // Run message ordering tests
        results.extend(
            self.harnesses
                .actor_harness
                .run_message_ordering_tests()
                .await
        );
        
        // Run recovery tests
        results.extend(
            self.harnesses
                .actor_harness
                .run_recovery_tests()
                .await
        );
        
        results
    }
    
    /// Validate sync improvements
    async fn validate_sync(&self) -> Vec<TestResult> {
        info!("Validating sync improvements");
        let mut results = Vec::new();
        
        // Run full sync tests
        results.extend(
            self.harnesses
                .sync_harness
                .run_full_sync_tests()
                .await
        );
        
        // Run sync resilience tests
        results.extend(
            self.harnesses
                .sync_harness
                .run_resilience_tests()
                .await
        );
        
        // Run parallel sync tests
        results.extend(
            self.harnesses
                .sync_harness
                .run_parallel_sync_tests()
                .await
        );
        
        results
    }
    
    /// Validate lighthouse migration
    async fn validate_lighthouse(&self) -> Vec<TestResult> {
        info!("Validating lighthouse migration");
        let mut results = Vec::new();
        
        // Run lighthouse compatibility tests
        results.extend(
            self.harnesses
                .lighthouse_harness
                .run_compatibility_tests()
                .await
        );
        
        // Run consensus integration tests
        results.extend(
            self.harnesses
                .lighthouse_harness
                .run_consensus_integration_tests()
                .await
        );
        
        results
    }
    
    /// Validate governance integration
    async fn validate_governance(&self) -> Vec<TestResult> {
        info!("Validating governance integration");
        let mut results = Vec::new();
        
        // Run governance workflow tests
        results.extend(
            self.harnesses
                .governance_harness
                .run_workflow_tests()
                .await
        );
        
        // Run signature validation tests
        results.extend(
            self.harnesses
                .governance_harness
                .run_signature_validation_tests()
                .await
        );
        
        results
    }
    
    /// Collect comprehensive metrics from all components
    pub async fn collect_metrics(&self) -> TestMetrics {
        self.metrics.collect_comprehensive_metrics().await
    }
    
    /// Get the shared runtime for external use
    pub fn runtime(&self) -> Arc<Runtime> {
        self.runtime.clone()
    }
    
    /// Get framework configuration
    pub fn config(&self) -> &TestConfig {
        &self.config
    }
    
    /// Get test harnesses for direct access
    pub fn harnesses(&self) -> &TestHarnesses {
        &self.harnesses
    }
    
    /// Gracefully shutdown the framework and cleanup resources
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down MigrationTestFramework");
        
        // Shutdown harnesses first
        self.harnesses.shutdown().await?;
        
        // Collect final metrics
        let final_metrics = self.collect_metrics().await;
        info!("Final test metrics: {:?}", final_metrics);
        
        // Shutdown metrics collector
        self.metrics.shutdown().await?;
        
        info!("MigrationTestFramework shutdown completed");
        Ok(())
    }
}

impl Drop for MigrationTestFramework {
    fn drop(&mut self) {
        debug!("MigrationTestFramework dropping, runtime cleanup will be handled by Arc");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    fn create_test_config() -> TestConfig {
        TestConfig::development()
    }
    
    #[tokio::test]
    async fn test_framework_initialization() {
        let config = create_test_config();
        let framework = MigrationTestFramework::new(config).unwrap();
        
        assert_eq!(framework.harnesses.count(), 5);
        assert!(framework.config.parallel_tests);
    }
    
    #[tokio::test]
    async fn test_foundation_validation() {
        let config = create_test_config();
        let framework = MigrationTestFramework::new(config).unwrap();
        
        let result = framework.run_phase_validation(MigrationPhase::Foundation).await;
        
        assert!(result.success);
        assert!(result.test_results.len() > 0);
        assert_eq!(result.phase, MigrationPhase::Foundation);
    }
    
    #[tokio::test]
    async fn test_metrics_collection() {
        let config = create_test_config();
        let framework = MigrationTestFramework::new(config).unwrap();
        
        let metrics = framework.collect_metrics().await;
        
        assert_eq!(metrics.total_tests, 0); // No tests run yet
    }
    
    #[tokio::test]
    async fn test_graceful_shutdown() {
        let config = create_test_config();
        let framework = MigrationTestFramework::new(config).unwrap();
        
        let result = framework.shutdown().await;
        assert!(result.is_ok());
    }
}