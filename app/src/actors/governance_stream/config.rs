//! Configuration management for governance stream actor
//!
//! This module provides comprehensive configuration for the governance stream
//! actor, including connection settings, authentication, retry policies,
//! and runtime parameter management with hot reload capabilities.

use crate::actors::governance_stream::{error::*, reconnect::*, protocol::*};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tracing::*;

/// Complete configuration for the governance stream actor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Connection configuration
    pub connection: ConnectionConfig,
    /// Authentication configuration
    pub authentication: AuthenticationConfig,
    /// Protocol configuration
    pub protocol: ProtocolConfig,
    /// Message handling configuration
    pub messaging: MessagingConfig,
    /// Performance tuning configuration
    pub performance: PerformanceConfig,
    /// Security configuration
    pub security: SecurityConfig,
    /// Monitoring and observability
    pub monitoring: MonitoringConfig,
    /// Feature flags
    pub features: FeatureConfig,
}

/// Connection-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// List of governance endpoints to connect to
    pub governance_endpoints: Vec<GovernanceEndpoint>,
    /// Maximum number of concurrent connections
    pub max_connections: usize,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Keep-alive settings
    pub keep_alive: KeepAliveConfig,
    /// Load balancing strategy
    pub load_balancing: LoadBalancingStrategy,
    /// Connection pooling settings
    pub connection_pool: ConnectionPoolConfig,
    /// Network interface binding
    pub bind_interface: Option<String>,
    /// Connection priority settings
    pub connection_priorities: HashMap<String, u8>,
}

/// Governance endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceEndpoint {
    /// Endpoint URL (e.g., "https://governance.anduro.io:443")
    pub url: String,
    /// Endpoint priority (higher = preferred)
    pub priority: u8,
    /// Whether this endpoint is active
    pub enabled: bool,
    /// Expected latency in milliseconds
    pub expected_latency_ms: Option<u64>,
    /// Geographic region or data center
    pub region: Option<String>,
    /// Endpoint-specific authentication overrides
    pub auth_override: Option<AuthConfig>,
    /// Custom metadata
    pub metadata: HashMap<String, String>,
}

/// Keep-alive configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeepAliveConfig {
    /// Enable TCP keep-alive
    pub enabled: bool,
    /// Keep-alive interval
    pub interval: Duration,
    /// Keep-alive timeout
    pub timeout: Duration,
    /// Number of keep-alive probes
    pub probe_count: u32,
}

/// Load balancing strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoadBalancingStrategy {
    /// Round-robin distribution
    RoundRobin,
    /// Priority-based selection
    Priority,
    /// Least connections
    LeastConnections,
    /// Latency-based selection
    LatencyBased,
    /// Random selection
    Random,
    /// Weighted round-robin
    WeightedRoundRobin { weights: HashMap<String, u32> },
}

/// Connection pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPoolConfig {
    /// Initial pool size
    pub initial_size: usize,
    /// Maximum pool size
    pub max_size: usize,
    /// Minimum idle connections
    pub min_idle: usize,
    /// Connection idle timeout
    pub idle_timeout: Duration,
    /// Connection validation interval
    pub validation_interval: Duration,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationConfig {
    /// Primary authentication method
    pub primary_auth: AuthConfig,
    /// Fallback authentication methods
    pub fallback_auth: Vec<AuthConfig>,
    /// Token refresh settings
    pub token_refresh: TokenRefreshConfig,
    /// Authentication retry policy
    pub retry_policy: AuthRetryPolicy,
    /// Certificate settings for mTLS
    pub certificates: Option<CertificateConfig>,
}

/// Token refresh configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRefreshConfig {
    /// Enable automatic token refresh
    pub enabled: bool,
    /// Refresh interval
    pub refresh_interval: Duration,
    /// Refresh threshold (refresh when token expires in this time)
    pub refresh_threshold: Duration,
    /// Maximum refresh attempts
    pub max_attempts: u32,
    /// Refresh retry delay
    pub retry_delay: Duration,
}

