//! Configuration for mock governance server.

use clap::Parser;
use std::time::Duration;

/// Mock governance server for Alys testnet.
///
/// Provides a gRPC server implementing the GovernanceService for testing validator
/// integration. Supports auto-ACK for peg-ins and chaos injection modes.
#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
pub struct Config {
    /// gRPC listen address
    #[arg(long, default_value = "0.0.0.0:50051")]
    pub listen_addr: String,

    /// Authentication token for validators
    #[arg(long, default_value = "test-token-123")]
    pub auth_token: String,

    /// Chain ID to validate against
    #[arg(long, default_value = "alys-regtest")]
    pub chain_id: String,

    /// Automatically ACK all peg-in verification requests
    #[arg(long)]
    pub auto_ack_pegins: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    // === Chaos Testing Options ===

    /// Enable chaos testing mode
    #[arg(long)]
    pub chaos_mode: bool,

    /// Rate of peg-in rejections (0.0 - 1.0)
    #[arg(long, default_value = "0.0")]
    pub pegin_reject_rate: f64,

    /// Response delay in milliseconds
    #[arg(long, default_value = "0")]
    pub response_delay_ms: u64,

    /// Disconnect after N requests (0 = never)
    #[arg(long, default_value = "0")]
    pub disconnect_after: u64,

    /// Push validator set update every N seconds (0 = never)
    #[arg(long, default_value = "0")]
    pub push_validator_update_interval: u64,

    // === Dynamic Validator Addition Options ===

    /// BLS public key (hex) for validator update push
    /// If specified, uses this key instead of generating a random one
    #[arg(long)]
    pub validator_update_pubkey: Option<String>,

    /// Voting power for validator update (default: 100)
    #[arg(long, default_value = "100")]
    pub validator_update_power: u64,

    /// Delay before pushing validator update (seconds after first connection)
    /// Only used when validator_update_pubkey is specified
    #[arg(long, default_value = "60")]
    pub validator_update_delay: u64,

    /// Push validator update only once (after delay), then stop
    /// If false, uses push_validator_update_interval for periodic updates
    #[arg(long)]
    pub validator_update_one_shot: bool,
}

impl Config {
    /// Get response delay as Duration
    pub fn response_delay(&self) -> Duration {
        Duration::from_millis(self.response_delay_ms)
    }

    /// Get validator update push interval as Duration (None if disabled)
    pub fn validator_update_interval(&self) -> Option<Duration> {
        if self.push_validator_update_interval > 0 {
            Some(Duration::from_secs(self.push_validator_update_interval))
        } else {
            None
        }
    }
}
