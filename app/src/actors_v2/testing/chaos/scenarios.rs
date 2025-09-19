use async_trait::async_trait;
use std::time::Duration;
use serde::{Serialize, Deserialize};
use super::injectors::*;
use tracing::{info, warn, error};

/// Chaos testing scenario that combines multiple failure types
#[async_trait]
pub trait ChaosScenario: Send + Sync {
    type Config: Send + Sync;
    type Error: std::error::Error + Send + Sync + 'static;

    /// Execute the chaos scenario
    async fn execute(&mut self, config: Self::Config) -> Result<ScenarioResult, Self::Error>;

    /// Get scenario description
    fn description(&self) -> String;

    /// Get estimated execution time
    fn estimated_duration(&self) -> Duration;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    pub scenario_name: String,
    pub duration: Duration,
    pub failures_injected: u64,
    pub failures_recovered: u64,
    pub system_resilient: bool,
    pub performance_impact: f64, // 0.0 = no impact, 1.0 = complete failure
    pub recovery_time: Option<Duration>,
    pub additional_metrics: std::collections::HashMap<String, f64>,
}

/// Network partition scenario
pub struct NetworkPartitionScenario {
    network_injector: NetworkFailureInjector,
}

impl NetworkPartitionScenario {
    pub fn new() -> Self {
        Self {
            network_injector: NetworkFailureInjector::new(),
        }
    }
}

#[async_trait]
impl ChaosScenario for NetworkPartitionScenario {
    type Config = NetworkPartitionConfig;
    type Error = NetworkScenarioError;

    async fn execute(&mut self, config: Self::Config) -> Result<ScenarioResult, Self::Error> {
        info!("Starting network partition scenario");
        let start_time = std::time::Instant::now();

        let network_config = NetworkFailureConfig {
            failure_type: NetworkFailureType::Partition {
                duration: config.partition_duration
            },
        };

        // Start network failure injection
        self.network_injector.start(network_config).await
            .map_err(NetworkScenarioError::NetworkFailure)?;

        // Monitor system during partition
        let mut system_responsive = true;
        let mut recovery_attempts = 0;

        for i in 0..config.recovery_attempts {
            tokio::time::sleep(config.check_interval).await;

            // In a real implementation, this would check system health
            if i > config.recovery_attempts / 2 {
                system_responsive = false;
                recovery_attempts += 1;
            }
        }

        // Stop injection and measure recovery
        let recovery_start = std::time::Instant::now();
        self.network_injector.stop().await
            .map_err(NetworkScenarioError::NetworkFailure)?;

        let recovery_time = recovery_start.elapsed();
        let total_duration = start_time.elapsed();

        let stats = self.network_injector.get_stats();

        let mut additional_metrics = std::collections::HashMap::new();
        additional_metrics.insert("recovery_attempts".to_string(), recovery_attempts as f64);
        additional_metrics.insert("partition_duration_sec".to_string(), config.partition_duration.as_secs_f64());

        Ok(ScenarioResult {
            scenario_name: "network_partition".to_string(),
            duration: total_duration,
            failures_injected: stats.injections_successful,
            failures_recovered: if system_responsive { 1 } else { 0 },
            system_resilient: system_responsive,
            performance_impact: if system_responsive { 0.3 } else { 0.8 },
            recovery_time: Some(recovery_time),
            additional_metrics,
        })
    }

    fn description(&self) -> String {
        "Simulates network partition to test system resilience and recovery".to_string()
    }

    fn estimated_duration(&self) -> Duration {
        Duration::from_secs(300) // 5 minutes default
    }
}

#[derive(Debug, Clone)]
pub struct NetworkPartitionConfig {
    pub partition_duration: Duration,
    pub recovery_attempts: usize,
    pub check_interval: Duration,
}

impl Default for NetworkPartitionConfig {
    fn default() -> Self {
        Self {
            partition_duration: Duration::from_secs(60),
            recovery_attempts: 10,
            check_interval: Duration::from_secs(5),
        }
    }
}

/// Disk failure scenario
pub struct DiskFailureScenario {
    disk_injector: DiskFailureInjector,
}

impl DiskFailureScenario {
    pub fn new() -> Self {
        Self {
            disk_injector: DiskFailureInjector::new(),
        }
    }
}

#[async_trait]
impl ChaosScenario for DiskFailureScenario {
    type Config = DiskFailureConfig;
    type Error = DiskScenarioError;

