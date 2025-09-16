//! Configuration management for the Alys V2 actor system
//!
//! This module provides comprehensive configuration structures and management
//! for the V2 actor-based architecture, including environment-specific overrides,
//! validation, and hot-reload capabilities.

pub mod alys_config;
pub mod actor_config;
pub mod sync_config;
pub mod governance_config;
pub mod chain_config;
pub mod network_config;
pub mod bridge_config;
pub mod storage_config;
pub mod execution_config;
pub mod hot_reload;

/// Bitcoin configuration for node connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitcoinConfig {
    /// Bitcoin node RPC URL
    pub rpc_url: String,
    /// Bitcoin node RPC username
    pub rpc_username: Option<String>,
    /// Bitcoin node RPC password
    pub rpc_password: Option<String>,
    /// Connection timeout in seconds
    pub timeout: u64,
}

impl Default for BitcoinConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://localhost:8332".to_string(),
            rpc_username: None,
            rpc_password: None,
            timeout: 30,
        }
    }
}

impl BitcoinConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            rpc_url: std::env::var("BITCOIN_RPC_URL").unwrap_or_else(|_| "http://localhost:8332".to_string()),
            rpc_username: std::env::var("BITCOIN_RPC_USER").ok(),
            rpc_password: std::env::var("BITCOIN_RPC_PASS").ok(),
            timeout: std::env::var("BITCOIN_RPC_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
        })
    }
}

// Re-exports for convenience
pub use alys_config::*;
pub use actor_config::*;
pub use sync_config::*;
pub use governance_config::*;
pub use chain_config::*;
pub use network_config::*;
pub use bridge_config::*;
pub use storage_config::*;
pub use execution_config::*;
pub use hot_reload::*;

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

/// Configuration errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Configuration file not found: {path}")]
    FileNotFound { path: String },
    
    #[error("Configuration parse error: {reason}")]
    ParseError { reason: String },
    
    #[error("Configuration validation error: {field} - {reason}")]
    ValidationError { field: String, reason: String },
    
    #[error("Environment variable error: {var} - {reason}")]
    EnvVarError { var: String, reason: String },
    
    #[error("IO error during {operation}: {error}")]
    IoError { operation: String, error: String },
    
    #[error("Serialization error: {reason}")]
    SerializationError { reason: String },
}

/// Configuration validation trait
pub trait Validate {
    fn validate(&self) -> Result<(), ConfigError>;
}

/// Configuration loading trait
pub trait ConfigLoader<T> {
    fn load_from_file<P: AsRef<Path>>(path: P) -> Result<T, ConfigError>;
    fn load_from_env() -> Result<T, ConfigError>;
    fn load_with_overrides<P: AsRef<Path>>(
        path: P,
        env_prefix: Option<&str>,
    ) -> Result<T, ConfigError>;
}

/// Configuration hot-reload support
pub trait HotReload {
    fn supports_hot_reload(&self) -> bool;
    fn reload_config(&mut self, new_config: Self) -> Result<(), ConfigError>;
}

/// Environment types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Development,
    Testing,
    Staging,
    Production,
}

impl Default for Environment {
    fn default() -> Self {
        Environment::Development
    }
}

impl std::str::FromStr for Environment {
    type Err = ConfigError;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "development" | "dev" => Ok(Environment::Development),
            "testing" | "test" => Ok(Environment::Testing),
            "staging" | "stage" => Ok(Environment::Staging),
            "production" | "prod" => Ok(Environment::Production),
            _ => Err(ConfigError::ValidationError {
                field: "environment".to_string(),
                reason: format!("Invalid environment: {}", s),
            }),
        }
    }
}