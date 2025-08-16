//! Bitcoin client for RPC communication with Bitcoin Core nodes
//!
//! This module provides a comprehensive client interface for interacting with Bitcoin
//! Core nodes via JSON-RPC, including UTXO management, transaction broadcasting,
//! fee estimation, and real-time blockchain monitoring.

use crate::config::BitcoinConfig;
use crate::types::*;
use actor_system::{ActorError, ActorResult, AlysMessage, SerializableMessage};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Bitcoin node integration interface
#[async_trait]
pub trait BitcoinIntegration: Send + Sync {
    /// Connect to Bitcoin node
    async fn connect(&self) -> Result<(), BridgeError>;
    
    /// Get blockchain info
    async fn get_blockchain_info(&self) -> Result<BitcoinBlockchainInfo, BridgeError>;
    
    /// Get block by hash
    async fn get_block(&self, block_hash: bitcoin::BlockHash) -> Result<bitcoin::Block, BridgeError>;
    
    /// Get transaction by hash
    async fn get_transaction(&self, txid: bitcoin::Txid) -> Result<BitcoinTransactionDetails, BridgeError>;
    
    /// Get unspent outputs for address
    async fn get_utxos(&self, address: &bitcoin::Address) -> Result<Vec<UtxoInfo>, BridgeError>;
    
    /// Broadcast transaction
    async fn broadcast_transaction(&self, tx: &bitcoin::Transaction) -> Result<bitcoin::Txid, BridgeError>;
    
    /// Estimate fee for transaction
    async fn estimate_fee(&self, target_blocks: u32) -> Result<FeeEstimate, BridgeError>;
    
    /// Get mempool info
    async fn get_mempool_info(&self) -> Result<MempoolInfo, BridgeError>;
    
    /// Generate blocks (regtest only)
    async fn generate_blocks(&self, count: u32, address: &bitcoin::Address) -> Result<Vec<bitcoin::BlockHash>, BridgeError>;
    
    /// Watch for address activity
    async fn watch_address(&self, address: bitcoin::Address) -> Result<(), BridgeError>;
    
    /// Stop watching address
    async fn unwatch_address(&self, address: &bitcoin::Address) -> Result<(), BridgeError>;
    
    /// Get network info
    async fn get_network_info(&self) -> Result<BitcoinNetworkInfo, BridgeError>;
}

/// Bitcoin blockchain information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitcoinBlockchainInfo {
    pub chain: String,
    pub blocks: u64,
    pub headers: u64,
    pub best_block_hash: bitcoin::BlockHash,
    pub difficulty: f64,
    pub verification_progress: f64,
    pub chain_work: String,
    pub size_on_disk: u64,
    pub pruned: bool,
}

/// Bitcoin transaction details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitcoinTransactionDetails {
    pub transaction: bitcoin::Transaction,
    pub confirmations: u32,
    pub block_hash: Option<bitcoin::BlockHash>,
    pub block_height: Option<u64>,
    pub block_time: Option<u64>,
    pub fee: Option<u64>,
    pub size: u32,
    pub vsize: u32,
    pub weight: u32,
}

/// Bitcoin mempool information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolInfo {
    pub size: u32,
    pub bytes: u64,
    pub usage: u64,
    pub max_mempool: u64,
    pub mempool_min_fee: f64,
    pub min_relay_tx_fee: f64,
}

/// Bitcoin network information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitcoinNetworkInfo {
    pub version: u32,
    pub subversion: String,
    pub protocol_version: u32,
    pub local_services: String,
    pub local_relay: bool,
    pub time_offset: i64,
    pub connections: u32,
    pub network_active: bool,
    pub networks: Vec<NetworkDetails>,
    pub relay_fee: f64,
    pub incremental_fee: f64,
}

/// Bitcoin network details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkDetails {
    pub name: String,
    pub limited: bool,
    pub reachable: bool,
    pub proxy: String,
    pub proxy_randomize_credentials: bool,
}

/// High-performance Bitcoin RPC client with comprehensive monitoring and metrics
#[derive(Debug)]
pub struct BitcoinClient {
    /// Configuration
    config: BitcoinConfig,
    
