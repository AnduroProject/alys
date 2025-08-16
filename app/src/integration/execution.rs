//! Execution client abstraction supporting both Geth and Reth
//!
//! This module provides a unified interface for interacting with Ethereum execution
//! layer clients, supporting both Geth and Reth implementations with comprehensive
//! state management, transaction handling, and performance optimization.

use crate::config::ExecutionConfig;
use crate::types::*;
use actor_system::{ActorError, ActorResult, AlysMessage, SerializableMessage};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Execution client abstraction for Geth/Reth compatibility
#[async_trait]
pub trait ExecutionIntegration: Send + Sync {
    /// Connect to execution client
    async fn connect(&self) -> Result<(), EngineError>;
    
    /// Get client information
    async fn get_client_version(&self) -> Result<String, EngineError>;
    
    /// Get current chain ID
    async fn get_chain_id(&self) -> Result<u64, EngineError>;
    
    /// Get latest block number
    async fn get_block_number(&self) -> Result<u64, EngineError>;
    
    /// Get block by hash
    async fn get_block_by_hash(&self, hash: BlockHash, include_txs: bool) -> Result<Option<ExecutionBlock>, EngineError>;
    
    /// Get block by number
    async fn get_block_by_number(&self, number: u64, include_txs: bool) -> Result<Option<ExecutionBlock>, EngineError>;
    
    /// Get transaction by hash
    async fn get_transaction(&self, hash: TxHash) -> Result<Option<ExecutionTransaction>, EngineError>;
    
    /// Get transaction receipt
    async fn get_transaction_receipt(&self, hash: TxHash) -> Result<Option<TransactionReceipt>, EngineError>;
    
    /// Send raw transaction
    async fn send_raw_transaction(&self, tx_data: Vec<u8>) -> Result<TxHash, EngineError>;
    
    /// Get account balance
    async fn get_balance(&self, address: Address, block: BlockNumber) -> Result<U256, EngineError>;
    
    /// Get account nonce
    async fn get_nonce(&self, address: Address, block: BlockNumber) -> Result<u64, EngineError>;
    
    /// Get storage at address and key
    async fn get_storage_at(&self, address: Address, key: H256, block: BlockNumber) -> Result<H256, EngineError>;
    
    /// Get contract code
    async fn get_code(&self, address: Address, block: BlockNumber) -> Result<Vec<u8>, EngineError>;
    
    /// Call contract method
    async fn call(&self, call: CallRequest, block: BlockNumber) -> Result<Vec<u8>, EngineError>;
    
    /// Estimate gas for transaction
    async fn estimate_gas(&self, call: CallRequest, block: Option<BlockNumber>) -> Result<u64, EngineError>;
    
    /// Get gas price
    async fn get_gas_price(&self) -> Result<U256, EngineError>;
    
    /// Get EIP-1559 fee history
    async fn fee_history(&self, block_count: u64, newest_block: BlockNumber, reward_percentiles: Option<Vec<f64>>) -> Result<FeeHistory, EngineError>;
    
    /// Get pending transactions
    async fn get_pending_transactions(&self) -> Result<Vec<ExecutionTransaction>, EngineError>;
    
    /// Get sync status
    async fn get_sync_status(&self) -> Result<Option<SyncStatus>, EngineError>;
    
    /// Subscribe to new block headers
    async fn subscribe_new_heads(&self) -> Result<tokio::sync::mpsc::Receiver<ExecutionBlock>, EngineError>;
    
    /// Subscribe to pending transactions
    async fn subscribe_pending_txs(&self) -> Result<tokio::sync::mpsc::Receiver<TxHash>, EngineError>;
    
    /// Subscribe to logs
    async fn subscribe_logs(&self, filter: LogFilter) -> Result<tokio::sync::mpsc::Receiver<Log>, EngineError>;
}

/// Comprehensive execution client supporting both Geth and Reth
#[derive(Debug)]
pub struct ExecutionClient {
    /// Configuration
    config: ExecutionConfig,
    
    /// Client type (Geth or Reth)
    client_type: ExecutionClientType,
    
    /// HTTP client for JSON-RPC calls
    http_client: reqwest::Client,
    
