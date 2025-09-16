//! Configuration for the Lighthouse facade
//!
//! This module provides configuration structures for the facade layer,
//! combining settings for both v4 and v7 implementations along with
//! facade-specific options.

use crate::{
    error::{FacadeError, FacadeResult}, 
    types::{FacadeMode, JwtKey}, 
    compatibility::MigrationMode
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Main configuration for the Lighthouse facade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacadeConfig {
    /// Facade operation mode
    pub mode: FacadeMode,
    
    /// Underlying compatibility layer configuration
    pub compatibility: CompatibilityConfig,
    
    /// Facade-specific settings
    pub facade_settings: FacadeSettings,
    
    /// Health check configuration
    pub health_check: HealthCheckConfig,
    
    /// Performance tuning
    pub performance: PerformanceConfig,
    
    /// Logging configuration
    pub logging: LoggingConfig,
}

/// Facade-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacadeSettings {
    /// Enable request tracing
    pub enable_tracing: bool,
    
    /// Enable metrics collection
    pub enable_metrics: bool,
    
    /// Enable health monitoring
    pub enable_health_monitoring: bool,
    
    /// Default request timeout
    pub default_timeout: Duration,
    
    /// Maximum concurrent requests
    pub max_concurrent_requests: usize,
    
    /// Enable request caching
    pub enable_caching: bool,
    
    /// Cache TTL
    pub cache_ttl: Duration,
    
    /// Enable automatic retries
    pub enable_retries: bool,
    
    /// Maximum retry attempts
    pub max_retry_attempts: usize,
    
    /// Retry delay
    pub retry_delay: Duration,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Enable health checks
    pub enabled: bool,
    
    /// Health check interval
    pub interval: Duration,
    
    /// Health check timeout
    pub timeout: Duration,
    
    /// Failure threshold before marking unhealthy
    pub failure_threshold: usize,
    
    /// Success threshold before marking healthy
    pub success_threshold: usize,
    
    /// Enable automatic failover
    pub enable_failover: bool,
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Request queue size
    pub request_queue_size: usize,
    
    /// Worker thread count
    pub worker_threads: usize,
    
    /// Enable request prioritization
    pub enable_prioritization: bool,
    
    /// High priority threshold (ms)
    pub high_priority_threshold_ms: u64,
    
    /// Circuit breaker settings
    pub circuit_breaker: CircuitBreakerConfig,
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Enable circuit breaker
    pub enabled: bool,
    
    /// Failure rate threshold (0.0 to 1.0)
    pub failure_rate_threshold: f64,
    
    /// Minimum request count before circuit breaker activates
    pub min_request_count: usize,
    
    /// Circuit breaker timeout
    pub timeout: Duration,
    
    /// Half-open state request count
    pub half_open_request_count: usize,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: String,
    
    /// Enable structured logging
    pub structured: bool,
    
    /// Log format
    pub format: LogFormat,
    
    /// Enable request logging
    pub log_requests: bool,
    
    /// Enable response logging
    pub log_responses: bool,
    
    /// Log rotation settings
    pub rotation: LogRotationConfig,
}

/// Log format options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    /// Pretty printed logs
    Pretty,
    /// JSON formatted logs
    Json,
    /// Compact format
    Compact,
}

/// Log rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRotationConfig {
    /// Enable log rotation
    pub enabled: bool,
    
    /// Maximum file size in MB
    pub max_file_size_mb: usize,
    
    /// Maximum number of log files
    pub max_files: usize,
    
    /// Rotation interval
    pub rotation_interval: Duration,
}

/// Compatibility layer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityConfig {
    /// Version configurations
    pub versions: VersionConfigs,
    
    /// Migration settings
    pub migration: MigrationConfig,
    
    /// Observability settings
    pub observability: ObservabilityConfig,
}

/// Version-specific configurations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionConfigs {
    /// V4 configuration
    pub v4: V4Config,
    
    /// V7 configuration
    pub v7: V7Config,
    
    /// Version compatibility settings
    pub compatibility: VersionCompatibilityConfig,
}

/// Lighthouse v4 configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V4Config {
    /// Enable v4 client
    pub enabled: bool,
    
    /// Engine API execution endpoint for connecting to execution layer
    pub execution_endpoint: Option<String>,
    
    /// Engine API endpoint (deprecated, use execution_endpoint)
    pub engine_endpoint: String,
    
    /// Public API endpoint (optional)
    pub public_endpoint: Option<String>,
    
    /// JWT secret file path
    pub jwt_secret_file: String,
    
    /// JWT secret for authentication
    pub jwt_secret: Option<JwtKey>,
    
    /// Connection timeout
    pub connection_timeout: Duration,
    
    /// Request timeout
    pub request_timeout: Duration,
    
    /// Maximum retries
    pub max_retries: usize,
}

