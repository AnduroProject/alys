//! Configuration for governance client.

use std::time::Duration;

/// Configuration for the GovernanceClientActor.
#[derive(Debug, Clone)]
pub struct GovernanceConfig {
    /// gRPC URL of the governance service (e.g., "http://governance:50051")
    pub grpc_url: String,

    /// Authentication token for the governance service
    pub auth_token: String,

    /// Chain ID to identify this chain to governance
    pub chain_id: String,

    /// Interval between reconnection attempts
    pub reconnect_interval: Duration,

    /// Interval between heartbeats to keep connection alive
    pub heartbeat_interval: Duration,

    /// Timeout for peg-in verification requests
    pub verify_timeout: Duration,
}

impl Default for GovernanceConfig {
    fn default() -> Self {
        Self {
            grpc_url: "http://localhost:50051".to_string(),
            auth_token: "test-token-123".to_string(),
            chain_id: "alys-regtest".to_string(),
            reconnect_interval: Duration::from_secs(5),
            heartbeat_interval: Duration::from_secs(30),
            verify_timeout: Duration::from_secs(30),
        }
    }
}
