//! Master configuration structure for the Alys V2 system

use super::*;
use crate::types::blockchain::ChainId;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Master configuration structure for the entire Alys system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlysConfig {
    /// Environment configuration
    pub environment: Environment,
    
    /// System-wide settings
    pub system: SystemConfig,
    
    /// Actor system configuration
    pub actors: ActorSystemConfig,
    
    /// Chain and consensus configuration
    pub chain: ChainConfig,
    
    /// Network and P2P configuration
    pub network: NetworkConfig,
    
    /// Bridge and peg operations configuration
    pub bridge: BridgeConfig,
    
    /// Storage and database configuration
    pub storage: StorageConfig,
    
    /// Governance integration configuration
    pub governance: GovernanceConfig,
    
    /// Sync engine configuration
    pub sync: SyncConfig,
    
    /// Monitoring and metrics configuration
    pub monitoring: MonitoringConfig,
    
    /// Logging configuration
    pub logging: LoggingConfig,
}

/// System-wide configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// System name
    pub name: String,
    
    /// System version
    pub version: String,
    
    /// Node ID
    pub node_id: String,
    
    /// Data directory
    pub data_dir: PathBuf,
    
    /// Configuration directory
    pub config_dir: PathBuf,
    
    /// Process ID file
    pub pid_file: Option<PathBuf>,
    
    /// Maximum file descriptors
    pub max_file_descriptors: Option<u64>,
    
    /// Thread pool settings
    pub thread_pool: ThreadPoolConfig,
    
    /// Memory limits
    pub memory: MemoryConfig,
    
    /// Security settings
    pub security: SecurityConfig,
}

/// Thread pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadPoolConfig {
    /// Core pool size
    pub core_threads: usize,
    
    /// Maximum pool size
    pub max_threads: usize,
    
    /// Thread keep-alive time
    pub keep_alive: Duration,
    
    /// Queue capacity
    pub queue_capacity: usize,
}

/// Memory configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Maximum heap size in MB
    pub max_heap_mb: Option<u64>,
    
    /// Cache sizes
    pub caches: CacheConfig,
    
    /// Buffer pool settings
    pub buffer_pool: BufferPoolConfig,
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Block cache size in MB
    pub block_cache_mb: u64,
    
    /// Transaction cache size in MB
    pub transaction_cache_mb: u64,
    
    /// State cache size in MB
    pub state_cache_mb: u64,
    
    /// Peer cache size (number of entries)
    pub peer_cache_entries: usize,
}

/// Buffer pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferPoolConfig {
    /// Buffer size in KB
    pub buffer_size_kb: u32,
    
    /// Number of buffers
    pub buffer_count: u32,
    
    /// Memory pool type
    pub pool_type: BufferPoolType,
}

/// Buffer pool types
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BufferPoolType {
    Fixed,
    Dynamic,
    Elastic,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable TLS for all connections
    pub enable_tls: bool,
    
    /// TLS certificate file
    pub tls_cert_file: Option<PathBuf>,
    
    /// TLS private key file
    pub tls_key_file: Option<PathBuf>,
    
    /// TLS CA certificate file
    pub tls_ca_file: Option<PathBuf>,
    
    /// API key for authenticated endpoints
    pub api_key: Option<String>,
    
    /// JWT secret for token authentication
    pub jwt_secret: Option<String>,
    
    /// JWT token expiration
    pub jwt_expiration: Duration,
    
    /// Rate limiting configuration
    pub rate_limits: RateLimitConfig,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    pub enabled: bool,
    
    /// Requests per second per IP
    pub requests_per_second: u32,
    
    /// Burst capacity
    pub burst_capacity: u32,
    
    /// Cleanup interval
    pub cleanup_interval: Duration,
}

/// Monitoring and metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable metrics collection
    pub enabled: bool,
    
    /// Metrics server bind address
    pub bind_addr: SocketAddr,
    
    /// Metrics collection interval
    pub collection_interval: Duration,
    
    /// Prometheus configuration
    pub prometheus: PrometheusConfig,
    
    /// Health check configuration
    pub health_check: HealthCheckConfig,
    
    /// Alert configuration
    pub alerts: AlertConfig,
}

/// Prometheus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusConfig {
    /// Enable Prometheus metrics
    pub enabled: bool,
    
    /// Prometheus endpoint path
    pub path: String,
    
    /// Metrics prefix
    pub prefix: String,
    
    /// Additional labels
    pub labels: HashMap<String, String>,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check endpoint path
    pub path: String,
    
    /// Health check interval
    pub interval: Duration,
    
    /// Health check timeout
    pub timeout: Duration,
    
    /// Unhealthy threshold
    pub unhealthy_threshold: u32,
    
    /// Healthy threshold
    pub healthy_threshold: u32,
}

/// Alert configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Enable alerting
    pub enabled: bool,
    
    /// Alert channels
    pub channels: Vec<AlertChannel>,
    
    /// Alert rules
    pub rules: Vec<AlertRule>,
}

/// Alert channels
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlertChannel {
    Email {
        smtp_server: String,
        smtp_port: u16,
        username: String,
        password: String,
        recipients: Vec<String>,
    },
    Slack {
        webhook_url: String,
        channel: String,
        username: Option<String>,
    },
    Discord {
        webhook_url: String,
    },
    Webhook {
        url: String,
        headers: HashMap<String, String>,
    },
}

/// Alert rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// Rule name
    pub name: String,
    
    /// Metric name
    pub metric: String,
    
    /// Comparison operator
    pub operator: ComparisonOperator,
    
    /// Threshold value
    pub threshold: f64,
    
    /// Duration threshold must be exceeded
    pub duration: Duration,
    
    /// Alert severity
    pub severity: AlertSeverity,
    
    /// Alert message template
    pub message: String,
}

