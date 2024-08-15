use crate::error::Error;
use ethereum_types::H256;
use ethers_core::types::TransactionReceipt;
use execution_layer::{auth::{Auth, JwtKey}, BlockByNumberQuery, ExecutionBlockWithTransactions, ForkchoiceState, HttpJsonRpc, PayloadAttributes, DEFAULT_EXECUTION_ENDPOINT, LATEST_TAG, NewPayloadRequest, NewPayloadRequestDeneb};
use sensitive_url::SensitiveUrl;
use serde_json::json;
use std::{
    ops::{Div, Mul},
    str::FromStr,
    time::Duration,
};
use tokio::sync::RwLock;
use tracing::{instrument, trace};
use types::{Address, VariableList, EthSpec, ExecutionBlockHash, ExecutionPayload, ExecutionPayloadDeneb, MainnetEthSpec, Transaction, Transactions, Uint256, Withdrawal, Withdrawals, Hash256};

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

#[derive(Debug)]
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
    finalized: RwLock<Option<ExecutionBlockHash>>,
}

impl Engine {
    pub fn new(api: HttpJsonRpc) -> Self {
        Self {
            api,
            finalized: Default::default(),
        }
    }

    pub async fn set_finalized(&self, block_hash: ExecutionBlockHash) {
        *self.finalized.write().await = Some(block_hash);
    }

