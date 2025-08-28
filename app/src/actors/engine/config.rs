//! Engine Actor Configuration
//!
//! Configuration structures and defaults for the EngineActor, including
//! JWT authentication, execution client URLs, timeouts, and performance tuning.

use std::time::Duration;
use serde::{Deserialize, Serialize};
use crate::types::*;

/// Configuration for the EngineActor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// JWT secret for engine API authentication (32 bytes)
    pub jwt_secret: [u8; 32],
    
    /// Engine API URL for authenticated operations
    pub engine_url: String,
    
    /// Public execution API URL for queries (optional)
    pub public_url: Option<String>,
    
    /// Timeout for engine API operations
    pub engine_timeout: Duration,
    
    /// Timeout for public API operations
    pub public_timeout: Duration,
    
    /// Execution client type preference
    pub client_type: ExecutionClientType,
    
    /// Maximum number of concurrent payload operations
    pub max_concurrent_payloads: usize,
    
    /// Payload building timeout
    pub payload_build_timeout: Duration,
    
    /// Payload execution timeout
    pub payload_execution_timeout: Duration,
    
    /// Health check interval for execution client
    pub health_check_interval: Duration,
    
    /// Maximum health check failures before restart
    pub max_health_failures: u32,
    
    /// Connection retry configuration
    pub retry_config: RetryConfig,
    
    /// Performance tuning parameters
    pub performance: PerformanceConfig,
    
    /// Actor integration settings
    pub actor_integration: ActorIntegrationConfig,
}

/// Supported execution client types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionClientType {
    /// Geth (go-ethereum)
    Geth,
    /// Reth (rust-ethereum) - future support
    Reth,
    /// Auto-detect based on client response
    Auto,
}

/// Retry configuration for failed operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    
    /// Initial retry delay
    pub initial_delay: Duration,
    
    /// Maximum retry delay
    pub max_delay: Duration,
    
    /// Backoff multiplier (exponential backoff)
    pub backoff_multiplier: f64,
    
    /// Jitter factor (0.0 to 1.0) for retry randomization
    pub jitter_factor: f64,
}

/// Performance tuning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Connection pool size for execution client
    pub connection_pool_size: usize,
    
    /// Keep-alive timeout for HTTP connections
    pub connection_keep_alive: Duration,
    
    /// Request timeout for individual HTTP requests
    pub request_timeout: Duration,
    
    /// Enable payload caching
    pub enable_payload_cache: bool,
    
    /// Maximum cache size for built payloads
    pub payload_cache_size: usize,
    
    /// Payload cache TTL
    pub payload_cache_ttl: Duration,
    
    /// Enable batch processing of operations
    pub enable_batching: bool,
    
    /// Maximum batch size for operations
    pub max_batch_size: usize,
    
    /// Batch timeout (flush incomplete batches)
    pub batch_timeout: Duration,
}

/// Actor integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorIntegrationConfig {
    /// Timeout for ChainActor communication
    pub chain_actor_timeout: Duration,
    
    /// Timeout for StorageActor communication (optional)
    pub storage_actor_timeout: Option<Duration>,
    
    /// Timeout for BridgeActor communication (optional)
    pub bridge_actor_timeout: Option<Duration>,
    
    /// Timeout for NetworkActor communication (optional)
    pub network_actor_timeout: Option<Duration>,
    
    /// Enable automatic actor address resolution
    pub enable_actor_discovery: bool,
    
    /// Circuit breaker configuration for actor communication
    pub circuit_breaker: CircuitBreakerConfig,
}

/// Circuit breaker configuration for fault tolerance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Failure threshold before opening circuit
    pub failure_threshold: u32,
    
    /// Recovery timeout (time to wait before attempting recovery)
    pub recovery_timeout: Duration,
    
    /// Success threshold for closing circuit
    pub success_threshold: u32,
    
    /// Timeout for each recovery attempt
    pub recovery_attempt_timeout: Duration,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            jwt_secret: [0u8; 32], // Should be properly generated
            engine_url: "http://localhost:8551".to_string(),
            public_url: Some("http://localhost:8545".to_string()),
            engine_timeout: Duration::from_secs(30),
            public_timeout: Duration::from_secs(10),
            client_type: ExecutionClientType::Auto,
            max_concurrent_payloads: 10,
            payload_build_timeout: Duration::from_millis(500),
            payload_execution_timeout: Duration::from_millis(1000),
            health_check_interval: Duration::from_secs(30),
            max_health_failures: 3,
            retry_config: RetryConfig::default(),
            performance: PerformanceConfig::default(),
            actor_integration: ActorIntegrationConfig::default(),
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            connection_pool_size: 5,
            connection_keep_alive: Duration::from_secs(30),
            request_timeout: Duration::from_secs(10),
            enable_payload_cache: true,
            payload_cache_size: 100,
            payload_cache_ttl: Duration::from_secs(300),
            enable_batching: false, // Disabled by default for simplicity
            max_batch_size: 10,
            batch_timeout: Duration::from_millis(100),
        }
    }
}

