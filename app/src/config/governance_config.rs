//! Governance integration configuration

use super::*;
use std::net::SocketAddr;
use std::time::Duration;

/// Governance integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
    /// Enable governance integration
    pub enabled: bool,
    
    /// gRPC client configuration
    pub grpc: GrpcConfig,
    
    /// Governance endpoints
    pub endpoints: Vec<GovernanceEndpoint>,
    
    /// Authentication configuration
    pub auth: AuthConfig,
    
    /// Stream configuration
    pub streaming: StreamConfig,
    
    /// Federation configuration
    pub federation: FederationConfig,
}

/// gRPC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    /// Connection timeout
    pub connect_timeout: Duration,
    
    /// Request timeout
    pub request_timeout: Duration,
    
    /// Keep alive interval
    pub keep_alive_interval: Duration,
    
    /// Keep alive timeout
    pub keep_alive_timeout: Duration,
    
    /// Enable TLS
    pub enable_tls: bool,
    
    /// TLS configuration
    pub tls: Option<TlsConfig>,
    
    /// Maximum message size
    pub max_message_size: u32,
}

/// TLS configuration for gRPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// CA certificate file
    pub ca_cert_file: String,
    
    /// Client certificate file
    pub client_cert_file: Option<String>,
    
    /// Client private key file
    pub client_key_file: Option<String>,
    
    /// Server name for SNI
    pub server_name: Option<String>,
    
    /// Skip certificate verification (development only)
    pub skip_verification: bool,
}

/// Governance endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceEndpoint {
    /// Endpoint name
    pub name: String,
    
    /// Endpoint URL
    pub url: String,
    
    /// Priority (lower is higher priority)
    pub priority: u32,
    
    /// Weight for load balancing
    pub weight: u32,
    
    /// Enable this endpoint
    pub enabled: bool,
    
    /// Health check configuration
    pub health_check: EndpointHealthConfig,
}

/// Endpoint health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointHealthConfig {
    /// Enable health checks
    pub enabled: bool,
    
    /// Health check interval
    pub interval: Duration,
    
    /// Health check timeout
    pub timeout: Duration,
    
    /// Failure threshold
    pub failure_threshold: u32,
    
    /// Recovery threshold
    pub recovery_threshold: u32,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Authentication method
    pub method: AuthMethod,
    
    /// Token refresh configuration
    pub token_refresh: TokenRefreshConfig,
}

/// Authentication methods
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthMethod {
    /// No authentication
    None,
    /// API key authentication
    ApiKey {
        key: String,
        header: String,
    },
    /// JWT token authentication
    Jwt {
        token: String,
        header: String,
    },
    /// mTLS authentication
    Mtls {
        cert_file: String,
        key_file: String,
    },
}

/// Token refresh configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRefreshConfig {
    /// Enable automatic token refresh
    pub enabled: bool,
    
    /// Refresh interval
    pub interval: Duration,
    
    /// Refresh endpoint
    pub endpoint: Option<String>,
    
    /// Refresh credentials
    pub credentials: Option<String>,
}

/// Stream configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Enable bi-directional streaming
    pub enabled: bool,
    
    /// Stream keep-alive interval
    pub keep_alive_interval: Duration,
    
    /// Stream timeout
    pub stream_timeout: Duration,
    
    /// Reconnection configuration
    pub reconnection: ReconnectionConfig,
    
    /// Message buffer size
    pub buffer_size: usize,
    
    /// Enable compression
    pub compression: bool,
}

/// Reconnection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectionConfig {
    /// Enable automatic reconnection
    pub enabled: bool,
    
    /// Initial retry delay
    pub initial_delay: Duration,
    
    /// Maximum retry delay
    pub max_delay: Duration,
    
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    
    /// Maximum retry attempts
    pub max_attempts: u32,
    
    /// Jitter factor (0.0 to 1.0)
    pub jitter: f64,
}

/// Federation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    /// Federation ID
    pub federation_id: String,
    
    /// Member ID
    pub member_id: String,
    
    /// Signature threshold
    pub signature_threshold: u32,
    
    /// Maximum members
    pub max_members: u32,
    
    /// Voting configuration
    pub voting: VotingConfig,
    
    /// Consensus configuration
    pub consensus: ConsensusConfig,
}

