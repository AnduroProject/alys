#![allow(clippy::needless_question_mark)]

use crate::auxpow::AuxPow;
use crate::auxpow_miner::{
    get_next_work_required, BitcoinConsensusParams, BlockIndex, ChainManager,
};
use crate::block::{AuxPowHeader, ConsensusBlock, ConvertBlockHash};
use crate::block_candidate::block_candidate_cache::BlockCandidateCacheTrait;
use crate::block_candidate::BlockCandidates;
use crate::block_hash_cache::{BlockHashCache, BlockHashCacheInit};
use crate::engine::{ConsensusAmount, Engine};
use crate::error::AuxPowMiningError::NoWorkToDo;
use crate::error::BlockErrorBlockTypes;
use crate::error::BlockErrorBlockTypes::Head;
use crate::error::Error::ChainError;
use crate::metrics::{
    CHAIN_BLOCK_HEIGHT, CHAIN_BLOCK_PRODUCTION_TOTALS, CHAIN_BTC_BLOCK_MONITOR_TOTALS,
    CHAIN_DISCOVERED_PEERS, CHAIN_LAST_APPROVED_BLOCK, CHAIN_LAST_PROCESSED_BLOCK,
    CHAIN_NETWORK_GOSSIP_TOTALS, CHAIN_PEGIN_TOTALS, CHAIN_PROCESS_BLOCK_TOTALS,
    CHAIN_SYNCING_OPERATION_TOTALS, CHAIN_TOTAL_PEGIN_AMOUNT,
};
use crate::network::rpc::InboundRequest;
use crate::network::rpc::{RPCCodedResponse, RPCReceived, RPCResponse, ResponseTermination};
use crate::network::PubsubMessage;
use crate::network::{ApproveBlock, Client as NetworkClient, OutboundRequest};
use crate::signatures::CheckedIndividualApproval;
use crate::spec::ChainSpec;
use crate::store::{BlockByHeight, BlockRef};
use crate::{aura::Aura, block::SignedConsensusBlock, error::Error, store::Storage};
use async_trait::async_trait;
use bitcoin::{BlockHash, Transaction as BitcoinTransaction, Txid};
use bridge::SingleMemberTransactionSignatures;
use bridge::{BitcoinSignatureCollector, BitcoinSigner, Bridge, PegInInfo, Tree, UtxoManager};
use ethereum_types::{Address, H256, U64};
use ethers_core::types::{Block, Transaction, TransactionReceipt, U256};
use eyre::{eyre, Report, Result};
use libp2p::PeerId;
use lighthouse_wrapper::execution_layer::Error::MissingLatestValidHash;
use lighthouse_wrapper::store::ItemStore;
use lighthouse_wrapper::store::KeyValueStoreOp;
use lighthouse_wrapper::types::{ExecutionBlockHash, Hash256, MainnetEthSpec};
use std::collections::{BTreeMap, HashSet};
use std::ops::{Add, DerefMut, Div, Mul, Sub};
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};
use svix_ksuid::*;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::RwLock;
use tracing::*;

pub(crate) type BitcoinWallet = UtxoManager<Tree>;

#[derive(Debug)]
enum SyncStatus {
    InProgress,
    Synced,
}

impl SyncStatus {
    fn is_synced(&self) -> bool {
        matches!(self, SyncStatus::Synced)
    }
}

// based on https://github.com/sigp/lighthouse/blob/441fc1691b69f9edc4bbdc6665f3efab16265c9b/beacon_node/beacon_chain/src/beacon_chain.rs#L314
pub struct Chain<DB> {
    engine: Engine,
    network: NetworkClient,
    storage: Storage<MainnetEthSpec, DB>,
    aura: Aura,
    head: RwLock<Option<BlockRef>>,
    sync_status: RwLock<SyncStatus>,
    peers: RwLock<HashSet<PeerId>>,
    block_candidates: BlockCandidates,
    queued_pow: RwLock<Option<AuxPowHeader>>,
    max_blocks_without_pow: u64,
    federation: Vec<Address>,
    bridge: Bridge,
    queued_pegins: RwLock<BTreeMap<Txid, PegInInfo>>,
    bitcoin_wallet: RwLock<BitcoinWallet>,
    bitcoin_signature_collector: RwLock<BitcoinSignatureCollector>,
    maybe_bitcoin_signer: Option<BitcoinSigner>,
    pub retarget_params: BitcoinConsensusParams,
    pub is_validator: bool,
    pub block_hash_cache: Option<RwLock<BlockHashCache>>,
}

const MAINNET_MAX_WITHDRAWALS: usize = 16;

trait TxFees {
    fn gas_tip_cap(&self) -> U256;
    fn gas_fee_cap(&self) -> U256;
    #[allow(dead_code)]
    fn gas_price(&self) -> U256;
    fn effective_gas_tip(&self, base_fee: U256) -> U256;
}

impl TxFees for Transaction {
    fn gas_tip_cap(&self) -> U256 {
        self.max_priority_fee_per_gas
            .unwrap_or(self.gas_price.unwrap())
    }

    fn gas_fee_cap(&self) -> U256 {
        self.max_fee_per_gas.unwrap_or(self.gas_price.unwrap())
    }

    fn gas_price(&self) -> U256 {
        self.gas_fee_cap()
    }

    fn effective_gas_tip(&self, base_fee: U256) -> U256 {
        let gas_fee_cap = self.gas_fee_cap();
        self.gas_tip_cap().min(gas_fee_cap.sub(base_fee))
    }
}

