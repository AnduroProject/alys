//! Configuration management for the Lighthouse compatibility layer
//!
//! This module provides comprehensive configuration management for the migration
//! process, including version selection, A/B testing parameters, health monitoring
//! thresholds, and rollback criteria.

use crate::error::{CompatError, CompatResult};
use crate::compat::MigrationMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Main configuration for the compatibility layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatConfig {
    /// Version-specific configurations
    pub versions: VersionConfig,
    
    /// Migration control settings
    pub migration: MigrationConfig,
    
    /// A/B testing configuration
    pub ab_testing: ABTestingConfig,
    
    /// Health monitoring settings
    pub health: HealthConfig,
    
    /// Performance monitoring settings
    pub performance: PerformanceConfig,
    
    /// Rollback configuration
    pub rollback: RollbackConfig,
    
    /// Logging and metrics
    pub observability: ObservabilityConfig,
}

/// Version-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionConfig {
    /// Lighthouse v4 configuration
    pub v4: V4Config,
    
    /// Lighthouse v5 configuration
    pub v5: V5Config,
    
    /// Default version to use when starting
    pub default_version: DefaultVersion,
    
    /// Feature compatibility settings
    pub compatibility: CompatibilityConfig,
}

/// Lighthouse v4 specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V4Config {
    /// Enable v4 client
    pub enabled: bool,
    
    /// Git revision to use
    pub revision: String,
    
    /// HTTP endpoint for Engine API
    pub engine_endpoint: String,
    
    /// Public HTTP endpoint
    pub public_endpoint: Option<String>,
    
    /// JWT secret file path
    pub jwt_secret_file: PathBuf,
    
    /// Connection timeout
    pub timeout: Duration,
    
    /// Additional v4-specific settings
    pub extra_config: HashMap<String, serde_json::Value>,
}

/// Lighthouse v5 specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V5Config {
    /// Enable v5 client
    pub enabled: bool,
    
    /// Version tag to use
    pub version: String,
    
    /// HTTP endpoint for Engine API
    pub engine_endpoint: String,
    
    /// Public HTTP endpoint
    pub public_endpoint: Option<String>,
    
    /// JWT secret file path
    pub jwt_secret_file: PathBuf,
    
    /// Connection timeout
    pub timeout: Duration,
    
    /// Enable new v5 features
    pub enable_deneb_features: bool,
    
    /// Additional v5-specific settings
    pub extra_config: HashMap<String, serde_json::Value>,
}

/// Default version selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DefaultVersion {
    /// Use v4 by default
    V4,
    /// Use v5 by default
    V5,
    /// Automatic selection based on availability
    Auto,
}

/// Feature compatibility configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityConfig {
    /// Strict type checking
    pub strict_types: bool,
    
    /// Allow lossy conversions
    pub allow_lossy_conversions: bool,
    
    /// Default values for missing fields
    pub default_values: HashMap<String, serde_json::Value>,
    
    /// Feature toggles
    pub feature_flags: HashMap<String, bool>,
}

/// Migration control configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    /// Initial migration mode
    pub initial_mode: MigrationMode,
    
    /// Migration strategy
    pub strategy: MigrationStrategy,
    
    /// Phase durations
    pub phase_durations: PhaseDurations,
    
    /// Traffic splitting configuration
    pub traffic_splitting: TrafficSplittingConfig,
    
    /// Automatic progression settings
    pub auto_progression: AutoProgressionConfig,
}

/// Migration strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationStrategy {
    /// Manual progression through phases
    Manual,
    /// Automatic progression based on metrics
    Automatic,
    /// Time-based progression
    TimeBased,
    /// Canary deployment with gradual rollout
    Canary { initial_percentage: u8, step_size: u8 },
}

/// Duration settings for migration phases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseDurations {
    /// Parallel testing phase duration
    pub parallel_testing: Duration,
    
    /// Canary phase duration
    pub canary: Duration,
    
    /// Gradual rollout phase duration per step
    pub gradual_step: Duration,
    
    /// Observation period after each phase
    pub observation: Duration,
}

/// Traffic splitting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficSplittingConfig {
    /// Use sticky sessions
    pub sticky_sessions: bool,
    
    /// Session timeout for sticky sessions
    pub session_timeout: Duration,
    
    /// Hash algorithm for session assignment
    pub hash_algorithm: HashAlgorithm,
    
    /// Custom routing rules
    pub routing_rules: Vec<RoutingRule>,
}