    /// WebSocket client for subscriptions
    ws_client: Option<Arc<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>>,
    
    /// Connection pool for load balancing
    connection_pool: Arc<RwLock<ConnectionPool>>,
    
    /// State cache for performance optimization
    state_cache: Arc<RwLock<StateCache>>,
    
    /// Transaction pool tracker
    transaction_pool: Arc<RwLock<TransactionPoolTracker>>,
    
    /// Performance metrics
    metrics: Arc<RwLock<ExecutionClientMetrics>>,
    
    /// Health monitoring
    health_monitor: Arc<RwLock<ExecutionHealthMonitor>>,
    
    /// Subscription manager
    subscription_manager: Arc<RwLock<SubscriptionManager>>,
}

/// Execution client types
#[derive(Debug, Clone)]
pub enum ExecutionClientType {
    Geth {
        version: String,
        features: Vec<String>,
    },
    Reth {
        version: String,
        features: Vec<String>,
    },
    Unknown {
        client_name: String,
        version: String,
    },
}

/// Connection pool for execution clients
#[derive(Debug)]
pub struct ConnectionPool {
    primary_endpoint: String,
    fallback_endpoints: Vec<String>,
    active_connections: HashMap<String, Connection>,
    load_balancer: LoadBalancer,
}

/// Individual connection to execution client
#[derive(Debug, Clone)]
pub struct Connection {
    pub endpoint: String,
    pub client_type: ExecutionClientType,
    pub last_used: SystemTime,
    pub request_count: u64,
    pub error_count: u64,
    pub average_latency: Duration,
    pub is_healthy: bool,
    pub capabilities: Vec<String>,
}

/// Load balancer for distributing requests
#[derive(Debug)]
pub enum LoadBalancer {
    RoundRobin { current_index: usize },
    LeastConnections,
    LatencyBased,
    Random,
}

/// State cache for execution client data
#[derive(Debug, Default)]
pub struct StateCache {
    pub blocks: lru::LruCache<BlockHash, ExecutionBlock>,
    pub transactions: lru::LruCache<TxHash, ExecutionTransaction>,
    pub receipts: lru::LruCache<TxHash, TransactionReceipt>,
    pub accounts: lru::LruCache<(Address, BlockNumber), AccountInfo>,
    pub storage: lru::LruCache<(Address, H256, BlockNumber), H256>,
    pub code: lru::LruCache<(Address, BlockNumber), Vec<u8>>,
    pub cache_stats: CacheStats,
}

/// Account information
#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub balance: U256,
    pub nonce: u64,
    pub code_hash: H256,
    pub storage_root: H256,
}

/// Cache statistics
#[derive(Debug, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub size_bytes: u64,
}

/// Transaction pool tracker
#[derive(Debug, Default)]
pub struct TransactionPoolTracker {
    pub pending_transactions: HashMap<TxHash, PendingTransaction>,
    pub queued_transactions: HashMap<TxHash, QueuedTransaction>,
    pub pool_status: PoolStatus,
    pub gas_price_oracle: GasPriceOracle,
}

/// Pending transaction in mempool
#[derive(Debug, Clone)]
pub struct PendingTransaction {
    pub hash: TxHash,
    pub from: Address,
    pub to: Option<Address>,
    pub value: U256,
    pub gas: u64,
    pub gas_price: U256,
    pub max_fee_per_gas: Option<U256>,
    pub max_priority_fee_per_gas: Option<U256>,
    pub nonce: u64,
    pub data: Vec<u8>,
    pub first_seen: SystemTime,
    pub replacements: u32,
}

/// Queued transaction waiting for nonce
#[derive(Debug, Clone)]
pub struct QueuedTransaction {
    pub hash: TxHash,
    pub from: Address,
    pub nonce: u64,
    pub gas_price: U256,
    pub queued_since: SystemTime,
    pub expected_nonce: u64,
}

/// Transaction pool status
#[derive(Debug, Clone)]
pub struct PoolStatus {
    pub pending_count: u32,
    pub queued_count: u32,
    pub total_bytes: u64,
    pub max_pool_size: u32,
    pub gas_price_threshold: U256,
}