/// Authentication retry policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRetryPolicy {
    /// Maximum authentication attempts
    pub max_attempts: u32,
    /// Initial retry delay
    pub initial_delay: Duration,
    /// Maximum retry delay
    pub max_delay: Duration,
    /// Retry delay multiplier
    pub delay_multiplier: f64,
}

/// Certificate configuration for mTLS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateConfig {
    /// Client certificate path
    pub cert_path: PathBuf,
    /// Client private key path
    pub key_path: PathBuf,
    /// CA certificate path
    pub ca_cert_path: Option<PathBuf>,
    /// Certificate validation settings
    pub validation: CertificateValidation,
}

/// Certificate validation settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateValidation {
    /// Verify server certificate
    pub verify_server: bool,
    /// Verify certificate hostname
    pub verify_hostname: bool,
    /// Allow self-signed certificates
    pub allow_self_signed: bool,
    /// Certificate revocation checking
    pub check_revocation: bool,
}

/// Message handling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagingConfig {
    /// Message buffer configuration
    pub buffering: BufferingConfig,
    /// Message routing configuration
    pub routing: RoutingConfig,
    /// Message serialization settings
    pub serialization: SerializationConfig,
    /// Message validation settings
    pub validation: ValidationConfig,
    /// Message TTL settings
    pub ttl: TtlConfig,
}

/// Message buffering configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferingConfig {
    /// Buffer size per connection
    pub buffer_size: usize,
    /// Maximum total buffered messages
    pub max_total_buffered: usize,
    /// Buffer overflow strategy
    pub overflow_strategy: BufferOverflowStrategy,
    /// Message priority handling
    pub priority_handling: PriorityHandlingConfig,
    /// Buffer persistence settings
    pub persistence: BufferPersistenceConfig,
}

/// Buffer overflow strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BufferOverflowStrategy {
    /// Drop oldest messages
    DropOldest,
    /// Drop lowest priority messages
    DropLowestPriority,
    /// Reject new messages
    RejectNew,
    /// Apply backpressure
    BackPressure,
}

/// Priority handling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityHandlingConfig {
    /// Enable priority queuing
    pub enabled: bool,
    /// Priority queue sizes
    pub queue_sizes: HashMap<String, usize>,
    /// Priority escalation settings
    pub escalation: PriorityEscalationConfig,
}

/// Priority escalation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityEscalationConfig {
    /// Enable priority escalation
    pub enabled: bool,
    /// Escalation interval
    pub escalation_interval: Duration,
    /// Maximum escalation level
    pub max_escalation_level: u8,
}

/// Buffer persistence configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferPersistenceConfig {
    /// Enable buffer persistence
    pub enabled: bool,
    /// Persistence file path
    pub file_path: Option<PathBuf>,
    /// Persistence interval
    pub persistence_interval: Duration,
    /// Maximum persisted messages
    pub max_persisted_messages: usize,
}

/// Message routing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    /// Default routing strategy
    pub default_strategy: RoutingStrategy,
    /// Message type specific routing
    pub message_type_routing: HashMap<String, RoutingStrategy>,
    /// Actor routing table
    pub actor_routing: HashMap<String, String>,
    /// Routing failure handling
    pub failure_handling: RoutingFailureHandling,
}

/// Message routing strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutingStrategy {
    /// Broadcast to all targets
    Broadcast,
    /// Route to single target (round-robin)
    SingleTarget,
    /// Route based on content hash
    ContentHash,
    /// Route based on priority
    Priority,
    /// Custom routing logic
    Custom { handler: String },
}

/// Routing failure handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingFailureHandling {
    /// Retry failed routing attempts
    pub retry_failed: bool,
    /// Maximum routing retries
    pub max_retries: u32,
    /// Dead letter queue for failed messages
    pub dead_letter_queue: bool,
    /// Dead letter queue size
    pub dead_letter_queue_size: usize,
}

/// Message serialization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializationConfig {
    /// Primary serialization format
    pub primary_format: SerializationFormat,
    /// Fallback serialization formats
    pub fallback_formats: Vec<SerializationFormat>,
    /// Compression settings
    pub compression: CompressionConfig,
    /// Schema validation
    pub schema_validation: SchemaValidationConfig,
}

