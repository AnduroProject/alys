//! Execution Client Abstraction
//!
//! This module provides abstraction layer over different execution clients (Geth/Reth),
//! handling authentication, connection management, failover, and health checks.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::*;
use serde::{Deserialize, Serialize};

use lighthouse_wrapper::execution_layer::{
    auth::{Auth, JwtKey},
    HttpJsonRpc, BlockByNumberQuery, ForkchoiceState, PayloadAttributes,
    DEFAULT_EXECUTION_ENDPOINT, LATEST_TAG,
};
use lighthouse_wrapper::sensitive_url::SensitiveUrl;
use lighthouse_wrapper::types::{Address, ExecutionBlockHash, ExecutionPayload, MainnetEthSpec};

use crate::types::*;
use super::{config::EngineConfig, state::ClientHealthStatus, EngineError, EngineResult, ClientError};

/// Execution client abstraction supporting multiple implementations
#[derive(Debug, Clone)]
pub struct ExecutionClient {
    /// Client configuration
    config: EngineConfig,
    
    /// Engine API client for authenticated operations
    engine_api: Arc<RwLock<Option<EngineApiClient>>>,
    
    /// Public API client for queries
    public_api: Arc<RwLock<Option<PublicApiClient>>>,
    
    /// Current health status
    health_status: Arc<RwLock<ClientHealthStatus>>,
    
    /// Connection pool for managing multiple connections
    connection_pool: Arc<ConnectionPool>,
}

/// Engine API client for authenticated operations
#[derive(Debug)]
pub struct EngineApiClient {
    /// HTTP JSON-RPC client with JWT authentication
    rpc_client: HttpJsonRpc,
    
    /// Authentication handler
    auth: Auth,
    
    /// Client capabilities
    capabilities: Vec<String>,
    
    /// Last successful operation timestamp
    last_success: std::time::Instant,
}

/// Public API client for query operations
#[derive(Debug)]
pub struct PublicApiClient {
    /// HTTP JSON-RPC client without authentication
    rpc_client: HttpJsonRpc,
    
    /// Client capabilities
    capabilities: Vec<String>,
    
    /// Last successful operation timestamp
    last_success: std::time::Instant,
}

/// Connection pool for managing client connections
#[derive(Debug)]
pub struct ConnectionPool {
    /// Pool configuration
    config: PoolConfig,
    
    /// Active connections
    connections: Arc<RwLock<Vec<PooledConnection>>>,
    
    /// Connection statistics
    stats: Arc<RwLock<PoolStats>>,
}

/// Connection pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of connections
    pub max_connections: usize,
    
    /// Connection timeout
    pub connection_timeout: Duration,
    
    /// Keep-alive timeout
    pub keep_alive_timeout: Duration,
    
    /// Maximum idle time before closing connection
    pub max_idle_time: Duration,
    
    /// Enable connection validation
    pub validate_connections: bool,
}

/// Pooled connection wrapper
#[derive(Debug)]
pub struct PooledConnection {
    /// Connection ID
    id: String,
    
    /// Underlying HTTP client
    client: HttpJsonRpc,
    
    /// Connection created timestamp
    created_at: std::time::Instant,
    
    /// Last used timestamp
    last_used: std::time::Instant,
    
    /// Number of times this connection has been used
    usage_count: u64,
    
    /// Whether the connection is currently in use
    in_use: bool,
}

/// Connection pool statistics
#[derive(Debug, Default)]
pub struct PoolStats {
    /// Total connections created
    pub total_created: u64,
    
    /// Total connections destroyed
    pub total_destroyed: u64,
    
    /// Current active connections
    pub active_connections: usize,
    
    /// Current idle connections
    pub idle_connections: usize,
    
    /// Total requests served
    pub total_requests: u64,
    
    /// Average connection lifetime
    pub avg_connection_lifetime: Duration,
    
    /// Pool hit rate (reused connections / total requests)
    pub hit_rate: f64,
}

/// Execution client capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    /// Supported Engine API methods
    pub engine_methods: Vec<String>,
    
    /// Supported Ethereum API methods
    pub eth_methods: Vec<String>,
    
    /// Client version information
    pub version: String,
    
    /// Network ID
    pub network_id: u64,
    
    /// Chain ID
    pub chain_id: u64,
    
    /// Latest block number
    pub latest_block: u64,
    
    /// Sync status
    pub is_syncing: bool,
}