/// Hash algorithm for traffic splitting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HashAlgorithm {
    /// SipHash 2-4
    SipHash24,
    /// SHA-256
    Sha256,
    /// FxHash
    FxHash,
}

/// Custom routing rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    /// Rule name
    pub name: String,
    
    /// Condition for applying the rule
    pub condition: String,
    
    /// Target version for matching requests
    pub target_version: DefaultVersion,
    
    /// Rule priority (higher = applied first)
    pub priority: u8,
}

/// Automatic progression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoProgressionConfig {
    /// Enable automatic progression
    pub enabled: bool,
    
    /// Success criteria for progression
    pub success_criteria: SuccessCriteria,
    
    /// Failure criteria for rollback
    pub failure_criteria: FailureCriteria,
    
    /// Minimum observation period before progression
    pub min_observation_time: Duration,
    
    /// Required sample size for statistical significance
    pub min_sample_size: u64,
}

/// Success criteria for automatic progression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCriteria {
    /// Maximum allowed error rate
    pub max_error_rate: f64,
    
    /// Maximum allowed latency increase
    pub max_latency_increase: f64,
    
    /// Minimum success rate
    pub min_success_rate: f64,
    
    /// Required confidence level
    pub confidence_level: f64,
}

/// Failure criteria for automatic rollback
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureCriteria {
    /// Maximum acceptable error rate
    pub max_error_rate: f64,
    
    /// Maximum acceptable latency increase
    pub max_latency_increase: f64,
    
    /// Maximum memory usage increase
    pub max_memory_increase: f64,
    
    /// Consecutive failure threshold
    pub consecutive_failures: u32,
}

/// A/B testing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ABTestingConfig {
    /// Enable A/B testing
    pub enabled: bool,
    
    /// Test configurations
    pub tests: HashMap<String, ABTestConfig>,
    
    /// Statistical analysis settings
    pub statistics: StatisticsConfig,
    
    /// Data retention settings
    pub data_retention: DataRetentionConfig,
}

/// Individual A/B test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ABTestConfig {
    /// Test name
    pub name: String,
    
    /// Test description
    pub description: String,
    
    /// Percentage of traffic to route to v5
    pub v5_percentage: u8,
    
    /// Test duration
    pub duration: Duration,
    
    /// Start time (None = start immediately)
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    
    /// Use sticky sessions
    pub sticky_sessions: bool,
    
    /// Metrics to collect
    pub metrics: Vec<String>,
    
    /// Success criteria
    pub success_criteria: SuccessCriteria,
}

/// Statistical analysis configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticsConfig {
    /// Significance level (alpha)
    pub significance_level: f64,
    
    /// Statistical power (1 - beta)
    pub power: f64,
    
    /// Minimum effect size to detect
    pub min_effect_size: f64,
    
    /// Confidence interval level
    pub confidence_level: f64,
}

/// Data retention configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRetentionConfig {
    /// How long to keep raw test data
    pub raw_data_retention: Duration,
    
    /// How long to keep aggregated metrics
    pub aggregated_data_retention: Duration,
    
    /// Data export settings
    pub export_settings: Option<DataExportConfig>,
}

/// Data export configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataExportConfig {
    /// Export format
    pub format: ExportFormat,
    
    /// Export destination
    pub destination: PathBuf,
    
    /// Automatic export interval
    pub export_interval: Duration,
}

/// Data export format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportFormat {
    /// JSON format
    Json,
    /// CSV format
    Csv,
    /// Parquet format
    Parquet,
}

/// Health monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    /// Enable health monitoring
    pub enabled: bool,
    
    /// Health check interval
    pub check_interval: Duration,
    
    /// Health check timeout
    pub check_timeout: Duration,
    
    /// Individual health checks
    pub checks: Vec<HealthCheckConfig>,
    
    /// Alerting configuration
    pub alerting: AlertingConfig,
}

/// Individual health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Check name
    pub name: String,
    
    /// Check type
    pub check_type: HealthCheckType,
    
    /// Check parameters
    pub parameters: HashMap<String, serde_json::Value>,
    
    /// Failure threshold
    pub failure_threshold: u32,
    
    /// Recovery threshold
    pub recovery_threshold: u32,
    
    /// Enable alerting for this check
    pub alert_on_failure: bool,
}