impl<DB: ItemStore<MainnetEthSpec>> Chain<DB> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        engine: Engine,
        network: NetworkClient,
        storage: Storage<MainnetEthSpec, DB>,
        aura: Aura,
        max_blocks_without_pow: u64,
        federation: Vec<Address>,
        bridge: Bridge,
        bitcoin_wallet: BitcoinWallet,
        bitcoin_signature_collector: BitcoinSignatureCollector,
        maybe_bitcoin_signer: Option<BitcoinSigner>,
        retarget_params: BitcoinConsensusParams,
        is_validator: bool,
    ) -> Self {
        let head = storage.get_head().expect("Failed to get head from storage");
        Self {
            engine,
            network,
            storage,
            aura,
            head: RwLock::new(head),
            sync_status: RwLock::new(SyncStatus::Synced), // assume synced, we'll find out if not
            peers: RwLock::new(HashSet::new()),
            block_candidates: BlockCandidates::new(),
            queued_pow: RwLock::new(None),
            max_blocks_without_pow,
            federation,
            bridge,
            queued_pegins: RwLock::new(BTreeMap::new()),
            bitcoin_wallet: RwLock::new(bitcoin_wallet),
            bitcoin_signature_collector: RwLock::new(bitcoin_signature_collector),
            maybe_bitcoin_signer,
            retarget_params,
            is_validator,
            block_hash_cache: Some(RwLock::new(BlockHashCache::new(None))),
        }
    }

    // we collect fees from x to n-1 (where x <= n-1)
    // we are finalizing block n
    fn queued_fees(&self, parent_block_hash: &Hash256) -> Result<U256, Error> {
        let fees = self
            .storage
            .get_accumulated_block_fees(parent_block_hash)?
            .unwrap();
        Ok(fees)
    }

    fn split_fees(&self, fees: U256, miner_address: Address) -> Vec<(Address, ConsensusAmount)> {
        if fees.is_zero() {
            info!("No fees to mint");
            return vec![];
        }

        let miner_fee = fees.mul(80u64).div(100);
        let federation_fee = fees.sub(miner_fee).div(self.federation.len());
        info!("Miner reward: {miner_fee}");
        info!("Federation reward: {federation_fee}");

        let mut add_balances = vec![(miner_address, ConsensusAmount::from_wei(miner_fee))];
        add_balances.extend(
            self.federation
                .iter()
                .map(|address| (*address, ConsensusAmount::from_wei(federation_fee))),
        );
        add_balances
    }

    async fn fill_pegins(
        &self,
        add_balances: &mut Vec<(Address, ConsensusAmount)>,
    ) -> Vec<(Txid, BlockHash)> {
        let _span = tracing::info_span!("fill_pegins").entered();

        let mut withdrawals = BTreeMap::<_, u64>::new();
        let mut processed_pegins = Vec::new();
        let mut total_pegin_amount: u64 = 0;

        // Track initial queue size
        let initial_queue_size = self.queued_pegins.read().await.len();
        debug!(initial_queue_size, "Starting fill_pegins operation");

        {
            // Remove pegins that we already processed. In the happy path, this code
            // shouldn't really do anything. It's added to prevent the block producer
            // from permanently being rejected by other nodes.
            // NOTE: this code takes care to hold only 1 lock at the time, to ensure
            // it can't create any deadlocks

            let mut txids = self
                .queued_pegins
                .read()
                .await
                .keys()
                .copied()
                .collect::<Vec<_>>();

            debug!(total_txids = txids.len(), "Retrieved queued pegin txids");

            {
                let wallet = self.bitcoin_wallet.read().await;
                let initial_txid_count = txids.len();
                txids.retain(|txid| {
                    let exists = wallet.get_tx(txid).unwrap().is_some();
                    trace!("Checking if txid {:?} exists in wallet: {}", txid, exists);
                    exists
                });
                let filtered_count = initial_txid_count - txids.len();
                debug!(
                    initial_count = initial_txid_count,
                    retained_count = txids.len(),
                    filtered_count,
                    "Filtered txids based on wallet existence"
                );
            }

            info!(count = txids.len(), "Already processed peg-ins");

            for already_processed_txid in txids {
                self.queued_pegins
                    .write()
                    .await
                    .remove(&already_processed_txid);
                CHAIN_PEGIN_TOTALS.with_label_values(&["removed"]).inc();
                debug!(txid = %already_processed_txid, "Removed already processed pegin");
            }
        }

        // Process remaining pegins
        let queued_pegins = self.queued_pegins.read().await;
        let total_available_pegins = queued_pegins.len();
        debug!(
            available_pegins = total_available_pegins,
            "Processing available pegins"
        );

        let mut skipped_pegins = 0;
        let mut unique_addresses = std::collections::HashSet::new();

        for pegin in queued_pegins.values() {
            if withdrawals.len() < MAINNET_MAX_WITHDRAWALS
                || withdrawals.contains_key(&pegin.evm_account)
            {
                withdrawals.insert(
                    pegin.evm_account,
                    withdrawals
                        .get(&pegin.evm_account)
                        .cloned()
                        .unwrap_or_default()
                        .add(pegin.amount),
                );
                processed_pegins.push((pegin.txid, pegin.block_hash));
                CHAIN_PEGIN_TOTALS.with_label_values(&["added"]).inc();
                total_pegin_amount += pegin.amount;
                unique_addresses.insert(pegin.evm_account);

                debug!(
                    txid = %pegin.txid,
                    amount = pegin.amount,
                    evm_account = %pegin.evm_account,
                    "Added pegin to processing queue"
                );
            } else {
                skipped_pegins += 1;
                debug!(
                    txid = %pegin.txid,
                    current_withdrawals = withdrawals.len(),
                    max_withdrawals = MAINNET_MAX_WITHDRAWALS,
                    "Skipped pegin due to withdrawal limit"
                );
            }
        }
        drop(queued_pegins);

        let withdrawals: Vec<(Address, u64)> = withdrawals.into_iter().collect();

        info!(
            processed_count = processed_pegins.len(),
            skipped_count = skipped_pegins,
            unique_addresses = unique_addresses.len(),
            total_amount = total_pegin_amount,
            "Completed pegin processing"
        );

        // these are the withdrawals, merge payments to the same EVM address
        add_balances.extend(
            withdrawals
                .iter()
                .map(|(address, amount)| (*address, ConsensusAmount::from_satoshi(*amount))),
        );

        // Update prometheus metrics
        CHAIN_PEGIN_TOTALS
            .with_label_values(&["processed"])
            .inc_by(processed_pegins.len() as u64);
        CHAIN_TOTAL_PEGIN_AMOUNT.set(total_pegin_amount as i64);

        processed_pegins
    }

    async fn check_withdrawals(
        self: &Arc<Self>,
        unverified_block: &SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<(), Error> {
        // compute the expected withdrawals from the fees (miner + federation)
        let mut expected = if let Some(ref header) = unverified_block.message.auxpow_header {
            self.split_fees(
                self.queued_fees(&unverified_block.message.parent_hash)?,
                header.fee_recipient,
            )
            .into_iter()
            .collect::<BTreeMap<_, _>>()
        } else {
            Default::default()
        };

        // add the expected withdrawals for the pegins
        for (txid, block_hash) in &unverified_block.message.pegins {
            if self.bitcoin_wallet.read().await.get_tx(txid)?.is_some() {
                return Err(Error::PegInAlreadyIncluded);
            }
            let info = self
                .bridge
                .get_confirmed_pegin_from_txid(txid, block_hash)?;
            expected.insert(
                info.evm_account,
                expected
                    .get(&info.evm_account)
                    .cloned()
                    .unwrap_or_default()
                    .add(ConsensusAmount::from_satoshi(info.amount)),
            );
        }

        // remove all expected withdrawals
        for withdrawal in &unverified_block.message.execution_payload.withdrawals {
            if expected
                .get(&withdrawal.address)
                .is_some_and(|x| x.eq(&withdrawal.amount))
            {
                expected.remove(&withdrawal.address);
                continue;
            }
        }

        if !expected.is_empty() {
            // block proposer has added unexpected withdrawal
            Err(Error::UnknownWithdrawal)
        } else {
            Ok(())
        }
    }

    pub async fn produce_block(
        self: &Arc<Self>,
        slot: u64,
        timestamp: Duration,
    ) -> Result<(), Error> {
        let ksuid = Ksuid::new(None, None);
        let _span = tracing::info_span!("produce_block", trace_id = %ksuid.to_string()).entered();

        CHAIN_BLOCK_PRODUCTION_TOTALS
            .with_label_values(&["attempted", "default"])
            .inc();
        if !self.sync_status.read().await.is_synced() {
            CHAIN_BLOCK_PRODUCTION_TOTALS
                .with_label_values(&["attempted", "not_synced"])
                .inc();
            info!("Node is not synced, skipping block production.");
            return Ok(());
        }
        let mut prev_height = 0;
        let mut rollback_head = false;

        // TODO: should we set forkchoice here?
        let (prev, prev_payload_head) = {
            let _span = tracing::info_span!("determine_previous_block", slot = slot).entered();

            match *(self.head.read().await) {
                Some(ref x) => {
                    trace!("Head block found: hash={:?}, height={}", x.hash, x.height);

                    let prev = {
                        let _span = tracing::debug_span!("get_previous_block", head_hash = %x.hash)
                            .entered();
                        self.storage
                            .get_block(&x.hash)
                            .map_err(|_| Error::MissingParent)?
                            .ok_or(Error::MissingParent)?
                    };

                    // make sure payload is built on top of the correct block
                    let prev_payload_hash = prev.message.execution_payload.block_hash;
                    prev_height = prev.message.execution_payload.block_number;

                    trace!(
                        "Previous block details: height={}, payload_hash={:?}",
                        prev_height,
                        prev_payload_hash
                    );

                    // make sure that the execution payload is available
                    let prev_payload_body = {
                        let _span = tracing::debug_span!(
                            "check_payload_availability",
                            payload_hash = %prev_payload_hash
                        )
                        .entered();

                        self.engine
                            .api
                            .get_payload_bodies_by_hash_v1::<MainnetEthSpec>(vec![
                                prev_payload_hash,
                            ])
                            .await
                            .map_err(|_| Error::ExecutionLayerError(MissingLatestValidHash))?
                    };

                    if prev_payload_body.is_empty() || prev_payload_body[0].is_none() {
                        warn!(
                            "Payload body not available for hash {:?}, triggering rollback",
                            prev_payload_hash
                        );
                        rollback_head = true;
                        (Hash256::zero(), None)
                    } else {
                        trace!("Payload body available, proceeding with block production");
                        (x.hash, Some(prev_payload_hash))
                    }
                }
                None => {
                    debug!("No head block found, starting from genesis");
                    (Hash256::zero(), None)
                }
            }
        };

        if rollback_head {
            warn!("No payload head found");
            self.rollback_head(prev_height - 1).await?;
            return Ok(());
        }

        let (queued_pow, finalized_pegouts) = match self.queued_pow.read().await.clone() {
            None => (None, vec![]),
            Some(pow) => {
                let signature_collector = self.bitcoin_signature_collector.read().await;
                // TODO: BTC txn caching
                let finalized_txs = self
                    .get_bitcoin_payment_proposals_in_range(pow.range_start, pow.range_end)?
                    .into_iter()
                    .filter_map(|tx| match signature_collector.get_finalized(tx.txid()) {
                        Ok(finalized_tx) => Some(finalized_tx),
                        Err(err) => {
                            warn!("Skipping transaction with txid {}: {}", tx.txid(), err);
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                let finalized_txs: Result<
                    Vec<bitcoin::blockdata::transaction::Transaction>,
                    Error,
                > = Ok(finalized_txs);

                match finalized_txs {
                    Err(err) => {
                        warn!("Failed to use queued PoW - it finalizes blocks with pegouts that have insufficient signatures ({err:?})");
                        (None, vec![])
                    }
                    Ok(txs) => (Some(pow), txs),
                }
            }
        };

        let mut add_balances = if let Some(ref header) = queued_pow {
            self.split_fees(self.queued_fees(&prev)?, header.fee_recipient)
        } else {
            Default::default()
        };
        debug!("Add balances: {:?}", add_balances.len());

        let pegins = self.fill_pegins(&mut add_balances).await;
        debug!("Filled pegins: {:?}", pegins.len());

        let payload_result = self
            .engine
            .build_block(
                timestamp,
                prev_payload_head,
                add_balances.into_iter().map(Into::into).collect(),
            )
            .await;

        let payload = match payload_result {
            Ok(payload) => {
                CHAIN_BLOCK_PRODUCTION_TOTALS
                    .with_label_values(&["blocks_built", "success"])
                    .inc();
                payload
            }
            Err(err) => {
                match err {
                    Error::PayloadIdUnavailable => {
                        warn!(
                            "PayloadIdUnavailable: Slot {}, Timestamp {:?}",
                            slot, timestamp
                        );
                        CHAIN_BLOCK_PRODUCTION_TOTALS
                            .with_label_values(&["blocks_built", "failed"])
                            .inc();
                        // self.clone().sync(None).await;
                        self.clone().sync().await;

                        // we are missing a parent, this is normal if we are syncing
                        return Ok(());
                    }
                    _ => {
                        warn!("Failed to build block payload: {:?}", err);
                        CHAIN_BLOCK_PRODUCTION_TOTALS
                            .with_label_values(&["blocks_built", "failed"])
                            .inc();
                        return Ok(());
                    }
                }
            }
        };

        // generate a unsigned bitcoin tx for pegout requests made in the previous block, if any
        let pegouts = self.create_pegout_payments(prev_payload_head).await;
        if pegouts.is_some() {
            // Increment the pegouts created counter
            CHAIN_BLOCK_PRODUCTION_TOTALS
                .with_label_values(&["pegouts_created", "success"])
                .inc();
            info!("Created pegout payments.");
        }

        if !finalized_pegouts.is_empty() {
            trace!("Finalized pegouts: {:?}", finalized_pegouts[0].input);
        }

        let block = ConsensusBlock::new(
            slot,
            payload.clone(),
            prev,
            queued_pow,
            pegins,
            pegouts,
            finalized_pegouts,
        );

        let signed_block =
            block.sign_block(self.aura.authority.as_ref().expect("Only called by signer"));

        CHAIN_BLOCK_PRODUCTION_TOTALS
            .with_label_values(&["blocks_signed", "success"])
            .inc();

        let root_hash = signed_block.canonical_root();
        info!(
            "⛏️  Proposed block on slot {slot} (block {}) {prev} -> {root_hash}",
            payload.block_number()
        );

        match self.process_block(signed_block.clone()).await {
            Err(Error::MissingRequiredPow) => {
                warn!("Could not produce block - need PoW");
                CHAIN_BLOCK_PRODUCTION_TOTALS
                    .with_label_values(&["process_block", "failed"])
                    .inc();
                // don't consider this fatal
                return Ok(());
            }
            Err(e) => {
                error!("Failed to process block we created ourselves... {e:?}");
                CHAIN_BLOCK_PRODUCTION_TOTALS
                    .with_label_values(&["process_block", "failed"])
                    .inc();
                return Ok(());
            }
            Ok(_) => {}
        }

        if let Err(x) = self.network.publish_block(signed_block.clone()).await {
            info!("Failed to publish block: {x}");
            CHAIN_BLOCK_PRODUCTION_TOTALS
                .with_label_values(&["blocks_published", "failed"])
                .inc();
        } else {
            CHAIN_BLOCK_PRODUCTION_TOTALS
                .with_label_values(&["blocks_published", "success"])
                .inc();
        }

        CHAIN_BLOCK_PRODUCTION_TOTALS
            .with_label_values(&["success", "default"])
            .inc();
        Ok(())
    }

    /// Finds the preceding finalized block for a given block.
    ///
    /// This method traverses backwards from the given block to find the most recent
    /// finalized block (a block with auxpow_header). If the given block itself is
    /// finalized, it returns the block that was finalized by the previous PoW block.
    ///
    /// # Arguments
    /// * `block` - The block to find the preceding finalized block for
    ///
    /// # Returns
    /// * `Ok(Some(block))` - The preceding finalized block if found
    /// * `Ok(None)` - No preceding finalized block found (e.g., genesis block)
    /// * `Err(Error)` - Error occurred during the search
    fn get_preceding_finalized_block(
        &self,
        block: &SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<Option<SignedConsensusBlock<MainnetEthSpec>>, Error> {
        let mut current_block = block.clone();

        // If the current block is finalized (has auxpow_header), we need to find
        // the block that was finalized by the previous PoW block
        if let Some(ref auxpow) = current_block.message.auxpow_header {
            // For genesis block (height 0), there's no preceding finalized block
            if auxpow.height == 0 {
                return Ok(None);
            }

            // The block at auxpow.range_end is the one that was finalized by this PoW
            // We need to find the block that was finalized by the previous PoW
            let finalized_block = self.storage.get_block(&auxpow.range_end)?;
            if let Some(finalized) = finalized_block {
                current_block = finalized;
            } else {
                return Err(Error::MissingBlock);
            }
        }

        // Now traverse backwards to find the most recent finalized block
        loop {
            // Check if current block is finalized
            if let Some(ref auxpow) = current_block.message.auxpow_header {
                // For genesis block, return None as there's no preceding finalized block
                if auxpow.height == 0 {
                    return Ok(None);
                }

                // Return the block that was finalized by this PoW
                let finalized_block = self.storage.get_block(&auxpow.range_end)?;
                if let Some(finalized) = finalized_block {
                    return Ok(Some(finalized));
                } else {
                    return Err(Error::MissingBlock);
                }
            }

            // If not finalized, move to parent
            if current_block.message.parent_hash.is_zero() {
                // Reached genesis block without finding a finalized block
                return Ok(None);
            }

            let parent_block = self.storage.get_block(&current_block.message.parent_hash)?;
            if let Some(parent) = parent_block {
                current_block = parent;
            } else {
                return Err(Error::MissingParent);
            }
        }
    }

    async fn rollback_head(self: &Arc<Self>, target_height: u64) -> Result<(), Error> {
        info!("Starting head rollback to height {}", target_height);

        // Get the block at the target height
        let target_block = {
            let _span =
                tracing::debug_span!("get_target_block", target_height = target_height).entered();

            let block = self
                .storage
                .get_block_by_height(target_height)?
                .ok_or(Error::MissingBlock)?;

            trace!(
                "Retrieved target block: hash={:?}, height={}",
                block.canonical_root(),
                block.message.height()
            );

            block
        };

        // Find the preceding finalized block
        let preceding_finalized = {
            let _span = tracing::debug_span!("find_preceding_finalized_block").entered();

            let finalized = self.get_preceding_finalized_block(&target_block)?;

            match &finalized {
                Some(block) => {
                    trace!(
                        "Found preceding finalized block: hash={:?}, height={}",
                        block.canonical_root(),
                        block.message.height()
                    );
                }
                None => {
                    debug!("No preceding finalized block found, will use target block");
                }
            }

            finalized
        };

        let rollback_block = if let Some(finalized) = preceding_finalized {
            // Use the preceding finalized block if found
            trace!("Using preceding finalized block for rollback");
            finalized
        } else {
            // Fall back to the target block if no preceding finalized block exists
            trace!("Using target block for rollback (no preceding finalized block)");
            target_block
        };

        let rollback_hash = rollback_block.canonical_root();
        let rollback_height = rollback_block.message.height();
        
        let update_head_ref_ops = {
            let _span = tracing::debug_span!(
                "update_head_ref_ops",
                rollback_hash = %rollback_hash,
                rollback_height = rollback_height
            )
            .entered();
            
            // Drop the span before the await
            drop(_span);
            
            self.update_head_ref_ops(
                rollback_hash,
                rollback_height,
                true,
            )
            .await
        };

        trace!(
            "Rolling back head to {:?} (preceding finalized block)",
            rollback_block.message.height()
        );

        {
            let _span = tracing::debug_span!("commit_rollback_ops").entered();
            self.storage.commit_ops(update_head_ref_ops)?;
        }

        info!(
            "Successfully rolled back head to height {} (hash: {:?})",
            rollback_block.message.height(),
            rollback_block.canonical_root()
        );

        Ok(())
    }

    async fn create_pegout_payments(
        &self,
        payload_hash: Option<ExecutionBlockHash>,
    ) -> Option<BitcoinTransaction> {
        let (_execution_block, execution_receipts) =
            self.get_block_and_receipts(&payload_hash?).await.unwrap();

        let fee_rate = self.bridge.fee_rate();
        match Bridge::filter_pegouts(execution_receipts) {
            x if x.is_empty() => {
                info!("Adding 0 pegouts to block");
                None
            }
            payments => {
                info!("⬅️  Creating bitcoin tx for {} peg-outs", payments.len());
                match self
                    .bitcoin_wallet
                    .write()
                    .await
                    .create_payment(payments, fee_rate)
                {
                    Ok(unsigned_txn) => Some(unsigned_txn),
                    Err(e) => {
                        error!("Failed to create pegout payment: {e}");
                        None
                    }
                }
            }
        }
    }

    fn get_parent(
        &self,
        unverified_block: &SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<SignedConsensusBlock<MainnetEthSpec>, Error> {
        self.storage
            .get_block(&unverified_block.message.parent_hash)
            .map_err(|_| Error::MissingParent)?
            .ok_or(Error::MissingParent)
    }

    #[tracing::instrument(name = "process_block", skip_all, fields(height = unverified_block.message.execution_payload.block_number
	))]
    async fn process_block(
        self: &Arc<Self>,
        unverified_block: SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<Option<CheckedIndividualApproval>, Error> {
        CHAIN_PROCESS_BLOCK_TOTALS
            .with_label_values(&["attempted", "default"])
            .inc();
        CHAIN_LAST_PROCESSED_BLOCK
            .set(unverified_block.message.execution_payload.block_number as i64);

        let root_hash = unverified_block.canonical_root();
        info!(
            "Processing block at height {}",
            unverified_block.message.execution_payload.block_number
        );

        // TODO: check that EL approved of the payload, ideally without
        // actually already importing the block

        if unverified_block.message.parent_hash.is_zero() {
            // no need to process genesis
            CHAIN_PROCESS_BLOCK_TOTALS
                .with_label_values(&["rejected", "genesis_not_needed"])
                .inc();
            return Err(Error::ProcessGenesis);
        }

        if self
            .head
            .read()
            .await
            .as_ref()
            .is_some_and(|x| x.height >= unverified_block.message.execution_payload.block_number)
        {
            // TODO: Better handling for this specific case considering it could be considered a soft error
            // ignore proposals at old heights, this can happen when a new
            // node joins the network but has not yet synced the chain
            // also when another node proposes at the same height
            warn!("Rejecting old block");
            CHAIN_PROCESS_BLOCK_TOTALS
                .with_label_values(&["rejected", "old_height"])
                .inc();
            return Err(Error::InvalidBlock);
        }

        let prev = self.get_parent(&unverified_block)?;
        let prev_payload_hash_according_to_consensus = prev.message.execution_payload.block_hash;
        let prev_payload_hash = unverified_block.message.execution_payload.parent_hash;

        // unverified_block.prev().payload must match unverified_block.payload.prev()
        if prev_payload_hash != prev_payload_hash_according_to_consensus {
            error!("EL chain not contiguous");

            error!(
                "payload new: height {} hash {}",
                unverified_block.message.execution_payload.block_hash,
                unverified_block.message.execution_payload.block_number
            );
            error!(
                "payload.prev.hash: {}",
                unverified_block.message.execution_payload.parent_hash
            );
            error!(
                "block.prev.payload height {} hash {}",
                prev.message.execution_payload.block_hash,
                prev.message.execution_payload.block_number
            );
            CHAIN_PROCESS_BLOCK_TOTALS
                .with_label_values(&["rejected", "chain_incontiguous"])
                .inc();

            return Err(Error::ExecutionHashChainIncontiguous);
        }

        self.aura.check_signed_by_author(&unverified_block)?;

        if self.is_validator {
            self.check_withdrawals(&unverified_block).await?;

            self.check_pegout_proposal(&unverified_block, prev_payload_hash)
                .await?;
        }
        trace!("Made it past withdrawals and pegouts");

        // TODO: We should set the bitcoin connection to be optional
        if let Some(ref pow) = unverified_block.message.auxpow_header {
            // NOTE: Should be removed after chain deprecation
            let mut pow_override = false;

            // TODO: Historical Context
            if unverified_block.message.execution_payload.block_number <= 533683 {
                pow_override = true;
            }
            self.check_pow(pow, pow_override).await?;

            // also check the finalized pegouts
            let required_finalizations = self
                .get_bitcoin_payment_proposals_in_range(pow.range_start, pow.range_end)?
                .into_iter()
                .map(|tx| tx.txid())
                .collect::<Vec<_>>();

            trace!("{} to finalize", required_finalizations.len());

            if required_finalizations.len() != unverified_block.message.finalized_pegouts.len() {
                return Err(Error::IllegalFinalization);
            }

            if self.is_validator {
                for (expected_txid, tx) in required_finalizations
                    .into_iter()
                    .zip(unverified_block.message.finalized_pegouts.iter())
                {
                    if tx.txid() != expected_txid {
                        CHAIN_PROCESS_BLOCK_TOTALS
                            .with_label_values(&["rejected", "invalid_finalization"])
                            .inc();
                        return Err(Error::IllegalFinalization);
                    }

                    let wallet = self.bitcoin_wallet.read().await;
                    trace!("Checking signature for finalized pegout {:?}", tx.txid());
                    // NOTE: same as auxpow_override
                    wallet.check_transaction_signatures(tx, pow_override)?;
                }
            }
        } else {
            trace!("Block does not have PoW");
            // make sure we can only produce a limited number of blocks without PoW
            let latest_finalized_height = self
                .get_latest_finalized_block_ref()?
                .map(|x| x.height)
                .unwrap_or_default();

            let block_height = unverified_block.message.execution_payload.block_number;

            if block_height.saturating_sub(latest_finalized_height) > self.max_blocks_without_pow {
                CHAIN_PROCESS_BLOCK_TOTALS
                    .with_label_values(&["rejected", "missing_pow"])
                    .inc();
                return Err(Error::MissingRequiredPow);
            }

            if !unverified_block.message.finalized_pegouts.is_empty() {
                CHAIN_PROCESS_BLOCK_TOTALS
                    .with_label_values(&["rejected", "invalid_finalization"])
                    .inc();
                return Err(Error::IllegalFinalization);
            }
        }
        let sync_status = self.sync_status.read().await.is_synced();
        trace!("Sync status: {:?}", sync_status);

        // store the candidate
        // TODO: this is also called on sync which isn't strictly required
        let our_approval = if let Some(authority) = &self.aura.authority {
            let our_approval = unverified_block.message.sign(authority);

            // First insert the block
            self.block_candidates
                .insert(unverified_block.clone(), sync_status)
                .await?;

            // Then add our approval
            let approval = ApproveBlock {
                block_hash: root_hash,
                signature: our_approval.clone().into(),
            };
            self.block_candidates
                .add_approval(approval, &self.aura.authorities, sync_status)
                .await?;

            Some(our_approval)
        } else {
            trace!("Full node doesn't need to approve");
            // full node doesn't need to approve
            self.block_candidates
                .insert(unverified_block.clone(), sync_status)
                .await?;
            None
        };

        self.maybe_accept_block(root_hash).await?;

        CHAIN_PROCESS_BLOCK_TOTALS
            .with_label_values(&["success", "default"])
            .inc();

        Ok(our_approval)
    }

    async fn check_pegout_proposal(
        &self,
        unverified_block: &SignedConsensusBlock<MainnetEthSpec>,
        prev_payload_hash: ExecutionBlockHash,
    ) -> Result<(), Error> {
        let (_execution_block, execution_receipts) =
            self.get_block_and_receipts(&prev_payload_hash).await?;

        let required_outputs = Bridge::filter_pegouts(execution_receipts);

        trace!(
            "Found {} pegouts in block after filtering",
            required_outputs.len()
        );

        self.bitcoin_wallet.read().await.check_payment_proposal(
            required_outputs,
            unverified_block.message.pegout_payment_proposal.as_ref(),
        )?;

        trace!("Pegout proposal is valid");
        Ok(())
    }

    fn get_bitcoin_payment_proposals_in_range(
        &self,
        from: Hash256, // inclusive
        to: Hash256,   // inclusive
    ) -> Result<Vec<BitcoinTransaction>, Error> {
        let mut current = to;
        let mut ret = vec![];
        loop {
            let block = match self.storage.get_block(&current) {
                Ok(Some(block)) => block.message,
                Ok(None) => {
                    error!("Failed to get block {:?}", current);
                    return Err(Error::InvalidBlockRange);
                }
                Err(e) => {
                    return Err(Error::GenericError(Report::from(e)));
                }
            };

            if let Some(proposal) = block.pegout_payment_proposal {
                ret.push(proposal);
            }

            if current == from {
                break;
            }
            current = block.parent_hash;
        }
        ret.reverse();
        Ok(ret)
    }

    /// Retrieves a sequence of block hashes from the blockchain between two specified blocks.
    ///
    /// This method iterates backwards from the chain head (`to`) towards the last finalized block (`from`),
    /// collecting block hashes along the way. The method traverses the chain by following parent_hash
    /// references from each block.
    ///
    /// # Parameters
    /// - `from`: The hash of the last finalized block (exclusive) - should have smaller block height than `to`
    /// - `to`: The hash of the chain head (inclusive) - should have larger block height than `from`
    ///
    /// # Returns
    /// A vector of block hashes in chronological order (oldest to newest), where:
    /// - The first element is the block immediately after `from`
    /// - The last element is the chain head (`to`)
    ///
    /// # Example
    /// If we have blocks 100 (finalized) and 105 (head):
    /// - `from` = block 100 hash (exclusive)
    /// - `to` = block 105 hash (inclusive)
    ///
    /// The method will:
    /// 1. Start at block 105 (head) and push its hash to the array
    /// 2. Get block 105's parent (block 104) and push its hash
    /// 3. Get block 104's parent (block 103) and push its hash
    /// 4. Get block 103's parent (block 102) and push its hash
    /// 5. Get block 102's parent (block 101) and push its hash
    /// 6. Stop when reaching block 100 (from parameter)
    ///
    /// After reversal, the returned array will be: [block101, block102, block103, block104, block105]
    ///
    /// # Note
    /// The vector is reversed at the end because we collect hashes in reverse chronological order
    /// (newest to oldest) during iteration, but need to return them in chronological order
    /// (oldest to newest) for proper processing.
    fn get_hashes(
        &self,
        from: Hash256, // exclusive
        to: Hash256,   // inclusive
    ) -> Result<Vec<Hash256>, Error> {
        trace!("Getting hashes from {:?} to {:?}", from, to);
        let mut current = to;
        let mut hashes = vec![];

        // Query block inputs to assert the range is valid
        let from_block = self.storage.get_block(&from)?.ok_or(Error::MissingBlock)?;
        let to_block = self.storage.get_block(&to)?.ok_or(Error::MissingBlock)?;

        // Assert that execution block number for `from` is smaller than `to`
        if from_block.message.execution_payload.block_number
            >= to_block.message.execution_payload.block_number
        {
            return Ok(vec![from]);
        }

        loop {
            if current == from {
                break;
            }

            hashes.push(current);

            match self.storage.get_block(&current) {
                Ok(Some(block)) => {
                    current = block.message.parent_hash;
                }
                Ok(None) => {
                    error!("Failed to get block {:?}", current);
                    return Err(Error::InvalidBlockRange);
                }
                Err(e) => {
                    return Err(Error::GenericError(Report::from(e)));
                }
            }
        }
        hashes.reverse();

        Ok(hashes)
    }

    async fn queue_pow(&self, pow: AuxPowHeader) {
        info!("Queued valid pow");
        *self.queued_pow.write().await = Some(pow.clone());
        self.maybe_generate_signatures(&pow).await.unwrap();
    }

    pub async fn share_pow(&self, pow: AuxPowHeader) -> Result<(), Error> {
        info!("Sending pow for {}..{}", pow.range_start, pow.range_end);
        let _ = self
            .network
            .send(PubsubMessage::QueuePow(pow.clone()))
            .await;
        self.queue_pow(pow).await;
        Ok(())
    }

    async fn check_pow(&self, header: &AuxPowHeader, pow_overide: bool) -> Result<(), Error> {
        info!(
            "Checking AuxPow: {} -> {}",
            header.range_start, header.range_end,
        );

        let last_pow_block = self.storage.get_latest_pow_block()?.unwrap();
        let last_pow = last_pow_block.hash;

        info!("Last pow block: {}", last_pow);

        let last_pow_block = self
            .storage
            .get_block(&last_pow)?
            .ok_or(Error::MissingBlock)?;

        info!("Last pow block {}", last_pow_block.canonical_root());

        let last_finalized = self
            .get_latest_finalized_block_ref()?
            .ok_or(Error::MissingBlock)?;

        info!(
            "Last finalized {} (height {}) in block {}",
            last_finalized.hash, last_finalized.height, last_pow,
        );
        let range_start_block =
            self.storage
                .get_block(&header.range_start)?
                .ok_or(Error::GenericError(eyre!(
                    "Failed to get block: {}",
                    &header.range_start
                )))?;

        // TODO: Historical Context
        if range_start_block.message.height() > 53143
            && range_start_block.message.parent_hash != last_finalized.hash
        {
            debug!(
                "last_finalized.hash: {:?}\n{}",
                last_finalized.hash, last_finalized.height
            );
            debug!(
                "range_start_block.message.parent_hash: {:?}\n{}",
                range_start_block.message.parent_hash,
                range_start_block.message.height()
            );
            warn!("AuxPow check failed - last finalized = {}, attempted to finalize {} while its parent is {}",
                    last_finalized.hash, header.range_start, range_start_block.message.parent_hash);
            return Err(Error::InvalidPowRange);
        }

        let hashes = self.get_hashes(range_start_block.message.parent_hash, header.range_end)?;

        let hash = AuxPow::aggregate_hash(
            &hashes
                .into_iter()
                .map(|hash| hash.to_block_hash())
                .collect::<Vec<_>>(),
        );

        let head_height = self.get_head()?.message.height();
        let bits = get_next_work_required(
            &self
                .get_block_by_hash(&last_pow.to_block_hash())
                .map_err(Error::GenericError)?,
            &self.retarget_params,
            head_height,
        )
        .map_err(Error::GenericError)?;

        // TODO: ignore if genesis
        let auxpow = header.auxpow.as_ref().unwrap();
        // NOTE: We might want to consider lockings these to a struct & predefine the expected values until they are moved into the DB
        if pow_overide || auxpow.check_proof_of_work(bits) {
            auxpow.check(hash, header.chain_id).unwrap();
            info!("AuxPow valid");
            Ok(())
        } else {
            Err(Error::InvalidPow)
        }
    }

    async fn maybe_generate_signatures(&self, pow: &AuxPowHeader) -> Result<(), Error> {
        let bitcoin_signer = if let Some(bitcoin_signer) = &self.maybe_bitcoin_signer {
            bitcoin_signer
        } else {
            // full-node doesn't sign
            return Ok(());
        };
        let wallet = self.bitcoin_wallet.read().await;
        let signatures = self
            .get_bitcoin_payment_proposals_in_range(pow.range_start, pow.range_end)?
            .into_iter()
            .map(|tx| {
                bitcoin_signer
                    .get_input_signatures(&wallet, &tx)
                    .map(|sig| (tx.txid(), sig))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

        trace!("Generated {} signature(s)", signatures.len());
        for (txid, sig) in signatures.iter() {
            trace!("Signature for txid {:?}: {:?}", txid, sig);
        }

        drop(wallet);

        self.store_signatures(signatures.clone()).await.unwrap();

        let _ = self
            .network
            .send(PubsubMessage::PegoutSignatures(signatures))
            .await;

        Ok(())
    }

    fn get_latest_finalized_block_ref(&self) -> Result<Option<BlockRef>, Error> {
        match self.storage.get_latest_pow_block()? {
            Some(blockref) => {
                let pow_block = match self.storage.get_block(&blockref.hash) {
                    Ok(Some(block)) => block,
                    Ok(None) => {
                        error!("Failed to get latest pow block {:?}", blockref.height);
                        error!("Failed to get latest pow block {:?}", blockref.hash);
                        return Err(Error::InvalidBlockRange);
                    }
                    Err(e) => {
                        error!("Failed to get latest pow block {:?}", blockref.hash);
                        error!("Failed to get latest pow block {:?}", blockref.height);
                        return Err(Error::GenericError(Report::from(e)));
                    }
                };

                let pow = match pow_block.message.auxpow_header {
                    Some(pow) => pow,
                    None => {
                        error!("Failed to get auxpow header {:?}", blockref.height);
                        return Err(Error::InvalidBlockRange);
                    }
                };

                // Conditional to check if the block is the genesis block
                let last_finalized_blockref = if pow.height != 0 {
                    match self.storage.get_block(&pow.range_end) {
                        Ok(Some(block)) => block.block_ref(),
                        Ok(None) => {
                            error!(
                                "Failed to get last block in prev-aux range {:?}",
                                blockref.height
                            );
                            error!(
                                "Failed to get last block in prev-aux range {:?}",
                                blockref.hash
                            );
                            return Err(Error::InvalidBlockRange);
                        }
                        Err(e) => {
                            error!(
                                "Failed to get last block in prev-aux range {:?}",
                                blockref.height
                            );
                            error!(
                                "Failed to get last block in prev-aux range {:?}",
                                blockref.hash
                            );
                            return Err(Error::GenericError(Report::from(e)));
                        }
                    }
                } else {
                    blockref
                };
                Ok(Some(last_finalized_blockref))
            }
            None => Ok(None),
        }
    }

    async fn process_approval(self: &Arc<Self>, approval: ApproveBlock) -> Result<(), Error> {
        let hash = approval.block_hash;

        let sync_status = self.sync_status.read().await.is_synced();

        // Add the approval to the cache
        self.block_candidates
            .add_approval(approval, &self.aura.authorities, sync_status)
            .await?;

        // Process the block if it has reached majority approval
        self.maybe_accept_block(hash).await
    }

    async fn maybe_accept_block(self: &Arc<Self>, hash: Hash256) -> Result<(), Error> {
        // Get the block from the cache
        let block_opt = self.block_candidates.get_block(&hash).await;

        if let Some(block) = block_opt {
            // Check if the block has reached majority approval
            if self.aura.majority_approved(&block)? {
                info!("🤝 Block {hash} has reached majority approval");

                // Clear the cache to free up memory
                self.block_candidates.clear().await;

                // Import the verified block
                self.import_verified_block(block).await?;
            } else {
                debug!("Block {hash} has not reached majority approval");
                // nothing to do
            }
        } else {
            debug!("Block {hash} not found in cache");
            return Err(Error::CandidateCacheError);
        }

        Ok(())
    }

    pub async fn store_genesis(self: &Arc<Self>, chain_spec: ChainSpec) -> Result<(), Error> {
        let execution_payload = self
            .engine
            .get_payload_by_tag_from_engine(
                lighthouse_wrapper::execution_layer::BlockByNumberQuery::Tag("0x0"),
            )
            .await
            .expect("Should have genesis");

        let genesis_block = SignedConsensusBlock::genesis(chain_spec, execution_payload);

        if self
            .storage
            .get_block(&genesis_block.canonical_root())?
            .is_some()
        {
            info!("Not storing genesis block");
            return Ok(());
        }

        info!("Storing genesis block");
        self.import_verified_block_no_commit(genesis_block).await
    }

    async fn get_block_and_receipts(
        &self,
        block_hash: &ExecutionBlockHash,
    ) -> Result<(Block<Transaction>, Vec<TransactionReceipt>), Error> {
        let block_with_txs = match self.engine.get_block_with_txs(block_hash).await {
            Ok(block_option) => match block_option {
                Some(block) => {
                    trace!(
                        "Block found - Hash: {:x} Number: {}",
                        block.hash.unwrap_or(H256::zero()),
                        block.number.unwrap_or(U64::from(0))
                    );
                    block
                }
                None => {
                    return Err(Error::MissingBlock);
                }
            },
            Err(err) => return Err(Error::ExecutionLayerError(err)),
        };

        let mut receipt_result = Vec::new();
        for tx in block_with_txs.transactions.iter() {
            let receipt = self.engine.get_transaction_receipt(tx.hash).await;
            match receipt {
                Ok(receipt_opt) => {
                    if let Some(receipt) = receipt_opt {
                        trace!(
                            "Receipt found - Hash: {:x} Block Hash: {:x}",
                            tx.hash,
                            block_with_txs.hash.unwrap_or(H256::zero())
                        );
                        receipt_result.push(receipt);
                    }
                }
                Err(err) => {
                    trace!(
                        "Receipt not found - Hash: {:x} Block Hash: {:x} - Error: {:?}",
                        tx.hash,
                        block_with_txs.hash.unwrap_or(H256::zero()),
                        err
                    );
                }
            }
        }

        // let receipt_result = try_join_all(
        //     block_with_txs
        //         .transactions
        //         .iter()
        //         .map(|tx| self.engine.get_transaction_receipt(tx.hash)),
        // )
        // .await;
        Ok((
            block_with_txs,
            receipt_result.into_iter().collect(),
            // receipts.into_iter().map(|x| x.unwrap()).collect(),
        ))
        // match Ok(receipt_result) {
        //     Ok(receipts) => Ok((
        //         block_with_txs,
        //         receipts.into_iter().map(|x| x).collect(),
        //         // receipts.into_iter().map(|x| x.unwrap()).collect(),

        //     )),
        //     Err(err) => {
        //         error!(
        //             "Error retrieving block txn receipts for block hash: {:x} #: {}",
        //             block_with_txs.hash.unwrap_or(H256::zero()),
        //             block_with_txs.number.unwrap_or(U64::from(0))
        //         );
        //         Err(Error::ExecutionLayerError(err))
        //     }
        // }
    }

    //  ____________      __________________      ____________
    // | n-2        | <- | n-1              | <- | n (AuxPow) |
    // | fees = n-2 |    | fees = n-2 + n-1 |    | fees = n   |
    //  ‾‾‾‾‾‾‾‾‾‾‾‾      ‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾      ‾‾‾‾‾‾‾‾‾‾‾‾
    async fn accumulate_fees(
        self: &Arc<Self>,
        verified_block: &SignedConsensusBlock<MainnetEthSpec>,
        execution_block: Block<Transaction>,
        execution_receipts: &Vec<TransactionReceipt>,
    ) -> Result<Vec<KeyValueStoreOp>, Error> {
        // https://github.com/ethereum/go-ethereum/blob/f55a10b64d511b27beb02ff4978a6ed66d604cd8/miner/worker.go#L1192
        fn total_fees(block: Block<Transaction>, receipts: &Vec<TransactionReceipt>) -> U256 {
            let mut fees_wei = U256::zero();
            for (tx, receipt) in block.transactions.iter().zip(receipts) {
                let miner_fee = tx.effective_gas_tip(block.base_fee_per_gas.unwrap());
                fees_wei += receipt.gas_used.unwrap() * miner_fee;
            }
            fees_wei
        }

        let mut fees = if verified_block.message.auxpow_header.is_some() {
            // the current AuxPow block collects fees from x to n-1
            // where x is the last AuxPow block we collected fees for
            // so we initialize the accumulator to zero
            Default::default()
        } else {
            // initialize the accumulator to the total at n-1
            let accumulated_fees = self
                .storage
                .get_accumulated_block_fees(&verified_block.message.parent_hash)?
                .unwrap_or_default();

            trace!("Accumulated fees: {}", accumulated_fees);
            accumulated_fees
        };

        // add the fees for block n
        let block_fees = total_fees(execution_block, execution_receipts);
        info!("💰 Collecting {} fees from block", block_fees);
        fees += block_fees;

        Ok(self
            .storage
            .set_accumulated_block_fees(&verified_block.canonical_root(), fees))
    }

    async fn update_head_ref_ops(
        self: &Arc<Self>,
        new_head_canonical_root: H256,
        new_head_height: u64,
        rollback_override: bool,
    ) -> Vec<KeyValueStoreOp> {
        match self.head.write().await.deref_mut() {
            Some(x) if x.height > new_head_height && !rollback_override => {
                trace!("Rollback not allowed");
                // don't update - no db ops
                vec![]
            }
            x => {
                let new_head = BlockRef {
                    hash: new_head_canonical_root,
                    height: new_head_height,
                };
                *x = Some(new_head.clone());
                self.storage.set_head(&new_head)
            }
        }
    }

    async fn import_verified_block_no_commit(
        self: &Arc<Self>,
        verified_block: SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<(), Error> {
        let block_root = verified_block.canonical_root();
        let payload_hash = verified_block.message.execution_payload.block_hash;
        let payload_prev_hash = verified_block.message.execution_payload.parent_hash;

        info!(
            "🔗 Importing block at height {} from parent {} -> {}",
            verified_block.message.execution_payload.block_number,
            verified_block.message.parent_hash,
            block_root
        );
        info!("Corresponding payload: {payload_prev_hash} -> {payload_hash}");

        // we use these to track fees and handle pegouts
        let (execution_block, execution_receipts) = self
            .get_block_and_receipts(&verified_block.message.execution_payload.block_hash)
            .await?;
        // NOTE: GetPayloadResponse has `block_value` but we cannot determine this
        // on import so there is no way to verify that value is correct
        let accumulate_fees_ops = self
            .accumulate_fees(&verified_block, execution_block, &execution_receipts)
            .await?;
        if self.is_validator {
            // process pegins:
            for (txid, block_hash) in verified_block.message.pegins.iter() {
                info!("➡️  Processed peg-in with txid {txid}");
                self.queued_pegins.write().await.remove(txid);

                // Make the bitcoin utxos available for spending
                let tx = self.bridge.fetch_transaction(txid, block_hash).unwrap();
                self.bitcoin_wallet
                    .write()
                    .await
                    .register_pegin(&tx)
                    .unwrap();
            }

            trace!(
                "Processing {} pegouts",
                verified_block.message.finalized_pegouts.len()
            );
            // process peg-out proposals:
            if let Some(ref pegout_tx) = verified_block.message.pegout_payment_proposal {
                trace!("⬅️ Registered peg-out proposal");
                self.bitcoin_wallet
                    .write()
                    .await
                    .register_pegout(pegout_tx)
                    .unwrap();
            }

            // process finalized peg-outs:
            for tx in verified_block.message.finalized_pegouts.iter() {
                let txid = tx.txid();
                match self.bridge.broadcast_signed_tx(tx) {
                    Ok(txid) => {
                        info!("⬅️  Broadcasted peg-out, txid {txid}");
                    }
                    Err(_) => {
                        warn!("⬅️  Failed to process peg-out, txid {}", tx.txid());
                    }
                };
                self.bitcoin_signature_collector
                    .write()
                    .await
                    .cleanup_signatures_for(&txid);
            }
        }

        // store block in DB
        let put_block_ops = self.storage.put_block(&block_root, verified_block.clone());

        let set_head_ops: Vec<KeyValueStoreOp> = self
            .update_head_ref_ops(
                block_root,
                verified_block.message.execution_payload.block_number,
                false,
            )
            .await;

        let finalization_ops = if let Some(ref pow) = verified_block.message.auxpow_header {
            self.finalize(&verified_block, block_root, pow).await?
        } else {
            vec![]
        };

        let all_ops = [
            accumulate_fees_ops,
            put_block_ops,
            set_head_ops,
            finalization_ops,
        ]
        .into_iter()
        .flatten();
        self.storage.commit_ops(all_ops.collect())?;

        // Ignore if genesis block
        if verified_block.message.height() != 0 {
            if let Some(block_hash_cache) = self.block_hash_cache.as_ref() {
                // Check to see if we have a PoW block
                if let Some(ref aux_header) = verified_block.message.auxpow_header {
                    // Since we are using a PoW block, we need to flush the block hash cache
                    // and store the latest block hash
                    let mut block_hash_cache = block_hash_cache.write().await;

                    // Reset the block hash cache to the hash after the range end
                    block_hash_cache
                        .reset_with(aux_header.range_end.to_block_hash())
                        .map_err(Error::GenericError)?;
                    block_hash_cache.add(verified_block.canonical_root().to_block_hash());
                } else {
                    // Insert the block hash to the block hash cache
                    block_hash_cache
                        .write()
                        .await
                        .add(verified_block.canonical_root().to_block_hash());
                }
            }
        }
        Ok(())
    }

    async fn import_verified_block(
        self: &Arc<Self>,
        verified_block: SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<(), Error> {
        self.engine
            .commit_block(verified_block.message.execution_payload.clone().into())
            .await?;

        self.import_verified_block_no_commit(verified_block).await
    }

    async fn finalize(
        self: &Arc<Self>,
        block: &SignedConsensusBlock<MainnetEthSpec>,
        block_root: Hash256,
        pow: &AuxPowHeader,
    ) -> Result<Vec<KeyValueStoreOp>, Error> {
        info!("Finalizing up to block {}", pow.range_end);

        *self.queued_pow.write().await = None;

        // don't finalize EL for genesis
        if !pow.range_end.is_zero() {
            info!("Finalizing payload");

            let finalized_block = self.storage.get_block(&pow.range_end)?.unwrap();
            self.engine
                .set_finalized(finalized_block.message.execution_payload.block_hash)
                .await;
        } else {
            info!("Not finalizing payload for genesis");
        }

        Ok(self.storage.set_latest_pow_block(&BlockRef {
            hash: block_root,
            height: block.message.execution_payload.block_number,
        }))
    }

    async fn store_signatures(
        &self,
        pegout_sigs: HashMap<Txid, SingleMemberTransactionSignatures>,
    ) -> Result<(), Error> {
        let mut collector = self.bitcoin_signature_collector.write().await;
        let wallet = self.bitcoin_wallet.read().await;
        for (txid, sigs) in pegout_sigs {
            collector.add_signature(&wallet, txid, sigs.clone())?;
            trace!(
                "Successfully added signature {:?} for txid {:?}",
                sigs,
                txid
            );
        }
        Ok(())
    }

    pub async fn monitor_gossip(self: Arc<Self>) {
        let mut listener = self.network.subscribe_events().await.unwrap();
        let chain = self.clone();
        tokio::spawn(async move {
            loop {
                let msg = match listener.recv().await {
                    Err(RecvError::Lagged(x)) => {
                        warn!("Missed {x} network messages");
                        CHAIN_NETWORK_GOSSIP_TOTALS
                            .with_label_values(&["msg_received", "error"])
                            .inc_by(x);
                        continue;
                    }
                    Err(_) => panic!("failed to read network stream"),
                    Ok(x) => {
                        CHAIN_NETWORK_GOSSIP_TOTALS
                            .with_label_values(&["msg_received", "success"])
                            .inc();
                        x
                    }
                };
                match msg {
                    PubsubMessage::ConsensusBlock(x) => {
                        CHAIN_NETWORK_GOSSIP_TOTALS
                            .with_label_values(&["consensus_block", "received"])
                            .inc();

                        let number = x.message.execution_payload.block_number;
                        let payload_hash = x.message.execution_payload.block_hash;
                        let payload_prev_hash = x.message.execution_payload.parent_hash;

                        info!("Received payload at height {number} {payload_prev_hash} -> {payload_hash}");
                        let head_hash = self.head.read().await.as_ref().unwrap().hash;
                        let head_height = self.head.read().await.as_ref().unwrap().height;
                        debug!("Local head: {:#?}, height: {}", head_hash, head_height);

                        // sync first then process block so we don't skip and trigger a re-sync
                        if matches!(self.get_parent(&x), Err(Error::MissingParent)) {
                            // TODO: we need to sync before processing (this is triggered by proposal)
                            // TODO: additional case needed where head height is not behind
                            // self.clone().sync(Some((number - head_height) as u32)).await;
                            self.clone().sync().await;
                        }

                        match chain.process_block(x.clone()).await {
                            Err(x) => match x {
                                Error::MissingParent => {
                                    // self.clone().sync(Some((number - head_height) as u32)).await;
                                    self.clone().sync().await;
                                }
                                Error::MissingBlock => {
                                    self.clone().sync().await;
                                }
                                _ => {
                                    error!("Got error while processing: {x:?}");
                                }
                            },
                            Ok(Some(our_approval)) => {
                                CHAIN_LAST_APPROVED_BLOCK
                                    .set(x.message.execution_payload.block_number as i64);

                                // broadcast our approval
                                let block_hash = x.canonical_root();
                                info!("✅ Sending approval for {block_hash}");
                                let _ = self
                                    .network
                                    .send(PubsubMessage::ApproveBlock(ApproveBlock {
                                        block_hash,
                                        signature: our_approval.into(),
                                    }))
                                    .await;
                            }
                            Ok(None) => {}
                        }
                    }
                    PubsubMessage::ApproveBlock(approval) => {
                        if self.sync_status.read().await.is_synced() {
                            info!("✅ Received approval for block {}", approval.block_hash);
                            CHAIN_NETWORK_GOSSIP_TOTALS
                                .with_label_values(&["approve_block", "received"])
                                .inc();
                            match self.process_approval(approval).await {
                                Err(err) => {
                                    warn!("Error processing approval: {err:?}");
                                }
                                Ok(()) => {
                                    // nothing to do
                                }
                            };
                        }
                    }
                    PubsubMessage::QueuePow(pow) => match self
                        .check_pow(&pow, false)
                        .instrument(info_span!("queued"))
                        .await
                    {
                        Err(err) => {
                            warn!("Received invalid pow: {err:?}");
                            CHAIN_NETWORK_GOSSIP_TOTALS
                                .with_label_values(&["queue_pow", "error"])
                                .inc();
                        }
                        Ok(()) => {
                            self.queue_pow(pow.clone()).await;
                            CHAIN_NETWORK_GOSSIP_TOTALS
                                .with_label_values(&["queue_pow", "success"])
                                .inc();
                        }
                    },
                    PubsubMessage::PegoutSignatures(pegout_sigs) => {
                        CHAIN_NETWORK_GOSSIP_TOTALS
                            .with_label_values(&["pegout_sigs", "success"])
                            .inc();

                        if let Err(err) = self.store_signatures(pegout_sigs).await {
                            warn!("Failed to add signature: {err:?}");
                            CHAIN_NETWORK_GOSSIP_TOTALS
                                .with_label_values(&["pegout_sigs", "error"])
                                .inc();
                        }
                    }
                }
            }
        });
    }

    async fn get_blocks(
        self: &Arc<Self>,
        mut start_height: u64,
        requested_count: u64, // TODO: limit to requested_count
    ) -> Result<Vec<SignedConsensusBlock<MainnetEthSpec>>, Error> {
        // start at head, iterate backwards. We'll be able to have a more efficient implementation once we have finalization.
        let mut blocks: Vec<SignedConsensusBlock<MainnetEthSpec>> = vec![];
        let original_start_height = start_height;

        let head_ref = self
            .head
            .read()
            .await
            .as_ref()
            .ok_or(ChainError(Head.into()))?
            .clone();

        for i in 0..requested_count {
            if i > head_ref.height {
                break;
            }

            let current_height = original_start_height + i;
            start_height = current_height;
            let current = match self.storage.get_block_by_height(current_height) {
                Ok(Some(block)) => block,
                Ok(None) => {
                    debug!(
                        "Block at height {} not found, reverting to previous logic",
                        current_height
                    );
                    let mut blocks_from_head = vec![];
                    let mut current = head_ref.hash;
                    while let Some(block) = self.storage.get_block(&current).unwrap() {
                        if block.message.execution_payload.block_number < start_height {
                            break;
                        }

                        self.storage
                            .put_block_by_height(&block)
                            .unwrap_or_else(|err| {
                                error!("Failed to store block by height: {err:?}");
                            });

                        debug!(
                            "Got block at height {} via old logic",
                            block.message.execution_payload.block_number
                        );
                        blocks_from_head.push(block.clone());
                        if block.message.parent_hash.is_zero() {
                            break;
                        }
                        current = block.message.parent_hash;
                    }

                    blocks_from_head.reverse();

                    blocks.extend(blocks_from_head);
                    break;
                }
                Err(err) => {
                    error!("Error getting block: {err:?}");
                    return Err(err);
                }
            };

            blocks.push(current);
        }
        let mut block_counter = HashMap::new();

        blocks.iter().for_each(|block| {
            let block_height = block.message.execution_payload.block_number;
            if let Some(count) = block_counter.get_mut(&block_height) {
                *count += 1;
            } else {
                block_counter.insert(block_height, 1);
            }
        });

        Ok(blocks)
    }

    async fn sync(self: Arc<Self>) {
        info!("Syncing!");
        *self.sync_status.write().await = SyncStatus::InProgress;

        let peer_id = loop {
            if let Some(peer) = self.peers.read().await.iter().next() {
                break *peer;
            }
            info!("Waiting for peers...");
            tokio::time::sleep(Duration::from_secs(1)).await;
        };

        let head = self
            .head
            .read()
            .await
            .as_ref()
            .map(|x| x.height)
            .unwrap_or_default();
        info!("Starting sync from {}", head);
        CHAIN_SYNCING_OPERATION_TOTALS
            .with_label_values(&[head.to_string().as_str(), "called"])
            .inc();

        let mut receive_stream = self
            .network
            .send_rpc(
                peer_id,
                OutboundRequest::BlocksByRange(
                    crate::network::rpc::methods::BlocksByRangeRequest {
                        start_height: head + 1,
                        count: 1024,
                    },
                ),
            )
            .await
            .unwrap();

        while let Some(x) = receive_stream.recv().await {
            match x {
                RPCResponse::BlocksByRange(block) => {
                    // trace!("Received block: {:#?}", block);
                    match self.process_block((*block).clone()).await {
                        Err(Error::ProcessGenesis) | Ok(_) => {
                            // nothing to do
                        }
                        Err(err) => {
                            error!("Unexpected block import error: {:?}", err);
                            if let Err(rollback_err) = self.rollback_head(head - 1).await {
                                error!("Failed to rollback head: {:?}", rollback_err);
                            }

                            return;
                        }
                    }
                }
                err => {
                    error!("Received unexpected result: {err:?}");
                }
            }
        }

        *self.sync_status.write().await = SyncStatus::Synced;

        info!("Finished syncing...");
        CHAIN_SYNCING_OPERATION_TOTALS
            .with_label_values(&[head.to_string().as_str(), "success"])
            .inc();
    }

    pub async fn listen_for_peer_discovery(self: Arc<Self>) {
        let mut listener = self.network.subscribe_peers().await.unwrap();
        tokio::spawn(async move {
            loop {
                let peer_ids = listener.recv().await.unwrap();
                debug!("Got peers {peer_ids:?}");
                CHAIN_DISCOVERED_PEERS.set(peer_ids.len() as f64);

                let mut peers = self.peers.write().await;
                *peers = peer_ids;
            }
        });
    }

    pub async fn listen_for_rpc_requests(self: Arc<Self>) {
        let mut listener = self.network.subscribe_rpc_events().await.unwrap();
        tokio::spawn(async move {
            loop {
                let msg = listener.recv().await.unwrap();
                // info!("Got rpc request {msg:?}");

                #[allow(clippy::single_match)]
                match msg.event {
                    Ok(RPCReceived::Request(substream_id, InboundRequest::BlocksByRange(x))) => {
                        trace!("Got BlocksByRange request {x:?}");
                        let blocks = self
                            .get_blocks(x.start_height, x.count)
                            .await
                            .unwrap_or_else(|err| {
                                error!("Failed to get blocks: {err:?}");
                                vec![]
                            });
                        for block in blocks {
                            let payload = RPCCodedResponse::Success(RPCResponse::BlocksByRange(
                                Arc::new(block.clone()),
                            ));
                            // FIXME: handle result
                            if let Err(_err) = self
                                .network
                                .respond_rpc(msg.peer_id, msg.conn_id, substream_id, payload)
                                .await
                            {
                                // error!("Failed to respond to rpc BlocksByRange request: {err:?}");
                            }
                        }

                        let payload =
                            RPCCodedResponse::StreamTermination(ResponseTermination::BlocksByRange);
                        // FIXME: handle result
                        if let Err(err) = self
                            .network
                            .respond_rpc(msg.peer_id, msg.conn_id, substream_id, payload)
                            .await
                        {
                            error!("Failed to respond to rpc BlocksByRange request to terminate: {err:?}");
                        }
                    }
                    _ => {
                        error!("Received unexpected rpc request: {msg:?}");
                    }
                }
            }
        });
    }

    pub async fn monitor_bitcoin_blocks(self: Arc<Self>, start_height: u32) {
        info!("Starting to monitor bitcoin blocks from height {start_height}");

        tokio::spawn(async move {
            let chain = &self;

            let sync_status = self.sync_status.read().await;
            let is_synced = sync_status.is_synced();
            drop(sync_status);

            debug!("Inside monitor_bitcoin_blocks, Sync status: {}", is_synced);

            self.bridge
                .stream_blocks_for_pegins(start_height, |pegins, bitcoin_height| async move {
                    debug!(
                        "Inside stream_blocks_for_pegins, pegins: {:?}",
                        pegins.len()
                    );
                    for pegin in pegins.into_iter() {
                        if is_synced {
                            info!(
                                "Found pegin {} for {} in {}",
                                pegin.amount, pegin.evm_account, pegin.txid
                            );
                            chain.queued_pegins.write().await.insert(pegin.txid, pegin);
                            CHAIN_BTC_BLOCK_MONITOR_TOTALS
                                .with_label_values(&["queued_pegins", "synced"])
                                .inc();
                        } else {
                            debug!(
                                "Not synced, ignoring pegin {} for {} in {}",
                                pegin.amount, pegin.evm_account, pegin.txid
                            );
                            CHAIN_BTC_BLOCK_MONITOR_TOTALS
                                .with_label_values(&["ignored_pegins", "not_synced"])
                                .inc();

                            break;
                        }
                    }
                    // if we have queued pegins, start next rescan (after a node restart) at
                    // height of the oldest pegin. If there are no pegins, just start from the
                    // next block
                    let rescan_start = chain
                        .queued_pegins
                        .read()
                        .await
                        .iter()
                        .map(|(_, pegin)| pegin.block_height)
                        .min()
                        .unwrap_or(bitcoin_height + 1);
                    chain
                        .storage
                        .set_bitcoin_scan_start_height(rescan_start)
                        .unwrap();

                    debug!("Set next rescan start height to {}", rescan_start);
                })
                .await;
        });
    }

    pub fn get_block_by_height(
        self: &Arc<Self>,
        block_height: u64,
    ) -> Result<Option<SignedConsensusBlock<MainnetEthSpec>>> {
        let block = self.storage.get_block_by_height(block_height)?;
        if let Some(block) = block {
            Ok(Some(block))
        } else {
            warn!("Block: {:#?} not found", block_height);
            Ok(None)
        }
    }

    pub fn get_block(
        self: &Arc<Self>,
        block_hash: &Hash256,
    ) -> Result<Option<SignedConsensusBlock<MainnetEthSpec>>> {
        let block = self.storage.get_block(block_hash)?;
        if let Some(block) = block {
            Ok(Some(block))
        } else {
            warn!("Block: {:#?} not found", block_hash);
            Ok(None)
        }
    }

    pub async fn aggregate_hashes(self: &Arc<Self>) -> Result<Vec<BlockHash>> {
        let head = self
            .head
            .read()
            .await
            .as_ref()
            .ok_or(ChainError(Head.into()))?
            .hash;
        trace!("Head: {:?}", head);

        trace!("Getting aggregate hashes");
        let hashes = self.get_hashes(
            self.get_latest_finalized_block_ref()?
                .ok_or(ChainError(BlockErrorBlockTypes::LastFinalized.into()))?
                .hash,
            // self.head.read().await.as_ref().ok_or(Error::ChainError(BlockErrorBlockTypes::Head.into()))?.hash,
            head,
        )?;
        trace!("Got {} hashes", hashes.len());
        Ok(hashes
            .into_iter()
            .map(|hash| hash.to_block_hash())
            .collect())
    }
}

#[async_trait::async_trait]
impl<DB: ItemStore<MainnetEthSpec>> ChainManager<ConsensusBlock<MainnetEthSpec>> for Chain<DB> {
    async fn get_aggregate_hashes(&self) -> Result<Vec<BlockHash>> {
        let head = self
            .head
            .read()
            .await
            .as_ref()
            .ok_or(ChainError(Head.into()))?
            .hash;
        trace!("Head: {:?}", head);

        let queued_pow = self.queued_pow.read().await;

        let has_work = queued_pow
            .as_ref()
            .map(|pow| pow.range_end != head)
            .unwrap_or(true);

        #[allow(clippy::collapsible_else_if)]
        if !has_work {
            Err(NoWorkToDo.into())
        } else {
            if let Some(ref block_hash_cache) = self.block_hash_cache {
                Ok(block_hash_cache.read().await.get())
            } else {
                Err(eyre!("Block hash cache is not initialized"))
            }
        }
    }

    fn get_last_finalized_block(&self) -> ConsensusBlock<MainnetEthSpec> {
        trace!("Getting last finalized block");
        match self.storage.get_latest_pow_block() {
            Ok(Some(x)) => self.storage.get_block(&x.hash).unwrap().unwrap().message,
            _ => unreachable!("Should always have AuxPow"),
        }
    }

    fn get_block_by_hash(&self, hash: &BlockHash) -> Result<ConsensusBlock<MainnetEthSpec>> {
        trace!("Getting block by hash: {:?}", hash);
        let block = self.storage.get_block(&hash.to_block_hash())?.unwrap();
        Ok(block.message)
    }

    async fn get_queued_auxpow(&self) -> Option<AuxPowHeader> {
        self.queued_pow.read().await.clone()
    }

    fn get_block_at_height(&self, height: u64) -> Result<ConsensusBlock<MainnetEthSpec>> {
        match self.storage.get_block_by_height(height) {
            Ok(Some(block)) => Ok(block.message),
            Ok(None) => Err(eyre!("Block not found")),
            Err(err) => Err(eyre!(err)),
        }
    }

    async fn push_auxpow(
        &self,
        start_hash: BlockHash,
        end_hash: BlockHash,
        bits: u32,
        chain_id: u32,
        height: u64,
        auxpow: AuxPow,
        address: Address,
    ) -> bool {
        let pow = AuxPowHeader {
            range_start: start_hash.to_block_hash(),
            range_end: end_hash.to_block_hash(),
            bits,
            chain_id,
            height,
            auxpow: Some(auxpow),
            fee_recipient: address,
        };
        if self.queued_pow.read().await.as_ref().is_some_and(|prev| {
            prev.range_start.eq(&pow.range_start) && prev.range_end.eq(&pow.range_end)
        }) {
            return false;
        }
        self.check_pow(&pow, false).await.is_ok() && self.share_pow(pow).await.is_ok()
    }

    async fn is_synced(&self) -> bool {
        self.sync_status.read().await.is_synced()
    }

    fn get_head(&self) -> Result<SignedConsensusBlock<MainnetEthSpec>, Error> {
        #[allow(clippy::needless_question_mark)]
        let head_block = self
            .storage
            .get_block(
                &self
                    .storage
                    .get_head()?
                    .ok_or(ChainError(Head.into()))?
                    .hash,
            )?
            .ok_or(ChainError(Head.into()))?;

        // Set the CHAIN_BLOCK_HEIGHT gauge with the block height of the head block
        CHAIN_BLOCK_HEIGHT.set(head_block.message.execution_payload.block_number as i64);

        Ok(head_block)
    }
}

#[async_trait]
impl<DB: ItemStore<MainnetEthSpec>> BlockHashCacheInit for Chain<DB> {
    async fn init_block_hash_cache(self: &Arc<Self>) -> Result<()> {
        if let Some(ref block_hash_cache) = self.block_hash_cache {
            let mut block_hash_cache = block_hash_cache.write().await;
            block_hash_cache.init(self.aggregate_hashes().await?)
        } else {
            Ok(())
        }
    }
}