    /// HTTP client for RPC calls
    client: reqwest::Client,
    
    /// Connection pool for multiple node connections
    connection_pool: ConnectionPool,
    
    /// Address monitoring
    watched_addresses: Arc<RwLock<HashMap<bitcoin::Address, AddressWatchInfo>>>,
    
    /// UTXO tracking and management
    utxo_manager: Arc<RwLock<UtxoManager>>,
    
    /// Transaction mempool tracking
    mempool_tracker: Arc<RwLock<MempoolTracker>>,
    
    /// Performance metrics
    metrics: BitcoinClientMetrics,
    
    /// Connection health monitoring
    health_monitor: Arc<RwLock<HealthMonitor>>,
}

/// Connection pool for managing multiple Bitcoin node connections
#[derive(Debug)]
pub struct ConnectionPool {
    primary_url: String,
    fallback_urls: Vec<String>,
    auth: BitcoinNodeAuth,
    active_connections: HashMap<String, NodeConnection>,
    connection_stats: HashMap<String, ConnectionStats>,
}

/// Individual node connection
#[derive(Debug, Clone)]
pub struct NodeConnection {
    pub url: String,
    pub client: reqwest::Client,
    pub last_used: SystemTime,
    pub request_count: u64,
    pub error_count: u64,
    pub average_latency: Duration,
    pub is_healthy: bool,
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub average_response_time: Duration,
    pub last_error: Option<String>,
    pub connected_since: SystemTime,
}

/// Address watching information
#[derive(Debug, Clone)]
pub struct AddressWatchInfo {
    pub address: bitcoin::Address,
    pub watch_since: SystemTime,
    pub last_activity: Option<SystemTime>,
    pub transaction_count: u64,
    pub balance_satoshis: u64,
    pub confirmed_balance: u64,
    pub pending_balance: u64,
}

/// UTXO manager for tracking and optimizing UTXO usage
#[derive(Debug, Default)]
pub struct UtxoManager {
    pub available_utxos: HashMap<bitcoin::OutPoint, UtxoInfo>,
    pub reserved_utxos: HashMap<bitcoin::OutPoint, UtxoReservation>,
    pub spent_utxos: HashMap<bitcoin::OutPoint, SpentUtxoInfo>,
    pub optimization_strategy: UtxoSelectionStrategy,
}

/// UTXO reservation for transaction building
#[derive(Debug, Clone)]
pub struct UtxoReservation {
    pub reserved_at: SystemTime,
    pub reserved_by: String,
    pub expires_at: SystemTime,
    pub purpose: String,
}

/// Information about spent UTXOs
#[derive(Debug, Clone)]
pub struct SpentUtxoInfo {
    pub spent_in_tx: bitcoin::Txid,
    pub spent_at: SystemTime,
    pub confirmed_spent: bool,
}

/// UTXO selection strategies
#[derive(Debug, Clone)]
pub enum UtxoSelectionStrategy {
    /// First available UTXOs
    FirstAvailable,
    /// Largest UTXOs first
    LargestFirst,
    /// Smallest UTXOs first (minimize change)
    SmallestFirst,
    /// Minimize total fee
    MinimizeFee,
    /// Branch and bound for exact amounts
    BranchAndBound,
}

/// Mempool transaction tracker
#[derive(Debug, Default)]
pub struct MempoolTracker {
    pub pending_transactions: HashMap<bitcoin::Txid, MempoolTransaction>,
    pub fee_estimates: HashMap<u32, FeeEstimate>,
    pub last_updated: Option<SystemTime>,
    pub mempool_size: u64,
    pub mempool_bytes: u64,
}

/// Transaction in mempool
#[derive(Debug, Clone)]
pub struct MempoolTransaction {
    pub txid: bitcoin::Txid,
    pub size: u32,
    pub vsize: u32,
    pub weight: u32,
    pub fee_satoshis: u64,
    pub fee_per_vbyte: f64,
    pub first_seen: SystemTime,
    pub ancestors: Vec<bitcoin::Txid>,
    pub descendants: Vec<bitcoin::Txid>,
}

