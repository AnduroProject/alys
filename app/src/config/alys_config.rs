//! Master configuration structure for the Alys V2 system

use super::*;
use crate::types::blockchain::ChainId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
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

impl AlysConfig {
    /// Apply environment variable overrides with given prefix
    fn apply_env_overrides(config: &mut AlysConfig, prefix: &str) -> Result<(), ConfigError> {
        let prefix = format!("{}_", prefix.to_uppercase());
        
        // System overrides
        if let Ok(name) = std::env::var(format!("{}SYSTEM_NAME", prefix)) {
            config.system.name = name;
        }
        if let Ok(node_id) = std::env::var(format!("{}NODE_ID", prefix)) {
            config.system.node_id = node_id;
        }
        if let Ok(data_dir) = std::env::var(format!("{}DATA_DIR", prefix)) {
            config.system.data_dir = PathBuf::from(data_dir);
        }
        
        // Network overrides
        if let Ok(listen_addr) = std::env::var(format!("{}LISTEN_ADDR", prefix)) {
            config.network.listen_address = listen_addr.parse()
                .map_err(|e| ConfigError::ValidationError {
                    field: "network.listen_address".to_string(),
                    reason: format!("Invalid socket address: {}", e),
                })?;
        }
        
        // Database overrides
        if let Ok(db_url) = std::env::var(format!("{}DATABASE_URL", prefix)) {
            config.storage.database_url = db_url;
        }
        
        // Security overrides
        if let Ok(_) = std::env::var(format!("{}ENABLE_TLS", prefix)) {
            config.system.security.enable_tls = true;
        }
        if let Ok(tls_cert) = std::env::var(format!("{}TLS_CERT_FILE", prefix)) {
            config.system.security.tls_cert_file = Some(PathBuf::from(tls_cert));
        }
        if let Ok(tls_key) = std::env::var(format!("{}TLS_KEY_FILE", prefix)) {
            config.system.security.tls_key_file = Some(PathBuf::from(tls_key));
        }
        
        // Monitoring overrides
        if let Ok(metrics_addr) = std::env::var(format!("{}METRICS_ADDR", prefix)) {
            config.monitoring.bind_addr = metrics_addr.parse()
                .map_err(|e| ConfigError::ValidationError {
                    field: "monitoring.bind_addr".to_string(),
                    reason: format!("Invalid metrics address: {}", e),
                })?;
        }
        
        // Thread pool overrides
        if let Ok(core_threads) = std::env::var(format!("{}CORE_THREADS", prefix)) {
            config.system.thread_pool.core_threads = core_threads.parse()
                .map_err(|e| ConfigError::ValidationError {
                    field: "system.thread_pool.core_threads".to_string(),
                    reason: format!("Invalid core threads value: {}", e),
                })?;
        }
        if let Ok(max_threads) = std::env::var(format!("{}MAX_THREADS", prefix)) {
            config.system.thread_pool.max_threads = max_threads.parse()
                .map_err(|e| ConfigError::ValidationError {
                    field: "system.thread_pool.max_threads".to_string(),
                    reason: format!("Invalid max threads value: {}", e),
                })?;
        }
        
        // Memory overrides
        if let Ok(max_heap) = std::env::var(format!("{}MAX_HEAP_MB", prefix)) {
            config.system.memory.max_heap_mb = Some(max_heap.parse()
                .map_err(|e| ConfigError::ValidationError {
                    field: "system.memory.max_heap_mb".to_string(),
                    reason: format!("Invalid max heap value: {}", e),
                })?);
        }
        
        Ok(())
    }
    
    /// Load configuration from multiple sources with priority order:
    /// 1. Default values
    /// 2. Configuration file
    /// 3. Environment variables
    /// 4. Command line arguments (future)
    pub fn load_layered(
        config_file: Option<&Path>,
        env_prefix: Option<&str>,
    ) -> Result<AlysConfig, ConfigError> {
        let mut config = AlysConfig::default();
        
        // Load from file if provided
        if let Some(file_path) = config_file {
            if file_path.exists() {
                config = Self::load_from_file(file_path)?;
            } else {
                tracing::warn!("Configuration file {:?} not found, using defaults", file_path);
            }
        }
        
        // Apply environment variable overrides
        if let Some(prefix) = env_prefix {
            Self::apply_env_overrides(&mut config, prefix)?;
        }
        
        // Also apply standard environment variables without prefix
        let env_config = Self::load_from_env()?;
        Self::merge_configs(&mut config, env_config);
        
        config.validate()?;
        Ok(config)
    }
    
    /// Merge configuration values, with `override_config` taking precedence
    fn merge_configs(base: &mut AlysConfig, override_config: AlysConfig) {
        // Merge system config
        if override_config.system.name != AlysConfig::default().system.name {
            base.system.name = override_config.system.name;
        }
        if override_config.system.node_id != AlysConfig::default().system.node_id {
            base.system.node_id = override_config.system.node_id;
        }
        if override_config.system.data_dir != AlysConfig::default().system.data_dir {
            base.system.data_dir = override_config.system.data_dir;
        }
        
        // Merge network config
        if override_config.network.listen_address != AlysConfig::default().network.listen_address {
            base.network.listen_address = override_config.network.listen_address;
        }
        if override_config.network.external_address.is_some() {
            base.network.external_address = override_config.network.external_address;
        }
        
        // Merge security config
        if override_config.system.security.enable_tls != AlysConfig::default().system.security.enable_tls {
            base.system.security.enable_tls = override_config.system.security.enable_tls;
        }
        if override_config.system.security.api_key.is_some() {
            base.system.security.api_key = override_config.system.security.api_key;
        }
        
        // Merge logging config
        if override_config.logging.level as u8 != AlysConfig::default().logging.level as u8 {
            base.logging.level = override_config.logging.level;
        }
    }
    
