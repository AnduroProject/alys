use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Test configuration for various testing scenarios
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    /// Global test timeout
    pub timeout: Duration,
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Enable detailed logging during tests
    pub verbose_logging: bool,
    /// Enable memory profiling
    pub enable_memory_profiling: bool,
    /// Enable performance metrics collection
    pub enable_performance_metrics: bool,
    /// Custom configuration parameters
    pub custom_params: HashMap<String, String>,
    /// Test data directory
    pub test_data_dir: Option<String>,
    /// Cleanup policy
    pub cleanup_policy: CleanupPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CleanupPolicy {
    /// Always cleanup after tests
    Always,
    /// Only cleanup on success
    OnSuccess,
    /// Only cleanup on failure
    OnFailure,
    /// Never cleanup (for debugging)
    Never,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
            max_retries: 3,
            verbose_logging: false,
            enable_memory_profiling: true,
            enable_performance_metrics: true,
            custom_params: HashMap::new(),
            test_data_dir: None,
            cleanup_policy: CleanupPolicy::Always,
        }
    }
}

impl TestConfig {
    /// Create a minimal test configuration for fast unit tests
    pub fn unit_test() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            max_retries: 1,
            verbose_logging: false,
            enable_memory_profiling: false,
            enable_performance_metrics: false,
            custom_params: HashMap::new(),
            test_data_dir: None,
            cleanup_policy: CleanupPolicy::Always,
        }
    }

    /// Create a comprehensive test configuration for integration tests
    pub fn integration_test() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            max_retries: 3,
            verbose_logging: true,
            enable_memory_profiling: true,
            enable_performance_metrics: true,
            custom_params: HashMap::new(),
            test_data_dir: None,
            cleanup_policy: CleanupPolicy::Always,
        }
    }

    /// Create a configuration optimized for property-based tests
    pub fn property_test() -> Self {
        Self {
            timeout: Duration::from_secs(600),
            max_retries: 5,
            verbose_logging: false,
            enable_memory_profiling: true,
            enable_performance_metrics: true,
            custom_params: HashMap::new(),
            test_data_dir: None,
            cleanup_policy: CleanupPolicy::OnFailure,
        }
    }

    /// Create a configuration for chaos testing with extended timeouts
    pub fn chaos_test() -> Self {
        Self {
            timeout: Duration::from_secs(1800), // 30 minutes
            max_retries: 1,                     // Don't retry chaos tests
            verbose_logging: true,
            enable_memory_profiling: true,
            enable_performance_metrics: true,
            custom_params: HashMap::new(),
            test_data_dir: None,
            cleanup_policy: CleanupPolicy::Never, // Keep data for analysis
        }
    }

    /// Add a custom parameter
    pub fn with_param<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.custom_params.insert(key.into(), value.into());
        self
    }

    /// Set test data directory
    pub fn with_test_data_dir<P: Into<String>>(mut self, path: P) -> Self {
        self.test_data_dir = Some(path.into());
        self
    }

    /// Set cleanup policy
    pub fn with_cleanup_policy(mut self, policy: CleanupPolicy) -> Self {
        self.cleanup_policy = policy;
        self
    }

    /// Get a custom parameter value
    pub fn get_param(&self, key: &str) -> Option<&String> {
        self.custom_params.get(key)
    }
}