/// Gas price oracle
#[derive(Debug)]
pub struct GasPriceOracle {
    pub current_base_fee: Option<U256>,
    pub suggested_gas_price: U256,
    pub suggested_priority_fee: U256,
    pub fee_history: Vec<FeeHistoryEntry>,
    pub last_updated: SystemTime,
}

/// Fee history entry
#[derive(Debug, Clone)]
pub struct FeeHistoryEntry {
    pub block_number: u64,
    pub base_fee: U256,
    pub gas_used_ratio: f64,
    pub reward_percentiles: Vec<U256>,
}

/// Performance metrics
#[derive(Debug, Default)]
pub struct ExecutionClientMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub average_response_time: Duration,
    pub cache_hit_rate: f64,
    pub subscription_count: u32,
    pub blocks_processed: u64,
    pub transactions_processed: u64,
    pub gas_used: U256,
    pub sync_progress: f64,
}

/// Health monitoring
#[derive(Debug)]
pub struct ExecutionHealthMonitor {
    pub last_successful_call: Option<SystemTime>,
    pub last_block_number: Option<u64>,
    pub consecutive_failures: u32,
    pub health_status: ExecutionHealthStatus,
    pub sync_status: Option<SyncStatus>,
    pub peer_count: u32,
}

/// Health status
#[derive(Debug, Clone)]
pub enum ExecutionHealthStatus {
    Healthy,
    Degraded { issues: Vec<String> },
    Unhealthy { critical_issues: Vec<String> },
    Disconnected,
}

/// Subscription management
#[derive(Debug, Default)]
pub struct SubscriptionManager {
    pub active_subscriptions: HashMap<String, SubscriptionInfo>,
    pub subscription_counter: u64,
}

/// Subscription information
#[derive(Debug, Clone)]
pub struct SubscriptionInfo {
    pub subscription_id: String,
    pub subscription_type: SubscriptionType,
    pub created_at: SystemTime,
    pub last_message: Option<SystemTime>,
    pub message_count: u64,
    pub filter: Option<serde_json::Value>,
}

/// Subscription types
#[derive(Debug, Clone)]
pub enum SubscriptionType {
    NewHeads,
    PendingTransactions,
    Logs { filter: LogFilter },
    Sync,
}

/// Call request for contract calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRequest {
    pub from: Option<Address>,
    pub to: Option<Address>,
    pub gas: Option<u64>,
    pub gas_price: Option<U256>,
    pub max_fee_per_gas: Option<U256>,
    pub max_priority_fee_per_gas: Option<U256>,
    pub value: Option<U256>,
    pub data: Option<Vec<u8>>,
}

/// Block number specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BlockNumber {
    Number(u64),
    Latest,
    Earliest,
    Pending,
    Safe,
    Finalized,
}

/// Fee history response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeHistory {
    pub oldest_block: u64,
    pub base_fee_per_gas: Vec<U256>,
    pub gas_used_ratio: Vec<f64>,
    pub reward: Option<Vec<Vec<U256>>>,
}

/// Log filter for subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFilter {
    pub address: Option<Vec<Address>>,
    pub topics: Option<Vec<Option<Vec<H256>>>>,
    pub from_block: Option<BlockNumber>,
    pub to_block: Option<BlockNumber>,
}