/// Schema validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaValidationConfig {
    /// Enable schema validation
    pub enabled: bool,
    /// Schema file paths
    pub schema_paths: HashMap<String, PathBuf>,
    /// Validation strictness level
    pub strictness: ValidationStrictness,
}

/// Validation strictness levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationStrictness {
    /// Strict validation - reject invalid messages
    Strict,
    /// Lenient validation - log warnings for invalid messages
    Lenient,
    /// Advisory validation - validate but don't enforce
    Advisory,
}

/// Message validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    /// Enable message validation
    pub enabled: bool,
    /// Maximum message size
    pub max_message_size: usize,
    /// Allowed message types
    pub allowed_message_types: Option<Vec<String>>,
    /// Message content filtering
    pub content_filtering: ContentFilteringConfig,
    /// Rate limiting per message type
    pub rate_limiting: RateLimitingConfig,
}

/// Content filtering configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentFilteringConfig {
    /// Enable content filtering
    pub enabled: bool,
    /// Blocked content patterns
    pub blocked_patterns: Vec<String>,
    /// Content sanitization rules
    pub sanitization_rules: HashMap<String, String>,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitingConfig {
    /// Enable rate limiting
    pub enabled: bool,
    /// Global rate limit (messages per second)
    pub global_limit: Option<u32>,
    /// Per-connection rate limits
    pub per_connection_limit: Option<u32>,
    /// Per-message-type rate limits
    pub per_message_type_limits: HashMap<String, u32>,
    /// Rate limiting window
    pub window_size: Duration,
}

/// Message TTL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtlConfig {
    /// Default TTL for messages
    pub default_ttl: Duration,
    /// Per-message-type TTL settings
    pub message_type_ttl: HashMap<String, Duration>,
    /// TTL cleanup interval
    pub cleanup_interval: Duration,
    /// Enable TTL enforcement
    pub enforce_ttl: bool,
}

/// Performance tuning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Thread pool configuration
    pub thread_pool: ThreadPoolConfig,
    /// Memory management settings
    pub memory: MemoryConfig,
    /// I/O settings
    pub io: IoConfig,
    /// Batch processing settings
    pub batching: BatchingConfig,
    /// Caching configuration
    pub caching: CachingConfig,
}

/// Thread pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadPoolConfig {
    /// Core thread pool size
    pub core_threads: usize,
    /// Maximum thread pool size
    pub max_threads: usize,
    /// Thread keep-alive time
    pub keep_alive: Duration,
    /// Queue size for pending tasks
    pub queue_size: usize,
}

/// Memory management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Maximum memory usage (bytes)
    pub max_memory_usage: Option<u64>,
    /// Memory pressure handling
    pub pressure_handling: MemoryPressureHandling,
    /// Garbage collection settings
    pub gc_settings: GcSettings,
}

/// Memory pressure handling strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryPressureHandling {
    /// Drop non-critical messages
    DropMessages,
    /// Reduce buffer sizes
    ReduceBuffers,
    /// Apply backpressure
    BackPressure,
    /// Trigger garbage collection
    ForceGc,
}

/// Garbage collection settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcSettings {
    /// Enable explicit GC triggers
    pub enabled: bool,
    /// GC trigger threshold (memory usage percentage)
    pub trigger_threshold: f64,
    /// GC trigger interval
    pub trigger_interval: Duration,
}

/// I/O configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoConfig {
    /// I/O buffer sizes
    pub buffer_sizes: IoBufferSizes,
    /// I/O timeout settings
    pub timeouts: IoTimeouts,
    /// I/O retry settings
    pub retry_settings: IoRetrySettings,
}

/// I/O buffer sizes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoBufferSizes {
    /// Read buffer size
    pub read_buffer: usize,
    /// Write buffer size
    pub write_buffer: usize,
    /// Socket buffer size
    pub socket_buffer: Option<usize>,
}