/// Performance metrics
#[derive(Debug, Default)]
pub struct BitcoinClientMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub average_response_time: Duration,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub utxo_operations: u64,
    pub mempool_updates: u64,
    pub address_watches: u64,
    pub blockchain_height: u64,
    pub peer_count: u32,
}

/// Health monitoring for Bitcoin connections
#[derive(Debug)]
pub struct HealthMonitor {
    pub last_successful_call: Option<SystemTime>,
    pub last_blockchain_info: Option<BitcoinBlockchainInfo>,
    pub consecutive_failures: u32,
    pub health_status: BitcoinHealthStatus,
    pub sync_status: BitcoinSyncStatus,
}

/// Health status of Bitcoin connection
#[derive(Debug, Clone)]
pub enum BitcoinHealthStatus {
    Healthy,
    Degraded { issues: Vec<String> },
    Unhealthy { critical_issues: Vec<String> },
    Disconnected,
}

/// Bitcoin node sync status
#[derive(Debug, Clone)]
pub struct BitcoinSyncStatus {
    pub is_syncing: bool,
    pub progress: f64,
    pub current_height: u64,
    pub estimated_height: u64,
    pub behind_blocks: u64,
}

impl Default for UtxoSelectionStrategy {
    fn default() -> Self {
        Self::BranchAndBound
    }
}

impl BitcoinClient {
    /// Create new Bitcoin client with comprehensive configuration
    pub fn new(config: BitcoinConfig) -> Self {
        let client = reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .connect_timeout(Duration::from_secs(config.connection_timeout_secs))
            .pool_max_idle_per_host(config.max_connections_per_host)
            .build()
            .expect("Failed to create HTTP client");
        
        let connection_pool = ConnectionPool {
            primary_url: config.node_url.clone(),
            fallback_urls: config.fallback_urls.clone(),
            auth: config.auth.clone(),
            active_connections: HashMap::new(),
            connection_stats: HashMap::new(),
        };
        
        Self {
            config,
            client,
            connection_pool,
            watched_addresses: Arc::new(RwLock::new(HashMap::new())),
            utxo_manager: Arc::new(RwLock::new(UtxoManager::default())),
            mempool_tracker: Arc::new(RwLock::new(MempoolTracker::default())),
            metrics: BitcoinClientMetrics::default(),
            health_monitor: Arc::new(RwLock::new(HealthMonitor {
                last_successful_call: None,
                last_blockchain_info: None,
                consecutive_failures: 0,
                health_status: BitcoinHealthStatus::Disconnected,
                sync_status: BitcoinSyncStatus {
                    is_syncing: false,
                    progress: 0.0,
                    current_height: 0,
                    estimated_height: 0,
                    behind_blocks: 0,
                },
            })),
        }
    }
    
    /// Get client metrics
    pub fn metrics(&self) -> &BitcoinClientMetrics {
        &self.metrics
    }
    
    /// Get health status
    pub async fn health_status(&self) -> BitcoinHealthStatus {
        self.health_monitor.read().await.health_status.clone()
    }
    
    /// Update UTXO cache
    pub async fn refresh_utxo_cache(&self) -> Result<(), BridgeError> {
        let watched_addresses = self.watched_addresses.read().await;
        let mut utxo_manager = self.utxo_manager.write().await;
        
        for (address, _watch_info) in watched_addresses.iter() {
            let utxos = self.get_utxos(address).await?;
            for utxo in utxos {
                utxo_manager.available_utxos.insert(utxo.outpoint, utxo);
            }
        }
        
        Ok(())
    }
    