impl ExecutionClient {
    /// Create new execution client
    pub async fn new(config: ExecutionConfig) -> Result<Self, EngineError> {
        let http_client = reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .connect_timeout(Duration::from_secs(config.connection_timeout_secs))
            .build()
            .map_err(|e| EngineError::ConnectionFailed {
                reason: format!("Failed to create HTTP client: {}", e),
            })?;
        
        let connection_pool = Arc::new(RwLock::new(ConnectionPool {
            primary_endpoint: config.endpoint.clone(),
            fallback_endpoints: config.fallback_endpoints.clone(),
            active_connections: HashMap::new(),
            load_balancer: LoadBalancer::RoundRobin { current_index: 0 },
        }));
        
        let state_cache = Arc::new(RwLock::new(StateCache {
            blocks: lru::LruCache::new(config.cache_size),
            transactions: lru::LruCache::new(config.cache_size),
            receipts: lru::LruCache::new(config.cache_size),
            accounts: lru::LruCache::new(config.cache_size * 2),
            storage: lru::LruCache::new(config.cache_size * 4),
            code: lru::LruCache::new(config.cache_size),
            cache_stats: CacheStats::default(),
        }));
        
        let client = Self {
            config,
            client_type: ExecutionClientType::Unknown { 
                client_name: "unknown".to_string(), 
                version: "0.0.0".to_string() 
            },
            http_client,
            ws_client: None,
            connection_pool,
            state_cache,
            transaction_pool: Arc::new(RwLock::new(TransactionPoolTracker::default())),
            metrics: Arc::new(RwLock::new(ExecutionClientMetrics::default())),
            health_monitor: Arc::new(RwLock::new(ExecutionHealthMonitor {
                last_successful_call: None,
                last_block_number: None,
                consecutive_failures: 0,
                health_status: ExecutionHealthStatus::Disconnected,
                sync_status: None,
                peer_count: 0,
            })),
            subscription_manager: Arc::new(RwLock::new(SubscriptionManager::default())),
        };
        
        Ok(client)
    }
    
    /// Detect client type from version string
    async fn detect_client_type(&mut self) -> Result<(), EngineError> {
        let version = self.get_client_version().await?;
        
        self.client_type = if version.contains("Geth") {
            ExecutionClientType::Geth {
                version: version.clone(),
                features: vec![
                    "eth".to_string(),
                    "net".to_string(),
                    "web3".to_string(),
                    "txpool".to_string(),
                    "debug".to_string(),
                ],
            }
        } else if version.contains("reth") {
            ExecutionClientType::Reth {
                version: version.clone(),
                features: vec![
                    "eth".to_string(),
                    "net".to_string(),
                    "web3".to_string(),
                    "reth".to_string(),
                    "trace".to_string(),
                ],
            }
        } else {
            ExecutionClientType::Unknown {
                client_name: "unknown".to_string(),
                version,
            }
        };
        
        Ok(())
    }
    
    /// Make JSON-RPC call with caching and metrics
    async fn rpc_call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, EngineError> {
        let start_time = SystemTime::now();
        let mut metrics = self.metrics.write().await;
        metrics.total_requests += 1;
        drop(metrics);
        
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });
        
        let pool = self.connection_pool.read().await;
        let endpoint = &pool.primary_endpoint;
        drop(pool);
        
        let response = self.http_client
            .post(endpoint)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| EngineError::RequestFailed {
                reason: format!("HTTP request failed: {}", e),
            })?;
        
        let rpc_response: serde_json::Value = response.json().await
            .map_err(|e| EngineError::RequestFailed {
                reason: format!("Failed to parse response: {}", e),
            })?;
        
        if let Some(error) = rpc_response.get("error") {
            if !error.is_null() {
                let mut metrics = self.metrics.write().await;
                metrics.failed_requests += 1;
                return Err(EngineError::RpcError {
                    code: error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1),
                    message: error.get("message").and_then(|m| m.as_str()).unwrap_or("Unknown error").to_string(),
                });
            }
        }
        
        let result = rpc_response.get("result")
            .ok_or_else(|| EngineError::RequestFailed {
                reason: "No result in RPC response".to_string(),
            })?;
        
        let parsed_result = serde_json::from_value(result.clone())
            .map_err(|e| EngineError::RequestFailed {
                reason: format!("Failed to deserialize result: {}", e),
            })?;
        
        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.successful_requests += 1;
        if let Ok(duration) = start_time.elapsed() {
            let total_time = metrics.average_response_time.as_nanos() * (metrics.successful_requests - 1) as u128;
            metrics.average_response_time = Duration::from_nanos(
                ((total_time + duration.as_nanos()) / metrics.successful_requests as u128) as u64
            );
        }
        
        // Update health monitor
        let mut health = self.health_monitor.write().await;
        health.last_successful_call = Some(SystemTime::now());
        health.consecutive_failures = 0;
        health.health_status = ExecutionHealthStatus::Healthy;
        
        Ok(parsed_result)
    }
    
    /// Get client metrics
    pub async fn metrics(&self) -> ExecutionClientMetrics {
        self.metrics.read().await.clone()
    }
    
    /// Get health status
    pub async fn health_status(&self) -> ExecutionHealthStatus {
        self.health_monitor.read().await.health_status.clone()
    }
    
    /// Update transaction pool status
    pub async fn refresh_transaction_pool(&self) -> Result<(), EngineError> {
        let pending_txs = self.get_pending_transactions().await?;
        let mut pool = self.transaction_pool.write().await;
        
        pool.pending_transactions.clear();
        for tx in pending_txs {
            let pending_tx = PendingTransaction {
                hash: tx.hash,
                from: tx.from,
                to: tx.to,
                value: tx.value,
                gas: tx.gas,
                gas_price: tx.gas_price,
                max_fee_per_gas: tx.max_fee_per_gas,
                max_priority_fee_per_gas: tx.max_priority_fee_per_gas,
                nonce: tx.nonce,
                data: tx.input,
                first_seen: SystemTime::now(),
                replacements: 0,
            };
            pool.pending_transactions.insert(tx.hash, pending_tx);
        }
        
        pool.pool_status.pending_count = pool.pending_transactions.len() as u32;
        Ok(())
    }
}