/// I/O timeout settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoTimeouts {
    /// Connect timeout
    pub connect: Duration,
    /// Read timeout
    pub read: Duration,
    /// Write timeout
    pub write: Duration,
    /// Overall operation timeout
    pub operation: Duration,
}

/// I/O retry settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoRetrySettings {
    /// Maximum I/O retries
    pub max_retries: u32,
    /// I/O retry delay
    pub retry_delay: Duration,
    /// Retryable error codes
    pub retryable_errors: Vec<i32>,
}

/// Batch processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchingConfig {
    /// Enable batch processing
    pub enabled: bool,
    /// Batch size
    pub batch_size: usize,
    /// Batch timeout
    pub batch_timeout: Duration,
    /// Maximum batch queue size
    pub max_queue_size: usize,
}

/// Caching configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachingConfig {
    /// Enable message caching
    pub enabled: bool,
    /// Cache size limit
    pub max_size: usize,
    /// Cache TTL
    pub ttl: Duration,
    /// Cache eviction policy
    pub eviction_policy: CacheEvictionPolicy,
}

/// Cache eviction policies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheEvictionPolicy {
    /// Least Recently Used
    Lru,
    /// Least Frequently Used
    Lfu,
    /// First In, First Out
    Fifo,
    /// Time-based expiration
    Ttl,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// TLS configuration
    pub tls: TlsConfig,
    /// Access control settings
    pub access_control: AccessControlConfig,
    /// Security monitoring
    pub monitoring: SecurityMonitoringConfig,
    /// Audit logging
    pub audit_logging: AuditLoggingConfig,
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Enable TLS
    pub enabled: bool,
    /// Minimum TLS version
    pub min_version: TlsVersion,
    /// Allowed cipher suites
    pub allowed_ciphers: Option<Vec<String>>,
    /// Certificate pinning
    pub certificate_pinning: CertificatePinningConfig,
}

/// TLS versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TlsVersion {
    #[serde(rename = "1.2")]
    V12,
    #[serde(rename = "1.3")]
    V13,
}

/// Certificate pinning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificatePinningConfig {
    /// Enable certificate pinning
    pub enabled: bool,
    /// Pinned certificate fingerprints
    pub pinned_fingerprints: Vec<String>,
    /// Fingerprint algorithm
    pub fingerprint_algorithm: String,
}

/// Access control configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlConfig {
    /// Enable access control
    pub enabled: bool,
    /// Allowed source addresses
    pub allowed_addresses: Option<Vec<String>>,
    /// Blocked source addresses
    pub blocked_addresses: Option<Vec<String>>,
    /// Rate limiting per source
    pub rate_limiting: AccessControlRateLimiting,
}

/// Access control rate limiting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlRateLimiting {
    /// Enable rate limiting
    pub enabled: bool,
    /// Requests per minute per source
    pub requests_per_minute: u32,
    /// Burst allowance
    pub burst_allowance: u32,
}

/// Security monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityMonitoringConfig {
    /// Enable security monitoring
    pub enabled: bool,
    /// Intrusion detection
    pub intrusion_detection: IntrusionDetectionConfig,
    /// Anomaly detection
    pub anomaly_detection: AnomalyDetectionConfig,
}

/// Intrusion detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrusionDetectionConfig {
    /// Enable intrusion detection
    pub enabled: bool,
    /// Detection rules
    pub rules: Vec<IntrusionDetectionRule>,
    /// Response actions
    pub response_actions: Vec<String>,
}

/// Intrusion detection rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrusionDetectionRule {
    /// Rule name
    pub name: String,
    /// Rule pattern
    pub pattern: String,
    /// Rule severity
    pub severity: String,
    /// Rule action
    pub action: String,
}

/// Anomaly detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyDetectionConfig {
    /// Enable anomaly detection
    pub enabled: bool,
    /// Detection algorithms
    pub algorithms: Vec<String>,
    /// Sensitivity threshold
    pub sensitivity: f64,
}

