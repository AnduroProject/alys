//! ChainActor V2 State Management
//!
//! State management for ChainActor V2 with Arc<RwLock> wrapping for async handler compatibility.
//!
//! ## State Mutation Safety (Phase 2 Fix)
//!
//! All mutable state fields are wrapped in `Arc<RwLock>` to ensure mutations made in
//! async handlers (which receive cloned ChainActor instances) propagate back to the
//! original state. This fixes the state mutation bug where cloned state changes were lost.
//!
//! ## Cumulative Difficulty Tracking (Gap FC-2)
//!
//! The ChainState now tracks cumulative difficulty for "most work wins" fork choice:
//! - `cumulative_difficulty`: Total difficulty of the current canonical chain tip
//! - `difficulty_cache`: LRU cache for recent heights to avoid DB lookups

use bitcoin::{BlockHash, Txid};
use ethereum_types::{Address, H256};
use lru::LruCache;
use std::collections::{BTreeMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

use crate::actors_v2::storage::actor::BlockRef;
use crate::auxpow_miner::BitcoinConsensusParams;
use crate::block::AuxPowHeader;
use crate::block_hash_cache::BlockHashCache;
use bridge::{BitcoinSignatureCollector, BitcoinSigner, Bridge};

// Import V2 Tendermint types for peg-in handling (Doc 16)
use super::tendermint::pegin::QueuedPegIn;
use super::tendermint::types::Commit;

/// Default size for the difficulty cache (number of heights to cache)
const DEFAULT_DIFFICULTY_CACHE_SIZE: usize = 256;

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

impl SyncStatus {
    /// Returns true if currently syncing
    pub fn is_syncing(&self) -> bool {
        matches!(self, SyncStatus::Syncing { .. })
    }
}

/// ChainActor state with Arc<RwLock> wrapping for async handler compatibility.
///
/// ## State Mutation Safety
///
/// All mutable fields are wrapped in `Arc<RwLock>` to ensure mutations made in
/// async handlers propagate correctly. When handlers clone ChainActor for async use,
/// the Arc<RwLock> fields share state, so mutations are visible across clones.
///
/// ## Field Categories
///
/// - **Read-only**: `federation`, `is_validator`, `retarget_params`, `max_blocks_without_pow`
/// - **Mutable (Arc<RwLock>)**: All other fields that may be modified during operation
///
/// ## Consensus Note
///
/// With Tendermint-only consensus, block finality is proven via `last_commit` containing
/// 2/3+ validator precommit signatures. Aura (PoA) is no longer used.
#[derive(Clone)]
pub struct ChainState {
    // ========================================================================
    // Read-Only Components (no Arc<RwLock> needed)
    // ========================================================================

    /// Federation members (read-only after initialization)
    pub federation: Vec<Address>,

    /// Essential configuration (read-only)
    pub is_validator: bool,
    pub retarget_params: BitcoinConsensusParams,
    pub max_blocks_without_pow: u64,

    /// Block hash cache for AuxPoW aggregate calculation
    /// Tracks unfinalized blocks that need AuxPoW coverage
    /// Wrapped in Arc<RwLock> for async mutation from handlers
    pub block_hash_cache: Arc<RwLock<BlockHashCache>>,

    // ========================================================================
    // Mutable State (Arc<RwLock> for async handler compatibility)
    // ========================================================================

    /// Current chain head - WRAPPED for async handler access
    /// Previously: `head: Option<BlockRef>` (mutations lost in cloned handlers)
    pub head: Arc<RwLock<Option<BlockRef>>>,

    /// Current sync status - WRAPPED for async handler access
    /// Previously: `sync_status: SyncStatus` (mutations lost in cloned handlers)
    pub sync_status: Arc<RwLock<SyncStatus>>,

    /// Queued AuxPoW header for next block - WRAPPED for async handler access
    /// Previously: `queued_pow: Option<AuxPowHeader>` (mutations lost in cloned handlers)
    pub queued_pow: Arc<RwLock<Option<AuxPowHeader>>>,

    /// Blocks since last AuxPoW - WRAPPED for async handler access
    /// Previously: `blocks_without_pow: u64` (mutations lost in cloned handlers)
    pub blocks_without_pow: Arc<RwLock<u64>>,

    /// Last block production time - WRAPPED for async handler access
    /// Previously: `last_block_time: Option<SystemTime>` (mutations lost in cloned handlers)
    pub last_block_time: Arc<RwLock<Option<SystemTime>>>,

    // ========================================================================
    // Mining Context (Already Arc<RwLock>)
    // ========================================================================

    /// Mining context state (Priority 3: tracks issued work for validation)
    pub mining_contexts: Arc<RwLock<BTreeMap<BlockHash, MiningContext>>>,

    // ========================================================================
    // Peg Operations (Already Arc<RwLock>)
    // ========================================================================

    /// Bridge for Bitcoin interaction
    pub bridge: Arc<RwLock<Bridge>>,

    /// Queued peg-ins awaiting block inclusion (Doc 16: uses QueuedPegIn with fee_recipient)
    /// Changed from `BTreeMap<Txid, PegInInfo>` to `BTreeMap<Txid, QueuedPegIn>`
    pub queued_pegins: Arc<RwLock<BTreeMap<Txid, QueuedPegIn>>>,

    /// Bitcoin wallet for peg-out UTXO management
    pub bitcoin_wallet: Arc<RwLock<BitcoinWallet>>,

    /// Signature collector for multi-sig peg-outs
    pub bitcoin_signature_collector: Arc<RwLock<BitcoinSignatureCollector>>,

    /// Bitcoin signer for validators (None for non-validators)
    pub maybe_bitcoin_signer: Option<Arc<RwLock<BitcoinSigner>>>,

    // ========================================================================
    // Peg-In Deduplication (Doc 16 Layer 2: Producer filter)
    // ========================================================================

    /// Processed peg-in txids - prevents re-processing same deposit
    /// Uses Arc<RwLock> for async handler compatibility
    pub processed_pegin_txids: Arc<RwLock<HashSet<Txid>>>,

    // ========================================================================
    // Tendermint Runtime State
    // ========================================================================

    /// Tendermint-specific runtime state (separate from ChainParams static config)
    /// Includes emergency pause flags that can be toggled at runtime
    /// None if Tendermint mode is disabled
    pub tendermint_runtime: Option<Arc<RwLock<TendermintRuntimeState>>>,

    // ========================================================================
    // Cumulative Difficulty Tracking (Gap FC-2)
    // ========================================================================

    /// Cumulative difficulty of the current canonical chain tip.
    ///
    /// This is cached in memory for fast fork choice decisions.
    /// It's updated on every block import and reorg.
    ///
    /// Uses Arc<RwLock<>> to allow updates from cloned ChainActor instances
    /// (required by the async handler pattern).
    ///
    /// Formula: cumulative_difficulty = parent_cumulative_difficulty + block_difficulty
    pub cumulative_difficulty: Arc<RwLock<u128>>,

    /// Cache of recent cumulative difficulties for fork choice.
    ///
    /// Stores the last N heights' cumulative difficulties to avoid
    /// frequent database lookups during fork detection.
    ///
    /// Key: height, Value: cumulative_difficulty
    pub difficulty_cache: Arc<RwLock<LruCache<u64, u128>>>,
}

/// Runtime state for Tendermint consensus
///
/// Separate from TendermintState (consensus state machine) and ChainParams (static config).
/// Contains emergency pause flags and runtime state that can be modified during operation.
#[derive(Debug, Clone, Default)]
pub struct TendermintRuntimeState {
    /// Emergency: Chain is paused (no new blocks)
    pub chain_paused: bool,

    /// Emergency: Peg-ins are paused (new deposits rejected)
    pub pegins_paused: bool,

    /// Emergency: Peg-outs are paused (withdrawals halted)
    pub pegouts_paused: bool,

    /// Cached last commit for next block's last_commit field
    pub pending_commit: Option<Commit>,
}

impl std::fmt::Debug for ChainState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainState")
            .field("head", &"<Arc<RwLock<Option<BlockRef>>>>")
            .field("sync_status", &"<Arc<RwLock<SyncStatus>>>")
            .field("queued_pow", &"<Arc<RwLock<Option<AuxPowHeader>>>>")
            .field("blocks_without_pow", &"<Arc<RwLock<u64>>>")
            .field("last_block_time", &"<Arc<RwLock<Option<SystemTime>>>>")
            .field("max_blocks_without_pow", &self.max_blocks_without_pow)
            .field("mining_contexts", &"<BTreeMap<BlockHash, MiningContext>>")
            .field("federation", &self.federation)
            .field("queued_pegins", &"<Arc<RwLock<BTreeMap<Txid, QueuedPegIn>>>>")
            .field("processed_pegin_txids", &"<Arc<RwLock<HashSet<Txid>>>>")
            .field("tendermint_runtime", &"<Option<Arc<RwLock<TendermintRuntimeState>>>>")
            .field("is_validator", &self.is_validator)
            .field("retarget_params", &self.retarget_params)
            .field("block_hash_cache", &"<Arc<RwLock<BlockHashCache>>>")
            .field("cumulative_difficulty", &"<Arc<RwLock<u128>>>")
            .field("difficulty_cache", &"<LruCache<u64, u128>>")
            .field("bridge", &"<Bridge>")
            .field("bitcoin_wallet", &"<BitcoinWallet>")
            .field(
                "bitcoin_signature_collector",
                &"<BitcoinSignatureCollector>",
            )
            .field(
                "maybe_bitcoin_signer",
                &format_args!("<Option<BitcoinSigner>>"),
            )
            .finish()
    }
}

