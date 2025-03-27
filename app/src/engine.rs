use crate::error::Error;
use ethereum_types::H256;
use ethers_core::types::TransactionReceipt;
use execution_layer::{
    auth::{Auth, JwtKey},
    BlockByNumberQuery, ExecutionBlockWithTransactions, ForkchoiceState, HttpJsonRpc,
    PayloadAttributes, DEFAULT_EXECUTION_ENDPOINT, LATEST_TAG,
};
use sensitive_url::SensitiveUrl;
use serde_json::json;
use ssz_types::VariableList;
use std::{
    ops::{Div, Mul},
    str::FromStr,
    time::Duration,
};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, trace};
use types::{
    Address, ExecutionBlockHash, ExecutionPayload, ExecutionPayloadCapella, MainnetEthSpec,
    Uint256, Withdrawal,
};

const DEFAULT_EXECUTION_PUBLIC_ENDPOINT: &str = "http://0.0.0.0:8545";
const DEFAULT_JWT_SECRET: [u8; 32] = [42; 32];

#[derive(Debug, Default, Clone)]
pub struct ConsensusAmount(pub u64); // Gwei = 1e9

impl ConsensusAmount {
    pub fn from_wei(amount: Uint256) -> Self {
        // https://github.com/ethereum/go-ethereum/blob/6a724b94db95a58fae772c389e379bb38ed5b93c/consensus/beacon/consensus.go#L359
        Self(amount.div(10u32.pow(9)).try_into().unwrap())
    }

    pub fn from_satoshi(amount: u64) -> Self {
        Self(amount.mul(10))
    }
}

impl std::cmp::PartialEq<u64> for ConsensusAmount {
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

const DEAD_ADDRESS: &str = "0x000000000000000000000000000000000000dEaD";

pub struct Engine {
    pub api: HttpJsonRpc,
    pub execution_api: HttpJsonRpc,
    finalized: RwLock<Option<ExecutionBlockHash>>,
    jwt_key: [u8; 32],
}

impl Engine {
    pub fn new(api: HttpJsonRpc, execution_api: HttpJsonRpc, jwt_key_bytes: [u8; 32]) -> Self {
        Self {
            api,
            execution_api,
            finalized: Default::default(),
            jwt_key: jwt_key_bytes,
        }
    }

    pub async fn set_finalized(&self, block_hash: ExecutionBlockHash) {
        *self.finalized.write().await = Some(block_hash);
    }

    pub async fn build_block(
        &self,
        timestamp: Duration,
        payload_head: Option<ExecutionBlockHash>,
        add_balances: Vec<AddBalance>,
    ) -> Result<ExecutionPayload<MainnetEthSpec>, Error> {
        // FIXME: geth is not accepting >4 withdrawals
        let payload_attributes = PayloadAttributes::new(
            timestamp.as_secs(),
            // TODO: set randao
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
                    .unwrap()
                    .unwrap();
                latest_block.block_hash
            }
        };

        let finalized = self.finalized.read().await.unwrap_or_default();
        let forkchoice_state = ForkchoiceState {
            head_block_hash: head,
            finalized_block_hash: finalized,
            safe_block_hash: finalized,
        };

        // lighthouse should automatically call `engine_exchangeCapabilities` if not cached
        let response = self
            .api
            .forkchoice_updated(forkchoice_state, Some(payload_attributes))
            .await
            .map_err(|err| Error::EngineApiError(format!("{:?}", err)))?;
        trace!("Forkchoice updated: {:?}", response);
        let payload_id = response.payload_id.ok_or(Error::PayloadIdUnavailable)?;

        let response = self
            .api
            .get_payload::<MainnetEthSpec>(types::ForkName::Capella, payload_id)
            .await
            .map_err(|err| Error::EngineApiError(format!("{:?}", err)))?;

        tracing::info!("Expected block value is {}", response.block_value());

        // https://github.com/ethereum/go-ethereum/blob/577be37e0e7a69564224e0a15e49d648ed461ac5/miner/payload_building.go#L178
        let execution_payload = response.execution_payload_ref().clone_from_ref();

        Ok(execution_payload)
    }

