//! Bridge System Configuration
//! 
//! Unified configuration system for all bridge actors and operations

use bitcoin::{Address as BtcAddress, Network};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::types::*;

/// Comprehensive bridge system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeSystemConfig {
    /// Core bridge configuration
    pub bridge: BridgeConfig,
    
    /// Peg-in specific configuration
    pub pegin: PegInConfig,
    
    /// Peg-out specific configuration
    pub pegout: PegOutConfig,
    
    /// Stream actor configuration
    pub stream: StreamConfig,
    
    /// Supervision configuration
    pub supervision: SupervisionConfig,
    
    /// Migration mode for gradual rollout
    pub migration_mode: MigrationMode,
}

/// Core bridge configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub required_confirmations: u32,
    pub bitcoin_network: Network,
    pub federation_threshold: usize,
    pub max_concurrent_operations: usize,
    pub operation_timeout: Duration,
    pub health_check_interval: Duration,
}

/// Peg-in actor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegInConfig {
    pub confirmation_threshold: u32,
    pub monitoring_interval: Duration,
    pub max_pending_deposits: usize,
    pub validation_timeout: Duration,
    pub retry_attempts: u32,
    pub retry_delay: Duration,
}

/// Peg-out actor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegOutConfig {
    pub signature_timeout: Duration,
    pub transaction_fee_rate: u64,
    pub max_pending_pegouts: usize,
    pub utxo_selection_strategy: UtxoSelectionStrategy,
    pub broadcast_retry_attempts: u32,
    pub broadcast_retry_delay: Duration,
}

/// Stream actor configuration for bridge integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    pub governance_endpoints: Vec<String>,
    pub connection_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub max_connections: usize,
    pub message_buffer_size: usize,
    pub reconnect_attempts: u32,
    pub reconnect_delay: Duration,
    
    /// TLS certificate paths
    pub ca_cert_path: Option<String>,
    pub client_cert_path: Option<String>,
    pub client_key_path: Option<String>,
    
    /// Authentication token
    pub auth_token: Option<String>,
}

/// Bridge supervision configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisionConfig {
    pub health_check_interval: Duration,
    pub failure_threshold: u32,
    pub restart_delay: Duration,
    pub max_restart_attempts: u32,
    pub escalation_timeout: Duration,
}

/// UTXO selection strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UtxoSelectionStrategy {
    /// Select oldest UTXOs first
    OldestFirst,
    /// Select largest UTXOs first  
    LargestFirst,
    /// Select UTXOs to minimize fees
    MinimizeFees,
    /// Random selection
    Random,
}

/// Migration mode for gradual rollout
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationMode {
    /// Use legacy monolithic BridgeActor
    Legacy,
    /// Gradual migration with fallback
    Hybrid,
    /// Full specialized actor system
    Specialized,
}

impl Default for BridgeSystemConfig {
    fn default() -> Self {
        Self {
            bridge: BridgeConfig::default(),
            pegin: PegInConfig::default(),
            pegout: PegOutConfig::default(),
            stream: StreamConfig::default(),
            supervision: SupervisionConfig::default(),
            migration_mode: MigrationMode::Specialized,
        }
    }
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            required_confirmations: 6,
            bitcoin_network: Network::Regtest,
            federation_threshold: 2,
            max_concurrent_operations: 100,
            operation_timeout: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(30),
        }
    }
}

impl Default for PegInConfig {
    fn default() -> Self {
        Self {
            confirmation_threshold: 6,
            monitoring_interval: Duration::from_secs(30),
            max_pending_deposits: 1000,
            validation_timeout: Duration::from_secs(60),
            retry_attempts: 3,
            retry_delay: Duration::from_secs(5),
        }
    }
}

impl Default for PegOutConfig {
    fn default() -> Self {
        Self {
            signature_timeout: Duration::from_secs(120),
            transaction_fee_rate: 10, // sat/vB
            max_pending_pegouts: 500,
            utxo_selection_strategy: UtxoSelectionStrategy::MinimizeFees,
            broadcast_retry_attempts: 3,
            broadcast_retry_delay: Duration::from_secs(10),
        }
    }
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            governance_endpoints: vec!["https://governance.anduro.io:443".to_string()],
            connection_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
            max_connections: 10,
            message_buffer_size: 1000,
            reconnect_attempts: 5,
            reconnect_delay: Duration::from_secs(5),
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
            auth_token: None,
        }
    }
}

impl Default for SupervisionConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(10),
            failure_threshold: 3,
            restart_delay: Duration::from_secs(5),
            max_restart_attempts: 5,
            escalation_timeout: Duration::from_secs(300),
        }
    }
}