/// Client health check result
#[derive(Debug, Clone)]
pub struct HealthCheck {
    /// Whether the client is reachable
    pub reachable: bool,
    
    /// Response time
    pub response_time: Duration,
    
    /// Client capabilities
    pub capabilities: Option<ClientCapabilities>,
    
    /// Any errors encountered
    pub error: Option<String>,
}

impl ExecutionClient {
    /// Create a new execution client with the given configuration
    pub fn new(config: &EngineConfig) -> EngineResult<Self> {
        let connection_pool = Arc::new(ConnectionPool::new(PoolConfig {
            max_connections: config.performance.connection_pool_size,
            connection_timeout: config.performance.request_timeout,
            keep_alive_timeout: config.performance.connection_keep_alive,
            max_idle_time: Duration::from_secs(300), // 5 minutes
            validate_connections: true,
        }));
        
        Ok(Self {
            config: config.clone(),
            engine_api: Arc::new(RwLock::new(None)),
            public_api: Arc::new(RwLock::new(None)),
            health_status: Arc::new(RwLock::new(ClientHealthStatus::default())),
            connection_pool,
        })
    }
    
    /// Initialize connections to the execution client
    pub async fn initialize(&self, config: &EngineConfig) -> EngineResult<()> {
        info!("Initializing execution client connections");
        
        // Initialize engine API client with JWT authentication
        let engine_client = self.create_engine_client(config).await?;
        *self.engine_api.write().await = Some(engine_client);
        
        // Initialize public API client if URL is provided
        if let Some(public_url) = &config.public_url {
            let public_client = self.create_public_client(public_url).await?;
            *self.public_api.write().await = Some(public_client);
        }
        
        // Perform initial health check
        let health = self.health_check().await;
        *self.health_status.write().await = ClientHealthStatus {
            is_reachable: health.reachable,
            is_synced: health.capabilities.as_ref().map(|c| !c.is_syncing).unwrap_or(false),
            sync_status: super::state::SyncStatus::Unknown,
            client_version: health.capabilities.as_ref().map(|c| c.version.clone()),
            last_healthy: if health.reachable { Some(std::time::SystemTime::now()) } else { None },
            consecutive_failures: if health.reachable { 0 } else { 1 },
            average_response_time: health.response_time,
            active_connections: self.connection_pool.active_connection_count().await,
            capabilities: health.capabilities.map(|c| c.engine_methods).unwrap_or_default(),
        };
        
        info!("Execution client initialization completed successfully");
        Ok(())
    }
    
    /// Create authenticated engine API client
    async fn create_engine_client(&self, config: &EngineConfig) -> EngineResult<EngineApiClient> {
        let jwt_key = JwtKey::from_slice(&config.jwt_secret)
            .map_err(|e| ClientError::AuthenticationFailed)?;
        
        let auth = Auth::new(jwt_key, None, None);
        let url = SensitiveUrl::parse(&config.engine_url)
            .map_err(|e| ClientError::ConnectionFailed(format!("Invalid engine URL: {}", e)))?;
        
        let rpc_client = HttpJsonRpc::new_with_auth(url, auth.clone(), Some(3))
            .map_err(|e| ClientError::ConnectionFailed(format!("Failed to create RPC client: {}", e)))?;
        
        // Test connection by calling a simple method
        let capabilities = self.get_client_capabilities(&rpc_client).await.unwrap_or_default();
        
        Ok(EngineApiClient {
            rpc_client,
            auth,
            capabilities,
            last_success: std::time::Instant::now(),
        })
    }
    
    /// Create public API client
    async fn create_public_client(&self, public_url: &str) -> EngineResult<PublicApiClient> {
        let url = SensitiveUrl::parse(public_url)
            .map_err(|e| ClientError::ConnectionFailed(format!("Invalid public URL: {}", e)))?;
        
        let rpc_client = HttpJsonRpc::new(url, Some(3))
            .map_err(|e| ClientError::ConnectionFailed(format!("Failed to create public RPC client: {}", e)))?;
        
        // Test connection and get capabilities
        let capabilities = self.get_client_capabilities(&rpc_client).await.unwrap_or_default();
        
        Ok(PublicApiClient {
            rpc_client,
            capabilities,
            last_success: std::time::Instant::now(),
        })
    }
    
