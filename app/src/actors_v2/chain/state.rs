//! ChainActor V2 State Management
//!
//! Simplified state management derived from chain.rs without complex RwLock patterns

use std::collections::BTreeMap;
use std::time::SystemTime;
use bitcoin::Txid;
use ethereum_types::{Address, H256};

use crate::auxpow_miner::BitcoinConsensusParams;
use crate::block::{AuxPowHeader};
use crate::block_hash_cache::BlockHashCache;
use crate::store::BlockRef;
use bridge::{Bridge, PegInInfo, BitcoinSignatureCollector, BitcoinSigner};
use crate::engine::Engine;
use crate::aura::Aura;

pub(crate) type BitcoinWallet = bridge::UtxoManager<bridge::Tree>;

/// Sync status enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    Synced,
    Syncing { progress: f64, target_height: u64 },
    NotSynced,
    Error(String),
}

/// ChainActor state (simplified from chain.rs)
pub struct ChainState {
    /// Core blockchain state (derived from chain.rs)
    pub engine: Engine,
    pub aura: Aura,
    pub head: Option<BlockRef>,
    pub sync_status: SyncStatus,

    /// Essential AuxPoW and consensus
    pub queued_pow: Option<AuxPowHeader>,
    pub max_blocks_without_pow: u64,
    pub federation: Vec<Address>,

    /// Peg operations (simplified from chain.rs)
    pub bridge: Bridge,
    pub queued_pegins: BTreeMap<Txid, PegInInfo>,
    pub bitcoin_wallet: BitcoinWallet,
    pub bitcoin_signature_collector: BitcoinSignatureCollector,
    pub maybe_bitcoin_signer: Option<BitcoinSigner>,

    /// Essential configuration
    pub is_validator: bool,
    pub retarget_params: BitcoinConsensusParams,
    pub block_hash_cache: Option<BlockHashCache>,

    /// Runtime state
    pub blocks_without_pow: u64,
    pub last_block_time: Option<SystemTime>,
}

impl std::fmt::Debug for ChainState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainState")
            .field("head", &self.head)
            .field("sync_status", &self.sync_status)
            .field("queued_pow", &self.queued_pow)
            .field("max_blocks_without_pow", &self.max_blocks_without_pow)
            .field("federation", &self.federation)
            .field("queued_pegins", &self.queued_pegins)
            .field("is_validator", &self.is_validator)
            .field("retarget_params", &self.retarget_params)
            .field("block_hash_cache", &self.block_hash_cache)
            .field("blocks_without_pow", &self.blocks_without_pow)
            .field("last_block_time", &self.last_block_time)
            .field("engine", &"<Engine>")
            .field("aura", &"<Aura>")
            .field("bridge", &"<Bridge>")
            .field("bitcoin_wallet", &"<BitcoinWallet>")
            .field("bitcoin_signature_collector", &"<BitcoinSignatureCollector>")
            .field("maybe_bitcoin_signer", &format_args!("<Option<BitcoinSigner>>"))
            .finish()
    }
}

impl ChainState {
    /// Create new chain state
    pub fn new(
        engine: Engine,
        aura: Aura,
        federation: Vec<Address>,
        bridge: Bridge,
        bitcoin_wallet: BitcoinWallet,
        bitcoin_signature_collector: BitcoinSignatureCollector,
        maybe_bitcoin_signer: Option<BitcoinSigner>,
        retarget_params: BitcoinConsensusParams,
        is_validator: bool,
        max_blocks_without_pow: u64,
        head: Option<BlockRef>,
    ) -> Self {
        Self {
            engine,
            aura,
            head,
            sync_status: SyncStatus::Synced,
            queued_pow: None,
            max_blocks_without_pow,
            federation,
            bridge,
            queued_pegins: BTreeMap::new(),
            bitcoin_wallet,
            bitcoin_signature_collector,
            maybe_bitcoin_signer,
            is_validator,
            retarget_params,
            block_hash_cache: Some(BlockHashCache::new(None)),
            blocks_without_pow: 0,
            last_block_time: None,
        }
    }

    /// Update chain head
    pub fn update_head(&mut self, head: BlockRef) {
        self.head = Some(head);
        self.last_block_time = Some(SystemTime::now());
    }

    /// Check if chain is synced
    pub fn is_synced(&self) -> bool {
        matches!(self.sync_status, SyncStatus::Synced)
    }

    /// Update sync status
    pub fn set_sync_status(&mut self, status: SyncStatus) {
        self.sync_status = status;
    }

    /// Add queued peg-in
    pub fn add_queued_pegin(&mut self, txid: Txid, pegin: PegInInfo) {
        self.queued_pegins.insert(txid, pegin);
    }

    /// Remove processed peg-in
    pub fn remove_queued_pegin(&mut self, txid: &Txid) -> Option<PegInInfo> {
        self.queued_pegins.remove(txid)
    }

    /// Get current height
    pub fn get_height(&self) -> u64 {
        self.head.as_ref().map(|h| h.height).unwrap_or(0)
    }

    /// Get head hash
    pub fn get_head_hash(&self) -> Option<H256> {
        self.head.as_ref().map(|h| h.hash)
    }

    /// Check if we need AuxPoW
    pub fn needs_auxpow(&self) -> bool {
        self.blocks_without_pow >= self.max_blocks_without_pow
    }

    /// Increment blocks without AuxPoW
    pub fn increment_blocks_without_pow(&mut self) {
        self.blocks_without_pow += 1;
    }

    /// Reset blocks without AuxPoW (when AuxPoW is processed)
    pub fn reset_blocks_without_pow(&mut self) {
        self.blocks_without_pow = 0;
    }

    /// Set queued AuxPoW
    pub fn set_queued_pow(&mut self, auxpow: Option<AuxPowHeader>) {
        self.queued_pow = auxpow;
    }

    /// Get queued AuxPoW
    pub fn get_queued_pow(&self) -> &Option<AuxPowHeader> {
        &self.queued_pow
    }
}