/// Types of health checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthCheckType {
    /// HTTP endpoint health check
    HttpEndpoint,
    /// Response time check
    ResponseTime,
    /// Error rate check
    ErrorRate,
    /// Memory usage check
    MemoryUsage,
    /// CPU usage check
    CpuUsage,
    /// Consensus consistency check
    ConsensusConsistency,
    /// Custom check
    Custom { script_path: PathBuf },
}

/// Alerting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertingConfig {
    /// Enable alerting
    pub enabled: bool,
    
    /// Alert destinations
    pub destinations: Vec<AlertDestination>,
    
    /// Alert throttling settings
    pub throttling: AlertThrottlingConfig,
}

/// Alert destination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertDestination {
    /// Log alert
    Log { level: String },
    
    /// Email alert
    Email { 
        smtp_server: String, 
        recipients: Vec<String> 
    },
    
    /// Slack webhook
    Slack { webhook_url: String },
    
    /// Custom webhook
    Webhook { 
        url: String, 
        headers: HashMap<String, String> 
    },
}

/// Alert throttling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThrottlingConfig {
    /// Minimum time between alerts of the same type
    pub min_interval: Duration,
    
    /// Maximum alerts per time window
    pub max_alerts_per_window: u32,
    
    /// Time window for alert limiting
    pub time_window: Duration,
}

/// Performance monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable performance monitoring
    pub enabled: bool,
    
    /// Metrics collection interval
    pub collection_interval: Duration,
    
    /// Performance thresholds
    pub thresholds: PerformanceThresholds,
    
    /// Benchmarking settings
    pub benchmarking: BenchmarkingConfig,
}

/// Performance thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceThresholds {
    /// Maximum acceptable latency
    pub max_latency: Duration,
    
    /// Maximum acceptable error rate
    pub max_error_rate: f64,
    
    /// Maximum memory usage
    pub max_memory_mb: u64,
    
    /// Maximum CPU usage
    pub max_cpu_usage: f64,
    
    /// Minimum throughput
    pub min_throughput: f64,
}

/// Benchmarking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkingConfig {
    /// Enable automatic benchmarking
    pub enabled: bool,
    
    /// Benchmark interval
    pub interval: Duration,
    
    /// Benchmark duration
    pub duration: Duration,
    
    /// Load generation settings
    pub load_generation: LoadGenerationConfig,
}

/// Load generation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadGenerationConfig {
    /// Target requests per second
    pub target_rps: f64,
    
    /// Request pattern
    pub pattern: RequestPattern,
    
    /// Payload size range
    pub payload_size_range: (u32, u32),
}

/// Request pattern for load generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestPattern {
    /// Constant rate
    Constant,
    /// Stepped increase
    Stepped { step_size: f64, step_duration: Duration },
    /// Ramp up
    Ramp { start_rps: f64, end_rps: f64 },
    /// Spike testing
    Spike { base_rps: f64, spike_rps: f64, spike_duration: Duration },
}

/// Rollback configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackConfig {
    /// Enable automatic rollback
    pub automatic: bool,
    
    /// Maximum rollback time
    pub max_rollback_time: Duration,
    
    /// Rollback triggers
    pub triggers: RollbackTriggers,
    
    /// Rollback verification steps
    pub verification: RollbackVerificationConfig,
}

/// Rollback trigger configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackTriggers {
    /// Error rate threshold
    pub error_rate_threshold: f64,
    
    /// Latency threshold
    pub latency_threshold: Duration,
    
    /// Consecutive failure threshold
    pub consecutive_failures: u32,
    
    /// Memory usage threshold
    pub memory_threshold_mb: u64,
    
    /// Custom trigger conditions
    pub custom_conditions: Vec<String>,
}

/// Rollback verification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackVerificationConfig {
    /// Enable rollback verification
    pub enabled: bool,
    
    /// Verification timeout
    pub timeout: Duration,
    
    /// Post-rollback health checks
    pub health_checks: Vec<String>,
    
    /// Performance validation
    pub performance_validation: bool,
}

/// Observability configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Logging configuration
    pub logging: LoggingConfig,
    
    /// Metrics configuration
    pub metrics: MetricsConfig,
    
    /// Tracing configuration
    pub tracing: TracingConfig,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: String,
    
    /// Log format
    pub format: LogFormat,
    
    /// Log output destinations
    pub outputs: Vec<LogOutput>,
}

/// Log format options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    /// Human readable format
    Human,
    /// JSON format
    Json,
    /// Compact format
    Compact,
}