impl ChainState {
    /// Create new chain state with Arc-wrapped components for async handler compatibility
    ///
    /// Note: Tendermint-only consensus - Aura is no longer required.
    /// Block finality is proven via last_commit with 2/3+ validator signatures.
    pub fn new(
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
            // Read-only components
            federation,
            is_validator,
            retarget_params,
            max_blocks_without_pow,

            // Block hash cache for AuxPoW (wrapped for async access)
            block_hash_cache: Arc::new(RwLock::new(BlockHashCache::new(None))),

            // Mutable state wrapped in Arc<RwLock>
            head: Arc::new(RwLock::new(head)),
            sync_status: Arc::new(RwLock::new(SyncStatus::Synced)),
            queued_pow: Arc::new(RwLock::new(None)),
            blocks_without_pow: Arc::new(RwLock::new(0)),
            last_block_time: Arc::new(RwLock::new(None)),

            // Mining contexts
            mining_contexts: Arc::new(RwLock::new(BTreeMap::new())),

            // Bridge components
            bridge: Arc::new(RwLock::new(bridge)),
            queued_pegins: Arc::new(RwLock::new(BTreeMap::new())),
            bitcoin_wallet: Arc::new(RwLock::new(bitcoin_wallet)),
            bitcoin_signature_collector: Arc::new(RwLock::new(bitcoin_signature_collector)),
            maybe_bitcoin_signer: maybe_bitcoin_signer.map(|signer| Arc::new(RwLock::new(signer))),

            // Peg-in deduplication (Doc 16 Layer 2)
            processed_pegin_txids: Arc::new(RwLock::new(HashSet::new())),

            // Tendermint runtime state (None until Tendermint mode enabled)
            tendermint_runtime: None,

            // Cumulative difficulty tracking (Gap FC-2)
            cumulative_difficulty: Arc::new(RwLock::new(0)),
            difficulty_cache: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(DEFAULT_DIFFICULTY_CACHE_SIZE).unwrap(),
            ))),
        }
    }

    /// Enable Tendermint runtime state
    pub fn enable_tendermint_runtime(&mut self) {
        self.tendermint_runtime = Some(Arc::new(RwLock::new(TendermintRuntimeState::default())));
    }

    // ========================================================================
    // Head/Height Methods (async due to Arc<RwLock>)
    // ========================================================================

    /// Update chain head (async for Arc<RwLock> compatibility)
    pub async fn update_head(&self, head: BlockRef) {
        *self.head.write().await = Some(head);
        *self.last_block_time.write().await = Some(SystemTime::now());
    }

    /// Get current chain head (async for Arc<RwLock> compatibility)
    pub async fn get_head(&self) -> Option<BlockRef> {
        self.head.read().await.clone()
    }

    /// Get current height (async for Arc<RwLock> compatibility)
    pub async fn get_height(&self) -> u64 {
        self.head.read().await.as_ref().map(|h| h.number).unwrap_or(0)
    }

    /// Get current height (blocking version for sync contexts)
    ///
    /// Uses try_read to avoid blocking. Returns 0 if lock is held.
    /// Prefer the async version when possible.
    pub fn get_height_blocking(&self) -> u64 {
        self.head
            .try_read()
            .map(|guard| guard.as_ref().map(|h| h.number).unwrap_or(0))
            .unwrap_or(0)
    }

    /// Get head hash (async for Arc<RwLock> compatibility)
    pub async fn get_head_hash(&self) -> Option<lighthouse_wrapper::types::Hash256> {
        self.head.read().await.as_ref().map(|h| h.hash)
    }

    // ========================================================================
    // Sync Status Methods (async due to Arc<RwLock>)
    // ========================================================================

    /// Check if chain is synced (async for Arc<RwLock> compatibility)
    pub async fn is_synced(&self) -> bool {
        matches!(*self.sync_status.read().await, SyncStatus::Synced)
    }

    /// Check if chain is synced (blocking version for sync contexts)
    pub fn is_synced_blocking(&self) -> bool {
        self.sync_status
            .try_read()
            .map(|guard| matches!(*guard, SyncStatus::Synced))
            .unwrap_or(false)
    }

    /// Update sync status (async for Arc<RwLock> compatibility)
    pub async fn set_sync_status(&self, status: SyncStatus) {
        *self.sync_status.write().await = status;
    }

    /// Get current sync status (async for Arc<RwLock> compatibility)
    pub async fn get_sync_status(&self) -> SyncStatus {
        self.sync_status.read().await.clone()
    }

    // ========================================================================
    // AuxPoW Methods (async due to Arc<RwLock>)
    // ========================================================================

    /// Check if we need AuxPoW (async for Arc<RwLock> compatibility)
    pub async fn needs_auxpow(&self) -> bool {
        *self.blocks_without_pow.read().await >= self.max_blocks_without_pow
    }

    /// Check if we need AuxPoW (blocking version for sync contexts)
    pub fn needs_auxpow_blocking(&self) -> bool {
        self.blocks_without_pow
            .try_read()
            .map(|guard| *guard >= self.max_blocks_without_pow)
            .unwrap_or(false)
    }

    /// Increment blocks without AuxPoW (async for Arc<RwLock> compatibility)
    pub async fn increment_blocks_without_pow(&self) {
        *self.blocks_without_pow.write().await += 1;
    }

    /// Reset blocks without AuxPoW (async for Arc<RwLock> compatibility)
    pub async fn reset_blocks_without_pow(&self) {
        *self.blocks_without_pow.write().await = 0;
    }

    /// Set queued AuxPoW (async for Arc<RwLock> compatibility)
    pub async fn set_queued_pow(&self, auxpow: Option<AuxPowHeader>) {
        *self.queued_pow.write().await = auxpow;
    }

    /// Get queued AuxPoW (async for Arc<RwLock> compatibility)
    pub async fn get_queued_pow(&self) -> Option<AuxPowHeader> {
        self.queued_pow.read().await.clone()
    }

    /// Check if AuxPoW is queued (blocking version for sync contexts)
    pub fn has_queued_pow_blocking(&self) -> bool {
        self.queued_pow
            .try_read()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    /// Take queued AuxPoW (consumes it for block inclusion)
    pub async fn take_queued_pow(&self) -> Option<AuxPowHeader> {
        self.queued_pow.write().await.take()
    }

    /// Get blocks without AuxPoW count (blocking version for sync contexts)
    pub fn get_blocks_without_pow_blocking(&self) -> u64 {
        self.blocks_without_pow
            .try_read()
            .map(|guard| *guard)
            .unwrap_or(0)
    }

    // ========================================================================
    // Block Hash Cache Methods (for AuxPoW aggregate calculation)
    // ========================================================================

    /// Add a block hash to the AuxPoW cache
    ///
    /// Called when a block is committed via Tendermint consensus.
    /// The hash is used for aggregate calculation in createauxblock.
    pub async fn add_block_to_auxpow_cache(&self, hash: bitcoin::BlockHash) {
        let mut cache = self.block_hash_cache.write().await;
        cache.add(hash);
        tracing::debug!(
            hash = %hash,
            cache_size = cache.len(),
            "Added block hash to AuxPoW cache"
        );
    }

    /// Get all block hashes from the AuxPoW cache
    ///
    /// Returns hashes of unfinalized blocks for aggregate calculation.
    pub async fn get_auxpow_cache_hashes(&self) -> Vec<bitcoin::BlockHash> {
        self.block_hash_cache.read().await.get()
    }

    /// Clear the AuxPoW cache through a specific hash
    ///
    /// Called after successful submitauxblock to remove finalized blocks.
    /// Returns Ok if the hash was found and cleared, Err if not found.
    pub async fn clear_auxpow_cache_through(&self, hash: bitcoin::BlockHash) -> eyre::Result<()> {
        let mut cache = self.block_hash_cache.write().await;
        let result = cache.reset_with(hash);
        if result.is_ok() {
            tracing::debug!(
                hash = %hash,
                remaining = cache.len(),
                "Cleared AuxPoW cache through hash"
            );
        }
        result
    }

    /// Check if the AuxPoW cache is empty
    pub async fn is_auxpow_cache_empty(&self) -> bool {
        self.block_hash_cache.read().await.is_empty()
    }

    /// Get the number of hashes in the AuxPoW cache
    pub async fn auxpow_cache_len(&self) -> usize {
        self.block_hash_cache.read().await.len()
    }

    // ========================================================================
    // Peg-in Methods (Doc 16: Producer Filter and Deduplication)
    // ========================================================================

    /// Check if peg-in already processed (Doc 16 Layer 2: Producer filter)
    pub async fn is_pegin_processed(&self, txid: &Txid) -> bool {
        self.processed_pegin_txids.read().await.contains(txid)
    }

    /// Mark peg-in as processed (Doc 16 Layer 2: Producer filter)
    pub async fn mark_pegin_processed(&self, txid: Txid) {
        self.processed_pegin_txids.write().await.insert(txid);
    }

    /// Queue a peg-in with deduplication (Doc 16 Layer 1 + Layer 2)
    ///
    /// Takes QueuedPegIn which includes fee_recipient for miner compensation.
    /// Returns false if already queued or processed.
    pub async fn queue_pegin(&self, queued_pegin: QueuedPegIn) -> bool {
        let txid = queued_pegin.info.txid;

        // Layer 2: Check if already processed
        if self.is_pegin_processed(&txid).await {
            return false;
        }

        // Layer 1: Check if already queued
        let mut queued = self.queued_pegins.write().await;
        if queued.contains_key(&txid) {
            return false;
        }

        queued.insert(txid, queued_pegin);
        true
    }

    /// Drain all queued peg-ins for block production
    ///
    /// Returns Vec<QueuedPegIn> and clears the queue
    pub async fn drain_queued_pegins(&self) -> Vec<QueuedPegIn> {
        let mut queued = self.queued_pegins.write().await;
        std::mem::take(&mut *queued).into_values().collect()
    }

    /// Remove a specific queued peg-in by txid
    pub async fn remove_queued_pegin(&self, txid: &Txid) -> Option<QueuedPegIn> {
        self.queued_pegins.write().await.remove(txid)
    }

    /// Get count of queued peg-ins
    pub async fn queued_pegins_count(&self) -> usize {
        self.queued_pegins.read().await.len()
    }

    /// Store mining context for submitted work validation (Priority 3)
    pub async fn store_mining_context(&self, aggregate_hash: BlockHash, context: MiningContext) {
        self.mining_contexts
            .write()
            .await
            .insert(aggregate_hash, context);
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

    // ========================================================================
    // Cumulative Difficulty Management (Gap FC-2)
    // ========================================================================

    /// Update cumulative difficulty when a new block is imported.
    ///
    /// # Arguments
    /// * `block_height` - Height of the newly imported block
    /// * `block_difficulty` - Difficulty of the newly imported block
    /// * `parent_cumulative_difficulty` - Cumulative difficulty of the parent block
    ///
    /// # Returns
    /// The new cumulative difficulty for this block
    pub async fn update_difficulty_on_import(
        &self,
        block_height: u64,
        block_difficulty: u128,
        parent_cumulative_difficulty: u128,
    ) -> u128 {
        // Calculate new cumulative difficulty
        let new_cumulative = parent_cumulative_difficulty.saturating_add(block_difficulty);

        // Update state (Arc<RwLock<>> allows update from &self)
        *self.cumulative_difficulty.write().await = new_cumulative;

        // Cache the difficulty for this height
        self.difficulty_cache
            .write()
            .await
            .put(block_height, new_cumulative);

        tracing::debug!(
            height = block_height,
            block_difficulty = block_difficulty,
            parent_cumulative = parent_cumulative_difficulty,
            new_cumulative = new_cumulative,
            "Updated cumulative difficulty"
        );

        new_cumulative
    }

    /// Get cumulative difficulty at a specific height from cache.
    ///
    /// Returns None if not in cache - caller should fall back to database.
    pub async fn get_cached_difficulty(&self, height: u64) -> Option<u128> {
        self.difficulty_cache.write().await.get(&height).copied()
    }

    /// Cache a cumulative difficulty value.
    ///
    /// Used when loading from database to populate cache.
    pub async fn cache_difficulty(&self, height: u64, cumulative_difficulty: u128) {
        self.difficulty_cache
            .write()
            .await
            .put(height, cumulative_difficulty);
    }

    /// Get current tip's cumulative difficulty (async version).
    pub async fn get_cumulative_difficulty(&self) -> u128 {
        *self.cumulative_difficulty.read().await
    }

    /// Get current tip's cumulative difficulty (blocking version for sync contexts).
    ///
    /// Uses try_read to avoid blocking. Returns 0 if lock is held.
    /// Prefer the async version when possible.
    pub fn get_cumulative_difficulty_blocking(&self) -> u128 {
        self.cumulative_difficulty
            .try_read()
            .map(|guard| *guard)
            .unwrap_or(0)
    }

    /// Set cumulative difficulty (used during initialization or reorg).
    pub async fn set_cumulative_difficulty(&self, difficulty: u128) {
        *self.cumulative_difficulty.write().await = difficulty;
    }

    /// Rollback cumulative difficulty during reorg.
    ///
    /// Invalidates cache entries above the rollback height and
    /// sets the cumulative difficulty to the value at rollback height.
    pub async fn rollback_difficulty(&self, rollback_height: u64, new_cumulative: u128) {
        *self.cumulative_difficulty.write().await = new_cumulative;

        // Remove all cache entries above the rollback height
        let mut cache = self.difficulty_cache.write().await;

        // LruCache doesn't have retain, so we need to rebuild
        // Get all entries below or at rollback height
        let to_keep: Vec<(u64, u128)> = cache
            .iter()
            .filter(|(h, _)| **h <= rollback_height)
            .map(|(h, d)| (*h, *d))
            .collect();

        cache.clear();
        for (h, d) in to_keep {
            cache.put(h, d);
        }

        tracing::info!(
            rollback_height = rollback_height,
            new_cumulative = new_cumulative,
            "Rolled back cumulative difficulty"
        );
    }

    /// Clear the difficulty cache (used during major state changes).
    pub async fn clear_difficulty_cache(&self) {
        self.difficulty_cache.write().await.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test fixture for cumulative difficulty testing
    /// Uses only the Arc<RwLock<>> components needed for difficulty tests
    struct DifficultyTestFixture {
        cumulative_difficulty: Arc<RwLock<u128>>,
        difficulty_cache: Arc<RwLock<LruCache<u64, u128>>>,
    }

    impl DifficultyTestFixture {
        fn new() -> Self {
            Self {
                cumulative_difficulty: Arc::new(RwLock::new(0)),
                difficulty_cache: Arc::new(RwLock::new(LruCache::new(
                    NonZeroUsize::new(DEFAULT_DIFFICULTY_CACHE_SIZE).unwrap(),
                ))),
            }
        }

        async fn get_cumulative_difficulty(&self) -> u128 {
            *self.cumulative_difficulty.read().await
        }

        async fn set_cumulative_difficulty(&self, difficulty: u128) {
            *self.cumulative_difficulty.write().await = difficulty;
        }

        fn get_cumulative_difficulty_blocking(&self) -> u128 {
            self.cumulative_difficulty
                .try_read()
                .map(|guard| *guard)
                .unwrap_or(0)
        }

        async fn cache_difficulty(&self, height: u64, cumulative_difficulty: u128) {
            self.difficulty_cache
                .write()
                .await
                .put(height, cumulative_difficulty);
        }

        async fn get_cached_difficulty(&self, height: u64) -> Option<u128> {
            self.difficulty_cache.write().await.get(&height).copied()
        }

        async fn clear_difficulty_cache(&self) {
            self.difficulty_cache.write().await.clear();
        }

        async fn update_difficulty_on_import(
            &self,
            block_height: u64,
            block_difficulty: u128,
            parent_cumulative_difficulty: u128,
        ) -> u128 {
            let new_cumulative = parent_cumulative_difficulty.saturating_add(block_difficulty);
            *self.cumulative_difficulty.write().await = new_cumulative;
            self.difficulty_cache
                .write()
                .await
                .put(block_height, new_cumulative);
            new_cumulative
        }

        async fn rollback_difficulty(&self, rollback_height: u64, new_cumulative: u128) {
            *self.cumulative_difficulty.write().await = new_cumulative;

            let mut cache = self.difficulty_cache.write().await;
            let to_keep: Vec<(u64, u128)> = cache
                .iter()
                .filter(|(h, _)| **h <= rollback_height)
                .map(|(h, d)| (*h, *d))
                .collect();

            cache.clear();
            for (h, d) in to_keep {
                cache.put(h, d);
            }
        }
    }

    impl Clone for DifficultyTestFixture {
        fn clone(&self) -> Self {
            Self {
                cumulative_difficulty: Arc::clone(&self.cumulative_difficulty),
                difficulty_cache: Arc::clone(&self.difficulty_cache),
            }
        }
    }

    #[tokio::test]
    async fn test_cumulative_difficulty_initial_value() {
        let fixture = DifficultyTestFixture::new();
        assert_eq!(fixture.get_cumulative_difficulty().await, 0);
    }

    #[tokio::test]
    async fn test_set_cumulative_difficulty() {
        let fixture = DifficultyTestFixture::new();

        fixture.set_cumulative_difficulty(1_000_000).await;
        assert_eq!(fixture.get_cumulative_difficulty().await, 1_000_000);

        fixture.set_cumulative_difficulty(2_000_000).await;
        assert_eq!(fixture.get_cumulative_difficulty().await, 2_000_000);
    }

    #[tokio::test]
    async fn test_update_difficulty_on_import() {
        let fixture = DifficultyTestFixture::new();

        // Import first block with difficulty 100
        let new_cumulative = fixture.update_difficulty_on_import(1, 100, 0).await;
        assert_eq!(new_cumulative, 100);
        assert_eq!(fixture.get_cumulative_difficulty().await, 100);

        // Import second block with difficulty 200
        let new_cumulative = fixture.update_difficulty_on_import(2, 200, 100).await;
        assert_eq!(new_cumulative, 300);
        assert_eq!(fixture.get_cumulative_difficulty().await, 300);

        // Verify cache was populated
        assert_eq!(fixture.get_cached_difficulty(1).await, Some(100));
        assert_eq!(fixture.get_cached_difficulty(2).await, Some(300));
    }

    #[tokio::test]
    async fn test_difficulty_cache() {
        let fixture = DifficultyTestFixture::new();

        // Cache some difficulties
        fixture.cache_difficulty(10, 1000).await;
        fixture.cache_difficulty(20, 2000).await;
        fixture.cache_difficulty(30, 3000).await;

        // Verify retrieval
        assert_eq!(fixture.get_cached_difficulty(10).await, Some(1000));
        assert_eq!(fixture.get_cached_difficulty(20).await, Some(2000));
        assert_eq!(fixture.get_cached_difficulty(30).await, Some(3000));
        assert_eq!(fixture.get_cached_difficulty(40).await, None);
    }

    #[tokio::test]
    async fn test_rollback_difficulty() {
        let fixture = DifficultyTestFixture::new();

        // Set up initial state with cached difficulties
        fixture.set_cumulative_difficulty(5000).await;
        fixture.cache_difficulty(1, 1000).await;
        fixture.cache_difficulty(2, 2000).await;
        fixture.cache_difficulty(3, 3000).await;
        fixture.cache_difficulty(4, 4000).await;
        fixture.cache_difficulty(5, 5000).await;

        // Rollback to height 3
        fixture.rollback_difficulty(3, 3000).await;

        // Verify cumulative difficulty was updated
        assert_eq!(fixture.get_cumulative_difficulty().await, 3000);

        // Verify cache entries above height 3 were removed
        assert_eq!(fixture.get_cached_difficulty(1).await, Some(1000));
        assert_eq!(fixture.get_cached_difficulty(2).await, Some(2000));
        assert_eq!(fixture.get_cached_difficulty(3).await, Some(3000));
        assert_eq!(fixture.get_cached_difficulty(4).await, None); // Removed
        assert_eq!(fixture.get_cached_difficulty(5).await, None); // Removed
    }

    #[tokio::test]
    async fn test_clear_difficulty_cache() {
        let fixture = DifficultyTestFixture::new();

        // Populate cache
        fixture.cache_difficulty(1, 100).await;
        fixture.cache_difficulty(2, 200).await;

        // Clear cache
        fixture.clear_difficulty_cache().await;

        // Verify cache is empty
        assert_eq!(fixture.get_cached_difficulty(1).await, None);
        assert_eq!(fixture.get_cached_difficulty(2).await, None);
    }

    #[tokio::test]
    async fn test_cumulative_difficulty_blocking_getter() {
        let fixture = DifficultyTestFixture::new();

        fixture.set_cumulative_difficulty(12345).await;

        // Test blocking getter (should return same value)
        assert_eq!(fixture.get_cumulative_difficulty_blocking(), 12345);
    }

    #[tokio::test]
    async fn test_cumulative_difficulty_clone_visibility() {
        // This test verifies that updates to cumulative_difficulty
        // are visible across cloned instances (Arc<RwLock<>> pattern)
        let fixture1 = DifficultyTestFixture::new();
        let fixture2 = fixture1.clone();

        // Update via fixture1
        fixture1.set_cumulative_difficulty(999).await;

        // Should be visible via fixture2 (same Arc)
        assert_eq!(fixture2.get_cumulative_difficulty().await, 999);

        // Update via fixture2
        fixture2.set_cumulative_difficulty(888).await;

        // Should be visible via fixture1
        assert_eq!(fixture1.get_cumulative_difficulty().await, 888);
    }

    #[tokio::test]
    async fn test_saturating_add_overflow_protection() {
        let fixture = DifficultyTestFixture::new();

        // Test that saturating_add prevents overflow
        let result = fixture
            .update_difficulty_on_import(1, u128::MAX, u128::MAX)
            .await;
        assert_eq!(result, u128::MAX); // Should saturate, not overflow
    }
}