/// Voting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VotingConfig {
    /// Voting timeout
    pub timeout: Duration,
    
    /// Minimum quorum percentage
    pub min_quorum: f64,
    
    /// Super majority threshold
    pub super_majority: f64,
    
    /// Enable weighted voting
    pub weighted_voting: bool,
}

/// Consensus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// Consensus algorithm
    pub algorithm: ConsensusAlgorithm,
    
    /// Consensus timeout
    pub timeout: Duration,
    
    /// Maximum consensus rounds
    pub max_rounds: u32,
    
    /// Round timeout
    pub round_timeout: Duration,
}

/// Consensus algorithms
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsensusAlgorithm {
    /// Byzantine fault tolerant consensus
    Bft,
    /// Practical Byzantine fault tolerance
    Pbft,
    /// HoneyBadgerBFT
    HoneyBadger,
    /// Simple majority
    SimpleMajority,
}

impl Default for GovernanceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            grpc: GrpcConfig::default(),
            endpoints: vec![GovernanceEndpoint::default()],
            auth: AuthConfig::default(),
            streaming: StreamConfig::default(),
            federation: FederationConfig::default(),
        }
    }
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            keep_alive_interval: Duration::from_secs(30),
            keep_alive_timeout: Duration::from_secs(5),
            enable_tls: true,
            tls: Some(TlsConfig::default()),
            max_message_size: 4 * 1024 * 1024, // 4MB
        }
    }
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            ca_cert_file: "./certs/ca.pem".to_string(),
            client_cert_file: Some("./certs/client.pem".to_string()),
            client_key_file: Some("./certs/client.key".to_string()),
            server_name: None,
            skip_verification: false,
        }
    }
}

impl Default for GovernanceEndpoint {
    fn default() -> Self {
        Self {
            name: "primary".to_string(),
            url: "https://governance.anduro.io:443".to_string(),
            priority: 1,
            weight: 100,
            enabled: true,
            health_check: EndpointHealthConfig::default(),
        }
    }
}

impl Default for EndpointHealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
            failure_threshold: 3,
            recovery_threshold: 2,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            method: AuthMethod::None,
            token_refresh: TokenRefreshConfig::default(),
        }
    }
}

impl Default for TokenRefreshConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: Duration::from_secs(3600), // 1 hour
            endpoint: None,
            credentials: None,
        }
    }
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            keep_alive_interval: Duration::from_secs(30),
            stream_timeout: Duration::from_secs(300),
            reconnection: ReconnectionConfig::default(),
            buffer_size: 1000,
            compression: true,
        }
    }
}

impl Default for ReconnectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_attempts: 10,
            jitter: 0.1,
        }
    }
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            federation_id: "alys_federation".to_string(),
            member_id: uuid::Uuid::new_v4().to_string(),
            signature_threshold: 2,
            max_members: 5,
            voting: VotingConfig::default(),
            consensus: ConsensusConfig::default(),
        }
    }
}

impl Default for VotingConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300), // 5 minutes
            min_quorum: 0.67, // 2/3 majority
            super_majority: 0.75, // 3/4 for critical decisions
            weighted_voting: false,
        }
    }
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            algorithm: ConsensusAlgorithm::Bft,
            timeout: Duration::from_secs(30),
            max_rounds: 10,
            round_timeout: Duration::from_secs(3),
        }
    }
}

impl Validate for GovernanceConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.endpoints.is_empty() {
            return Err(ConfigError::ValidationError {
                field: "governance.endpoints".to_string(),
                reason: "At least one governance endpoint must be configured".to_string(),
            });
        }
        
        if self.federation.signature_threshold == 0 {
            return Err(ConfigError::ValidationError {
                field: "governance.federation.signature_threshold".to_string(),
                reason: "Signature threshold must be greater than 0".to_string(),
            });
        }
        
        if self.federation.signature_threshold > self.federation.max_members {
            return Err(ConfigError::ValidationError {
                field: "governance.federation".to_string(),
                reason: "Signature threshold cannot exceed max members".to_string(),
            });
        }
        
        Ok(())
    }
}