    pub async fn commit_block(
        &self,
        execution_payload: ExecutionPayload<MainnetEthSpec>,
    ) -> Result<ExecutionBlockHash, Error> {
        let finalized = self.finalized.read().await.unwrap_or_default();

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
            .unwrap();

        // we need to push the payload back to geth
        // https://github.com/ethereum/go-ethereum/blob/577be37e0e7a69564224e0a15e49d648ed461ac5/eth/catalyst/api.go#L259
        let response = self
            .api
            .new_payload::<MainnetEthSpec>(execution_payload)
            .await
            .map_err(|err| Error::EngineApiError(format!("{:?}", err)))?;
        let head = response.latest_valid_hash.ok_or(Error::InvalidBlockHash)?;

        // update now to the new head so we can fetch the txs and
        // receipts from the ethereum rpc
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
            .unwrap();

        Ok(head)
    }

    // workaround for a problem where the non-engine rpc interfaces fail to fetch blocks:
    // we use the engine's rpc connection. Despite the spec not requiring the support
    // of this function, it works for geth
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
                Duration::from_secs(1),
            )
            .await;

        Ok(rpc_result?)
    }

    // workaround for a problem where the non-engine rpc interfaces fail to fetch blocks:
    // we use the engine's rpc connection. Despite the spec not requiring the support
    // of this function, it works for geth
    pub async fn get_transaction_receipt(
        &self,
        transaction_hash: H256,
    ) -> Result<Option<TransactionReceipt>, execution_layer::Error> {
        let params = json!([transaction_hash]);
        for i in 0..1 {
            debug!(
                "Querying `eth_getTransactionReceipt` with params: {:?}, attempt: {}",
                params, i
            );
            let rpc_result = self
                .execution_api
                .rpc_request::<Option<TransactionReceipt>>(
                    "eth_getTransactionReceipt",
                    params.clone(),
                    Duration::from_secs(3),
                )
                .await;
            if rpc_result.is_ok() {
                return Ok(rpc_result?);
            } else if i > 0 {
                sleep(Duration::from_millis(500)).await;
            }
        }
        Err(execution_layer::Error::InvalidPayloadBody(
            "Failed to fetch transaction receipt".to_string(),
        ))
        // let rpc_result = self
        //     .api
        //     .rpc_request::<Option<TransactionReceipt>>(
        //         "eth_getTransactionReceipt",
        //         params,
        //         Duration::from_secs(1),
        //     )
        //     .await;
        // Ok(rpc_result?)
    }

    // https://github.com/sigp/lighthouse/blob/441fc1691b69f9edc4bbdc6665f3efab16265c9b/beacon_node/execution_layer/src/lib.rs#L1634
    pub async fn get_payload_by_tag_from_engine(
        &self,
        query: BlockByNumberQuery<'_>,
    ) -> Result<ExecutionPayloadCapella<MainnetEthSpec>, Error> {
        // TODO: handle errors
        let execution_block = self.api.get_block_by_number(query).await.unwrap().unwrap();

        // https://github.com/sigp/lighthouse/blob/441fc1691b69f9edc4bbdc6665f3efab16265c9b/beacon_node/execution_layer/src/lib.rs#L1634
        let execution_block_with_txs = self
            .api
            .get_block_by_hash_with_txns::<MainnetEthSpec>(
                execution_block.block_hash,
                types::ForkName::Capella,
            )
            .await
            .unwrap()
            .unwrap();

        let transactions = VariableList::new(
            execution_block_with_txs
                .transactions()
                .iter()
                .map(|transaction| VariableList::new(transaction.rlp().to_vec()))
                .collect::<Result<_, _>>()
                .unwrap(),
        )
        .unwrap();

        Ok(match execution_block_with_txs {
            ExecutionBlockWithTransactions::Capella(capella_block) => {
                let withdrawals = VariableList::new(
                    capella_block
                        .withdrawals
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                )
                .unwrap();
                ExecutionPayloadCapella {
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
                }
            }
            _ => panic!("Unknown fork"),
        })
    }
}

pub fn new_http_engine_json_rpc(url_override: Option<String>, jwt_key: JwtKey) -> HttpJsonRpc {
    let rpc_auth = Auth::new(jwt_key, None, None);
    let rpc_url =
        SensitiveUrl::parse(&url_override.unwrap_or(DEFAULT_EXECUTION_ENDPOINT.to_string()))
            .unwrap();
    HttpJsonRpc::new_with_auth(rpc_url, rpc_auth, Some(3)).unwrap()
}

pub fn new_http_public_execution_json_rpc(url_override: Option<String>) -> HttpJsonRpc {
    let rpc_url =
        SensitiveUrl::parse(&url_override.unwrap_or(DEFAULT_EXECUTION_PUBLIC_ENDPOINT.to_string()))
            .unwrap();
    HttpJsonRpc::new(rpc_url, Some(3)).unwrap()
}