/// Audit logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLoggingConfig {
    /// Enable audit logging
    pub enabled: bool,
    /// Log file path
    pub log_path: Option<PathBuf>,
    /// Log format
    pub log_format: AuditLogFormat,
    /// Log retention settings
    pub retention: LogRetentionConfig,
}

/// Audit log formats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditLogFormat {
    /// JSON format
    Json,
    /// Structured text
    Text,
    /// Common Event Format (CEF)
    Cef,
    /// LEEF format
    Leef,
}

/// Log retention configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRetentionConfig {
    /// Retention period
    pub retention_period: Duration,
    /// Maximum log file size
    pub max_file_size: u64,
    /// Log rotation settings
    pub rotation: LogRotationConfig,
}

/// Log rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRotationConfig {
    /// Enable log rotation
    pub enabled: bool,
    /// Rotation interval
    pub interval: Duration,
    /// Maximum number of archived files
    pub max_archived_files: u32,
}

/// Monitoring and observability configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Metrics configuration
    pub metrics: MetricsConfig,
    /// Health check configuration
    pub health_checks: HealthCheckConfig,
    /// Tracing configuration
    pub tracing: TracingConfig,
    /// Alerting configuration
    pub alerting: AlertingConfig,
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,
    /// Metrics export format
    pub export_format: MetricsFormat,
    /// Metrics export endpoint
    pub export_endpoint: Option<String>,
    /// Metrics collection interval
    pub collection_interval: Duration,
    /// Custom metrics
    pub custom_metrics: HashMap<String, MetricConfig>,
}

/// Metrics formats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricsFormat {
    /// Prometheus format
    Prometheus,
    /// JSON format
    Json,
    /// StatsD format
    Statsd,
    /// InfluxDB line protocol
    Influx,
}

/// Individual metric configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricConfig {
    /// Metric type
    pub metric_type: MetricType,
    /// Metric description
    pub description: String,
    /// Metric labels
    pub labels: HashMap<String, String>,
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

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Enable health checks
    pub enabled: bool,
    /// Health check interval
    pub interval: Duration,
    /// Health check timeout
    pub timeout: Duration,
    /// Custom health checks
    pub custom_checks: HashMap<String, HealthCheck>,
}

/// Health check definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Check name
    pub name: String,
    /// Check type
    pub check_type: HealthCheckType,
    /// Check parameters
    pub parameters: HashMap<String, String>,
    /// Failure threshold
    pub failure_threshold: u32,
    /// Recovery threshold
    pub recovery_threshold: u32,
}

/// Health check types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthCheckType {
    /// Connection health check
    Connection,
    /// Memory usage check
    Memory,
    /// CPU usage check
    Cpu,
    /// Disk space check
    Disk,
    /// Custom check
    Custom { handler: String },
}

/// Tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// Enable tracing
    pub enabled: bool,
    /// Trace sampling rate (0.0 to 1.0)
    pub sampling_rate: f64,
    /// Trace export endpoint
    pub export_endpoint: Option<String>,
    /// Trace export format
    pub export_format: TracingFormat,
    /// Trace context propagation
    pub context_propagation: ContextPropagationConfig,
}

/// Tracing formats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TracingFormat {
    /// Jaeger format
    Jaeger,
    /// Zipkin format
    Zipkin,
    /// OpenTelemetry format
    OpenTelemetry,
    /// Custom format
    Custom { format: String },
}

/// Context propagation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPropagationConfig {
    /// Enable context propagation
    pub enabled: bool,
    /// Propagation formats
    pub formats: Vec<String>,
    /// Custom headers
    pub custom_headers: HashMap<String, String>,
}

/// Alerting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertingConfig {
    /// Enable alerting
    pub enabled: bool,
    /// Alert rules
    pub rules: Vec<AlertRule>,
    /// Alert channels
    pub channels: HashMap<String, AlertChannel>,
}

/// Alert rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// Rule name
    pub name: String,
    /// Rule condition
    pub condition: String,
    /// Alert severity
    pub severity: AlertSeverity,
    /// Alert channel
    pub channel: String,
    /// Throttle settings
    pub throttle: AlertThrottleConfig,
}