/// Log output destination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogOutput {
    /// Standard output
    Stdout,
    /// Standard error
    Stderr,
    /// File output
    File { path: PathBuf },
    /// Syslog
    Syslog,
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,
    
    /// Prometheus endpoint
    pub prometheus_endpoint: Option<String>,
    
    /// Metrics namespace
    pub namespace: String,
    
    /// Custom metrics
    pub custom_metrics: Vec<CustomMetricConfig>,
}

/// Custom metric configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomMetricConfig {
    /// Metric name
    pub name: String,
    
    /// Metric type
    pub metric_type: MetricType,
    
    /// Metric description
    pub description: String,
    
    /// Metric labels
    pub labels: Vec<String>,
}

/// Metric types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricType {
    /// Counter metric
    Counter,
    /// Gauge metric
    Gauge,
    /// Histogram metric
    Histogram,
    /// Summary metric
    Summary,
}

/// Tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// Enable distributed tracing
    pub enabled: bool,
    
    /// Tracing service endpoint
    pub endpoint: Option<String>,
    
    /// Service name
    pub service_name: String,
    
    /// Sampling rate
    pub sampling_rate: f64,
}

impl Default for CompatConfig {
    fn default() -> Self {
        Self {
            versions: VersionConfig::default(),
            migration: MigrationConfig::default(),
            ab_testing: ABTestingConfig::default(),
            health: HealthConfig::default(),
            performance: PerformanceConfig::default(),
            rollback: RollbackConfig::default(),
            observability: ObservabilityConfig::default(),
        }
    }
}

impl Default for VersionConfig {
    fn default() -> Self {
        Self {
            v4: V4Config::default(),
            v5: V5Config::default(),
            default_version: DefaultVersion::V4,
            compatibility: CompatibilityConfig::default(),
        }
    }
}

impl Default for V4Config {
    fn default() -> Self {
        Self {
            enabled: true,
            revision: "441fc16".to_string(),
            engine_endpoint: "http://localhost:8551".to_string(),
            public_endpoint: Some("http://localhost:8545".to_string()),
            jwt_secret_file: PathBuf::from("jwt.hex"),
            timeout: Duration::from_secs(30),
            extra_config: HashMap::new(),
        }
    }
}

impl Default for V5Config {
    fn default() -> Self {
        Self {
            enabled: false,
            version: "v5.0.0".to_string(),
            engine_endpoint: "http://localhost:8552".to_string(),
            public_endpoint: Some("http://localhost:8546".to_string()),
            jwt_secret_file: PathBuf::from("jwt.hex"),
            timeout: Duration::from_secs(30),
            enable_deneb_features: true,
            extra_config: HashMap::new(),
        }
    }
}

impl Default for CompatibilityConfig {
    fn default() -> Self {
        Self {
            strict_types: false,
            allow_lossy_conversions: true,
            default_values: HashMap::new(),
            feature_flags: HashMap::new(),
        }
    }
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            initial_mode: MigrationMode::V4Only,
            strategy: MigrationStrategy::Manual,
            phase_durations: PhaseDurations::default(),
            traffic_splitting: TrafficSplittingConfig::default(),
            auto_progression: AutoProgressionConfig::default(),
        }
    }
}

impl Default for PhaseDurations {
    fn default() -> Self {
        Self {
            parallel_testing: Duration::from_hours(2),
            canary: Duration::from_hours(6),
            gradual_step: Duration::from_hours(2),
            observation: Duration::from_minutes(30),
        }
    }
}

impl Default for TrafficSplittingConfig {
    fn default() -> Self {
        Self {
            sticky_sessions: true,
            session_timeout: Duration::from_hours(24),
            hash_algorithm: HashAlgorithm::SipHash24,
            routing_rules: Vec::new(),
        }
    }
}

impl Default for AutoProgressionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            success_criteria: SuccessCriteria::default(),
            failure_criteria: FailureCriteria::default(),
            min_observation_time: Duration::from_minutes(30),
            min_sample_size: 1000,
        }
    }
}

impl Default for SuccessCriteria {
    fn default() -> Self {
        Self {
            max_error_rate: 0.01, // 1%
            max_latency_increase: 0.1, // 10%
            min_success_rate: 0.99, // 99%
            confidence_level: 0.95, // 95%
        }
    }
}