/// Comparison operators for alerts
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOperator {
    GreaterThan,
    LessThan,
    Equal,
    NotEqual,
    GreaterThanOrEqual,
    LessThanOrEqual,
}

/// Alert severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Global log level
    pub level: LogLevel,
    
    /// Per-module log levels
    pub modules: HashMap<String, LogLevel>,
    
    /// Log format
    pub format: LogFormat,
    
    /// Log outputs
    pub outputs: Vec<LogOutput>,
    
    /// Structured logging fields
    pub structured_fields: HashMap<String, String>,
}

/// Log levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Log formats
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    Plain,
    Json,
    Logfmt,
}

/// Log outputs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LogOutput {
    Stdout,
    Stderr,
    File {
        path: PathBuf,
        max_size_mb: u64,
        max_files: u32,
        compress: bool,
    },
    Syslog {
        facility: String,
        tag: String,
    },
}

impl Default for AlysConfig {
    fn default() -> Self {
        Self {
            environment: Environment::Development,
            system: SystemConfig::default(),
            actors: ActorSystemConfig::default(),
            chain: ChainConfig::default(),
            network: NetworkConfig::default(),
            bridge: BridgeConfig::default(),
            storage: StorageConfig::default(),
            governance: GovernanceConfig::default(),
            sync: SyncConfig::default(),
            monitoring: MonitoringConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            name: "alys-v2".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            node_id: uuid::Uuid::new_v4().to_string(),
            data_dir: PathBuf::from("./data"),
            config_dir: PathBuf::from("./config"),
            pid_file: Some(PathBuf::from("alys.pid")),
            max_file_descriptors: Some(65536),
            thread_pool: ThreadPoolConfig::default(),
            memory: MemoryConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

impl Default for ThreadPoolConfig {
    fn default() -> Self {
        Self {
            core_threads: num_cpus::get(),
            max_threads: num_cpus::get() * 4,
            keep_alive: Duration::from_secs(60),
            queue_capacity: 10000,
        }
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_heap_mb: None,
            caches: CacheConfig::default(),
            buffer_pool: BufferPoolConfig::default(),
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            block_cache_mb: 256,
            transaction_cache_mb: 128,
            state_cache_mb: 512,
            peer_cache_entries: 1000,
        }
    }
}

impl Default for BufferPoolConfig {
    fn default() -> Self {
        Self {
            buffer_size_kb: 64,
            buffer_count: 1000,
            pool_type: BufferPoolType::Dynamic,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_tls: false,
            tls_cert_file: None,
            tls_key_file: None,
            tls_ca_file: None,
            api_key: None,
            jwt_secret: None,
            jwt_expiration: Duration::from_hours(24),
            rate_limits: RateLimitConfig::default(),
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_second: 100,
            burst_capacity: 1000,
            cleanup_interval: Duration::from_secs(60),
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_addr: "127.0.0.1:9090".parse().unwrap(),
            collection_interval: Duration::from_secs(30),
            prometheus: PrometheusConfig::default(),
            health_check: HealthCheckConfig::default(),
            alerts: AlertConfig::default(),
        }
    }
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: "/metrics".to_string(),
            prefix: "alys_".to_string(),
            labels: HashMap::new(),
        }
    }
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            path: "/health".to_string(),
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
            unhealthy_threshold: 3,
            healthy_threshold: 2,
        }
    }
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            channels: Vec::new(),
            rules: Vec::new(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            modules: HashMap::new(),
            format: LogFormat::Plain,
            outputs: vec![LogOutput::Stdout],
            structured_fields: HashMap::new(),
        }
    }
}

impl Validate for AlysConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        self.system.validate()?;
        self.actors.validate()?;
        self.chain.validate()?;
        self.network.validate()?;
        self.bridge.validate()?;
        self.storage.validate()?;
        self.governance.validate()?;
        self.sync.validate()?;
        Ok(())
    }
}

impl Validate for SystemConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.name.is_empty() {
            return Err(ConfigError::ValidationError {
                field: "system.name".to_string(),
                reason: "System name cannot be empty".to_string(),
            });
        }
        
        if !self.data_dir.exists() && std::fs::create_dir_all(&self.data_dir).is_err() {
            return Err(ConfigError::ValidationError {
                field: "system.data_dir".to_string(),
                reason: "Cannot create data directory".to_string(),
            });
        }
        
        Ok(())
    }
}

impl ConfigLoader<AlysConfig> for AlysConfig {
    fn load_from_file<P: AsRef<Path>>(path: P) -> Result<AlysConfig, ConfigError> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| ConfigError::FileNotFound {
                path: path.as_ref().display().to_string(),
            })?;
            
        let config: AlysConfig = toml::from_str(&content)
            .map_err(|e| ConfigError::ParseError {
                reason: e.to_string(),
            })?;
            
        config.validate()?;
        Ok(config)
    }
    
    fn load_from_env() -> Result<AlysConfig, ConfigError> {
        // Load configuration from environment variables
        // This would implement environment variable parsing
        Ok(AlysConfig::default())
    }
    
    fn load_with_overrides<P: AsRef<Path>>(
        path: P,
        env_prefix: Option<&str>,
    ) -> Result<AlysConfig, ConfigError> {
        let mut config = Self::load_from_file(path)?;
        
        // Apply environment variable overrides
        if let Some(prefix) = env_prefix {
            // Override configuration from environment variables
            // This would implement env var override logic
        }
        
        config.validate()?;
        Ok(config)
    }
}