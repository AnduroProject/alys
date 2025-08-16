//! Bitcoin node integration interface
//!
//! Provides integration with Bitcoin Core nodes for merged mining,
//! UTXO management, and blockchain monitoring.

use crate::types::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

/// Bitcoin RPC client implementation
#[derive(Debug)]
pub struct BitcoinRpcClient {
    url: String,
    auth: BitcoinNodeAuth,
    client: reqwest::Client,
    watched_addresses: std::sync::RwLock<HashMap<bitcoin::Address, std::time::SystemTime>>,
}

impl BitcoinRpcClient {
    /// Create new Bitcoin RPC client
    pub fn new(url: String, auth: BitcoinNodeAuth) -> Self {
        Self {
            url,
            auth,
            client: reqwest::Client::new(),
            watched_addresses: std::sync::RwLock::new(HashMap::new()),
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
impl BitcoinIntegration for BitcoinRpcClient {
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
    pub fn create(config: &BridgeConfig) -> Box<dyn BitcoinIntegration> {
        Box::new(BitcoinRpcClient::new(
            config.bitcoin_node_url.clone(),
            config.bitcoin_node_auth.clone(),
        ))
    }
}