impl Default for FailureCriteria {
    fn default() -> Self {
        Self {
            max_error_rate: 0.05, // 5%
            max_latency_increase: 0.5, // 50%
            max_memory_increase: 0.3, // 30%
            consecutive_failures: 5,
        }
    }
}

impl Default for ABTestingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            tests: HashMap::new(),
            statistics: StatisticsConfig::default(),
            data_retention: DataRetentionConfig::default(),
        }
    }
}

impl Default for StatisticsConfig {
    fn default() -> Self {
        Self {
            significance_level: 0.05,
            power: 0.8,
            min_effect_size: 0.1,
            confidence_level: 0.95,
        }
    }
}

impl Default for DataRetentionConfig {
    fn default() -> Self {
        Self {
            raw_data_retention: Duration::from_days(30),
            aggregated_data_retention: Duration::from_days(365),
            export_settings: None,
        }
    }
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval: Duration::from_secs(30),
            check_timeout: Duration::from_secs(10),
            checks: vec![
                HealthCheckConfig {
                    name: "endpoint_health".to_string(),
                    check_type: HealthCheckType::HttpEndpoint,
                    parameters: HashMap::new(),
                    failure_threshold: 3,
                    recovery_threshold: 2,
                    alert_on_failure: true,
                },
                HealthCheckConfig {
                    name: "error_rate".to_string(),
                    check_type: HealthCheckType::ErrorRate,
                    parameters: HashMap::new(),
                    failure_threshold: 5,
                    recovery_threshold: 2,
                    alert_on_failure: true,
                },
            ],
            alerting: AlertingConfig::default(),
        }
    }
}

impl Default for AlertingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            destinations: vec![AlertDestination::Log {
                level: "error".to_string(),
            }],
            throttling: AlertThrottlingConfig::default(),
        }
    }
}

impl Default for AlertThrottlingConfig {
    fn default() -> Self {
        Self {
            min_interval: Duration::from_minutes(5),
            max_alerts_per_window: 10,
            time_window: Duration::from_hours(1),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval: Duration::from_secs(10),
            thresholds: PerformanceThresholds::default(),
            benchmarking: BenchmarkingConfig::default(),
        }
    }
}

impl Default for PerformanceThresholds {
    fn default() -> Self {
        Self {
            max_latency: Duration::from_millis(100),
            max_error_rate: 0.01,
            max_memory_mb: 1000,
            max_cpu_usage: 80.0,
            min_throughput: 100.0,
        }
    }
}

impl Default for BenchmarkingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: Duration::from_hours(24),
            duration: Duration::from_minutes(5),
            load_generation: LoadGenerationConfig::default(),
        }
    }
}

impl Default for LoadGenerationConfig {
    fn default() -> Self {
        Self {
            target_rps: 100.0,
            pattern: RequestPattern::Constant,
            payload_size_range: (100, 1000),
        }
    }
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            automatic: true,
            max_rollback_time: Duration::from_minutes(5),
            triggers: RollbackTriggers::default(),
            verification: RollbackVerificationConfig::default(),
        }
    }
}

impl Default for RollbackTriggers {
    fn default() -> Self {
        Self {
            error_rate_threshold: 0.1, // 10%
            latency_threshold: Duration::from_secs(5),
            consecutive_failures: 3,
            memory_threshold_mb: 2000,
            custom_conditions: Vec::new(),
        }
    }
}

impl Default for RollbackVerificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout: Duration::from_minutes(2),
            health_checks: vec!["endpoint_health".to_string(), "error_rate".to_string()],
            performance_validation: true,
        }
    }
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            logging: LoggingConfig::default(),
            metrics: MetricsConfig::default(),
            tracing: TracingConfig::default(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: LogFormat::Human,
            outputs: vec![LogOutput::Stdout],
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prometheus_endpoint: Some("http://localhost:9090/metrics".to_string()),
            namespace: "lighthouse_compat".to_string(),
            custom_metrics: Vec::new(),
        }
    }
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            service_name: "lighthouse-compat".to_string(),
            sampling_rate: 0.1,
        }
    }
}

