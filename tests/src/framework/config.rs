use std::path::PathBuf;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Test configuration for the migration testing framework
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    /// Enable parallel test execution
    pub parallel_tests: bool,
    
    /// Enable chaos testing
    pub chaos_enabled: bool,
    
    /// Enable performance tracking
    pub performance_tracking: bool,
    
    /// Enable code coverage collection
    pub coverage_enabled: bool,
    
    /// Path to Docker Compose file for test environment
    pub docker_compose_file: String,
    
    /// Directory for test data and temporary files
    pub test_data_dir: PathBuf,
    
    /// Network configuration
    pub network: NetworkConfig,
    
    /// Actor system configuration
    pub actor_system: ActorSystemConfig,
    
    /// Sync testing configuration
    pub sync: SyncConfig,
    
    /// Performance testing configuration
    pub performance: PerformanceConfig,
    
    /// Chaos testing configuration
    pub chaos: ChaosConfig,
}

/// Network testing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Maximum number of peers for network tests
    pub max_peers: usize,
    
    /// Network latency simulation (milliseconds)
    pub latency_ms: u64,
    
    /// Network failure rate (0.0 to 1.0)
    pub failure_rate: f64,
    
    /// Enable network partitioning tests
    pub partition_enabled: bool,
}

/// Actor system testing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorSystemConfig {
    /// Maximum number of test actors
    pub max_actors: usize,
    
    /// Message timeout (milliseconds)
    pub message_timeout_ms: u64,
    
    /// Supervision restart strategy
    pub restart_strategy: RestartStrategy,
    
    /// Enable actor lifecycle testing
    pub lifecycle_testing: bool,
    
    /// Enable message ordering verification
    pub message_ordering_verification: bool,
}

/// Actor restart strategies for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RestartStrategy {
    /// Always restart failed actors
    Always,
    /// Never restart failed actors
    Never,
    /// Restart with exponential backoff
    ExponentialBackoff { max_retries: u32 },
}

/// Sync testing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Maximum chain height for sync tests
    pub max_chain_height: u64,
    
    /// Block generation rate (blocks per second)
    pub block_rate: f64,
    
    /// Checkpoint interval for sync validation
    pub checkpoint_interval: u64,
    
    /// Enable full sync testing
    pub full_sync_enabled: bool,
    
    /// Enable parallel sync testing
    pub parallel_sync_enabled: bool,
    
    /// Sync timeout (seconds)
    pub sync_timeout_seconds: u64,
}

/// Performance testing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable memory profiling
    pub memory_profiling: bool,
    
    /// Enable CPU profiling
    pub cpu_profiling: bool,
    
    /// Benchmark iterations
    pub benchmark_iterations: u32,
    
    /// Performance regression threshold (percentage)
    pub regression_threshold: f64,
    
    /// Enable flamegraph generation
    pub flamegraph_enabled: bool,
}

/// Chaos testing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosConfig {
    /// Enable network chaos
    pub network_chaos: bool,
    
    /// Enable resource chaos (memory, CPU, disk)
    pub resource_chaos: bool,
    
    /// Enable Byzantine behavior simulation
    pub byzantine_chaos: bool,
    
    /// Chaos event frequency (events per minute)
    pub event_frequency: f64,
    
    /// Duration of chaos tests (minutes)
    pub test_duration_minutes: u32,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            parallel_tests: true,
            chaos_enabled: false,
            performance_tracking: true,
            coverage_enabled: true,
            docker_compose_file: "docker-compose.test.yml".to_string(),
            test_data_dir: PathBuf::from("/tmp/alys-test-data"),
            network: NetworkConfig::default(),
            actor_system: ActorSystemConfig::default(),
            sync: SyncConfig::default(),
            performance: PerformanceConfig::default(),
            chaos: ChaosConfig::default(),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            max_peers: 50,
            latency_ms: 100,
            failure_rate: 0.01,
            partition_enabled: true,
        }
    }
}

impl Default for ActorSystemConfig {
    fn default() -> Self {
        Self {
            max_actors: 1000,
            message_timeout_ms: 5000,
            restart_strategy: RestartStrategy::ExponentialBackoff { max_retries: 3 },
            lifecycle_testing: true,
            message_ordering_verification: true,
        }
    }
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            max_chain_height: 10000,
            block_rate: 0.5, // 0.5 blocks per second (2 second block time)
            checkpoint_interval: 100,
            full_sync_enabled: true,
            parallel_sync_enabled: true,
            sync_timeout_seconds: 300, // 5 minutes
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            memory_profiling: true,
            cpu_profiling: true,
            benchmark_iterations: 100,
            regression_threshold: 10.0, // 10% regression threshold
            flamegraph_enabled: true,
        }
    }
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            network_chaos: true,
            resource_chaos: true,
            byzantine_chaos: false, // Disabled by default for safety
            event_frequency: 2.0, // 2 chaos events per minute
            test_duration_minutes: 10,
        }
    }
}

impl TestConfig {
    /// Create a new TestConfig from environment variables and defaults
    pub fn new() -> Result<Self> {
        let mut config = Self::default();
        
        // Override with environment variables if present
        if let Ok(parallel) = std::env::var("TEST_PARALLEL") {
            config.parallel_tests = parallel.parse()
                .context("Failed to parse TEST_PARALLEL")?;
        }
        
        if let Ok(chaos) = std::env::var("TEST_CHAOS_ENABLED") {
            config.chaos_enabled = chaos.parse()
                .context("Failed to parse TEST_CHAOS_ENABLED")?;
        }
        
        if let Ok(perf) = std::env::var("TEST_PERFORMANCE_TRACKING") {
            config.performance_tracking = perf.parse()
                .context("Failed to parse TEST_PERFORMANCE_TRACKING")?;
        }
        
        if let Ok(coverage) = std::env::var("TEST_COVERAGE_ENABLED") {
            config.coverage_enabled = coverage.parse()
                .context("Failed to parse TEST_COVERAGE_ENABLED")?;
        }
        
        if let Ok(compose_file) = std::env::var("TEST_DOCKER_COMPOSE_FILE") {
            config.docker_compose_file = compose_file;
        }
        
        if let Ok(test_dir) = std::env::var("TEST_DATA_DIR") {
            config.test_data_dir = PathBuf::from(test_dir);
        }
        
        // Ensure test data directory exists
        std::fs::create_dir_all(&config.test_data_dir)
            .context("Failed to create test data directory")?;
        
        info!("Test configuration initialized: {:?}", config);
        Ok(config)
    }
    