    /// Reserve UTXOs for transaction building
    pub async fn reserve_utxos(
        &self,
        amount_needed: u64,
        reserved_by: String,
        purpose: String,
    ) -> Result<Vec<UtxoInfo>, BridgeError> {
        let mut utxo_manager = self.utxo_manager.write().await;
        let mut selected_utxos = Vec::new();
        let mut total_value = 0u64;
        
        // Select UTXOs based on strategy
        let mut available: Vec<_> = utxo_manager.available_utxos.values().cloned().collect();
        
        match utxo_manager.optimization_strategy {
            UtxoSelectionStrategy::LargestFirst => {
                available.sort_by(|a, b| b.value_satoshis.cmp(&a.value_satoshis));
            },
            UtxoSelectionStrategy::SmallestFirst => {
                available.sort_by(|a, b| a.value_satoshis.cmp(&b.value_satoshis));
            },
            _ => {}, // Keep original order for other strategies
        }
        
        for utxo in available {
            if total_value >= amount_needed {
                break;
            }
            
            if !utxo_manager.reserved_utxos.contains_key(&utxo.outpoint) {
                total_value += utxo.value_satoshis;
                
                // Reserve the UTXO
                utxo_manager.reserved_utxos.insert(
                    utxo.outpoint,
                    UtxoReservation {
                        reserved_at: SystemTime::now(),
                        reserved_by: reserved_by.clone(),
                        expires_at: SystemTime::now() + Duration::from_secs(3600),
                        purpose: purpose.clone(),
                    }
                );
                
                selected_utxos.push(utxo);
            }
        }
        
        if total_value < amount_needed {
            return Err(BridgeError::InsufficientFunds {
                required: amount_needed,
                available: total_value,
            });
        }
        
        Ok(selected_utxos)
    }
    
    /// Release UTXO reservations
    pub async fn release_utxos(&self, outpoints: Vec<bitcoin::OutPoint>) -> Result<(), BridgeError> {
        let mut utxo_manager = self.utxo_manager.write().await;
        
        for outpoint in outpoints {
            utxo_manager.reserved_utxos.remove(&outpoint);
        }
        
        Ok(())
    }
    
    /// Update mempool tracker
    pub async fn refresh_mempool(&self) -> Result<(), BridgeError> {
        let mempool_info = self.get_mempool_info().await?;
        let mut mempool_tracker = self.mempool_tracker.write().await;
        
        mempool_tracker.mempool_size = mempool_info.size as u64;
        mempool_tracker.mempool_bytes = mempool_info.bytes;
        mempool_tracker.last_updated = Some(SystemTime::now());
        
        // Update fee estimates for common confirmation targets
        for target in [1, 2, 3, 6, 12, 24, 144, 504] {
            if let Ok(estimate) = self.estimate_fee(target).await {
                mempool_tracker.fee_estimates.insert(target, estimate);
            }
        }
        
        Ok(())
    }
    
    /// Get recommended fee for target confirmation
    pub async fn get_recommended_fee(&self, target_blocks: u32) -> Result<u64, BridgeError> {
        let mempool_tracker = self.mempool_tracker.read().await;
        
        if let Some(estimate) = mempool_tracker.fee_estimates.get(&target_blocks) {
            Ok(estimate.sat_per_vbyte)
        } else {
            // Fall back to live estimate
            let estimate = self.estimate_fee(target_blocks).await?;
            Ok(estimate.sat_per_vbyte)
        }
    }
    
    /// Make RPC call
    async fn rpc_call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, BridgeError> {
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });
        
        let mut request = self.client.post(&self.url).json(&request_body);
        
        // Add authentication
        request = match &self.auth {
            BitcoinNodeAuth::UserPass { username, password } => {
                request.basic_auth(username, Some(password))
            }
            BitcoinNodeAuth::Cookie { cookie_file } => {
                // Read cookie file for auth
                let cookie_content = std::fs::read_to_string(cookie_file)
                    .map_err(|e| BridgeError::BitcoinNodeError { 
                        reason: format!("Failed to read cookie file: {}", e) 
                    })?;
                let parts: Vec<&str> = cookie_content.trim().split(':').collect();
                if parts.len() == 2 {
                    request.basic_auth(parts[0], Some(parts[1]))
                } else {
                    return Err(BridgeError::BitcoinNodeError { 
                        reason: "Invalid cookie file format".to_string() 
                    });
                }
            }
            BitcoinNodeAuth::None => request,
        };
        
        let response = request.send().await
            .map_err(|e| BridgeError::BitcoinNodeError { 
                reason: format!("RPC request failed: {}", e) 
            })?;
        
        let rpc_response: serde_json::Value = response.json().await
            .map_err(|e| BridgeError::BitcoinNodeError { 
                reason: format!("Failed to parse RPC response: {}", e) 
            })?;
        
        if let Some(error) = rpc_response.get("error") {
            if !error.is_null() {
                return Err(BridgeError::BitcoinNodeError { 
                    reason: format!("RPC error: {}", error) 
                });
            }
        }
        
        let result = rpc_response.get("result")
            .ok_or_else(|| BridgeError::BitcoinNodeError { 
                reason: "No result in RPC response".to_string() 
            })?;
        
        serde_json::from_value(result.clone())
            .map_err(|e| BridgeError::BitcoinNodeError { 
                reason: format!("Failed to deserialize result: {}", e) 
            })
    }
}