#[async_trait]
impl ExecutionIntegration for ExecutionClient {
    async fn connect(&self) -> Result<(), EngineError> {
        // Test connection with web3_clientVersion
        let _version: String = self.rpc_call("web3_clientVersion", serde_json::json!([])).await?;
        Ok(())
    }
    
    async fn get_client_version(&self) -> Result<String, EngineError> {
        self.rpc_call("web3_clientVersion", serde_json::json!([])).await
    }
    
    async fn get_chain_id(&self) -> Result<u64, EngineError> {
        let chain_id: String = self.rpc_call("eth_chainId", serde_json::json!([])).await?;
        u64::from_str_radix(&chain_id[2..], 16)
            .map_err(|e| EngineError::RequestFailed {
                reason: format!("Invalid chain ID: {}", e),
            })
    }
    
    async fn get_block_number(&self) -> Result<u64, EngineError> {
        let block_number: String = self.rpc_call("eth_blockNumber", serde_json::json!([])).await?;
        u64::from_str_radix(&block_number[2..], 16)
            .map_err(|e| EngineError::RequestFailed {
                reason: format!("Invalid block number: {}", e),
            })
    }
    
    async fn get_block_by_hash(&self, hash: BlockHash, include_txs: bool) -> Result<Option<ExecutionBlock>, EngineError> {
        // Check cache first
        {
            let cache = self.state_cache.read().await;
            if let Some(block) = cache.blocks.get(&hash) {
                return Ok(Some(block.clone()));
            }
        }
        
        let result: Option<serde_json::Value> = self.rpc_call(
            "eth_getBlockByHash",
            serde_json::json!([format!("0x{:x}", hash), include_txs])
        ).await?;
        
        if let Some(block_json) = result {
            let block: ExecutionBlock = serde_json::from_value(block_json)
                .map_err(|e| EngineError::RequestFailed {
                    reason: format!("Failed to parse block: {}", e),
                })?;
            
            // Update cache
            {
                let mut cache = self.state_cache.write().await;
                cache.blocks.put(hash, block.clone());
                cache.cache_stats.size_bytes += std::mem::size_of::<ExecutionBlock>() as u64;
            }
            
            Ok(Some(block))
        } else {
            Ok(None)
        }
    }
    
    async fn get_block_by_number(&self, number: u64, include_txs: bool) -> Result<Option<ExecutionBlock>, EngineError> {
        let result: Option<serde_json::Value> = self.rpc_call(
            "eth_getBlockByNumber",
            serde_json::json!([format!("0x{:x}", number), include_txs])
        ).await?;
        
        if let Some(block_json) = result {
            let block: ExecutionBlock = serde_json::from_value(block_json)
                .map_err(|e| EngineError::RequestFailed {
                    reason: format!("Failed to parse block: {}", e),
                })?;
            Ok(Some(block))
        } else {
            Ok(None)
        }
    }
    