    /// Load configuration from a TOML file
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .context("Failed to read config file")?;
        
        let config: TestConfig = toml::from_str(&content)
            .context("Failed to parse config file")?;
        
        // Ensure test data directory exists
        std::fs::create_dir_all(&config.test_data_dir)
            .context("Failed to create test data directory")?;
        
        info!("Test configuration loaded from file: {:?}", path);
        Ok(config)
    }
    
    /// Save configuration to a TOML file
    pub fn save_to_file(&self, path: &PathBuf) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        std::fs::write(path, content)
            .context("Failed to write config file")?;
        
        info!("Test configuration saved to file: {:?}", path);
        Ok(())
    }
    
    /// Validate the configuration
    pub fn validate(&self) -> bool {
        let mut valid = true;
        
        // Validate test data directory
        if !self.test_data_dir.exists() {
            warn!("Test data directory does not exist: {:?}", self.test_data_dir);
            valid = false;
        }
        
        // Validate Docker Compose file
        if !PathBuf::from(&self.docker_compose_file).exists() {
            warn!("Docker Compose file does not exist: {}", self.docker_compose_file);
        }
        
        // Validate network configuration
        if self.network.failure_rate < 0.0 || self.network.failure_rate > 1.0 {
            warn!("Invalid network failure rate: {}", self.network.failure_rate);
            valid = false;
        }
        
        // Validate sync configuration
        if self.sync.block_rate <= 0.0 {
            warn!("Invalid block rate: {}", self.sync.block_rate);
            valid = false;
        }
        
        if self.sync.checkpoint_interval == 0 {
            warn!("Invalid checkpoint interval: {}", self.sync.checkpoint_interval);
            valid = false;
        }
        
        // Validate performance configuration
        if self.performance.regression_threshold <= 0.0 {
            warn!("Invalid regression threshold: {}", self.performance.regression_threshold);
            valid = false;
        }
        
        // Validate chaos configuration
        if self.chaos.event_frequency <= 0.0 {
            warn!("Invalid chaos event frequency: {}", self.chaos.event_frequency);
            valid = false;
        }
        
        if valid {
            info!("Configuration validation passed");
        } else {
            warn!("Configuration validation failed");
        }
        
        valid
    }
    
    /// Get the full path to a test data file
    pub fn test_data_path(&self, filename: &str) -> PathBuf {
        self.test_data_dir.join(filename)
    }
    
    /// Create a configuration for development/debugging
    pub fn development() -> Self {
        let mut config = Self::default();
        config.parallel_tests = false; // Easier debugging
        config.chaos_enabled = false; // No chaos during development
        config.performance_tracking = false; // Skip perf overhead
        config.coverage_enabled = false; // Skip coverage overhead
        config.test_data_dir = PathBuf::from("/tmp/alys-dev-test");
        
        // Reduce test load for development
        config.sync.max_chain_height = 100;
        config.actor_system.max_actors = 10;
        config.performance.benchmark_iterations = 1;
        
        config
    }
    
    /// Create a configuration for CI/CD environments
    pub fn ci_cd() -> Self {
        let mut config = Self::default();
        config.parallel_tests = true; // Fast execution
        config.chaos_enabled = true; // Full testing
        config.performance_tracking = true; // Track regressions
        config.coverage_enabled = true; // Collect coverage
        config.test_data_dir = PathBuf::from("/tmp/alys-ci-test");
        
        // Optimize for CI environment
        config.sync.sync_timeout_seconds = 180; // Shorter timeout
        config.chaos.test_duration_minutes = 5; // Shorter chaos tests
        
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_default_config() {
        let config = TestConfig::default();
        assert!(config.parallel_tests);
        assert!(!config.chaos_enabled);
        assert!(config.performance_tracking);
        assert!(config.coverage_enabled);
    }
    
    #[test]
    fn test_config_validation() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = TestConfig::default();
        config.test_data_dir = temp_dir.path().to_path_buf();
        
        assert!(config.validate());
        
        // Test invalid configuration
        config.network.failure_rate = 2.0; // Invalid rate > 1.0
        assert!(!config.validate());
    }
    
    #[test]
    fn test_development_config() {
        let config = TestConfig::development();
        assert!(!config.parallel_tests);
        assert!(!config.chaos_enabled);
        assert!(!config.performance_tracking);
        assert_eq!(config.sync.max_chain_height, 100);
    }
    
    #[test]
    fn test_ci_cd_config() {
        let config = TestConfig::ci_cd();
        assert!(config.parallel_tests);
        assert!(config.chaos_enabled);
        assert!(config.performance_tracking);
        assert_eq!(config.sync.sync_timeout_seconds, 180);
    }
    
    #[test]
    fn test_config_serialization() {
        let config = TestConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: TestConfig = toml::from_str(&toml_str).unwrap();
        
        assert_eq!(config.parallel_tests, deserialized.parallel_tests);
        assert_eq!(config.chaos_enabled, deserialized.chaos_enabled);
    }
}