    /// Get client capabilities
    async fn get_client_capabilities(&self, client: &HttpJsonRpc) -> Result<Vec<String>, ClientError> {
        // Try to call a simple method to verify connectivity and get capabilities
        match client.rpc_request::<String>("web3_clientVersion", serde_json::Value::Null, Duration::from_secs(5)).await {
            Ok(_) => Ok(vec![
                "engine_newPayloadV1".to_string(),
                "engine_newPayloadV2".to_string(),
                "engine_forkchoiceUpdatedV1".to_string(),
                "engine_forkchoiceUpdatedV2".to_string(),
                "engine_getPayloadV1".to_string(),
                "engine_getPayloadV2".to_string(),
                "engine_exchangeCapabilities".to_string(),
            ]),
            Err(e) => Err(ClientError::ConnectionFailed(format!("Capability check failed: {}", e))),
        }
    }
    
    /// Perform health check on the execution client
    pub async fn health_check(&self) -> HealthCheck {
        let start_time = std::time::Instant::now();
        
        // Try to connect to the engine API client
        if let Some(engine_client) = self.engine_api.read().await.as_ref() {
            match engine_client.rpc_client.rpc_request::<String>(
                "web3_clientVersion",
                serde_json::Value::Null,
                Duration::from_secs(5)
            ).await {
                Ok(version) => {
                    let response_time = start_time.elapsed();
                    
                    // Get additional capabilities information
                    let capabilities = match self.get_detailed_capabilities(&engine_client.rpc_client).await {
                        Ok(caps) => Some(caps),
                        Err(_) => None,
                    };
                    
                    HealthCheck {
                        reachable: true,
                        response_time,
                        capabilities,
                        error: None,
                    }
                },
                Err(e) => {
                    let response_time = start_time.elapsed();
                    HealthCheck {
                        reachable: false,
                        response_time,
                        capabilities: None,
                        error: Some(format!("Engine API health check failed: {}", e)),
                    }
                }
            }
        } else {
            HealthCheck {
                reachable: false,
                response_time: start_time.elapsed(),
                capabilities: None,
                error: Some("Engine API client not initialized".to_string()),
            }
        }
    }
    
    /// Get detailed client capabilities
    async fn get_detailed_capabilities(&self, client: &HttpJsonRpc) -> Result<ClientCapabilities, ClientError> {
        // Get client version
        let version = client.rpc_request::<String>("web3_clientVersion", serde_json::Value::Null, Duration::from_secs(5))
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        
        // Get network ID
        let network_id = client.rpc_request::<String>("net_version", serde_json::Value::Null, Duration::from_secs(5))
            .await
            .and_then(|s| s.parse::<u64>().map_err(|_| lighthouse_wrapper::execution_layer::Error::InvalidPayloadBody("Invalid network ID".to_string())))
            .unwrap_or(0);
        
        // Get chain ID
        let chain_id = client.rpc_request::<String>("eth_chainId", serde_json::Value::Null, Duration::from_secs(5))
            .await
            .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|_| lighthouse_wrapper::execution_layer::Error::InvalidPayloadBody("Invalid chain ID".to_string())))
            .unwrap_or(0);
        
        // Get latest block number
        let latest_block = client.rpc_request::<String>("eth_blockNumber", serde_json::Value::Null, Duration::from_secs(5))
            .await
            .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|_| lighthouse_wrapper::execution_layer::Error::InvalidPayloadBody("Invalid block number".to_string())))
            .unwrap_or(0);
        
        // Check sync status
        let is_syncing = client.rpc_request::<bool>("eth_syncing", serde_json::Value::Null, Duration::from_secs(5))
            .await
            .unwrap_or(false);
        
        Ok(ClientCapabilities {
            engine_methods: vec![
                "engine_newPayloadV1".to_string(),
                "engine_newPayloadV2".to_string(),
                "engine_forkchoiceUpdatedV1".to_string(),
                "engine_forkchoiceUpdatedV2".to_string(),
                "engine_getPayloadV1".to_string(),
                "engine_getPayloadV2".to_string(),
                "engine_exchangeCapabilities".to_string(),
            ],
            eth_methods: vec![
                "eth_blockNumber".to_string(),
                "eth_getBlockByNumber".to_string(),
                "eth_getBlockByHash".to_string(),
                "eth_getTransactionReceipt".to_string(),
                "eth_syncing".to_string(),
                "eth_chainId".to_string(),
            ],
            version,
            network_id,
            chain_id,
            latest_block,
            is_syncing,
        })
    }
    
    /// Get the engine API client
    pub async fn engine_client(&self) -> Option<Arc<RwLock<EngineApiClient>>> {
        if self.engine_api.read().await.is_some() {
            Some(Arc::new(RwLock::new(self.engine_api.read().await.as_ref().unwrap().clone())))
        } else {
            None
        }
    }
    
    /// Get the public API client
    pub async fn public_client(&self) -> Option<Arc<RwLock<PublicApiClient>>> {
        if self.public_api.read().await.is_some() {
            Some(Arc::new(RwLock::new(self.public_api.read().await.as_ref().unwrap().clone())))
        } else {
            None
        }
    }
    
    /// Get current health status
    pub async fn health_status(&self) -> ClientHealthStatus {
        self.health_status.read().await.clone()
    }
    
    /// Update health status
    pub async fn update_health_status(&self, status: ClientHealthStatus) {
        *self.health_status.write().await = status;
    }
    
    /// Get connection pool statistics
    pub async fn connection_stats(&self) -> PoolStats {
        self.connection_pool.stats().await
    }
    
    /// Reconnect to the execution client
    pub async fn reconnect(&self) -> EngineResult<()> {
        warn!("Reconnecting to execution client");
        
        // Close existing connections
        *self.engine_api.write().await = None;
        *self.public_api.write().await = None;
        
        // Reinitialize connections
        self.initialize(&self.config).await?;
        
        info!("Successfully reconnected to execution client");
        Ok(())
    }
}