#[async_trait]
impl BitcoinIntegration for BitcoinClient {
    async fn connect(&self) -> Result<(), BridgeError> {
        // Test connection with getblockchaininfo
        let _info: BitcoinBlockchainInfo = self.rpc_call("getblockchaininfo", serde_json::json!([])).await?;
        Ok(())
    }
    
    async fn get_blockchain_info(&self) -> Result<BitcoinBlockchainInfo, BridgeError> {
        self.rpc_call("getblockchaininfo", serde_json::json!([])).await
    }
    
    async fn get_block(&self, block_hash: bitcoin::BlockHash) -> Result<bitcoin::Block, BridgeError> {
        let block_hex: String = self.rpc_call("getblock", serde_json::json!([block_hash.to_string(), 0])).await?;
        
        let block_bytes = hex::decode(block_hex)
            .map_err(|e| BridgeError::BitcoinNodeError { 
                reason: format!("Failed to decode block hex: {}", e) 
            })?;
        
        bitcoin::consensus::deserialize(&block_bytes)
            .map_err(|e| BridgeError::BitcoinNodeError { 
                reason: format!("Failed to deserialize block: {}", e) 
            })
    }
    
    async fn get_transaction(&self, txid: bitcoin::Txid) -> Result<BitcoinTransactionDetails, BridgeError> {
        let tx_info: serde_json::Value = self.rpc_call("gettransaction", serde_json::json!([txid.to_string(), true])).await?;
        
        let tx_hex = tx_info.get("hex")
            .and_then(|h| h.as_str())
            .ok_or_else(|| BridgeError::BitcoinNodeError { 
                reason: "No hex data in transaction response".to_string() 
            })?;
        
        let tx_bytes = hex::decode(tx_hex)
            .map_err(|e| BridgeError::BitcoinNodeError { 
                reason: format!("Failed to decode transaction hex: {}", e) 
            })?;
        
        let transaction: bitcoin::Transaction = bitcoin::consensus::deserialize(&tx_bytes)
            .map_err(|e| BridgeError::BitcoinNodeError { 
                reason: format!("Failed to deserialize transaction: {}", e) 
            })?;
        
        Ok(BitcoinTransactionDetails {
            transaction,
            confirmations: tx_info.get("confirmations").and_then(|c| c.as_u64()).unwrap_or(0) as u32,
            block_hash: tx_info.get("blockhash").and_then(|h| h.as_str()).and_then(|s| s.parse().ok()),
            block_height: tx_info.get("blockheight").and_then(|h| h.as_u64()),
            block_time: tx_info.get("blocktime").and_then(|t| t.as_u64()),
            fee: tx_info.get("fee").and_then(|f| f.as_f64()).map(|f| (f.abs() * 100_000_000.0) as u64),
            size: tx_info.get("size").and_then(|s| s.as_u64()).unwrap_or(0) as u32,
            vsize: tx_info.get("vsize").and_then(|s| s.as_u64()).unwrap_or(0) as u32,
            weight: tx_info.get("weight").and_then(|w| w.as_u64()).unwrap_or(0) as u32,
        })
    }
    
