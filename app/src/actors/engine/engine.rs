//! Core Engine Implementation
//!
//! This module contains the core Engine struct and implementation that was moved
//! from the main engine.rs file. It preserves all existing functionality while
//! being wrapped by the EngineActor for message-driven operations.

use std::ops::{Div, Mul};
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, info, trace, warn};

use lighthouse_wrapper::execution_layer::{
    auth::{Auth, JwtKey},
    BlockByNumberQuery, ExecutionBlockWithTransactions, ForkchoiceState, HttpJsonRpc,
    PayloadAttributes, DEFAULT_EXECUTION_ENDPOINT, LATEST_TAG,
};
use lighthouse_wrapper::sensitive_url::SensitiveUrl;
use lighthouse_wrapper::types::{
    Address, ExecutionBlockHash, ExecutionPayload, ExecutionPayloadCapella, MainnetEthSpec,
    Uint256, Withdrawal,
};
use lighthouse_wrapper::{execution_layer, types};
use serde_json::json;
use ssz_types::VariableList;

use crate::error::Error;
use crate::metrics::{ENGINE_BUILD_BLOCK_CALLS, ENGINE_COMMIT_BLOCK_CALLS};
use crate::types::*;
use super::{config::EngineConfig, EngineError, EngineResult};

const DEFAULT_EXECUTION_PUBLIC_ENDPOINT: &str = "http://0.0.0.0:8545";
const ENGINE_API_QUERY_RETRY_COUNT: i32 = 3;

/// Consensus amount representation (Gwei = 1e9 wei)
#[derive(Debug, Default, Clone)]
pub struct ConsensusAmount(pub u64);

impl ConsensusAmount {
    /// Convert from wei to consensus amount (Gwei)
    pub fn from_wei(amount: Uint256) -> Self {
        // https://github.com/ethereum/go-ethereum/blob/6a724b94db95a58fae772c389e379bb38ed5b93c/consensus/beacon/consensus.go#L359
        Self(amount.div(10u32.pow(9)).try_into().unwrap_or(0))
    }

    /// Convert from satoshi to consensus amount (with 10x multiplier for Alys)
    pub fn from_satoshi(amount: u64) -> Self {
        Self(amount.mul(10))
    }
}

impl PartialEq<u64> for ConsensusAmount {
    fn eq(&self, other: &u64) -> bool {
        self.0 == *other
    }
}

impl std::ops::Add for ConsensusAmount {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

/// Balance addition for withdrawals (peg-ins)
pub struct AddBalance(Address, ConsensusAmount);

impl From<(Address, ConsensusAmount)> for AddBalance {
    fn from((address, amount): (Address, ConsensusAmount)) -> Self {
        Self(address, amount)
    }
}

impl From<AddBalance> for Withdrawal {
    fn from(value: AddBalance) -> Self {
        Withdrawal {
            index: 0,
            validator_index: 0,
            address: value.0,
            amount: (value.1).0,
        }
    }
}

/// Dead address for burning fees
const DEAD_ADDRESS: &str = "0x000000000000000000000000000000000000dEaD";

/// Core Engine implementation that handles execution layer operations
pub struct Engine {
    /// Engine API client for authenticated operations
    pub api: HttpJsonRpc,
    /// Public execution API client for queries
    pub execution_api: HttpJsonRpc,
    /// Current finalized block hash
    finalized: RwLock<Option<ExecutionBlockHash>>,
}

impl Engine {
    /// Create a new Engine with the given API clients
    pub fn new(api: HttpJsonRpc, execution_api: HttpJsonRpc) -> Self {
        Self {
            api,
            execution_api,
            finalized: Default::default(),
        }
    }
    
    /// Create a new Engine from configuration
    pub fn from_config(config: &EngineConfig) -> EngineResult<Self> {
        let jwt_key = JwtKey::from_slice(&config.jwt_secret)
            .map_err(|_| EngineError::ConfigError("Invalid JWT secret".to_string()))?;
        
        let api = new_http_engine_json_rpc(Some(config.engine_url.clone()), jwt_key);
        let execution_api = new_http_public_execution_json_rpc(config.public_url.clone());
        
        Ok(Self::new(api, execution_api))
    }

