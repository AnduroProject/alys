//! Bitcoin RPC Client Abstraction
//! 
//! Unified interface for Bitcoin node communication

use bitcoin::{Transaction, Txid, Block, BlockHash, Address as BtcAddress, OutPoint, TxOut};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};
use crate::types::*;

/// Bitcoin RPC client interface
#[async_trait::async_trait]
pub trait BitcoinRpc: Send + Sync {
    /// Get transaction by txid
    async fn get_transaction(&self, txid: &Txid) -> Result<Transaction, BitcoinRpcError>;
    
    /// Get raw transaction with block info
    async fn get_raw_transaction_verbose(&self, txid: &Txid) -> Result<VerboseTransaction, BitcoinRpcError>;
    
    /// Get block by hash
    async fn get_block(&self, hash: &BlockHash) -> Result<Block, BitcoinRpcError>;
    
    /// Get block hash by height
    async fn get_block_hash(&self, height: u64) -> Result<BlockHash, BitcoinRpcError>;
    
    /// Get current block height
    async fn get_block_count(&self) -> Result<u64, BitcoinRpcError>;
    
    /// Get UTXOs for an address
    async fn list_unspent(&self, address: &BtcAddress) -> Result<Vec<Utxo>, BitcoinRpcError>;
    
    /// Broadcast transaction
    async fn send_raw_transaction(&self, tx: &Transaction) -> Result<Txid, BitcoinRpcError>;
    
    /// Estimate fee for transaction
    async fn estimate_smart_fee(&self, conf_target: u32) -> Result<FeeEstimate, BitcoinRpcError>;
    
    /// Get transaction confirmations
    async fn get_transaction_confirmations(&self, txid: &Txid) -> Result<u32, BitcoinRpcError>;
    
    /// Check if transaction exists in mempool
    async fn is_in_mempool(&self, txid: &Txid) -> Result<bool, BitcoinRpcError>;
}

/// Bitcoin RPC client implementation
pub struct BitcoinRpcClient {
    rpc_url: String,
    auth: RpcAuth,
    client: reqwest::Client,
    network: bitcoin::Network,
    connection_pool: Arc<RwLock<ConnectionPool>>,
}

/// RPC authentication
#[derive(Clone, Debug)]
pub enum RpcAuth {
    UserPass { username: String, password: String },
    Cookie { cookie_path: String },
}

/// Connection pool for RPC requests
#[derive(Debug)]
struct ConnectionPool {
    max_connections: usize,
    current_connections: usize,
    timeout: Duration,
}

/// Verbose transaction response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerboseTransaction {
    pub txid: Txid,
    pub hash: String,
    pub size: u32,
    pub vsize: u32,
    pub weight: u32,
    pub version: u32,
    pub locktime: u32,
    pub confirmations: Option<u32>,
    pub blockhash: Option<BlockHash>,
    pub blockindex: Option<u32>,
    pub blocktime: Option<u64>,
    pub hex: String,
}

/// Fee estimation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeEstimate {
    pub feerate: f64, // BTC/kB
    pub blocks: u32,
    pub errors: Option<Vec<String>>,
}

/// UTXO information from RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utxo {
    pub txid: Txid,
    pub vout: u32,
    pub address: BtcAddress,
    pub label: Option<String>,
    pub script_pubkey: String,
    pub amount: f64, // BTC amount
    pub confirmations: u32,
    pub spendable: bool,
    pub solvable: bool,
    pub safe: bool,
}

impl BitcoinRpcClient {
    /// Create new Bitcoin RPC client
    pub fn new(
        rpc_url: String,
        auth: RpcAuth,
        network: bitcoin::Network,
    ) -> Result<Self, BitcoinRpcError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| BitcoinRpcError::ConnectionError(e.to_string()))?;

        let connection_pool = Arc::new(RwLock::new(ConnectionPool {
            max_connections: 10,
            current_connections: 0,
            timeout: Duration::from_secs(30),
        }));

        Ok(Self {
            rpc_url,
            auth,
            client,
            network,
            connection_pool,
        })
    }

    /// Make RPC request
    async fn rpc_call<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, BitcoinRpcError> {
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });

        let mut request_builder = self.client.post(&self.rpc_url)
            .json(&request_body)
            .header("Content-Type", "application/json");

        // Add authentication
        request_builder = match &self.auth {
            RpcAuth::UserPass { username, password } => {
                request_builder.basic_auth(username, Some(password))
            }
            RpcAuth::Cookie { cookie_path: _ } => {
                // TODO: Implement cookie authentication
                request_builder
            }
        };

        let response = request_builder
            .send()
            .await
            .map_err(|e| BitcoinRpcError::RequestError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BitcoinRpcError::HttpError(response.status().as_u16()));
        }

        let rpc_response: serde_json::Value = response
            .json()
            .await
            .map_err(|e| BitcoinRpcError::ParseError(e.to_string()))?;

        // Check for RPC errors
        if let Some(error) = rpc_response.get("error") {
            if !error.is_null() {
                return Err(BitcoinRpcError::RpcError(error.to_string()));
            }
        }

        // Extract result
        let result = rpc_response.get("result")
            .ok_or_else(|| BitcoinRpcError::ParseError("No result field".to_string()))?;

        serde_json::from_value(result.clone())
            .map_err(|e| BitcoinRpcError::ParseError(e.to_string()))
    }

    /// Convert BTC amount to satoshis
    fn btc_to_satoshis(btc: f64) -> u64 {
        (btc * 100_000_000.0) as u64
    }

    /// Convert satoshis to BTC
    fn satoshis_to_btc(satoshis: u64) -> f64 {
        satoshis as f64 / 100_000_000.0
    }
}