/// Lighthouse v7 configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V7Config {
    /// Enable v7 client
    pub enabled: bool,
    
    /// Engine API execution endpoint for connecting to execution layer
    pub execution_endpoint: Option<String>,
    
    /// Engine API endpoint (deprecated, use execution_endpoint)
    pub engine_endpoint: String,
    
    /// Public API endpoint (optional)
    pub public_endpoint: Option<String>,
    
    /// JWT secret file path
    pub jwt_secret_file: String,
    
    /// JWT secret for authentication
    pub jwt_secret: Option<JwtKey>,
    
    /// Connection timeout
    pub connection_timeout: Duration,
    
    /// Request timeout
    pub request_timeout: Duration,
    
    /// Maximum retries
    pub max_retries: usize,
}

/// Version compatibility settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionCompatibilityConfig {
    /// Allow lossy type conversions
    pub allow_lossy_conversions: bool,
    
    /// Strict type validation
    pub strict_types: bool,
    
    /// Default values for missing fields
    pub default_values: std::collections::HashMap<String, serde_json::Value>,
}

/// Migration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    /// Initial migration mode
    pub initial_mode: MigrationMode,
    
    /// Traffic splitting settings
    pub traffic_splitting: TrafficSplittingConfig,
    
    /// Rollback configuration
    pub rollback: RollbackConfig,
}

/// Traffic splitting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficSplittingConfig {
    /// Session timeout for sticky sessions
    pub session_timeout: Duration,
    
    /// Enable session affinity
    pub enable_session_affinity: bool,
    
    /// Hash algorithm for routing
    pub hash_algorithm: String,
}

/// Rollback configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackConfig {
    /// Enable automatic rollback
    pub enable_automatic: bool,
    
    /// Error rate threshold for automatic rollback
    pub error_rate_threshold: f64,
    
    /// Time window for rollback decision
    pub decision_window: Duration,
    
    /// Minimum requests before rollback decision
    pub min_requests: usize,
}

/// Observability configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Metrics configuration
    pub metrics: MetricsConfig,
    
    /// Tracing configuration
    pub tracing: TracingConfig,
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,
    
    /// Metrics collection interval
    pub collection_interval: Duration,
    
    /// Prometheus configuration
    pub prometheus: PrometheusConfig,
    
    /// Custom metrics
    pub custom_metrics: Vec<String>,
}

/// Prometheus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusConfig {
    /// Enable Prometheus exports
    pub enabled: bool,
    
    /// Prometheus endpoint
    pub endpoint: String,
    
    /// Metrics namespace
    pub namespace: String,
    
    /// Additional labels
    pub labels: std::collections::HashMap<String, String>,
}

/// Tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// Enable distributed tracing
    pub enabled: bool,
    
    /// Tracing endpoint
    pub endpoint: String,
    
    /// Sample rate (0.0 to 1.0)
    pub sample_rate: f64,
    
    /// Service name
    pub service_name: String,
}

impl Default for FacadeConfig {
    fn default() -> Self {
        Self {
            mode: FacadeMode::default(),
            compatibility: CompatibilityConfig::default(),
            facade_settings: FacadeSettings::default(),
            health_check: HealthCheckConfig::default(),
            performance: PerformanceConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for FacadeSettings {
    fn default() -> Self {
        Self {
            enable_tracing: true,
            enable_metrics: true,
            enable_health_monitoring: true,
            default_timeout: Duration::from_secs(30),
            max_concurrent_requests: 100,
            enable_caching: false,
            cache_ttl: Duration::from_secs(60),
            enable_retries: true,
            max_retry_attempts: 3,
            retry_delay: Duration::from_millis(100),
        }
    }
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
            failure_threshold: 3,
            success_threshold: 2,
            enable_failover: true,
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            request_queue_size: 1000,
            worker_threads: num_cpus::get(),
            enable_prioritization: false,
            high_priority_threshold_ms: 100,
            circuit_breaker: CircuitBreakerConfig::default(),
        }
    }
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            failure_rate_threshold: 0.5,
            min_request_count: 10,
            timeout: Duration::from_secs(60),
            half_open_request_count: 3,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            structured: true,
            format: LogFormat::Pretty,
            log_requests: true,
            log_responses: false, // Can be verbose
            rotation: LogRotationConfig::default(),
        }
    }
}

