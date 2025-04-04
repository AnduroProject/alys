use crate::auxpow::AuxPow;
use crate::auxpow_miner::{
    get_next_work_required, BitcoinConsensusParams, BlockIndex, ChainManager,
};
use crate::block::{AuxPowHeader, ConsensusBlock, ConvertBlockHash};
use crate::block_candidate_cache::BlockCandidates;
use crate::block_hash_cache::{BlockHashCache, BlockHashCacheInit};
use crate::engine::{ConsensusAmount, Engine};
use crate::error::AuxPowMiningError::NoWorkToDo;
use crate::error::BlockErrorBlockTypes;
use crate::error::BlockErrorBlockTypes::Head;
use crate::error::Error::ChainError;
use crate::network::rpc::InboundRequest;
use crate::network::rpc::{RPCCodedResponse, RPCReceived, RPCResponse, ResponseTermination};
use crate::network::PubsubMessage;
use crate::network::{ApproveBlock, Client as NetworkClient, OutboundRequest};
use crate::signatures::CheckedIndividualApproval;
use crate::spec::ChainSpec;
use crate::store::{BlockByHeight, BlockRef};
use crate::{aura::Aura, block::SignedConsensusBlock, error::Error, store::Storage};
use async_trait::async_trait;
use bitcoin::{BlockHash, CompactTarget, Transaction as BitcoinTransaction, Txid};
use bridge::SingleMemberTransactionSignatures;
use bridge::{BitcoinSignatureCollector, BitcoinSigner, Bridge, PegInInfo, Tree, UtxoManager};
use ethereum_types::{Address, H256, U64};
use ethers_core::types::{Block, Transaction, TransactionReceipt, U256};
use execution_layer::Error::MissingLatestValidHash;
use eyre::{eyre, Result};
use libp2p::PeerId;
use std::collections::{BTreeMap, HashSet};
use std::ops::{Add, DerefMut, Div, Mul, Sub};
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};
use store::ItemStore;
use store::KeyValueStoreOp;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::RwLock;
use tracing::*;
use types::{ExecutionBlockHash, Hash256, MainnetEthSpec};

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
        let mut withdrawals = BTreeMap::<_, u64>::new();
        let mut processed_pegins = Vec::new();

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

            {
                let wallet = self.bitcoin_wallet.read().await;
                txids.retain(|txid| wallet.get_tx(txid).unwrap().is_some());
            }

            for already_processed_txid in txids {
                self.queued_pegins
                    .write()
                    .await
                    .remove(&already_processed_txid);
            }
        }

        for pegin in self.queued_pegins.read().await.values() {
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
            }
        }
        let withdrawals: Vec<(Address, u64)> = withdrawals.into_iter().collect();

        info!("Adding {} pegins", processed_pegins.len());
        // these are the withdrawals, merge payments to the same EVM address
        add_balances.extend(
            withdrawals
                .iter()
                .map(|(address, amount)| (*address, ConsensusAmount::from_satoshi(*amount))),
        );

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
        if !self.sync_status.read().await.is_synced() {
            return Ok(());
        }
        let mut prev_height = 0;
        let mut rollback_head = false;

        // TODO: should we set forkchoice here?
        let (prev, prev_payload_head) = match *(self.head.read().await) {
            Some(ref x) => {
                let prev = self
                    .storage
                    .get_block(&x.hash)
                    .map_err(|_| Error::MissingParent)?
                    .ok_or(Error::MissingParent)?;

                // make sure payload is built on top of the correct block
                let prev_payload_hash = prev.message.execution_payload.block_hash;
                prev_height = prev.message.execution_payload.block_number;

                // make sure that the execution payload is available
                let prev_payload_body = self
                    .engine
                    .api
                    .get_payload_bodies_by_hash_v1::<MainnetEthSpec>(vec![prev_payload_hash])
                    .await
                    .map_err(|_| Error::ExecutionLayerError(MissingLatestValidHash))?;

                if prev_payload_body.is_empty() || prev_payload_body[0].is_none() {
                    rollback_head = true;
                    (Hash256::zero(), None)
                } else {
                    (x.hash, Some(prev_payload_hash))
                }
            }
            None => (Hash256::zero(), None),
        };
        debug!("Previous block: {:?}", prev);

        if rollback_head {
            warn!("No payload head found");
            self.rollback_head(prev_height - 1).await?;
            return Ok(());
        }

        let (mut queued_pow, mut finalized_pegouts): (
            Option<AuxPowHeader>,
            Vec<bitcoin::Transaction>,
        ) = (None, vec![]);

        (queued_pow, finalized_pegouts) = match self.queued_pow.read().await.clone() {
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
        let pegins = self.fill_pegins(&mut add_balances).await;

        let payload_result = self
            .engine
            .build_block(
                timestamp,
                prev_payload_head,
                add_balances.into_iter().map(Into::into).collect(),
            )
            .await;

        let payload = match payload_result {
            Ok(payload) => payload,
            Err(err) => {
                match err {
                    Error::PayloadIdUnavailable => {
                        warn!(
                            "PayloadIdUnavailable: Slot {}, Timestamp {:?}",
                            slot, timestamp
                        );
                        // self.clone().sync(None).await;
                        self.clone().sync().await;

                        // we are missing a parent, this is normal if we are syncing
                        return Ok(());
                    }
                    _ => {
                        warn!("Failed to build block payload: {:?}", err);
                        return Ok(());
                    }
                }
            }
        };

        // generate a unsigned bitcoin tx for pegout requests made in the previous block, if any
        let pegouts = self.create_pegout_payments(prev_payload_head).await;

        if finalized_pegouts.len() > 0 {
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

        let root_hash = signed_block.canonical_root();
        info!(
            "⛏️  Proposed block on slot {slot} (block {}) {prev} -> {root_hash}",
            payload.block_number()
        );

        match self.process_block(signed_block.clone()).await {
            Err(Error::MissingRequiredPow) => {
                warn!("Could not produce block - need PoW");
                // don't consider this fatal
                return Ok(());
            }
            Err(e) => {
                error!("Failed to process block we created ourselves... {e:?}");
                return Ok(());
            }
            Ok(_) => {}
        }

        if let Err(x) = self.network.publish_block(signed_block.clone()).await {
            info!("Failed to publish block: {x}");
        }
        Ok(())
    }

    async fn rollback_head(self: &Arc<Self>, target_height: u64) -> Result<(), Error> {
        let prev_block_parent = self
            .storage
            .get_block_by_height(target_height)?
            .ok_or(Error::MissingParent)?;

        let update_head_ref_ops = self
            .update_head_ref_ops(
                prev_block_parent.canonical_root(),
                prev_block_parent.message.height(),
                true,
            )
            .await;

        trace!(
            "Rolling back head to {:?}",
            prev_block_parent.message.height()
        );

        self.storage.commit_ops(update_head_ref_ops)?;
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
        let root_hash = unverified_block.canonical_root();
        info!(
            "Processing block at height {}",
            unverified_block.message.execution_payload.block_number
        );

        // TODO: check that EL approved of the payload, ideally without
        // actually already importing the block

        if unverified_block.message.parent_hash.is_zero() {
            // no need to process genesis
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

            return Err(Error::ExecutionHashChainIncontiguous);
        }

        self.aura.check_signed_by_author(&unverified_block)?;

        if self.is_validator {
            self.check_withdrawals(&unverified_block).await?;

            self.check_pegout_proposal(&unverified_block, prev_payload_hash)
                .await?;
        }

        // TODO: We should set the bitcoin connection to be optional
        if let Some(ref pow) = unverified_block.message.auxpow_header {
            // NOTE: Should be removed after chain deprecation
            let mut pow_override = false;

            // TODO: Historical Context
            if unverified_block.message.execution_payload.block_number == 39171
                || unverified_block.message.execution_payload.block_number == 39264
                || unverified_block.message.execution_payload.block_number == 39266
                || unverified_block.message.execution_payload.block_number == 41369
                || unverified_block.message.execution_payload.block_number == 42338
                || unverified_block.message.execution_payload.block_number == 45989
                || unverified_block.message.execution_payload.block_number == 48055
                || unverified_block.message.execution_payload.block_number == 48263
                || unverified_block.message.execution_payload.block_number == 48288
                || unverified_block.message.execution_payload.block_number == 48765
                || unverified_block.message.execution_payload.block_number == 50260
                || unverified_block.message.execution_payload.block_number == 50544
                || unverified_block.message.execution_payload.block_number == 53143
            {
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

            for (expected_txid, tx) in required_finalizations
                .into_iter()
                .zip(unverified_block.message.finalized_pegouts.iter())
            {
                if tx.txid() != expected_txid {
                    return Err(Error::IllegalFinalization);
                }

                let wallet = self.bitcoin_wallet.read().await;
                trace!("Checking signature for finalized pegout {:?}", tx.txid());
                // NOTE: same as auxpow_override
                wallet.check_transaction_signatures(tx, pow_override)?;
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
                return Err(Error::MissingRequiredPow);
            }

            if !unverified_block.message.finalized_pegouts.is_empty() {
                return Err(Error::IllegalFinalization);
            }
        }

        // store the candidate
        // TODO: this is also called on sync which isn't strictly required
        let our_approval = if let Some(authority) = &self.aura.authority {
            let our_approval = unverified_block.message.sign(authority);

            // First insert the block
            self.block_candidates
                .insert(unverified_block.clone())
                .await?;

            // Then add our approval
            let approval = ApproveBlock {
                block_hash: root_hash,
                signature: our_approval.clone().into(),
            };
            self.block_candidates
                .add_approval(approval, &self.aura.authorities)
                .await?;

            Some(our_approval)
        } else {
            trace!("Full node doesn't need to approve");
            // full node doesn't need to approve
            self.block_candidates
                .insert(unverified_block.clone())
                .await?;
            None
        };

        self.maybe_accept_block(root_hash).await?;

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
            let block = self
                .storage
                .get_block(&current)?
                .ok_or(Error::InvalidBlockRange)?
                .message;

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

    fn get_hashes(
        &self,
        from: Hash256, // exclusive
        to: Hash256,   // inclusive
    ) -> Result<Vec<Hash256>, Error> {
        // trace!("Getting hashes from {:?} to {:?}", from, to);
        let mut current = to;
        let mut hashes = vec![];
        loop {
            if current == from {
                break;
            }

            hashes.push(current);
            // trace!("Pushing hash {:?}", current);

            current = self
                .storage
                .get_block(&current)?
                .ok_or(Error::InvalidBlockRange)?
                .message
                .parent_hash;
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

        let last_pow = self.storage.get_latest_pow_block()?.unwrap().hash;
        let last_finalized = self
            .get_latest_finalized_block_ref()?
            .ok_or(Error::MissingParent)?;

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
        if range_start_block.message.height() > 53143 {
            if range_start_block.message.parent_hash != last_finalized.hash {
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
        }

        let hashes = self.get_hashes(range_start_block.message.parent_hash, header.range_end)?;

        let hash = AuxPow::aggregate_hash(
            &hashes
                .into_iter()
                .map(|hash| hash.to_block_hash())
                .collect::<Vec<_>>(),
        );

        let bits = get_next_work_required(
            |height| self.get_block_at_height(height),
            &self
                .get_block_by_hash(&last_pow.to_block_hash())
                .map_err(|err| Error::GenericError(err))?,
            &self.retarget_params,
            self.storage.get_target_override()?,
        )
        .map_err(|err| Error::GenericError(err))?;

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
                let pow_block = self.storage.get_block(&blockref.hash)?.unwrap();
                let pow = pow_block.message.auxpow_header.unwrap();
                let last_finalized_blockref = if pow.height != 0 {
                    self.storage.get_block(&pow.range_end)?.unwrap().block_ref()
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

        // Add the approval to the cache
        self.block_candidates
            .add_approval(approval, &self.aura.authorities)
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
            .get_payload_by_tag_from_engine(execution_layer::BlockByNumberQuery::Tag("0x0"))
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

    pub(crate) async fn get_accumulated_fees(
        &self,
        block_hash: Option<&Hash256>,
    ) -> Result<U256, Error> {
        if let Some(block_hash) = block_hash {
            self.storage
                .get_accumulated_block_fees(block_hash)?
                .ok_or(Error::MissingBlock)
        } else {
            self.storage
                .get_accumulated_block_fees(&self.get_latest_finalized_block_ref()?.unwrap().hash)?
                .ok_or(Error::MissingBlock)
        }
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
                        .map_err(|err| Error::GenericError(err))?;
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
                        continue;
                    }
                    Err(_) => panic!("failed to read network stream"),
                    Ok(x) => x,
                };
                match msg {
                    PubsubMessage::ConsensusBlock(x) => {
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
                                _ => {
                                    error!("Got error while processing: {x:?}");
                                }
                            },
                            Ok(Some(our_approval)) => {
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
                        }
                        Ok(()) => {
                            self.queue_pow(pow.clone()).await;
                        }
                    },
                    PubsubMessage::PegoutSignatures(pegout_sigs) => {
                        if self.is_validator {}
                        if let Err(err) = self.store_signatures(pegout_sigs).await {
                            warn!("Failed to add signature: {err:?}");
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
    }

    pub async fn listen_for_peer_discovery(self: Arc<Self>) {
        let mut listener = self.network.subscribe_peers().await.unwrap();
        tokio::spawn(async move {
            loop {
                let peer_ids = listener.recv().await.unwrap();
                debug!("Got peers {peer_ids:?}");

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
        info!("Monitoring bitcoin blocks from height {start_height}");

        tokio::spawn(async move {
            let chain = &self;

            let sync_status = self.sync_status.read().await;
            let is_synced = sync_status.is_synced();
            drop(sync_status);

            self.bridge
                .stream_blocks_for_pegins(start_height, |pegins, bitcoin_height| async move {
                    for pegin in pegins.into_iter() {
                        if is_synced {
                            info!(
                                "Found pegin {} for {} in {}",
                                pegin.amount, pegin.evm_account, pegin.txid
                            );
                            chain.queued_pegins.write().await.insert(pegin.txid, pegin);
                        } else {
                            debug!(
                                "Not synced, ignoring pegin {} for {} in {}",
                                pegin.amount, pegin.evm_account, pegin.txid
                            );

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
                })
                .await;
        });
    }

    pub async fn get_blocks_with_pegouts(self: &Arc<Self>) -> Result<(), Error> {
        let head = self.head.read().await.as_ref().unwrap().clone();
        let mut pending_pegouts = HashMap::new();
        let mut finalized_pegouts = HashMap::new();
        let mut current = head.hash;
        debug!("Getting pegouts");
        loop {
            let block = self.storage.get_block(&current)?.unwrap();

            // revert previous checkpoints
            if block.message.auxpow_header.is_some() {
                let proposal = block.message.pegout_payment_proposal.clone().unwrap();
                info!("Found pegout tx: {:?}", proposal);
                pending_pegouts.insert(proposal.txid(), proposal);
            }
            if !block.message.finalized_pegouts.is_empty() {
                let _: Vec<_> = block
                    .message
                    .finalized_pegouts
                    .into_iter()
                    .map(|tx| {
                        info!("Found pegout tx: {:?}", tx);
                        finalized_pegouts.insert(tx.txid(), tx.clone());
                    })
                    .collect();
            }
            if block.message.parent_hash.is_zero() {
                break;
            }
            // blocks.push(block.clone());
            // block_heights.push(block.message.execution_payload.block_number);

            current = block.message.parent_hash;
        }
        // blocks.reverse();
        // block_heights.reverse();

        debug!("finalized_pegouts: {:#?}", finalized_pegouts);
        debug!("pending_pegouts: {:#?}", pending_pegouts);
        Ok(())
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
        // trace!("Head: {:?}", head);

        let queued_pow = self.queued_pow.read().await;

        let has_work = queued_pow
            .as_ref()
            .map(|pow| pow.range_end != head)
            .unwrap_or(true);
        if !has_work {
            // trace!("No work to do");
            // TODO: Change to Result so that we can return this error
            Err(NoWorkToDo.into())
        } else {
            if let Some(ref block_hash_cache) = self.block_hash_cache {
                // trace!("Returning cached hashes");
                Ok(block_hash_cache.read().await.get())
            } else {
                Err(eyre!("Block hash cache not initialized"))
            }
        }
    }

    fn get_last_finalized_block(&self) -> ConsensusBlock<MainnetEthSpec> {
        match self.storage.get_latest_pow_block() {
            Ok(Some(x)) => self.storage.get_block(&x.hash).unwrap().unwrap().message,
            _ => unreachable!("Should always have AuxPow"),
        }
    }

    fn get_block_by_hash(&self, hash: &BlockHash) -> Result<ConsensusBlock<MainnetEthSpec>> {
        // trace!("Getting block by hash: {:?}", hash);
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
    fn set_target_override(&self, target: CompactTarget) {
        self.storage.set_target_override(target).unwrap()
    }

    fn get_target_override(&self) -> Option<CompactTarget> {
        self.storage.get_target_override().unwrap()
    }

    async fn is_synced(&self) -> bool {
        self.sync_status.read().await.is_synced()
    }

    fn get_head(&self) -> Result<SignedConsensusBlock<MainnetEthSpec>, Error> {
        Ok(self
            .storage
            .get_block(
                &self
                    .storage
                    .get_head()?
                    .ok_or(ChainError(Head.into()))?
                    .hash,
            )?
            .ok_or(ChainError(Head.into()))?)
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