    /// Validate configuration and return detailed validation report
    pub fn validate_detailed(&self) -> ConfigValidationReport {
        let mut report = ConfigValidationReport {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        };
        
        // Validate system configuration
        if self.system.name.is_empty() {
            report.errors.push("System name cannot be empty".to_string());
            report.is_valid = false;
        }
        
        if self.system.thread_pool.core_threads == 0 {
            report.errors.push("Core threads must be greater than 0".to_string());
            report.is_valid = false;
        }
        
        if self.system.thread_pool.max_threads < self.system.thread_pool.core_threads {
            report.errors.push("Max threads cannot be less than core threads".to_string());
            report.is_valid = false;
        }
        
        // Validate network configuration
        if self.network.max_peers == 0 {
            report.warnings.push("Max peers is 0, node will not connect to network".to_string());
        }
        
        // Validate memory configuration
        if let Some(max_heap) = self.system.memory.max_heap_mb {
            let total_cache = self.system.memory.caches.block_cache_mb +
                             self.system.memory.caches.transaction_cache_mb +
                             self.system.memory.caches.state_cache_mb;
            
            if total_cache > max_heap / 2 {
                report.warnings.push(format!(
                    "Cache sizes ({} MB) may be too large for max heap ({} MB)",
                    total_cache, max_heap
                ));
            }
        }
        
        // Validate TLS configuration
        if self.system.security.enable_tls {
            if self.system.security.tls_cert_file.is_none() {
                report.errors.push("TLS certificate file required when TLS is enabled".to_string());
                report.is_valid = false;
            }
            if self.system.security.tls_key_file.is_none() {
                report.errors.push("TLS key file required when TLS is enabled".to_string());
                report.is_valid = false;
            }
        }
        
        report
    }
    
    /// Save configuration to file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::SerializationError {
                reason: e.to_string(),
            })?;
            
        std::fs::write(path.as_ref(), content)
            .map_err(|e| ConfigError::IoError {
                operation: "write config file".to_string(),
                error: e.to_string(),
            })?;
            
        Ok(())
    }
}

/// Configuration validation report
#[derive(Debug, Clone)]
pub struct ConfigValidationReport {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
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
        let mut config = AlysConfig::default();
        
        // System configuration from environment
        if let Ok(name) = std::env::var("ALYS_SYSTEM_NAME") {
            config.system.name = name;
        }
        if let Ok(node_id) = std::env::var("ALYS_NODE_ID") {
            config.system.node_id = node_id;
        }
        if let Ok(data_dir) = std::env::var("ALYS_DATA_DIR") {
            config.system.data_dir = PathBuf::from(data_dir);
        }
        
        // Network configuration from environment
        if let Ok(listen_addr) = std::env::var("ALYS_LISTEN_ADDR") {
            if let Ok(addr) = listen_addr.parse() {
                config.network.listen_address = addr;
            }
        }
        if let Ok(external_addr) = std::env::var("ALYS_EXTERNAL_ADDR") {
            if let Ok(addr) = external_addr.parse() {
                config.network.external_address = Some(addr);
            }
        }
        
        // Chain configuration from environment  
        if let Ok(chain_id_str) = std::env::var("ALYS_CHAIN_ID") {
            if let Ok(chain_id) = chain_id_str.parse::<u64>() {
                config.chain.chain_id = ChainId::from(chain_id);
            }
        }
        
        // Security configuration from environment
        if let Ok(_) = std::env::var("ALYS_ENABLE_TLS") {
            config.system.security.enable_tls = true;
        }
        if let Ok(api_key) = std::env::var("ALYS_API_KEY") {
            config.system.security.api_key = Some(api_key);
        }
        
        // Logging configuration from environment
        if let Ok(log_level) = std::env::var("ALYS_LOG_LEVEL") {
            config.logging.level = match log_level.to_lowercase().as_str() {
                "trace" => LogLevel::Trace,
                "debug" => LogLevel::Debug,
                "info" => LogLevel::Info,
                "warn" => LogLevel::Warn,
                "error" => LogLevel::Error,
                _ => LogLevel::Info,
            };
        }
        
        config.validate()?;
        Ok(config)
    }
    
    fn load_with_overrides<P: AsRef<Path>>(
        path: P,
        env_prefix: Option<&str>,
    ) -> Result<AlysConfig, ConfigError> {
        let mut config = Self::load_from_file(path)?;
        
        // Apply environment variable overrides
        if let Some(prefix) = env_prefix {
            Self::apply_env_overrides(&mut config, prefix)?;
        }
        
        config.validate()?;
        Ok(config)
    }
}