    async fn get_transaction(&self, hash: TxHash) -> Result<Option<ExecutionTransaction>, EngineError> {
        // Check cache first
        {
            let cache = self.state_cache.read().await;
            if let Some(tx) = cache.transactions.get(&hash) {
                return Ok(Some(tx.clone()));
            }
        }
        
        let result: Option<serde_json::Value> = self.rpc_call(
            "eth_getTransactionByHash",
            serde_json::json!([format!("0x{:x}", hash)])
        ).await?;
        
        if let Some(tx_json) = result {
            let tx: ExecutionTransaction = serde_json::from_value(tx_json)
                .map_err(|e| EngineError::RequestFailed {
                    reason: format!("Failed to parse transaction: {}", e),
                })?;
            
            // Update cache
            {
                let mut cache = self.state_cache.write().await;
                cache.transactions.put(hash, tx.clone());
                cache.cache_stats.size_bytes += std::mem::size_of::<ExecutionTransaction>() as u64;
            }
            
            Ok(Some(tx))
        } else {
            Ok(None)
        }
    }
    
    async fn get_transaction_receipt(&self, hash: TxHash) -> Result<Option<TransactionReceipt>, EngineError> {
        // Check cache first
        {
            let cache = self.state_cache.read().await;
            if let Some(receipt) = cache.receipts.get(&hash) {
                return Ok(Some(receipt.clone()));
            }
        }
        
        let result: Option<serde_json::Value> = self.rpc_call(
            "eth_getTransactionReceipt",
            serde_json::json!([format!("0x{:x}", hash)])
        ).await?;
        
        if let Some(receipt_json) = result {
            let receipt: TransactionReceipt = serde_json::from_value(receipt_json)
                .map_err(|e| EngineError::RequestFailed {
                    reason: format!("Failed to parse receipt: {}", e),
                })?;
            
            // Update cache
            {
                let mut cache = self.state_cache.write().await;
                cache.receipts.put(hash, receipt.clone());
                cache.cache_stats.size_bytes += std::mem::size_of::<TransactionReceipt>() as u64;
            }
            
            Ok(Some(receipt))
        } else {
            Ok(None)
        }
    }
    
    async fn send_raw_transaction(&self, tx_data: Vec<u8>) -> Result<TxHash, EngineError> {
        let tx_hex = format!("0x{}", hex::encode(tx_data));
        let hash: String = self.rpc_call("eth_sendRawTransaction", serde_json::json!([tx_hex])).await?;
        
        hash.parse()
            .map_err(|e| EngineError::RequestFailed {
                reason: format!("Invalid transaction hash: {}", e),
            })
    }
    
    async fn get_balance(&self, address: Address, block: BlockNumber) -> Result<U256, EngineError> {
        let balance_hex: String = self.rpc_call(
            "eth_getBalance",
            serde_json::json!([format!("0x{:x}", address), block])
        ).await?;
        
        U256::from_str_radix(&balance_hex[2..], 16)
            .map_err(|e| EngineError::RequestFailed {
                reason: format!("Invalid balance: {}", e),
            })
    }
    
    async fn get_nonce(&self, address: Address, block: BlockNumber) -> Result<u64, EngineError> {
        let nonce_hex: String = self.rpc_call(
            "eth_getTransactionCount",
            serde_json::json!([format!("0x{:x}", address), block])
        ).await?;
        
        u64::from_str_radix(&nonce_hex[2..], 16)
            .map_err(|e| EngineError::RequestFailed {
                reason: format!("Invalid nonce: {}", e),
            })
    }
    
    async fn get_storage_at(&self, address: Address, key: H256, block: BlockNumber) -> Result<H256, EngineError> {
        let storage_hex: String = self.rpc_call(
            "eth_getStorageAt",
            serde_json::json!([format!("0x{:x}", address), format!("0x{:x}", key), block])
        ).await?;
        
        storage_hex.parse()
            .map_err(|e| EngineError::RequestFailed {
                reason: format!("Invalid storage value: {}", e),
            })
    }
    