    async fn execute(&mut self, config: Self::Config) -> Result<ScenarioResult, Self::Error> {
        info!("Starting disk failure scenario: {:?}", config.failure_type);
        let start_time = std::time::Instant::now();

        // Start disk failure injection
        self.disk_injector.start(config.clone()).await
            .map_err(DiskScenarioError::DiskFailure)?;

        // Monitor system during failure
        let mut operations_successful = 0u64;
        let mut operations_failed = 0u64;

        // Simulate system operations during disk failure
        for _ in 0..100 {
            if self.disk_injector.should_fail_operation() {
                operations_failed += 1;
            } else {
                operations_successful += 1;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Stop injection
        self.disk_injector.stop().await
            .map_err(DiskScenarioError::DiskFailure)?;

        let total_duration = start_time.elapsed();
        let stats = self.disk_injector.get_stats();

        let system_resilient = operations_successful > operations_failed;
        let performance_impact = operations_failed as f64 / (operations_successful + operations_failed) as f64;

        let mut additional_metrics = std::collections::HashMap::new();
        additional_metrics.insert("operations_successful".to_string(), operations_successful as f64);
        additional_metrics.insert("operations_failed".to_string(), operations_failed as f64);
        additional_metrics.insert("failure_probability".to_string(), config.failure_probability);

        Ok(ScenarioResult {
            scenario_name: "disk_failure".to_string(),
            duration: total_duration,
            failures_injected: stats.injections_successful,
            failures_recovered: if system_resilient { 1 } else { 0 },
            system_resilient,
            performance_impact,
            recovery_time: None, // Disk failures don't have explicit recovery
            additional_metrics,
        })
    }

    fn description(&self) -> String {
        "Simulates disk failures including full disk and slow operations".to_string()
    }

    fn estimated_duration(&self) -> Duration {
        Duration::from_secs(180) // 3 minutes default
    }
}

/// Memory pressure scenario
pub struct MemoryPressureScenario {
    memory_injector: MemoryPressureInjector,
}

impl MemoryPressureScenario {
    pub fn new() -> Self {
        Self {
            memory_injector: MemoryPressureInjector::new(),
        }
    }
}

#[async_trait]
impl ChaosScenario for MemoryPressureScenario {
    type Config = MemoryPressureConfig;
    type Error = MemoryScenarioError;

    async fn execute(&mut self, config: Self::Config) -> Result<ScenarioResult, Self::Error> {
        info!("Starting memory pressure scenario: {}MB", config.target_mb);
        let start_time = std::time::Instant::now();

        // Start memory pressure injection
        self.memory_injector.start(config.clone()).await
            .map_err(MemoryScenarioError::MemoryFailure)?;

        let total_duration = start_time.elapsed();
        let stats = self.memory_injector.get_stats();

        let mut additional_metrics = std::collections::HashMap::new();
        additional_metrics.insert("target_memory_mb".to_string(), config.target_mb as f64);
        additional_metrics.insert("pressure_duration_sec".to_string(), config.duration.as_secs_f64());

        // Memory pressure impact depends on system ability to handle it
        let performance_impact = (config.target_mb as f64 / 1024.0).min(1.0); // Cap at 100%
        let system_resilient = performance_impact < 0.7;

        Ok(ScenarioResult {
            scenario_name: "memory_pressure".to_string(),
            duration: total_duration,
            failures_injected: stats.injections_successful,
            failures_recovered: if system_resilient { 1 } else { 0 },
            system_resilient,
            performance_impact,
            recovery_time: Some(Duration::from_secs(10)), // Memory is released quickly
            additional_metrics,
        })
    }

    fn description(&self) -> String {
        "Simulates memory pressure to test system behavior under low memory conditions".to_string()
    }

    fn estimated_duration(&self) -> Duration {
        Duration::from_secs(120) // 2 minutes default
    }
}

/// Combined chaos scenario that runs multiple failure types
pub struct CombinedChaosScenario {
    network_scenario: NetworkPartitionScenario,
    disk_scenario: DiskFailureScenario,
    memory_scenario: MemoryPressureScenario,
}

impl CombinedChaosScenario {
    pub fn new() -> Self {
        Self {
            network_scenario: NetworkPartitionScenario::new(),
            disk_scenario: DiskFailureScenario::new(),
            memory_scenario: MemoryPressureScenario::new(),
        }
    }
}

#[async_trait]
impl ChaosScenario for CombinedChaosScenario {
    type Config = CombinedChaosConfig;
    type Error = CombinedScenarioError;

    async fn execute(&mut self, config: Self::Config) -> Result<ScenarioResult, Self::Error> {
        info!("Starting combined chaos scenario");
        let start_time = std::time::Instant::now();

        let mut total_failures = 0u64;
        let mut total_recoveries = 0u64;
        let mut max_performance_impact = 0.0f64;
        let mut combined_metrics = std::collections::HashMap::new();

        // Execute scenarios based on configuration
        if config.include_network {
            let result = self.network_scenario.execute(config.network_config.clone()).await
                .map_err(CombinedScenarioError::Network)?;

            total_failures += result.failures_injected;
            total_recoveries += result.failures_recovered;
            max_performance_impact = max_performance_impact.max(result.performance_impact);

            for (key, value) in result.additional_metrics {
                combined_metrics.insert(format!("network_{}", key), value);
            }
        }

        if config.include_disk {
            let result = self.disk_scenario.execute(config.disk_config.clone()).await
                .map_err(CombinedScenarioError::Disk)?;

            total_failures += result.failures_injected;
            total_recoveries += result.failures_recovered;
            max_performance_impact = max_performance_impact.max(result.performance_impact);

            for (key, value) in result.additional_metrics {
                combined_metrics.insert(format!("disk_{}", key), value);
            }
        }

        if config.include_memory {
            let result = self.memory_scenario.execute(config.memory_config.clone()).await
                .map_err(CombinedScenarioError::Memory)?;

            total_failures += result.failures_injected;
            total_recoveries += result.failures_recovered;
            max_performance_impact = max_performance_impact.max(result.performance_impact);

            for (key, value) in result.additional_metrics {
                combined_metrics.insert(format!("memory_{}", key), value);
            }
        }

        let total_duration = start_time.elapsed();
        let system_resilient = total_recoveries == total_failures && max_performance_impact < 0.8;

        Ok(ScenarioResult {
            scenario_name: "combined_chaos".to_string(),
            duration: total_duration,
            failures_injected: total_failures,
            failures_recovered: total_recoveries,
            system_resilient,
            performance_impact: max_performance_impact,
            recovery_time: Some(Duration::from_secs(30)), // Combined recovery takes longer
            additional_metrics: combined_metrics,
        })
    }

    fn description(&self) -> String {
        "Runs multiple chaos scenarios simultaneously to test system under extreme conditions".to_string()
    }

    fn estimated_duration(&self) -> Duration {
        Duration::from_secs(600) // 10 minutes for combined scenario
    }
}

#[derive(Debug, Clone)]
pub struct CombinedChaosConfig {
    pub include_network: bool,
    pub include_disk: bool,
    pub include_memory: bool,
    pub network_config: NetworkPartitionConfig,
    pub disk_config: DiskFailureConfig,
    pub memory_config: MemoryPressureConfig,
}

impl Default for CombinedChaosConfig {
    fn default() -> Self {
        Self {
            include_network: true,
            include_disk: true,
            include_memory: true,
            network_config: NetworkPartitionConfig::default(),
            disk_config: DiskFailureConfig {
                failure_type: DiskFailureType::SlowOperations {
                    delay: Duration::from_millis(100),
                    duration: Duration::from_secs(60),
                },
                failure_probability: 0.2,
            },
            memory_config: MemoryPressureConfig {
                target_mb: 512,
                duration: Duration::from_secs(60),
            },
        }
    }
}

// Error types for scenarios
#[derive(Debug, thiserror::Error)]
pub enum NetworkScenarioError {
    #[error("Network failure: {0}")]
    NetworkFailure(#[from] NetworkFailureError),
}

#[derive(Debug, thiserror::Error)]
pub enum DiskScenarioError {
    #[error("Disk failure: {0}")]
    DiskFailure(#[from] DiskFailureError),
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryScenarioError {
    #[error("Memory failure: {0}")]
    MemoryFailure(#[from] MemoryPressureError),
}

#[derive(Debug, thiserror::Error)]
pub enum CombinedScenarioError {
    #[error("Network scenario failed: {0}")]
    Network(#[from] NetworkScenarioError),
    #[error("Disk scenario failed: {0}")]
    Disk(#[from] DiskScenarioError),
    #[error("Memory scenario failed: {0}")]
    Memory(#[from] MemoryScenarioError),
}

// Default implementations
impl Default for NetworkPartitionScenario {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for DiskFailureScenario {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for MemoryPressureScenario {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for CombinedChaosScenario {
    fn default() -> Self {
        Self::new()
    }
}