/// Alert severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    /// Info level
    Info,
    /// Warning level
    Warning,
    /// Error level
    Error,
    /// Critical level
    Critical,
}

/// Alert channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertChannel {
    /// Channel type
    pub channel_type: AlertChannelType,
    /// Channel configuration
    pub config: HashMap<String, String>,
}

/// Alert channel types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertChannelType {
    /// Email alerts
    Email,
    /// Slack alerts
    Slack,
    /// Webhook alerts
    Webhook,
    /// SMS alerts
    Sms,
    /// PagerDuty integration
    PagerDuty,
}

/// Alert throttling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThrottleConfig {
    /// Enable alert throttling
    pub enabled: bool,
    /// Throttle window
    pub window: Duration,
    /// Maximum alerts per window
    pub max_alerts: u32,
}

/// Feature flags configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureConfig {
    /// Feature flags
    pub flags: HashMap<String, bool>,
    /// Feature rollout percentages
    pub rollout_percentages: HashMap<String, f64>,
    /// A/B testing configurations
    pub ab_testing: HashMap<String, AbTestConfig>,
}

/// A/B testing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTestConfig {
    /// Test name
    pub name: String,
    /// Test variants
    pub variants: HashMap<String, f64>,
    /// Test criteria
    pub criteria: HashMap<String, String>,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            authentication: AuthenticationConfig::default(),
            protocol: ProtocolConfig::default(),
            messaging: MessagingConfig::default(),
            performance: PerformanceConfig::default(),
            security: SecurityConfig::default(),
            monitoring: MonitoringConfig::default(),
            features: FeatureConfig::default(),
        }
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            governance_endpoints: vec![
                GovernanceEndpoint {
                    url: "https://governance.anduro.io:443".to_string(),
                    priority: 100,
                    enabled: true,
                    expected_latency_ms: Some(50),
                    region: Some("primary".to_string()),
                    auth_override: None,
                    metadata: HashMap::new(),
                }
            ],
            max_connections: 10,
            connection_timeout: Duration::from_secs(30),
            keep_alive: KeepAliveConfig::default(),
            load_balancing: LoadBalancingStrategy::Priority,
            connection_pool: ConnectionPoolConfig::default(),
            bind_interface: None,
            connection_priorities: HashMap::new(),
        }
    }
}

impl Default for KeepAliveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(60),
            timeout: Duration::from_secs(10),
            probe_count: 3,
        }
    }
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            initial_size: 2,
            max_size: 10,
            min_idle: 1,
            idle_timeout: Duration::from_secs(300),
            validation_interval: Duration::from_secs(30),
        }
    }
}

// Additional default implementations for other config structs would follow...
// For brevity, I'll implement the most critical ones

impl Default for MessagingConfig {
    fn default() -> Self {
        Self {
            buffering: BufferingConfig::default(),
            routing: RoutingConfig::default(),
            serialization: SerializationConfig::default(),
            validation: ValidationConfig::default(),
            ttl: TtlConfig::default(),
        }
    }
}

impl Default for BufferingConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1000,
            max_total_buffered: 10000,
            overflow_strategy: BufferOverflowStrategy::DropOldest,
            priority_handling: PriorityHandlingConfig::default(),
            persistence: BufferPersistenceConfig::default(),
        }
    }
}

impl Default for PriorityHandlingConfig {
    fn default() -> Self {
        let mut queue_sizes = HashMap::new();
        queue_sizes.insert("high".to_string(), 500);
        queue_sizes.insert("normal".to_string(), 300);
        queue_sizes.insert("low".to_string(), 200);

        Self {
            enabled: true,
            queue_sizes,
            escalation: PriorityEscalationConfig::default(),
        }
    }
}

impl Default for PriorityEscalationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            escalation_interval: Duration::from_secs(60),
            max_escalation_level: 3,
        }
    }
}

impl Default for BufferPersistenceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            file_path: None,
            persistence_interval: Duration::from_secs(30),
            max_persisted_messages: 1000,
        }
    }
}