impl Default for ActorIntegrationConfig {
    fn default() -> Self {
        Self {
            chain_actor_timeout: Duration::from_secs(5),
            storage_actor_timeout: Some(Duration::from_secs(3)),
            bridge_actor_timeout: Some(Duration::from_secs(5)),
            network_actor_timeout: Some(Duration::from_secs(2)),
            enable_actor_discovery: true,
            circuit_breaker: CircuitBreakerConfig::default(),
        }
    }
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(10),
            success_threshold: 3,
            recovery_attempt_timeout: Duration::from_secs(2),
        }
    }
}

impl EngineConfig {
    /// Load configuration from environment variables with fallback to defaults
    pub fn from_env() -> Result<Self, crate::EngineError> {
        let mut config = Self::default();
        
        // Load JWT secret from environment
        if let Ok(jwt_hex) = std::env::var("ENGINE_JWT_SECRET") {
            let jwt_bytes = hex::decode(jwt_hex)
                .map_err(|e| crate::EngineError::ConfigError(format!("Invalid JWT secret hex: {}", e)))?;
            
            if jwt_bytes.len() != 32 {
                return Err(crate::EngineError::ConfigError(
                    "JWT secret must be 32 bytes".to_string()
                ));
            }
            
            config.jwt_secret.copy_from_slice(&jwt_bytes);
        }
        
        // Load URLs from environment
        if let Ok(engine_url) = std::env::var("ENGINE_API_URL") {
            config.engine_url = engine_url;
        }
        
        if let Ok(public_url) = std::env::var("ENGINE_PUBLIC_URL") {
            config.public_url = Some(public_url);
        }
        
        // Load timeouts from environment
        if let Ok(timeout_str) = std::env::var("ENGINE_TIMEOUT_SECONDS") {
            if let Ok(timeout_secs) = timeout_str.parse::<u64>() {
                config.engine_timeout = Duration::from_secs(timeout_secs);
            }
        }
        
        // Load client type preference
        if let Ok(client_type) = std::env::var("EXECUTION_CLIENT_TYPE") {
            config.client_type = match client_type.to_lowercase().as_str() {
                "geth" => ExecutionClientType::Geth,
                "reth" => ExecutionClientType::Reth,
                "auto" => ExecutionClientType::Auto,
                _ => ExecutionClientType::Auto,
            };
        }
        
        Ok(config)
    }
    
    /// Validate configuration parameters
    pub fn validate(&self) -> Result<(), crate::EngineError> {
        // Validate JWT secret is not all zeros
        if self.jwt_secret == [0u8; 32] {
            return Err(crate::EngineError::ConfigError(
                "JWT secret must be properly configured".to_string()
            ));
        }
        
        // Validate URLs
        if self.engine_url.is_empty() {
            return Err(crate::EngineError::ConfigError(
                "Engine URL cannot be empty".to_string()
            ));
        }
        
        // Validate timeouts are reasonable
        if self.engine_timeout < Duration::from_millis(100) {
            return Err(crate::EngineError::ConfigError(
                "Engine timeout too short (minimum 100ms)".to_string()
            ));
        }
        
        if self.payload_build_timeout > Duration::from_secs(5) {
            return Err(crate::EngineError::ConfigError(
                "Payload build timeout too long (maximum 5s)".to_string()
            ));
        }
        
        // Validate performance parameters
        if self.performance.connection_pool_size == 0 {
            return Err(crate::EngineError::ConfigError(
                "Connection pool size must be at least 1".to_string()
            ));
        }
        
        if self.max_concurrent_payloads == 0 {
            return Err(crate::EngineError::ConfigError(
                "Max concurrent payloads must be at least 1".to_string()
            ));
        }
        
        Ok(())
    }
    
    /// Get the effective engine API URL with JWT authentication
    pub fn engine_api_url(&self) -> String {
        self.engine_url.clone()
    }
    
    /// Get the public API URL for queries
    pub fn public_api_url(&self) -> Option<String> {
        self.public_url.clone()
    }
}