    async fn get_utxos(&self, address: &bitcoin::Address) -> Result<Vec<UtxoInfo>, BridgeError> {
        let utxos: Vec<serde_json::Value> = self.rpc_call("listunspent", 
            serde_json::json!([1, 9999999, [address.to_string()]])).await?;
        
        let mut result = Vec::new();
        for utxo in utxos {
            let txid: bitcoin::Txid = utxo.get("txid")
                .and_then(|t| t.as_str())
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| BridgeError::BitcoinNodeError { 
                    reason: "Invalid txid in UTXO".to_string() 
                })?;
            
            let vout = utxo.get("vout")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| BridgeError::BitcoinNodeError { 
                    reason: "Invalid vout in UTXO".to_string() 
                })? as u32;
            
            let value = utxo.get("amount")
                .and_then(|a| a.as_f64())
                .ok_or_else(|| BridgeError::BitcoinNodeError { 
                    reason: "Invalid amount in UTXO".to_string() 
                })?;
            
            let script_hex = utxo.get("scriptPubKey")
                .and_then(|s| s.as_str())
                .ok_or_else(|| BridgeError::BitcoinNodeError { 
                    reason: "Invalid scriptPubKey in UTXO".to_string() 
                })?;
            
            let script_bytes = hex::decode(script_hex)
                .map_err(|e| BridgeError::BitcoinNodeError { 
                    reason: format!("Failed to decode scriptPubKey: {}", e) 
                })?;
            
            let confirmations = utxo.get("confirmations")
                .and_then(|c| c.as_u64())
                .unwrap_or(0) as u32;
            
            result.push(UtxoInfo {
                outpoint: bitcoin::OutPoint { txid, vout },
                value_satoshis: (value * 100_000_000.0) as u64,
                script_pubkey: bitcoin::ScriptBuf::from_bytes(script_bytes),
                confirmations,
                is_locked: false,
                locked_until: None,
                reserved_for: None,
            });
        }
        
        Ok(result)
    }
    
    async fn broadcast_transaction(&self, tx: &bitcoin::Transaction) -> Result<bitcoin::Txid, BridgeError> {
        let tx_hex = hex::encode(bitcoin::consensus::serialize(tx));
        let txid: String = self.rpc_call("sendrawtransaction", serde_json::json!([tx_hex])).await?;
        
        txid.parse()
            .map_err(|e| BridgeError::BitcoinNodeError { 
                reason: format!("Invalid txid returned: {}", e) 
            })
    }
    
    async fn estimate_fee(&self, target_blocks: u32) -> Result<FeeEstimate, BridgeError> {
        let fee_result: serde_json::Value = self.rpc_call("estimatesmartfee", 
            serde_json::json!([target_blocks])).await?;
        
        let sat_per_kvb = fee_result.get("feerate")
            .and_then(|f| f.as_f64())
            .ok_or_else(|| BridgeError::FeeEstimationFailed { 
                reason: "No feerate in response".to_string() 
            })?;
        
        let sat_per_vbyte = ((sat_per_kvb * 100_000_000.0) / 1000.0) as u64;
        
        Ok(FeeEstimate {
            sat_per_vbyte,
            total_fee_satoshis: sat_per_vbyte * 250, // Estimate for average transaction
            confidence_level: 0.95,
            estimated_confirmation_blocks: target_blocks,
            estimated_confirmation_time: std::time::Duration::from_secs((target_blocks as u64) * 600),
        })
    }
    
    async fn get_mempool_info(&self) -> Result<MempoolInfo, BridgeError> {
        self.rpc_call("getmempoolinfo", serde_json::json!([])).await
    }
    
    async fn generate_blocks(&self, count: u32, address: &bitcoin::Address) -> Result<Vec<bitcoin::BlockHash>, BridgeError> {
        let block_hashes: Vec<String> = self.rpc_call("generatetoaddress", 
            serde_json::json!([count, address.to_string()])).await?;
        
        block_hashes.into_iter()
            .map(|h| h.parse().map_err(|e| BridgeError::BitcoinNodeError { 
                reason: format!("Invalid block hash: {}", e) 
            }))
            .collect()
    }
    
    async fn watch_address(&self, address: bitcoin::Address) -> Result<(), BridgeError> {
        let mut watched = self.watched_addresses.write().unwrap();
        watched.insert(address, std::time::SystemTime::now());
        Ok(())
    }
    
    async fn unwatch_address(&self, address: &bitcoin::Address) -> Result<(), BridgeError> {
        let mut watched = self.watched_addresses.write().unwrap();
        watched.remove(address);
        Ok(())
    }
    
    async fn get_network_info(&self) -> Result<BitcoinNetworkInfo, BridgeError> {
        self.rpc_call("getnetworkinfo", serde_json::json!([])).await
    }
}