impl Default for LogRotationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_file_size_mb: 100,
            max_files: 10,
            rotation_interval: Duration::from_secs(24 * 3600), // 24 hours
        }
    }
}

impl FacadeConfig {
    /// Validate the configuration
    pub fn validate(&self) -> FacadeResult<()> {
        // Validate facade settings
        if self.facade_settings.default_timeout.as_secs() == 0 {
            return Err(FacadeError::InvalidConfiguration {
                parameter: "facade_settings.default_timeout".to_string(),
                reason: "Timeout cannot be zero".to_string(),
            });
        }
        
        if self.facade_settings.max_concurrent_requests == 0 {
            return Err(FacadeError::InvalidConfiguration {
                parameter: "facade_settings.max_concurrent_requests".to_string(),
                reason: "Must allow at least one concurrent request".to_string(),
            });
        }
        
        // Validate health check settings
        if self.health_check.enabled {
            if self.health_check.interval.as_secs() == 0 {
                return Err(FacadeError::InvalidConfiguration {
                    parameter: "health_check.interval".to_string(),
                    reason: "Health check interval cannot be zero".to_string(),
                });
            }
            
            if self.health_check.failure_threshold == 0 {
                return Err(FacadeError::InvalidConfiguration {
                    parameter: "health_check.failure_threshold".to_string(),
                    reason: "Failure threshold must be greater than zero".to_string(),
                });
            }
        }
        
        // Validate performance settings
        if self.performance.request_queue_size == 0 {
            return Err(FacadeError::InvalidConfiguration {
                parameter: "performance.request_queue_size".to_string(),
                reason: "Request queue size must be greater than zero".to_string(),
            });
        }
        
        if self.performance.worker_threads == 0 {
            return Err(FacadeError::InvalidConfiguration {
                parameter: "performance.worker_threads".to_string(),
                reason: "Must have at least one worker thread".to_string(),
            });
        }
        
        // Validate circuit breaker
        let cb = &self.performance.circuit_breaker;
        if cb.enabled {
            if cb.failure_rate_threshold < 0.0 || cb.failure_rate_threshold > 1.0 {
                return Err(FacadeError::InvalidConfiguration {
                    parameter: "performance.circuit_breaker.failure_rate_threshold".to_string(),
                    reason: "Must be between 0.0 and 1.0".to_string(),
                });
            }
            
            if cb.min_request_count == 0 {
                return Err(FacadeError::InvalidConfiguration {
                    parameter: "performance.circuit_breaker.min_request_count".to_string(),
                    reason: "Must be greater than zero".to_string(),
                });
            }
        }
        
        // Validate underlying compatibility config
        self.compatibility.validate()?;
        
        Ok(())
    }
    
    /// Create configuration for development
    pub fn development() -> Self {
        let mut config = Self::default();
        config.logging.level = "debug".to_string();
        config.facade_settings.enable_tracing = true;
        config.health_check.interval = Duration::from_secs(10);
        config.performance.circuit_breaker.enabled = false; // Disable for development
        config
    }
    
    /// Create configuration for production
    pub fn production() -> Self {
        let mut config = Self::default();
        config.logging.level = "info".to_string();
        config.facade_settings.enable_caching = true;
        config.facade_settings.cache_ttl = Duration::from_secs(300); // 5 minutes
        config.performance.circuit_breaker.enabled = true;
        config.health_check.enable_failover = true;
        config
    }
}

impl Default for CompatibilityConfig {
    fn default() -> Self {
        Self {
            versions: VersionConfigs::default(),
            migration: MigrationConfig::default(),
            observability: ObservabilityConfig::default(),
        }
    }
}

impl Default for VersionConfigs {
    fn default() -> Self {
        Self {
            v4: V4Config::default(),
            v7: V7Config::default(),
            compatibility: VersionCompatibilityConfig::default(),
        }
    }
}

impl Default for V4Config {
    fn default() -> Self {
        Self {
            enabled: true,
            execution_endpoint: Some("http://localhost:8551".to_string()),
            engine_endpoint: "http://localhost:8551".to_string(),
            public_endpoint: Some("http://localhost:5052".to_string()),
            jwt_secret_file: "./jwt.hex".to_string(),
            jwt_secret: None,
            connection_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            max_retries: 3,
        }
    }
}

