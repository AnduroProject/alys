//! Bridge System Constants
//! 
//! Centralized constants used across bridge operations

use std::time::Duration;

/// Bitcoin dust limit - minimum value for a spendable output
pub const DUST_LIMIT: u64 = 546;

/// Maximum retry attempts for failed operations
pub const MAX_RETRY_ATTEMPTS: u32 = 3;

/// Default operation timeout
pub const OPERATION_TIMEOUT: Duration = Duration::from_secs(3600); // 1 hour

/// Minimum Bitcoin confirmations for peg-ins
pub const MIN_PEGIN_CONFIRMATIONS: u32 = 6;

/// Minimum Bitcoin confirmations for peg-outs
pub const MIN_PEGOUT_CONFIRMATIONS: u32 = 6;

/// Maximum concurrent peg-in operations
pub const MAX_CONCURRENT_PEGINS: usize = 100;

/// Maximum concurrent peg-out operations
pub const MAX_CONCURRENT_PEGOUTS: usize = 50;

/// Default fee rate in satoshis per vByte
pub const DEFAULT_FEE_RATE: u64 = 10;

/// Maximum fee rate to prevent excessive fees
pub const MAX_FEE_RATE: u64 = 1000;

/// Signature collection timeout
pub const SIGNATURE_TIMEOUT: Duration = Duration::from_secs(120);

/// Heartbeat interval for health checks
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Actor restart delay after failure
pub const ACTOR_RESTART_DELAY: Duration = Duration::from_secs(5);

/// Maximum actor restart attempts
pub const MAX_ACTOR_RESTARTS: u32 = 5;

/// UTXO refresh interval
pub const UTXO_REFRESH_INTERVAL: Duration = Duration::from_secs(120);

/// Message processing timeout
pub const MESSAGE_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum message buffer size
pub const MAX_MESSAGE_BUFFER: usize = 10000;

/// Connection timeout for external services
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Reconnection attempts for external services
pub const MAX_RECONNECTION_ATTEMPTS: u32 = 5;

/// Reconnection delay
pub const RECONNECTION_DELAY: Duration = Duration::from_secs(5);

/// Federation threshold (minimum signatures required)
pub const FEDERATION_THRESHOLD: usize = 2;

/// Maximum peg-out amount (10 BTC in satoshis)
pub const MAX_PEGOUT_AMOUNT: u64 = 1_000_000_000;

/// Minimum peg-in amount to prevent spam
pub const MIN_PEGIN_AMOUNT: u64 = 10_000; // 0.0001 BTC

/// Minimum peg-out amount
pub const MIN_PEGOUT_AMOUNT: u64 = 10_000; // 0.0001 BTC

/// Bridge actor names for identification
pub mod actor_names {
    pub const BRIDGE_SUPERVISOR: &str = "bridge_supervisor";
    pub const BRIDGE_COORDINATOR: &str = "bridge_coordinator";
    pub const PEGIN_ACTOR: &str = "pegin_actor";
    pub const PEGOUT_ACTOR: &str = "pegout_actor";
    pub const STREAM_ACTOR: &str = "stream_actor";
}

/// Metrics collection intervals
pub mod metrics {
    use std::time::Duration;
    
    pub const COLLECTION_INTERVAL: Duration = Duration::from_secs(10);
    pub const AGGREGATION_INTERVAL: Duration = Duration::from_secs(60);
    pub const RETENTION_PERIOD: Duration = Duration::from_secs(86400); // 24 hours
}

/// Error codes for bridge operations
pub mod error_codes {
    pub const INSUFFICIENT_FUNDS: u32 = 1001;
    pub const INVALID_ADDRESS: u32 = 1002;
    pub const SIGNATURE_FAILURE: u32 = 1003;
    pub const TIMEOUT_ERROR: u32 = 1004;
    pub const NETWORK_ERROR: u32 = 1005;
    pub const VALIDATION_ERROR: u32 = 1006;
    pub const ACTOR_FAILURE: u32 = 1007;
    pub const INTERNAL_ERROR: u32 = 1999;
}

/// Transaction size estimates for fee calculation
pub mod tx_sizes {
    /// Base transaction size (version, locktime, input/output counts)
    pub const BASE_SIZE: usize = 10;
    
    /// P2WPKH input size
    pub const P2WPKH_INPUT_SIZE: usize = 68;
    
    /// P2SH-wrapped P2WPKH input size
    pub const P2SH_P2WPKH_INPUT_SIZE: usize = 91;
    
    /// Taproot input size
    pub const TAPROOT_INPUT_SIZE: usize = 57;
    
    /// P2WPKH output size
    pub const P2WPKH_OUTPUT_SIZE: usize = 31;
    
    /// P2SH output size
    pub const P2SH_OUTPUT_SIZE: usize = 32;
    
    /// Taproot output size
    pub const TAPROOT_OUTPUT_SIZE: usize = 43;
    
    /// OP_RETURN output size (for peg-in address encoding)
    pub const OP_RETURN_OUTPUT_SIZE: usize = 43;
}