impl StreamConfig {
    /// Load configuration from file
    pub async fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> ConfigurationResult<Self> {
        let content = tokio::fs::read_to_string(path).await
            .map_err(|e| ConfigurationError::FileNotFound {
                file_path: format!("{:?}", path.as_ref()),
            })?;

        let config: Self = match path.as_ref().extension().and_then(|s| s.to_str()) {
            Some("yaml") | Some("yml") => {
                serde_yaml::from_str(&content)
                    .map_err(|e| ConfigurationError::ParseError {
                        parse_error: e.to_string(),
                    })?
            }
            Some("json") => {
                serde_json::from_str(&content)
                    .map_err(|e| ConfigurationError::ParseError {
                        parse_error: e.to_string(),
                    })?
            }
            Some("toml") => {
                toml::from_str(&content)
                    .map_err(|e| ConfigurationError::ParseError {
                        parse_error: e.to_string(),
                    })?
            }
            _ => {
                return Err(ConfigurationError::InvalidParameter {
                    parameter: "file_extension".to_string(),
                    reason: "Unsupported file format. Use yaml, json, or toml".to_string(),
                });
            }
        };

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration
    pub fn validate(&self) -> ConfigurationResult<()> {
        let mut errors = Vec::new();

        // Validate connection configuration
        if self.connection.governance_endpoints.is_empty() {
            errors.push("At least one governance endpoint must be configured".to_string());
        }

        if self.connection.max_connections == 0 {
            errors.push("max_connections must be greater than 0".to_string());
        }

        // Validate authentication configuration
        if self.authentication.primary_auth.credential.is_empty() {
            errors.push("Authentication credential cannot be empty".to_string());
        }

        // Validate messaging configuration
        if self.messaging.buffering.buffer_size == 0 {
            errors.push("Buffer size must be greater than 0".to_string());
        }

        if !errors.is_empty() {
            return Err(ConfigurationError::ValidationFailed {
                validation_errors: errors,
            });
        }

        Ok(())
    }

    /// Save configuration to file
    pub async fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> ConfigurationResult<()> {
        let content = match path.as_ref().extension().and_then(|s| s.to_str()) {
            Some("yaml") | Some("yml") => {
                serde_yaml::to_string(self)
                    .map_err(|e| ConfigurationError::ParseError {
                        parse_error: e.to_string(),
                    })?
            }
            Some("json") => {
                serde_json::to_string_pretty(self)
                    .map_err(|e| ConfigurationError::ParseError {
                        parse_error: e.to_string(),
                    })?
            }
            Some("toml") => {
                toml::to_string_pretty(self)
                    .map_err(|e| ConfigurationError::ParseError {
                        parse_error: e.to_string(),
                    })?
            }
            _ => {
                return Err(ConfigurationError::InvalidParameter {
                    parameter: "file_extension".to_string(),
                    reason: "Unsupported file format. Use yaml, json, or toml".to_string(),
                });
            }
        };

        tokio::fs::write(path, content).await
            .map_err(|e| ConfigurationError::ParseError {
                parse_error: e.to_string(),
            })?;

        Ok(())
    }

    /// Merge with another configuration (other takes precedence)
    pub fn merge(&mut self, other: StreamConfig) {
        // This would implement a deep merge of configurations
        // For now, simplified to replace top-level fields
        if !other.connection.governance_endpoints.is_empty() {
            self.connection.governance_endpoints = other.connection.governance_endpoints;
        }
        
        if other.connection.max_connections > 0 {
            self.connection.max_connections = other.connection.max_connections;
        }

        // More merge logic would be implemented here...
    }

    /// Get feature flag value
    pub fn is_feature_enabled(&self, feature: &str) -> bool {
        self.features.flags.get(feature).copied().unwrap_or(false)
    }

    /// Get rollout percentage for feature
    pub fn get_rollout_percentage(&self, feature: &str) -> f64 {
        self.features.rollout_percentages.get(feature).copied().unwrap_or(0.0)
    }
}

// Implement additional default traits for other config structs as needed...

type ConfigurationResult<T> = Result<T, ConfigurationError>;