    /// Set the finalized block hash
    pub async fn set_finalized(&self, block_hash: ExecutionBlockHash) {
        *self.finalized.write().await = Some(block_hash);
    }

    /// Build a new execution block
    pub async fn build_block(
        &self,
        timestamp: Duration,
        payload_head: Option<ExecutionBlockHash>,
        add_balances: Vec<AddBalance>,
    ) -> Result<ExecutionPayload<MainnetEthSpec>, Error> {
        ENGINE_BUILD_BLOCK_CALLS
            .with_label_values(&["called", "default"])
            .inc();

        info!(
            "Building block: timestamp={:?}, payload_head={:?}, withdrawals={}",
            timestamp,
            payload_head,
            add_balances.len()
        );

        // FIXME: Geth is not accepting >4 withdrawals currently
        let payload_attributes = PayloadAttributes::new(
            timestamp.as_secs(),
            // TODO: set proper randao value
            Default::default(),
            // NOTE: we burn fees at the EL and mint later
            Address::from_str(DEAD_ADDRESS).unwrap(),
            Some(add_balances.into_iter().map(Into::into).collect()),
        );

        let head = match payload_head {
            Some(head) => head, // all blocks except block 0 will be `Some`
            None => {
                let latest_block = self
                    .api
                    .get_block_by_number(BlockByNumberQuery::Tag(LATEST_TAG))
                    .await
                    .map_err(|err| {
                        ENGINE_BUILD_BLOCK_CALLS
                            .with_label_values(&["failed", "get_latest_block_error"])
                            .inc();
                        Error::EngineApiError(format!("Failed to get latest block: {:?}", err))
                    })?
                    .ok_or_else(|| {
                        ENGINE_BUILD_BLOCK_CALLS
                            .with_label_values(&["failed", "no_latest_block"])
                            .inc();
                        Error::EngineApiError("No latest block available".to_string())
                    })?;
                latest_block.block_hash
            }
        };

        let finalized = self.finalized.read().await.unwrap_or_default();
        let forkchoice_state = ForkchoiceState {
            head_block_hash: head,
            finalized_block_hash: finalized,
            safe_block_hash: finalized,
        };

        // Lighthouse should automatically call `engine_exchangeCapabilities` if not cached
        let response = self
            .api
            .forkchoice_updated(forkchoice_state, Some(payload_attributes))
            .await
            .map_err(|err| {
                ENGINE_BUILD_BLOCK_CALLS
                    .with_label_values(&["failed", "engine_api_forkchoice_updated_error"])
                    .inc();
                Error::EngineApiError(format!("Forkchoice update failed: {:?}", err))
            })?;
        
        trace!("Forkchoice updated response: {:?}", response);
        
        let payload_id = response.payload_id.ok_or_else(|| {
            ENGINE_BUILD_BLOCK_CALLS
                .with_label_values(&["failed", "no_payload_id"])
                .inc();
            Error::PayloadIdUnavailable
        })?;

        let response = self
            .api
            .get_payload::<MainnetEthSpec>(types::ForkName::Capella, payload_id)
            .await
            .map_err(|err| {
                ENGINE_BUILD_BLOCK_CALLS
                    .with_label_values(&["failed", "engine_api_get_payload_error"])
                    .inc();
                Error::EngineApiError(format!("Get payload failed: {:?}", err))
            })?;

        info!("Expected block value is {}", response.block_value());

        // Extract execution payload
        // https://github.com/ethereum/go-ethereum/blob/577be37e0e7a69564224e0a15e49d648ed461ac5/miner/payload_building.go#L178
        let execution_payload = response.execution_payload_ref().clone_from_ref();

        ENGINE_BUILD_BLOCK_CALLS
            .with_label_values(&["success", "default"])
            .inc();

        Ok(execution_payload)
    }