#[async_trait::async_trait]
impl BitcoinRpc for BitcoinRpcClient {
    async fn get_transaction(&self, txid: &Txid) -> Result<Transaction, BitcoinRpcError> {
        let hex_string: String = self.rpc_call(
            "getrawtransaction",
            serde_json::json!([txid.to_string()]),
        ).await?;

        let tx_bytes = hex::decode(hex_string)
            .map_err(|e| BitcoinRpcError::ParseError(e.to_string()))?;

        bitcoin::consensus::deserialize(&tx_bytes)
            .map_err(|e| BitcoinRpcError::ParseError(e.to_string()))
    }

    async fn get_raw_transaction_verbose(&self, txid: &Txid) -> Result<VerboseTransaction, BitcoinRpcError> {
        self.rpc_call(
            "getrawtransaction",
            serde_json::json!([txid.to_string(), true]),
        ).await
    }

    async fn get_block(&self, hash: &BlockHash) -> Result<Block, BitcoinRpcError> {
        let hex_string: String = self.rpc_call(
            "getblock",
            serde_json::json!([hash.to_string(), 0]),
        ).await?;

        let block_bytes = hex::decode(hex_string)
            .map_err(|e| BitcoinRpcError::ParseError(e.to_string()))?;

        bitcoin::consensus::deserialize(&block_bytes)
            .map_err(|e| BitcoinRpcError::ParseError(e.to_string()))
    }

    async fn get_block_hash(&self, height: u64) -> Result<BlockHash, BitcoinRpcError> {
        let hash_string: String = self.rpc_call(
            "getblockhash",
            serde_json::json!([height]),
        ).await?;

        BlockHash::from_str(&hash_string)
            .map_err(|e| BitcoinRpcError::ParseError(e.to_string()))
    }

    async fn get_block_count(&self) -> Result<u64, BitcoinRpcError> {
        self.rpc_call("getblockcount", serde_json::json!([])).await
    }

    async fn list_unspent(&self, address: &BtcAddress) -> Result<Vec<Utxo>, BitcoinRpcError> {
        let utxos: Vec<serde_json::Value> = self.rpc_call(
            "listunspent",
            serde_json::json!([0, 9999999, [address.to_string()]]),
        ).await?;

        let mut result = Vec::new();
        for utxo_json in utxos {
            let utxo: Utxo = serde_json::from_value(utxo_json)
                .map_err(|e| BitcoinRpcError::ParseError(e.to_string()))?;
            result.push(utxo);
        }

        Ok(result)
    }

    async fn send_raw_transaction(&self, tx: &Transaction) -> Result<Txid, BitcoinRpcError> {
        let tx_hex = hex::encode(bitcoin::consensus::serialize(tx));
        let txid_string: String = self.rpc_call(
            "sendrawtransaction",
            serde_json::json!([tx_hex]),
        ).await?;

        Txid::from_str(&txid_string)
            .map_err(|e| BitcoinRpcError::ParseError(e.to_string()))
    }

    async fn estimate_smart_fee(&self, conf_target: u32) -> Result<FeeEstimate, BitcoinRpcError> {
        self.rpc_call(
            "estimatesmartfee",
            serde_json::json!([conf_target]),
        ).await
    }

    async fn get_transaction_confirmations(&self, txid: &Txid) -> Result<u32, BitcoinRpcError> {
        let verbose_tx = self.get_raw_transaction_verbose(txid).await?;
        Ok(verbose_tx.confirmations.unwrap_or(0))
    }

    async fn is_in_mempool(&self, txid: &Txid) -> Result<bool, BitcoinRpcError> {
        // Try to get mempool entry
        match self.rpc_call::<serde_json::Value>(
            "getmempoolentry",
            serde_json::json!([txid.to_string()]),
        ).await {
            Ok(_) => Ok(true),
            Err(BitcoinRpcError::RpcError(_)) => Ok(false), // Transaction not in mempool
            Err(e) => Err(e),
        }
    }
}

/// Bitcoin RPC errors
#[derive(Debug, thiserror::Error)]
pub enum BitcoinRpcError {
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    #[error("Request error: {0}")]
    RequestError(String),
    