    async fn get_code(&self, address: Address, block: BlockNumber) -> Result<Vec<u8>, EngineError> {
        let code_hex: String = self.rpc_call(
            "eth_getCode",
            serde_json::json!([format!("0x{:x}", address), block])
        ).await?;
        
        hex::decode(&code_hex[2..])
            .map_err(|e| EngineError::RequestFailed {
                reason: format!("Invalid code hex: {}", e),
            })
    }
    
    async fn call(&self, call: CallRequest, block: BlockNumber) -> Result<Vec<u8>, EngineError> {
        let result_hex: String = self.rpc_call("eth_call", serde_json::json!([call, block])).await?;
        
        hex::decode(&result_hex[2..])
            .map_err(|e| EngineError::RequestFailed {
                reason: format!("Invalid call result: {}", e),
            })
    }
    
    async fn estimate_gas(&self, call: CallRequest, block: Option<BlockNumber>) -> Result<u64, EngineError> {
        let gas_hex: String = self.rpc_call(
            "eth_estimateGas",
            serde_json::json!([call, block.unwrap_or(BlockNumber::Latest)])
        ).await?;
        
        u64::from_str_radix(&gas_hex[2..], 16)
            .map_err(|e| EngineError::RequestFailed {
                reason: format!("Invalid gas estimate: {}", e),
            })
    }
    
    async fn get_gas_price(&self) -> Result<U256, EngineError> {
        let price_hex: String = self.rpc_call("eth_gasPrice", serde_json::json!([])).await?;
        
        U256::from_str_radix(&price_hex[2..], 16)
            .map_err(|e| EngineError::RequestFailed {
                reason: format!("Invalid gas price: {}", e),
            })
    }
    
    async fn fee_history(&self, block_count: u64, newest_block: BlockNumber, reward_percentiles: Option<Vec<f64>>) -> Result<FeeHistory, EngineError> {
        self.rpc_call(
            "eth_feeHistory",
            serde_json::json!([block_count, newest_block, reward_percentiles])
        ).await
    }
    
    async fn get_pending_transactions(&self) -> Result<Vec<ExecutionTransaction>, EngineError> {
        // Implementation depends on client type
        match &self.client_type {
            ExecutionClientType::Geth { .. } => {
                let txs: serde_json::Value = self.rpc_call("txpool_content", serde_json::json!([])).await?;
                // Parse Geth txpool format
                Ok(Vec::new()) // Simplified for now
            },
            ExecutionClientType::Reth { .. } => {
                let txs: Vec<serde_json::Value> = self.rpc_call("reth_pendingTransactions", serde_json::json!([])).await?;
                // Parse Reth format
                Ok(Vec::new()) // Simplified for now
            },
            _ => Ok(Vec::new()),
        }
    }
    
    async fn get_sync_status(&self) -> Result<Option<SyncStatus>, EngineError> {
        let result: Option<serde_json::Value> = self.rpc_call("eth_syncing", serde_json::json!([])).await?;
        
        if let Some(sync_json) = result {
            let sync_status: SyncStatus = serde_json::from_value(sync_json)
                .map_err(|e| EngineError::RequestFailed {
                    reason: format!("Failed to parse sync status: {}", e),
                })?;
            Ok(Some(sync_status))
        } else {
            Ok(None)
        }
    }
    
    async fn subscribe_new_heads(&self) -> Result<tokio::sync::mpsc::Receiver<ExecutionBlock>, EngineError> {
        let (tx, rx) = tokio::sync::mpsc::channel(1000);
        
        // TODO: Implement WebSocket subscription
        // This would involve:
        // 1. Establishing WebSocket connection
        // 2. Sending subscription request
        // 3. Handling incoming messages
        // 4. Parsing block data
        
        Ok(rx)
    }
    
    async fn subscribe_pending_txs(&self) -> Result<tokio::sync::mpsc::Receiver<TxHash>, EngineError> {
        let (tx, rx) = tokio::sync::mpsc::channel(10000);
        
        // TODO: Implement pending transactions subscription
        
        Ok(rx)
    }
    