impl CompatConfig {
    /// Load configuration from a file
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> CompatResult<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| CompatError::Configuration {
                parameter: "config_file".to_string(),
                reason: format!("Failed to read config file: {}", e),
            })?;
            
        let config: Self = toml::from_str(&content)
            .map_err(|e| CompatError::Configuration {
                parameter: "config_format".to_string(),
                reason: format!("Failed to parse config: {}", e),
            })?;
            
        config.validate()?;
        Ok(config)
    }
    
    /// Save configuration to a file
    pub fn to_file<P: AsRef<std::path::Path>>(&self, path: P) -> CompatResult<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| CompatError::Serialization {
                format: "toml".to_string(),
                reason: format!("Failed to serialize config: {}", e),
            })?;
            
        std::fs::write(path.as_ref(), content)
            .map_err(|e| CompatError::Configuration {
                parameter: "config_file".to_string(),
                reason: format!("Failed to write config file: {}", e),
            })?;
            
        Ok(())
    }
    
    /// Validate the configuration
    pub fn validate(&self) -> CompatResult<()> {
        // Version validation
        if !self.versions.v4.enabled && !self.versions.v5.enabled {
            return Err(CompatError::Configuration {
                parameter: "versions".to_string(),
                reason: "At least one version must be enabled".to_string(),
            });
        }
        
        // Migration validation
        match self.migration.initial_mode {
            MigrationMode::V4Only if !self.versions.v4.enabled => {
                return Err(CompatError::Configuration {
                    parameter: "migration.initial_mode".to_string(),
                    reason: "V4Only mode requires v4 to be enabled".to_string(),
                });
            }
            MigrationMode::V5Only if !self.versions.v5.enabled => {
                return Err(CompatError::Configuration {
                    parameter: "migration.initial_mode".to_string(),
                    reason: "V5Only mode requires v5 to be enabled".to_string(),
                });
            }
            _ => {}
        }
        
        // A/B testing validation
        if self.ab_testing.enabled {
            for (name, test) in &self.ab_testing.tests {
                if test.v5_percentage > 100 {
                    return Err(CompatError::Configuration {
                        parameter: format!("ab_testing.tests.{}.v5_percentage", name),
                        reason: "Percentage cannot exceed 100".to_string(),
                    });
                }
            }
        }
        
        // Performance thresholds validation
        if self.performance.thresholds.max_error_rate < 0.0 || self.performance.thresholds.max_error_rate > 1.0 {
            return Err(CompatError::Configuration {
                parameter: "performance.thresholds.max_error_rate".to_string(),
                reason: "Error rate must be between 0.0 and 1.0".to_string(),
            });
        }
        
        Ok(())
    }
    
    /// Builder method for chaining configuration
    pub fn with_migration_mode(mut self, mode: MigrationMode) -> Self {
        self.migration.initial_mode = mode;
        self
    }
    
    /// Enable health monitoring
    pub fn with_health_monitoring(mut self, enabled: bool) -> Self {
        self.health.enabled = enabled;
        self
    }
    
    /// Set A/B testing configuration
    pub fn with_ab_testing(mut self, enabled: bool) -> Self {
        self.ab_testing.enabled = enabled;
        self
    }
    
    /// Set performance monitoring
    pub fn with_performance_monitoring(mut self, enabled: bool) -> Self {
        self.performance.enabled = enabled;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    
    #[test]
    fn test_default_config() {
        let config = CompatConfig::default();
        assert!(config.validate().is_ok());
        assert!(config.versions.v4.enabled);
        assert!(!config.versions.v5.enabled);
        assert_eq!(config.migration.initial_mode, MigrationMode::V4Only);
    }
    
    #[test]
    fn test_config_validation() {
        let mut config = CompatConfig::default();
        
        // Test invalid configuration
        config.versions.v4.enabled = false;
        config.versions.v5.enabled = false;
        assert!(config.validate().is_err());
        
        // Test valid configuration
        config.versions.v4.enabled = true;
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_config_file_operations() {
        let config = CompatConfig::default();
        let temp_file = NamedTempFile::new().unwrap();
        
        // Test saving
        assert!(config.to_file(temp_file.path()).is_ok());
        
        // Test loading
        let loaded_config = CompatConfig::from_file(temp_file.path()).unwrap();
        assert_eq!(config.versions.v4.enabled, loaded_config.versions.v4.enabled);
    }
    
    #[test]
    fn test_builder_methods() {
        let config = CompatConfig::default()
            .with_migration_mode(MigrationMode::Parallel)
            .with_health_monitoring(true)
            .with_ab_testing(true)
            .with_performance_monitoring(true);
            
        assert_eq!(config.migration.initial_mode, MigrationMode::Parallel);
        assert!(config.health.enabled);
        assert!(config.ab_testing.enabled);
        assert!(config.performance.enabled);
    }
}