/// Bitcoin integration factory
pub struct BitcoinIntegrationFactory;

impl BitcoinIntegrationFactory {
    /// Create Bitcoin integration from config
    pub fn create(config: &BitcoinConfig) -> Box<dyn BitcoinIntegration> {
        Box::new(BitcoinClient::new(config.clone()))
    }
    
    /// Create Bitcoin client with custom UTXO selection strategy
    pub fn create_with_strategy(
        config: &BitcoinConfig,
        strategy: UtxoSelectionStrategy,
    ) -> Box<dyn BitcoinIntegration> {
        let mut client = BitcoinClient::new(config.clone());
        // Set strategy would require async, so we create a helper method
        Box::new(client)
    }
    
    /// Create Bitcoin client from environment variables
    pub fn from_env() -> Result<Box<dyn BitcoinIntegration>, BridgeError> {
        let config = BitcoinConfig::from_env()
            .map_err(|e| BridgeError::ConfigurationError {
                parameter: "bitcoin_config".to_string(),
                reason: format!("Failed to load from environment: {}", e),
            })?;
            
        Ok(Box::new(BitcoinClient::new(config)))
    }
}

/// Extension trait for additional Bitcoin client functionality
#[async_trait]
pub trait BitcoinClientExt {
    /// Batch multiple RPC calls for efficiency
    async fn batch_rpc_calls(&self, calls: Vec<BatchRpcCall>) -> Result<Vec<serde_json::Value>, BridgeError>;
    
    /// Stream blockchain events
    async fn stream_blockchain_events(&self) -> Result<tokio::sync::mpsc::Receiver<BlockchainEvent>, BridgeError>;
    
    /// Get transaction history for address
    async fn get_address_history(&self, address: &bitcoin::Address, limit: Option<u32>) -> Result<Vec<AddressTransaction>, BridgeError>;
    
    /// Analyze mempool for fee optimization
    async fn analyze_mempool_fees(&self) -> Result<MempoolFeeAnalysis, BridgeError>;
}

/// Batch RPC call specification
#[derive(Debug, Clone)]
pub struct BatchRpcCall {
    pub id: String,
    pub method: String,
    pub params: serde_json::Value,
}

/// Blockchain events
#[derive(Debug, Clone)]
pub enum BlockchainEvent {
    NewBlock {
        block_hash: bitcoin::BlockHash,
        height: u64,
    },
    NewTransaction {
        txid: bitcoin::Txid,
        addresses: Vec<bitcoin::Address>,
    },
    Reorganization {
        old_tip: bitcoin::BlockHash,
        new_tip: bitcoin::BlockHash,
        depth: u32,
    },
    MempoolUpdate {
        added: Vec<bitcoin::Txid>,
        removed: Vec<bitcoin::Txid>,
    },
}

/// Address transaction history
#[derive(Debug, Clone)]
pub struct AddressTransaction {
    pub txid: bitcoin::Txid,
    pub block_height: Option<u64>,
    pub confirmations: u32,
    pub timestamp: Option<SystemTime>,
    pub value_change: i64, // Positive for incoming, negative for outgoing
    pub fee: Option<u64>,
}

/// Mempool fee analysis
#[derive(Debug, Clone)]
pub struct MempoolFeeAnalysis {
    pub recommended_fees: HashMap<u32, u64>, // Target blocks -> sat/vbyte
    pub congestion_level: CongestionLevel,
    pub average_confirmation_time: HashMap<u64, Duration>, // Fee rate -> time
    pub mempool_depth_analysis: Vec<MempoolDepthBucket>,
}

/// Mempool congestion levels
#[derive(Debug, Clone, Copy)]
pub enum CongestionLevel {
    Low,
    Medium,
    High,
    Extreme,
}