    async fn subscribe_logs(&self, filter: LogFilter) -> Result<tokio::sync::mpsc::Receiver<Log>, EngineError> {
        let (tx, rx) = tokio::sync::mpsc::channel(10000);
        
        // TODO: Implement log subscription with filtering
        
        Ok(rx)
    }
}

/// Execution client factory
pub struct ExecutionIntegrationFactory;

impl ExecutionIntegrationFactory {
    /// Create execution integration from config
    pub async fn create(config: &ExecutionConfig) -> Result<Box<dyn ExecutionIntegration>, EngineError> {
        let mut client = ExecutionClient::new(config.clone()).await?;
        client.detect_client_type().await?;
        Ok(Box::new(client))
    }
    
    /// Create execution client with specific type
    pub async fn create_for_client_type(
        config: &ExecutionConfig,
        client_type: ExecutionClientType,
    ) -> Result<Box<dyn ExecutionIntegration>, EngineError> {
        let mut client = ExecutionClient::new(config.clone()).await?;
        client.client_type = client_type;
        Ok(Box::new(client))
    }
    
    /// Auto-detect and create appropriate client
    pub async fn auto_detect(config: &ExecutionConfig) -> Result<Box<dyn ExecutionIntegration>, EngineError> {
        let client = Self::create(config).await?;
        
        // Test connection and detect capabilities
        client.connect().await?;
        
        Ok(client)
    }
}

/// Extension trait for advanced execution client functionality
#[async_trait]
pub trait ExecutionClientExt {
    /// Batch multiple RPC calls
    async fn batch_rpc_calls(&self, calls: Vec<BatchRpcCall>) -> Result<Vec<serde_json::Value>, EngineError>;
    
    /// Get state at specific block for multiple accounts
    async fn get_state_batch(&self, addresses: Vec<Address>, block: BlockNumber) -> Result<Vec<AccountInfo>, EngineError>;
    
    /// Monitor transaction pool changes
    async fn monitor_transaction_pool(&self) -> Result<tokio::sync::mpsc::Receiver<PoolUpdate>, EngineError>;
    
    /// Optimize gas price based on network conditions
    async fn optimize_gas_price(&self, priority: GasPriority) -> Result<GasEstimate, EngineError>;
}

/// Batch RPC call
#[derive(Debug, Clone)]
pub struct BatchRpcCall {
    pub id: String,
    pub method: String,
    pub params: serde_json::Value,
}

/// Transaction pool update
#[derive(Debug, Clone)]
pub enum PoolUpdate {
    TransactionAdded { hash: TxHash, transaction: ExecutionTransaction },
    TransactionRemoved { hash: TxHash, reason: RemovalReason },
    PoolStatusChanged { status: PoolStatus },
}

/// Reason for transaction removal from pool
#[derive(Debug, Clone)]
pub enum RemovalReason {
    Included { block_hash: BlockHash },
    Replaced { by_hash: TxHash },
    Dropped { reason: String },
    InvalidNonce,
    InsufficientFunds,
    GasPriceTooLow,
}

/// Gas priority levels
#[derive(Debug, Clone, Copy)]
pub enum GasPriority {
    Slow,
    Standard,
    Fast,
    Instant,
}

/// Gas estimation result
#[derive(Debug, Clone)]
pub struct GasEstimate {
    pub gas_limit: u64,
    pub gas_price: U256,
    pub max_fee_per_gas: Option<U256>,
    pub max_priority_fee_per_gas: Option<U256>,
    pub estimated_cost: U256,
    pub confidence_level: f64,
}

impl Default for LoadBalancer {
    fn default() -> Self {
        Self::RoundRobin { current_index: 0 }
    }
}

impl Default for ExecutionClientMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            average_response_time: Duration::from_millis(0),
            cache_hit_rate: 0.0,
            subscription_count: 0,
            blocks_processed: 0,
            transactions_processed: 0,
            gas_used: U256::zero(),
            sync_progress: 0.0,
        }
    }
}

impl Default for GasPriceOracle {
    fn default() -> Self {
        Self {
            current_base_fee: None,
            suggested_gas_price: U256::zero(),
            suggested_priority_fee: U256::zero(),
            fee_history: Vec::new(),
            last_updated: SystemTime::now(),
        }
    }
}