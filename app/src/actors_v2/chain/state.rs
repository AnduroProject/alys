//! ChainActor V2 State Management
//!
//! Simplified state management derived from chain.rs without complex RwLock patterns

use std::collections::BTreeMap;
use std::time::SystemTime;
use std::sync::Arc;
use tokio::sync::RwLock;
use bitcoin::{BlockHash, Txid};
use ethereum_types::{Address, H256};

use crate::auxpow_miner::BitcoinConsensusParams;
use crate::block::{AuxPowHeader};
use crate::block_hash_cache::BlockHashCache;
use crate::actors_v2::storage::actor::BlockRef;
use bridge::{Bridge, PegInInfo, BitcoinSignatureCollector, BitcoinSigner};
use crate::aura::Aura;

/// Mining context for tracking issued AuxPoW work (Priority 3)
///
/// Stores context for work issued to miners via `createauxblock`.
/// Used to validate submissions in `submitauxblock`.
#[derive(Debug, Clone)]
pub struct MiningContext {
    /// When this work was issued
    pub issued_at: SystemTime,
    /// Last finalized block hash at time of issuance
    pub last_hash: H256,
    /// First block in range
    pub start_hash: BlockHash,
    /// Last block in range
    pub end_hash: BlockHash,
    /// Miner's reward address
    pub miner_address: Address,
    /// Difficulty target (compact form)
    pub bits: u32,
    /// Target height after mining
    pub height: u64,
}

pub(crate) type BitcoinWallet = bridge::UtxoManager<bridge::Tree>;

/// Sync status enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    Synced,
    Syncing { progress: f64, target_height: u64 },
    NotSynced,
    Error(String),
}

/// ChainActor state (simplified from chain.rs) - Arc/RwLock pattern for functional bridge processing
#[derive(Clone)]
pub struct ChainState {
    /// Core blockchain state (derived from chain.rs) - Read-only Arc-wrapped V0 components
    pub aura: Arc<Aura>, // ✅ Read-only: consensus validation only

    /// Essential blockchain state (simple types - cloneable as-is)
    pub head: Option<BlockRef>,
    pub sync_status: SyncStatus,
    pub federation: Vec<Address>,

    /// Essential AuxPoW and consensus
    pub queued_pow: Option<AuxPowHeader>,
    pub max_blocks_without_pow: u64,

    /// Mining context state (Priority 3: tracks issued work for validation)
    pub mining_contexts: Arc<RwLock<BTreeMap<BlockHash, MiningContext>>>,

    /// Peg operations (Arc<RwLock<T>> for mutable bridge processing)
    pub bridge: Arc<RwLock<Bridge>>,
    pub queued_pegins: Arc<RwLock<BTreeMap<Txid, PegInInfo>>>,
    pub bitcoin_wallet: Arc<RwLock<BitcoinWallet>>,
    pub bitcoin_signature_collector: Arc<RwLock<BitcoinSignatureCollector>>,
    pub maybe_bitcoin_signer: Option<Arc<RwLock<BitcoinSigner>>>,

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
            .field("mining_contexts", &"<BTreeMap<BlockHash, MiningContext>>")
            .field("federation", &self.federation)
            .field("queued_pegins", &self.queued_pegins)
            .field("is_validator", &self.is_validator)
            .field("retarget_params", &self.retarget_params)
            .field("block_hash_cache", &self.block_hash_cache)
            .field("blocks_without_pow", &self.blocks_without_pow)
            .field("last_block_time", &self.last_block_time)
            .field("aura", &"<Aura>")
            .field("bridge", &"<Bridge>")
            .field("bitcoin_wallet", &"<BitcoinWallet>")
            .field("bitcoin_signature_collector", &"<BitcoinSignatureCollector>")
            .field("maybe_bitcoin_signer", &format_args!("<Option<BitcoinSigner>>"))
            .finish()
    }
}

impl ChainState {
    /// Create new chain state with Arc-wrapped V0 components
    pub fn new(
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
            aura: Arc::new(aura),
            head,
            sync_status: SyncStatus::Synced,
            queued_pow: None,
            max_blocks_without_pow,
            mining_contexts: Arc::new(RwLock::new(BTreeMap::new())),
            federation,
            bridge: Arc::new(RwLock::new(bridge)),
            queued_pegins: Arc::new(RwLock::new(BTreeMap::new())),
            bitcoin_wallet: Arc::new(RwLock::new(bitcoin_wallet)),
            bitcoin_signature_collector: Arc::new(RwLock::new(bitcoin_signature_collector)),
            maybe_bitcoin_signer: maybe_bitcoin_signer.map(|signer| Arc::new(RwLock::new(signer))),
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

    /// Add queued peg-in (async due to RwLock)
    pub async fn add_queued_pegin(&self, txid: Txid, pegin: PegInInfo) {
        self.queued_pegins.write().await.insert(txid, pegin);
    }

    /// Remove processed peg-in (async due to RwLock)
    pub async fn remove_queued_pegin(&self, txid: &Txid) -> Option<PegInInfo> {
        self.queued_pegins.write().await.remove(txid)
    }

    /// Get current height
    pub fn get_height(&self) -> u64 {
        self.head.as_ref().map(|h| h.number).unwrap_or(0)
    }

    /// Get head hash
    pub fn get_head_hash(&self) -> Option<lighthouse_wrapper::types::Hash256> {
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

    /// Store mining context for submitted work validation (Priority 3)
    pub async fn store_mining_context(&self, aggregate_hash: BlockHash, context: MiningContext) {
        self.mining_contexts.write().await.insert(aggregate_hash, context);
    }

    /// Retrieve and remove mining context (Priority 3)
    pub async fn take_mining_context(&self, aggregate_hash: &BlockHash) -> Option<MiningContext> {
        self.mining_contexts.write().await.remove(aggregate_hash)
    }

    /// Cleanup stale mining contexts (Priority 3)
    ///
    /// Removes contexts older than the specified timeout duration.
    /// Returns count of removed contexts.
    pub async fn cleanup_stale_mining_contexts(&self, timeout_secs: u64) -> usize {
        let now = SystemTime::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);

        let mut contexts = self.mining_contexts.write().await;
        let initial_count = contexts.len();

        contexts.retain(|_hash, context| {
            let elapsed = now.duration_since(context.issued_at).unwrap_or_default();
            elapsed < timeout
        });

        let removed_count = initial_count - contexts.len();
        removed_count
    }
}