    #[instrument(level = "trace", skip(self, add_balances))]
    pub async fn build_block(
        &self,
        timestamp: Duration,
        payload_head: Option<ExecutionBlockHash>,
        add_balances: Vec<AddBalance>,
        previous_consensus_block_hash: Hash256,
    ) -> Result<ExecutionPayload<MainnetEthSpec>, Error> {
        // FIXME: geth is not accepting >4 withdrawals
        let payload_attributes = PayloadAttributes::new(
            timestamp.as_secs(),
            // TODO: set randao
            Default::default(),
            // NOTE: we burn fees at the EL and mint later
            Address::from_str(DEAD_ADDRESS).unwrap(),
            Some(add_balances.into_iter().map(Into::into).collect()),
            Some(previous_consensus_block_hash),
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
        let payload_id = response.payload_id.ok_or(Error::PayloadIdUnavailable)?;

        let response = self
            .api
            .get_payload::<MainnetEthSpec>(types::ForkName::Deneb, payload_id)
            .await
            .map_err(|err| Error::EngineApiError(format!("{:?}", err)))?;

        tracing::info!("Expected block value is {}", response.block_value());

        // https://github.com/ethereum/go-ethereum/blob/577be37e0e7a69564224e0a15e49d648ed461ac5/miner/payload_building.go#L178
        let execution_payload = response.execution_payload_ref().clone_from_ref();

        Ok(execution_payload)
    }

    #[instrument(level = "trace", skip(self))]
    pub async fn commit_block(
        &self,
        execution_payload: ExecutionPayload<MainnetEthSpec>,
        previous_consensus_block_hash: Hash256,
    ) -> Result<ExecutionBlockHash, Error> {
        let finalized = self.finalized.read().await.unwrap_or_default();

        let payload_attributes = PayloadAttributes::new(
            execution_payload.timestamp(),
            // TODO: set randao
            Default::default(),
            // NOTE: we burn fees at the EL and mint later
            Address::from_str(DEAD_ADDRESS).unwrap(),
            Some(execution_payload.withdrawals().unwrap().to_vec()),
            Some(previous_consensus_block_hash),
        );
        self.api
            .forkchoice_updated(
                ForkchoiceState {
                    head_block_hash: execution_payload.parent_hash(),
                    safe_block_hash: finalized,
                    finalized_block_hash: finalized,
                },
                Some(payload_attributes.clone()),
            )
            .await
            .unwrap();

        // we need to push the payload back to geth
        // https://github.com/ethereum/go-ethereum/blob/577be37e0e7a69564224e0a15e49d648ed461ac5/eth/catalyst/api.go#L259
        trace!("Pushing payload to geth");
        trace!("Parent hash: {:?}", hex::encode(previous_consensus_block_hash));
        let response = self
          .api
          .new_payload::<MainnetEthSpec>(NewPayloadRequest::Deneb( NewPayloadRequestDeneb{
              execution_payload: execution_payload.as_deneb().unwrap(),
              versioned_hashes: vec![],
              parent_beacon_block_root: previous_consensus_block_hash,
          }))
          .await
          .map_err(|err| Error::EngineApiError(format!("{:?}", err)))?;
        let head = response.latest_valid_hash.ok_or(Error::InvalidBlockHash)?;

        // update to the new head to fetch the txs and
        // receipts from the ethereum rpc
        self.api
            .forkchoice_updated(
                ForkchoiceState {
                    head_block_hash: head,
                    safe_block_hash: finalized,
                    finalized_block_hash: finalized,
                },
                Some(payload_attributes),
            )
            .await
            .unwrap();

        Ok(head)
    }

    // workaround for a problem where the non-engine rpc interfaces fail to fetch blocks:
    // we use the engine's rpc connection. Despite the spec not requiring the support
    // of this function, it works for geth
    #[instrument(level = "trace", skip(self))]
    pub async fn get_block_with_txs(
        &self,
        block_hash: &ExecutionBlockHash,
    ) -> Result<
        Option<ethers_core::types::Block<ethers_core::types::Transaction>>,
        execution_layer::Error,
    > {
        let params = json!([block_hash, true]);
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
    #[instrument(level = "trace", skip(self))]
    pub async fn get_transaction_receipt(
        &self,
        transaction_hash: H256,
    ) -> Result<Option<TransactionReceipt>, execution_layer::Error> {
        let params = json!([transaction_hash]);
        let rpc_result = self
            .api
            .rpc_request::<Option<TransactionReceipt>>(
                "eth_getTransactionReceipt",
                params,
                Duration::from_secs(1),
            )
            .await;
        Ok(rpc_result?)
    }

    // https://github.com/sigp/lighthouse/blob/441fc1691b69f9edc4bbdc6665f3efab16265c9b/beacon_node/execution_layer/src/lib.rs#L1634
    #[instrument(level = "trace", skip(self))]
    pub async fn get_payload_by_tag_from_engine<'a, E: EthSpec>(
        &self,
        query: BlockByNumberQuery<'a>,
    ) -> Result<ExecutionPayloadDeneb<E>, Error> {
        // TODO: handle errors
        let execution_block = self.api.get_block_by_number(query).await.unwrap().unwrap();

        // https://github.com/sigp/lighthouse/blob/441fc1691b69f9edc4bbdc6665f3efab16265c9b/beacon_node/execution_layer/src/lib.rs#L1634
        let execution_block_with_txs = self
            .api
            .get_block_by_hash_with_txns::<E>(
                execution_block.block_hash,
                types::ForkName::Deneb,
            )
            .await
            .unwrap()
            .unwrap();

        let transactions_vec: Vec<Transaction<<E as EthSpec>::MaxBytesPerTransaction>> = execution_block_with_txs
          .transactions()
          .iter()
          .map(|transaction| VariableList::new(transaction.rlp().to_vec()).unwrap())
          .collect();

        let transactions: Transactions<E> = VariableList::from(transactions_vec);

        Ok(match execution_block_with_txs {
            ExecutionBlockWithTransactions::Deneb(deneb_block) => {
                let withdrawals = Withdrawals::<E>::new(
                    deneb_block
                        .withdrawals
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                )
                .unwrap();
                ExecutionPayloadDeneb {
                    parent_hash: deneb_block.parent_hash,
                    fee_recipient: deneb_block.fee_recipient,
                    state_root: deneb_block.state_root,
                    receipts_root: deneb_block.receipts_root,
                    logs_bloom: deneb_block.logs_bloom,
                    prev_randao: deneb_block.prev_randao,
                    block_number: deneb_block.block_number,
                    gas_limit: deneb_block.gas_limit,
                    gas_used: deneb_block.gas_used,
                    timestamp: deneb_block.timestamp,
                    extra_data: deneb_block.extra_data,
                    base_fee_per_gas: deneb_block.base_fee_per_gas,
                    block_hash: deneb_block.block_hash,
                    transactions,
                    withdrawals,
                    blob_gas_used: deneb_block.blob_gas_used,
                    excess_blob_gas: deneb_block.excess_blob_gas,
                }
            }
            _ => panic!("Unknown fork"),
        })
    }
}

pub fn new_http_json_rpc(url_override: Option<String>) -> HttpJsonRpc {
    let rpc_auth = Auth::new(JwtKey::from_slice(&DEFAULT_JWT_SECRET).unwrap(), None, None);
    let rpc_url =
        SensitiveUrl::parse(&url_override.unwrap_or(DEFAULT_EXECUTION_ENDPOINT.to_string()))
            .unwrap();
    HttpJsonRpc::new_with_auth(rpc_url, rpc_auth, None).unwrap()
}