impl Default for V7Config {
    fn default() -> Self {
        Self {
            enabled: false, // v7 disabled by default
            execution_endpoint: Some("http://localhost:8561".to_string()),
            engine_endpoint: "http://localhost:8561".to_string(),
            public_endpoint: Some("http://localhost:5062".to_string()),
            jwt_secret_file: "./jwt.hex".to_string(),
            jwt_secret: None,
            connection_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            max_retries: 3,
        }
    }
}

impl Default for VersionCompatibilityConfig {
    fn default() -> Self {
        Self {
            allow_lossy_conversions: true,
            strict_types: false,
            default_values: std::collections::HashMap::new(),
        }
    }
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            initial_mode: MigrationMode::V4Only,
            traffic_splitting: TrafficSplittingConfig::default(),
            rollback: RollbackConfig::default(),
        }
    }
}

impl Default for TrafficSplittingConfig {
    fn default() -> Self {
        Self {
            session_timeout: Duration::from_secs(3600), // 1 hour
            enable_session_affinity: true,
            hash_algorithm: "siphasher".to_string(),
        }
    }
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            enable_automatic: true,
            error_rate_threshold: 0.1, // 10% error rate
            decision_window: Duration::from_secs(300), // 5 minutes
            min_requests: 20,
        }
    }
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            metrics: MetricsConfig::default(),
            tracing: TracingConfig::default(),
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval: Duration::from_secs(30),
            prometheus: PrometheusConfig::default(),
            custom_metrics: vec![],
        }
    }
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoint: "/metrics".to_string(),
            namespace: "lighthouse_facade".to_string(),
            labels: std::collections::HashMap::new(),
        }
    }
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.1, // 10% sampling
            service_name: "lighthouse_facade".to_string(),
        }
    }
}

impl CompatibilityConfig {
    /// Validate the compatibility configuration
    pub fn validate(&self) -> FacadeResult<()> {
        // Ensure at least one version is enabled
        if !self.versions.v4.enabled && !self.versions.v7.enabled {
            return Err(FacadeError::Configuration {
                parameter: "versions".to_string(),
                reason: "At least one version must be enabled".to_string(),
            });
        }
        
        // Validate migration mode compatibility
        match self.migration.initial_mode {
            MigrationMode::V4Only if !self.versions.v4.enabled => {
                return Err(FacadeError::Configuration {
                    parameter: "migration.initial_mode".to_string(),
                    reason: "V4Only mode requires v4 to be enabled".to_string(),
                });
            }
            MigrationMode::V7Only if !self.versions.v7.enabled => {
                return Err(FacadeError::Configuration {
                    parameter: "migration.initial_mode".to_string(),
                    reason: "V7Only mode requires v7 to be enabled".to_string(),
                });
            }
            MigrationMode::Parallel | MigrationMode::V4Primary | MigrationMode::V7Primary 
                if !self.versions.v4.enabled || !self.versions.v7.enabled => {
                return Err(FacadeError::Configuration {
                    parameter: "migration.initial_mode".to_string(),
                    reason: "Dual-version modes require both v4 and v7 to be enabled".to_string(),
                });
            }
            _ => {}
        }
        
        // Validate rollback settings
        let rollback = &self.migration.rollback;
        if rollback.enable_automatic {
            if rollback.error_rate_threshold < 0.0 || rollback.error_rate_threshold > 1.0 {
                return Err(FacadeError::Configuration {
                    parameter: "migration.rollback.error_rate_threshold".to_string(),
                    reason: "Must be between 0.0 and 1.0".to_string(),
                });
            }
        }
        
        // Validate metrics settings
        let metrics = &self.observability.metrics;
        if metrics.enabled && metrics.prometheus.enabled {
            if metrics.prometheus.endpoint.is_empty() {
                return Err(FacadeError::Configuration {
                    parameter: "observability.metrics.prometheus.endpoint".to_string(),
                    reason: "Prometheus endpoint cannot be empty when enabled".to_string(),
                });
            }
        }
        
        // Validate tracing settings  
        let tracing = &self.observability.tracing;
        if tracing.enabled {
            if tracing.sample_rate < 0.0 || tracing.sample_rate > 1.0 {
                return Err(FacadeError::Configuration {
                    parameter: "observability.tracing.sample_rate".to_string(),
                    reason: "Sample rate must be between 0.0 and 1.0".to_string(),
                });
            }
        }
        
        Ok(())
    }
}