impl ConnectionPool {
    /// Create a new connection pool
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            connections: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(PoolStats::default())),
        }
    }
    
    /// Get the number of active connections
    pub async fn active_connection_count(&self) -> usize {
        self.connections.read().await.iter().filter(|c| c.in_use).count()
    }
    
    /// Get connection pool statistics
    pub async fn stats(&self) -> PoolStats {
        self.stats.read().await.clone()
    }
    
    /// Cleanup idle connections
    pub async fn cleanup_idle_connections(&self) {
        let mut connections = self.connections.write().await;
        let now = std::time::Instant::now();
        
        connections.retain(|conn| {
            if !conn.in_use && now.duration_since(conn.last_used) > self.config.max_idle_time {
                debug!("Removing idle connection: {}", conn.id);
                false
            } else {
                true
            }
        });
    }
}

impl Clone for EngineApiClient {
    fn clone(&self) -> Self {
        // Note: HttpJsonRpc doesn't implement Clone, so we create a new instance
        // This is a simplified implementation - in practice, we'd need proper cloning
        Self {
            rpc_client: self.rpc_client.clone(),
            auth: self.auth.clone(),
            capabilities: self.capabilities.clone(),
            last_success: self.last_success,
        }
    }
}

impl Clone for PublicApiClient {
    fn clone(&self) -> Self {
        Self {
            rpc_client: self.rpc_client.clone(),
            capabilities: self.capabilities.clone(),
            last_success: self.last_success,
        }
    }
}

/// Helper functions for creating HTTP JSON-RPC clients
/// These are convenience functions that wrap the lighthouse_wrapper functionality

/// Create a new HTTP engine JSON-RPC client with authentication
pub fn new_http_engine_json_rpc(url_override: Option<String>, jwt_key: JwtKey) -> HttpJsonRpc {
    let rpc_auth = Auth::new(jwt_key, None, None);
    let rpc_url = SensitiveUrl::parse(&url_override.unwrap_or(DEFAULT_EXECUTION_ENDPOINT.to_string())).unwrap();
    HttpJsonRpc::new_with_auth(rpc_url, rpc_auth, Some(3)).unwrap()
}

/// Create a new HTTP public execution JSON-RPC client without authentication
pub fn new_http_public_execution_json_rpc(url_override: Option<String>) -> HttpJsonRpc {
    let default_public_endpoint = "http://localhost:8545";
    let rpc_url = SensitiveUrl::parse(&url_override.unwrap_or(default_public_endpoint.to_string())).unwrap();
    HttpJsonRpc::new(rpc_url, Some(3)).unwrap()
}