    /// Commit an execution block to the execution client
    pub async fn commit_block(
        &self,
        execution_payload: ExecutionPayload<MainnetEthSpec>,
    ) -> Result<ExecutionBlockHash, Error> {
        ENGINE_COMMIT_BLOCK_CALLS
            .with_label_values(&["called"])
            .inc();

        info!("Committing block with hash: {}", execution_payload.block_hash());

        let finalized = self.finalized.read().await.unwrap_or_default();

        // Update forkchoice to prepare for the new payload
        self.api
            .forkchoice_updated(
                ForkchoiceState {
                    head_block_hash: execution_payload.parent_hash(),
                    safe_block_hash: finalized,
                    finalized_block_hash: finalized,
                },
                None,
            )
            .await
            .map_err(|err| {
                warn!("Forkchoice update before commit failed: {:?}", err);
                // Continue anyway, as this is not critical
            });

        // Submit the new payload to the execution client
        // https://github.com/ethereum/go-ethereum/blob/577be37e0e7a69564224e0a15e49d648ed461ac5/eth/catalyst/api.go#L259
        let response = self
            .api
            .new_payload::<MainnetEthSpec>(execution_payload)
            .await
            .map_err(|err| {
                ENGINE_COMMIT_BLOCK_CALLS
                    .with_label_values(&["engine_api_new_payload_error"])
                    .inc();
                Error::EngineApiError(format!("New payload failed: {:?}", err))
            })?;
        
        let head = response.latest_valid_hash.ok_or_else(|| {
            ENGINE_COMMIT_BLOCK_CALLS
                .with_label_values(&["engine_api_invalid_block_hash_error"])
                .inc();
            Error::InvalidBlockHash
        })?;

        // Update forkchoice to the new head so we can fetch transactions and receipts
        self.api
            .forkchoice_updated(
                ForkchoiceState {
                    head_block_hash: head,
                    safe_block_hash: finalized,
                    finalized_block_hash: finalized,
                },
                None,
            )
            .await
            .map_err(|err| {
                warn!("Forkchoice update after commit failed: {:?}", err);
                // This is more critical, but we'll return the hash anyway
            });

        ENGINE_COMMIT_BLOCK_CALLS
            .with_label_values(&["success"])
            .inc();

        Ok(head)
    }

    /// Get a block with transactions using engine API
    /// 
    /// This is a workaround for issues where the non-engine RPC interfaces fail to fetch blocks.
    /// We use the engine's RPC connection. Despite the spec not requiring support for this
    /// function, it works for Geth.
    pub async fn get_block_with_txs(
        &self,
        block_hash: &ExecutionBlockHash,
    ) -> Result<
        Option<ethers_core::types::Block<ethers_core::types::Transaction>>,
        execution_layer::Error,
    > {
        let params = json!([block_hash, true]);

        trace!("Querying `eth_getBlockByHash` with params: {:?}", params);

        let rpc_result = self
            .api
            .rpc_request::<Option<ethers_core::types::Block<ethers_core::types::Transaction>>>(
                "eth_getBlockByHash",
                params,
                Duration::from_secs(10),
            )
            .await;

        Ok(rpc_result?)
    }

    /// Get transaction receipt with retry logic
    /// 
    /// This uses the execution API client with retry logic to handle temporary failures.
    pub async fn get_transaction_receipt(
        &self,
        transaction_hash: H256,
    ) -> Result<Option<ethers_core::types::TransactionReceipt>, execution_layer::Error> {
        let params = json!([transaction_hash]);
        
        for attempt in 0..ENGINE_API_QUERY_RETRY_COUNT {
            debug!(
                "Querying `eth_getTransactionReceipt` with params: {:?}, attempt: {}",
                params, attempt + 1
            );
            
            let rpc_result = self
                .execution_api
                .rpc_request::<Option<ethers_core::types::TransactionReceipt>>(
                    "eth_getTransactionReceipt",
                    params.clone(),
                    Duration::from_secs(5),
                )
                .await;
            
            match rpc_result {
                Ok(receipt) => return Ok(receipt),
                Err(e) if attempt < ENGINE_API_QUERY_RETRY_COUNT - 1 => {
                    warn!(
                        "Transaction receipt query failed (attempt {}): {}, retrying...",
                        attempt + 1, e
                    );
                    sleep(Duration::from_millis(500)).await;
                },
                Err(e) => {
                    return Err(execution_layer::Error::InvalidPayloadBody(format!(
                        "Failed to fetch transaction receipt after {} attempts: {}",
                        ENGINE_API_QUERY_RETRY_COUNT, e
                    )));
                }
            }
        }
        
        unreachable!()
    }