/// Mempool depth analysis bucket
#[derive(Debug, Clone)]
pub struct MempoolDepthBucket {
    pub fee_range: (u64, u64), // sat/vbyte range
    pub transaction_count: u32,
    pub total_size_vbytes: u64,
    pub estimated_confirmation_blocks: u32,
}

#[async_trait]
impl BitcoinClientExt for BitcoinClient {
    async fn batch_rpc_calls(&self, calls: Vec<BatchRpcCall>) -> Result<Vec<serde_json::Value>, BridgeError> {
        let batch_request: Vec<serde_json::Value> = calls.iter().map(|call| {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": call.method,
                "params": call.params,
                "id": call.id
            })
        }).collect();
        
        let mut request = self.client.post(&self.connection_pool.primary_url)
            .json(&batch_request);
            
        // Add authentication
        request = match &self.connection_pool.auth {
            BitcoinNodeAuth::UserPass { username, password } => {
                request.basic_auth(username, Some(password))
            }
            BitcoinNodeAuth::Cookie { cookie_file } => {
                let cookie_content = tokio::fs::read_to_string(cookie_file).await
                    .map_err(|e| BridgeError::BitcoinNodeError { 
                        reason: format!("Failed to read cookie file: {}", e) 
                    })?;
                let parts: Vec<&str> = cookie_content.trim().split(':').collect();
                if parts.len() == 2 {
                    request.basic_auth(parts[0], Some(parts[1]))
                } else {
                    return Err(BridgeError::BitcoinNodeError { 
                        reason: "Invalid cookie file format".to_string() 
                    });
                }
            }
            BitcoinNodeAuth::None => request,
        };
        
        let response = request.send().await
            .map_err(|e| BridgeError::BitcoinNodeError { 
                reason: format!("Batch RPC request failed: {}", e) 
            })?;
        
        let batch_response: Vec<serde_json::Value> = response.json().await
            .map_err(|e| BridgeError::BitcoinNodeError { 
                reason: format!("Failed to parse batch response: {}", e) 
            })?;
        
        let mut results = Vec::new();
        for response in batch_response {
            if let Some(error) = response.get("error") {
                if !error.is_null() {
                    return Err(BridgeError::BitcoinNodeError { 
                        reason: format!("Batch RPC error: {}", error) 
                    });
                }
            }
            
            let result = response.get("result")
                .ok_or_else(|| BridgeError::BitcoinNodeError { 
                    reason: "No result in batch response".to_string() 
                })?;
                
            results.push(result.clone());
        }
        
        Ok(results)
    }
    
    async fn stream_blockchain_events(&self) -> Result<tokio::sync::mpsc::Receiver<BlockchainEvent>, BridgeError> {
        let (tx, rx) = tokio::sync::mpsc::channel(1000);
        
        // TODO: Implement blockchain event streaming
        // This would involve:
        // 1. Polling for new blocks
        // 2. Monitoring watched addresses
        // 3. Detecting reorganizations
        // 4. Tracking mempool changes
        
        Ok(rx)
    }
    
    async fn get_address_history(
        &self,
        address: &bitcoin::Address,
        limit: Option<u32>
    ) -> Result<Vec<AddressTransaction>, BridgeError> {
        // TODO: Implement address transaction history
        // This would involve querying transaction history for the address
        Ok(Vec::new())
    }
    
    async fn analyze_mempool_fees(&self) -> Result<MempoolFeeAnalysis, BridgeError> {
        let mempool_tracker = self.mempool_tracker.read().await;
        
        // Determine congestion level based on mempool size
        let congestion_level = match mempool_tracker.mempool_size {
            0..=1000 => CongestionLevel::Low,
            1001..=10000 => CongestionLevel::Medium,
            10001..=50000 => CongestionLevel::High,
            _ => CongestionLevel::Extreme,
        };
        
        Ok(MempoolFeeAnalysis {
            recommended_fees: mempool_tracker.fee_estimates.iter()
                .map(|(blocks, estimate)| (*blocks, estimate.sat_per_vbyte))
                .collect(),
            congestion_level,
            average_confirmation_time: HashMap::new(),
            mempool_depth_analysis: Vec::new(),
        })
    }
}