    #[error("HTTP error: {0}")]
    HttpError(u16),
    
    #[error("RPC error: {0}")]
    RpcError(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Timeout error")]
    TimeoutError,
    
    #[error("Transaction not found: {txid}")]
    TransactionNotFound { txid: Txid },
    
    #[error("Block not found: {hash}")]
    BlockNotFound { hash: BlockHash },
}

/// Bitcoin client factory
pub struct BitcoinClientFactory;

impl BitcoinClientFactory {
    /// Create Bitcoin RPC client from configuration
    pub fn create(
        rpc_url: String,
        auth: RpcAuth,
        network: bitcoin::Network,
    ) -> Result<Arc<dyn BitcoinRpc>, BitcoinRpcError> {
        let client = BitcoinRpcClient::new(rpc_url, auth, network)?;
        Ok(Arc::new(client))
    }

    /// Create mock Bitcoin client for testing
    #[cfg(test)]
    pub fn create_mock() -> Arc<dyn BitcoinRpc> {
        Arc::new(MockBitcoinRpc::new())
    }
}

/// Mock Bitcoin RPC client for testing
#[cfg(test)]
pub struct MockBitcoinRpc {
    transactions: std::sync::RwLock<std::collections::HashMap<Txid, Transaction>>,
    blocks: std::sync::RwLock<std::collections::HashMap<BlockHash, Block>>,
    utxos: std::sync::RwLock<std::collections::HashMap<BtcAddress, Vec<Utxo>>>,
}

#[cfg(test)]
impl MockBitcoinRpc {
    pub fn new() -> Self {
        Self {
            transactions: std::sync::RwLock::new(std::collections::HashMap::new()),
            blocks: std::sync::RwLock::new(std::collections::HashMap::new()),
            utxos: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub fn add_transaction(&self, tx: Transaction) {
        let mut transactions = self.transactions.write().unwrap();
        transactions.insert(tx.compute_txid(), tx);
    }

    pub fn add_utxo(&self, address: BtcAddress, utxo: Utxo) {
        let mut utxos = self.utxos.write().unwrap();
        utxos.entry(address).or_default().push(utxo);
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl BitcoinRpc for MockBitcoinRpc {
    async fn get_transaction(&self, txid: &Txid) -> Result<Transaction, BitcoinRpcError> {
        let transactions = self.transactions.read().unwrap();
        transactions.get(txid)
            .cloned()
            .ok_or(BitcoinRpcError::TransactionNotFound { txid: *txid })
    }

    async fn get_raw_transaction_verbose(&self, txid: &Txid) -> Result<VerboseTransaction, BitcoinRpcError> {
        let tx = self.get_transaction(txid).await?;
        Ok(VerboseTransaction {
            txid: *txid,
            hash: txid.to_string(),
            size: 250, // Mock values
            vsize: 250,
            weight: 1000,
            version: tx.version.0,
            locktime: tx.lock_time.to_consensus_u32(),
            confirmations: Some(6),
            blockhash: None,
            blockindex: None,
            blocktime: None,
            hex: hex::encode(bitcoin::consensus::serialize(&tx)),
        })
    }

    async fn get_block(&self, hash: &BlockHash) -> Result<Block, BitcoinRpcError> {
        let blocks = self.blocks.read().unwrap();
        blocks.get(hash)
            .cloned()
            .ok_or(BitcoinRpcError::BlockNotFound { hash: *hash })
    }

    async fn get_block_hash(&self, _height: u64) -> Result<BlockHash, BitcoinRpcError> {
        // Return mock hash
        Ok(BlockHash::all_zeros())
    }

    async fn get_block_count(&self) -> Result<u64, BitcoinRpcError> {
        Ok(800000) // Mock block height
    }

    async fn list_unspent(&self, address: &BtcAddress) -> Result<Vec<Utxo>, BitcoinRpcError> {
        let utxos = self.utxos.read().unwrap();
        Ok(utxos.get(address).cloned().unwrap_or_default())
    }

    async fn send_raw_transaction(&self, tx: &Transaction) -> Result<Txid, BitcoinRpcError> {
        let txid = tx.compute_txid();
        self.add_transaction(tx.clone());
        Ok(txid)
    }

    async fn estimate_smart_fee(&self, _conf_target: u32) -> Result<FeeEstimate, BitcoinRpcError> {
        Ok(FeeEstimate {
            feerate: 0.00001000, // 10 sat/vB
            blocks: 6,
            errors: None,
        })
    }

    async fn get_transaction_confirmations(&self, _txid: &Txid) -> Result<u32, BitcoinRpcError> {
        Ok(6) // Mock confirmations
    }

    async fn is_in_mempool(&self, _txid: &Txid) -> Result<bool, BitcoinRpcError> {
        Ok(false) // Mock: not in mempool
    }
}