    /// Get payload by tag from engine API
    /// 
    /// This method fetches a payload by block number or tag and converts it to the
    /// appropriate format for Alys.
    /// 
    /// Reference: https://github.com/sigp/lighthouse/blob/441fc1691b69f9edc4bbdc6665f3efab16265c9b/beacon_node/execution_layer/src/lib.rs#L1634
    pub async fn get_payload_by_tag_from_engine(
        &self,
        query: BlockByNumberQuery<'_>,
    ) -> Result<ExecutionPayloadCapella<MainnetEthSpec>, Error> {
        debug!("Fetching payload by tag: {:?}", query);
        
        // Get the execution block header
        let execution_block = self.api.get_block_by_number(query).await
            .map_err(|err| Error::EngineApiError(format!("Failed to get block: {:?}", err)))?
            .ok_or_else(|| Error::EngineApiError("Block not found".to_string()))?;

        // Get the full block with transactions
        // https://github.com/sigp/lighthouse/blob/441fc1691b69f9edc4bbdc6665f3efab16265c9b/beacon_node/execution_layer/src/lib.rs#L1634
        let execution_block_with_txs = self
            .api
            .get_block_by_hash_with_txns::<MainnetEthSpec>(
                execution_block.block_hash,
                types::ForkName::Capella,
            )
            .await
            .map_err(|err| Error::EngineApiError(format!("Failed to get block with transactions: {:?}", err)))?
            .ok_or_else(|| Error::EngineApiError("Block with transactions not found".to_string()))?;

        // Convert transactions to the proper format
        let transactions = VariableList::new(
            execution_block_with_txs
                .transactions()
                .iter()
                .map(|transaction| VariableList::new(transaction.rlp().to_vec()))
                .collect::<Result<_, _>>()
                .map_err(|err| Error::EngineApiError(format!("Failed to process transactions: {:?}", err)))?
        )
        .map_err(|err| Error::EngineApiError(format!("Failed to create transaction list: {:?}", err)))?;

        // Handle different fork versions
        match execution_block_with_txs {
            ExecutionBlockWithTransactions::Capella(capella_block) => {
                let withdrawals = VariableList::new(
                    capella_block
                        .withdrawals
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                )
                .map_err(|err| Error::EngineApiError(format!("Failed to process withdrawals: {:?}", err)))?;
                
                Ok(ExecutionPayloadCapella {
                    parent_hash: capella_block.parent_hash,
                    fee_recipient: capella_block.fee_recipient,
                    state_root: capella_block.state_root,
                    receipts_root: capella_block.receipts_root,
                    logs_bloom: capella_block.logs_bloom,
                    prev_randao: capella_block.prev_randao,
                    block_number: capella_block.block_number,
                    gas_limit: capella_block.gas_limit,
                    gas_used: capella_block.gas_used,
                    timestamp: capella_block.timestamp,
                    extra_data: capella_block.extra_data,
                    base_fee_per_gas: capella_block.base_fee_per_gas,
                    block_hash: capella_block.block_hash,
                    transactions,
                    withdrawals,
                })
            }
            _ => {
                Err(Error::EngineApiError("Unsupported fork version".to_string()))
            }
        }
    }

    /// Get the current finalized block hash
    pub async fn get_finalized(&self) -> Option<ExecutionBlockHash> {
        *self.finalized.read().await
    }

    /// Check if the execution client is healthy
    pub async fn is_healthy(&self) -> bool {
        // Try a simple RPC call to check connectivity
        match self.api.rpc_request::<String>(
            "web3_clientVersion",
            serde_json::Value::Null,
            Duration::from_secs(5)
        ).await {
            Ok(_) => true,
            Err(e) => {
                warn!("Engine health check failed: {}", e);
                false
            }
        }
    }

    /// Get client version information
    pub async fn get_client_version(&self) -> Result<String, Error> {
        self.api.rpc_request::<String>(
            "web3_clientVersion",
            serde_json::Value::Null,
            Duration::from_secs(5)
        )
        .await
        .map_err(|e| Error::EngineApiError(format!("Failed to get client version: {}", e)))
    }

    /// Get the latest block number
    pub async fn get_latest_block_number(&self) -> Result<u64, Error> {
        let block_number_hex = self.execution_api.rpc_request::<String>(
            "eth_blockNumber",
            serde_json::Value::Null,
            Duration::from_secs(5)
        )
        .await
        .map_err(|e| Error::EngineApiError(format!("Failed to get block number: {}", e)))?;

        u64::from_str_radix(block_number_hex.trim_start_matches("0x"), 16)
            .map_err(|e| Error::EngineApiError(format!("Invalid block number format: {}", e)))
    }

    /// Check if the client is currently syncing
    pub async fn is_syncing(&self) -> Result<bool, Error> {
        // eth_syncing returns false when not syncing, or an object when syncing
        let syncing_result = self.execution_api.rpc_request::<serde_json::Value>(
            "eth_syncing",
            serde_json::Value::Null,
            Duration::from_secs(5)
        )
        .await
        .map_err(|e| Error::EngineApiError(format!("Failed to get sync status: {}", e)))?;

        match syncing_result {
            serde_json::Value::Bool(false) => Ok(false),
            serde_json::Value::Object(_) => Ok(true),
            _ => Ok(false), // Default to not syncing if unexpected format
        }
    }
}

/// Create a new HTTP engine JSON-RPC client with JWT authentication
pub fn new_http_engine_json_rpc(url_override: Option<String>, jwt_key: JwtKey) -> HttpJsonRpc {
    let rpc_auth = Auth::new(jwt_key, None, None);
    let rpc_url = SensitiveUrl::parse(&url_override.unwrap_or(DEFAULT_EXECUTION_ENDPOINT.to_string()))
        .expect("Invalid engine URL");
    HttpJsonRpc::new_with_auth(rpc_url, rpc_auth, Some(3))
        .expect("Failed to create engine API client")
}

/// Create a new HTTP public execution JSON-RPC client without authentication
pub fn new_http_public_execution_json_rpc(url_override: Option<String>) -> HttpJsonRpc {
    let rpc_url = SensitiveUrl::parse(&url_override.unwrap_or(DEFAULT_EXECUTION_PUBLIC_ENDPOINT.to_string()))
        .expect("Invalid public execution URL");
    HttpJsonRpc::new(rpc_url, Some(3))
        .expect("Failed to create public API client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consensus_amount_conversion() {
        // Test wei to consensus amount conversion
        let wei_amount = Uint256::from(1_000_000_000u64); // 1 Gwei in wei
        let consensus_amount = ConsensusAmount::from_wei(wei_amount);
        assert_eq!(consensus_amount.0, 1);

        // Test satoshi to consensus amount conversion
        let satoshi_amount = 100_000_000u64; // 1 BTC in satoshis
        let consensus_amount = ConsensusAmount::from_satoshi(satoshi_amount);
        assert_eq!(consensus_amount.0, 1_000_000_000); // 10x multiplier
    }

    #[test]
    fn test_add_balance_to_withdrawal() {
        let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
        let amount = ConsensusAmount(1000);
        let add_balance = AddBalance(address, amount);
        
        let withdrawal: Withdrawal = add_balance.into();
        assert_eq!(withdrawal.address, address);
        assert_eq!(withdrawal.amount, 1000);
        assert_eq!(withdrawal.index, 0);
        assert_eq!(withdrawal.validator_index, 0);
    }

    #[test]
    fn test_consensus_amount_arithmetic() {
        let amount1 = ConsensusAmount(100);
        let amount2 = ConsensusAmount(200);
        let sum = amount1 + amount2;
        assert_eq!(sum.0, 300);
    }

    #[test]
    fn test_consensus_amount_equality() {
        let amount = ConsensusAmount(123);
        assert_eq!(amount, 123u64);
        assert_ne!(amount, 124u64);
    }
}