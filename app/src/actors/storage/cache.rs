//! Multi-level cache implementation for Storage Actor
//!
//! This module provides efficient caching for frequently accessed blockchain data
//! including blocks, state, and other storage operations.

use crate::types::*;
use lru::LruCache;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::*;

/// Multi-level cache for storage operations
#[derive(Debug)]
pub struct StorageCache {
    /// Block cache (hash -> block)
    block_cache: Arc<RwLock<LruCache<BlockHash, CachedBlock>>>,
    /// State cache (key -> value with TTL)
    state_cache: Arc<RwLock<LruCache<StateKey, CachedStateValue>>>,
    /// Receipt cache for transaction receipts
    receipt_cache: Arc<RwLock<LruCache<H256, CachedReceipt>>>,
    /// Cache configuration
    config: CacheConfig,
    /// Cache statistics
    stats: Arc<RwLock<CacheStats>>,
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of blocks to cache
    pub max_blocks: usize,
    /// Maximum number of state entries to cache
    pub max_state_entries: usize,
    /// Maximum number of receipts to cache
    pub max_receipts: usize,
    /// TTL for state cache entries
    pub state_ttl: Duration,
    /// TTL for receipt cache entries
    pub receipt_ttl: Duration,
    /// Enable cache warming on startup
    pub enable_warming: bool,
}

/// Cached block with metadata
#[derive(Debug, Clone)]
pub struct CachedBlock {
    pub block: ConsensusBlock,
    pub cached_at: Instant,
    pub access_count: u64,
    pub size_bytes: usize,
}

/// Cached state value with TTL
#[derive(Debug, Clone)]
pub struct CachedStateValue {
    pub value: Vec<u8>,
    pub cached_at: Instant,
    pub expires_at: Instant,
    pub access_count: u64,
}

/// Cached transaction receipt
#[derive(Debug, Clone)]
pub struct CachedReceipt {
    pub receipt: TransactionReceipt,
    pub cached_at: Instant,
    pub expires_at: Instant,
    pub access_count: u64,
}

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Block cache statistics
    pub block_hits: u64,
    pub block_misses: u64,
    pub block_evictions: u64,
    
    /// State cache statistics
    pub state_hits: u64,
    pub state_misses: u64,
    pub state_evictions: u64,
    pub state_expirations: u64,
    
    /// Receipt cache statistics
    pub receipt_hits: u64,
    pub receipt_misses: u64,
    pub receipt_evictions: u64,
    pub receipt_expirations: u64,
    
    /// Memory usage
    pub total_memory_bytes: u64,
    pub block_cache_bytes: u64,
    pub state_cache_bytes: u64,
    pub receipt_cache_bytes: u64,
}

/// Custom state key type that implements required traits
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateKey(Vec<u8>);

impl Hash for StateKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl From<Vec<u8>> for StateKey {
    fn from(bytes: Vec<u8>) -> Self {
        StateKey(bytes)
    }
}

impl AsRef<[u8]> for StateKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl StorageCache {
    /// Create a new storage cache with the given configuration
    pub fn new(config: CacheConfig) -> Self {
        info!("Initializing storage cache with {} blocks, {} state entries, {} receipts",
            config.max_blocks, config.max_state_entries, config.max_receipts);
        
        let block_cache = Arc::new(RwLock::new(
            LruCache::new(NonZeroUsize::new(config.max_blocks).unwrap())
        ));
        
        let state_cache = Arc::new(RwLock::new(
            LruCache::new(NonZeroUsize::new(config.max_state_entries).unwrap())
        ));
        
        let receipt_cache = Arc::new(RwLock::new(
            LruCache::new(NonZeroUsize::new(config.max_receipts).unwrap())
        ));
        
        let stats = Arc::new(RwLock::new(CacheStats::default()));
        
        Self {
            block_cache,
            state_cache,
            receipt_cache,
            config,
            stats,
        }
    }
    
    /// Get a block from cache
    pub async fn get_block(&self, block_hash: &BlockHash) -> Option<ConsensusBlock> {
        let mut cache = self.block_cache.write().await;
        let mut stats = self.stats.write().await;
        
        if let Some(cached_block) = cache.get_mut(block_hash) {
            cached_block.access_count += 1;
            stats.block_hits += 1;
            debug!("Block cache hit: {}", block_hash);
            Some(cached_block.block.clone())
        } else {
            stats.block_misses += 1;
            debug!("Block cache miss: {}", block_hash);
            None
        }
    }
    
    /// Put a block in cache
    pub async fn put_block(&self, block_hash: BlockHash, block: ConsensusBlock) {
        let mut cache = self.block_cache.write().await;
        let mut stats = self.stats.write().await;
        
        let size_bytes = self.estimate_block_size(&block);
        let cached_block = CachedBlock {
            block,
            cached_at: Instant::now(),
            access_count: 1,
            size_bytes,
        };
        
        if cache.put(block_hash, cached_block).is_some() {
            stats.block_evictions += 1;
        }
        
        stats.block_cache_bytes = self.calculate_block_cache_size(&cache);
        debug!("Cached block: {} (size: {} bytes)", block_hash, size_bytes);
    }
    
    /// Get state value from cache
    pub async fn get_state(&self, key: &[u8]) -> Option<Vec<u8>> {
        let mut cache = self.state_cache.write().await;
        let mut stats = self.stats.write().await;
        
        let state_key = StateKey(key.to_vec());
        
        if let Some(cached_value) = cache.get_mut(&state_key) {
            // Check if entry has expired
            if cached_value.expires_at <= Instant::now() {
                cache.pop(&state_key);
                stats.state_expirations += 1;
                debug!("State cache entry expired: {:?}", hex::encode(&key[..std::cmp::min(key.len(), 8)]));
                return None;
            }
            
            cached_value.access_count += 1;
            stats.state_hits += 1;
            debug!("State cache hit: {:?}", hex::encode(&key[..std::cmp::min(key.len(), 8)]));
            Some(cached_value.value.clone())
        } else {
            stats.state_misses += 1;
            debug!("State cache miss: {:?}", hex::encode(&key[..std::cmp::min(key.len(), 8)]));
            None
        }
    }
    
    /// Put state value in cache
    pub async fn put_state(&self, key: Vec<u8>, value: Vec<u8>) {
        let mut cache = self.state_cache.write().await;
        let mut stats = self.stats.write().await;
        
        let state_key = StateKey(key);
        let cached_value = CachedStateValue {
            value,
            cached_at: Instant::now(),
            expires_at: Instant::now() + self.config.state_ttl,
            access_count: 1,
        };
        
        if cache.put(state_key, cached_value).is_some() {
            stats.state_evictions += 1;
        }
        
        stats.state_cache_bytes = self.calculate_state_cache_size(&cache);
        debug!("Cached state value (size: {} bytes)", stats.state_cache_bytes);
    }
    
    /// Get transaction receipt from cache
    pub async fn get_receipt(&self, tx_hash: &H256) -> Option<TransactionReceipt> {
        let mut cache = self.receipt_cache.write().await;
        let mut stats = self.stats.write().await;
        
        if let Some(cached_receipt) = cache.get_mut(tx_hash) {
            // Check if entry has expired
            if cached_receipt.expires_at <= Instant::now() {
                cache.pop(tx_hash);
                stats.receipt_expirations += 1;
                debug!("Receipt cache entry expired: {}", tx_hash);
                return None;
            }
            
            cached_receipt.access_count += 1;
            stats.receipt_hits += 1;
            debug!("Receipt cache hit: {}", tx_hash);
            Some(cached_receipt.receipt.clone())
        } else {
            stats.receipt_misses += 1;
            debug!("Receipt cache miss: {}", tx_hash);
            None
        }
    }
    
    /// Put transaction receipt in cache
    pub async fn put_receipt(&self, tx_hash: H256, receipt: TransactionReceipt) {
        let mut cache = self.receipt_cache.write().await;
        let mut stats = self.stats.write().await;
        
        let cached_receipt = CachedReceipt {
            receipt,
            cached_at: Instant::now(),
            expires_at: Instant::now() + self.config.receipt_ttl,
            access_count: 1,
        };
        
        if cache.put(tx_hash, cached_receipt).is_some() {
            stats.receipt_evictions += 1;
        }
        
        stats.receipt_cache_bytes = self.calculate_receipt_cache_size(&cache);
        debug!("Cached receipt: {} (total size: {} bytes)", tx_hash, stats.receipt_cache_bytes);
    }
    
    /// Clear expired entries from all caches
    pub async fn cleanup_expired(&self) {
        debug!("Starting cache cleanup of expired entries");
        
        let mut stats = self.stats.write().await;
        let now = Instant::now();
        
        // Clean up state cache
        {
            let mut state_cache = self.state_cache.write().await;
            let mut expired_keys = Vec::new();
            
            // Collect expired keys (we can't modify while iterating)
            for (key, value) in state_cache.iter() {
                if value.expires_at <= now {
                    expired_keys.push(key.clone());
                }
            }
            
            // Remove expired keys
            for key in expired_keys {
                state_cache.pop(&key);
                stats.state_expirations += 1;
            }
            
            stats.state_cache_bytes = self.calculate_state_cache_size(&state_cache);
        }
        
        // Clean up receipt cache
        {
            let mut receipt_cache = self.receipt_cache.write().await;
            let mut expired_keys = Vec::new();
            
            // Collect expired keys
            for (key, value) in receipt_cache.iter() {
                if value.expires_at <= now {
                    expired_keys.push(*key);
                }
            }
            
            // Remove expired keys
            for key in expired_keys {
                receipt_cache.pop(&key);
                stats.receipt_expirations += 1;
            }
            
            stats.receipt_cache_bytes = self.calculate_receipt_cache_size(&receipt_cache);
        }
        
        // Update total memory usage
        stats.total_memory_bytes = stats.block_cache_bytes + stats.state_cache_bytes + stats.receipt_cache_bytes;
        
        debug!("Cache cleanup completed. Expired {} state entries, {} receipt entries", 
            stats.state_expirations, stats.receipt_expirations);
    }
    
    /// Get cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        let mut stats = self.stats.write().await;
        
        // Update memory usage statistics
        {
            let block_cache = self.block_cache.read().await;
            stats.block_cache_bytes = self.calculate_block_cache_size(&block_cache);
        }
        
        {
            let state_cache = self.state_cache.read().await;
            stats.state_cache_bytes = self.calculate_state_cache_size(&state_cache);
        }
        
        {
            let receipt_cache = self.receipt_cache.read().await;
            stats.receipt_cache_bytes = self.calculate_receipt_cache_size(&receipt_cache);
        }
        
        stats.total_memory_bytes = stats.block_cache_bytes + stats.state_cache_bytes + stats.receipt_cache_bytes;
        
        stats.clone()
    }
    
    /// Calculate hit rates
    pub async fn get_hit_rates(&self) -> HashMap<String, f64> {
        let stats = self.stats.read().await;
        let mut hit_rates = HashMap::new();
        
        let block_total = stats.block_hits + stats.block_misses;
        let state_total = stats.state_hits + stats.state_misses;
        let receipt_total = stats.receipt_hits + stats.receipt_misses;
        
        hit_rates.insert("block".to_string(), if block_total > 0 {
            stats.block_hits as f64 / block_total as f64
        } else { 0.0 });
        
        hit_rates.insert("state".to_string(), if state_total > 0 {
            stats.state_hits as f64 / state_total as f64
        } else { 0.0 });
        
        hit_rates.insert("receipt".to_string(), if receipt_total > 0 {
            stats.receipt_hits as f64 / receipt_total as f64
        } else { 0.0 });
        
        let total_hits = stats.block_hits + stats.state_hits + stats.receipt_hits;
        let total_requests = block_total + state_total + receipt_total;
        
        hit_rates.insert("overall".to_string(), if total_requests > 0 {
            total_hits as f64 / total_requests as f64
        } else { 0.0 });
        
        hit_rates
    }
    
    /// Clear all caches
    pub async fn clear_all(&self) {
        info!("Clearing all caches");
        
        self.block_cache.write().await.clear();
        self.state_cache.write().await.clear();
        self.receipt_cache.write().await.clear();
        
        let mut stats = self.stats.write().await;
        *stats = CacheStats::default();
        
        info!("All caches cleared");
    }
    
    /// Warm up cache with frequently accessed data
    pub async fn warm_cache(&self, recent_blocks: Vec<ConsensusBlock>) {
        if !self.config.enable_warming {
            return;
        }
        
        info!("Warming cache with {} recent blocks", recent_blocks.len());
        
        for block in recent_blocks {
            let block_hash = block.hash();
            self.put_block(block_hash, block).await;
        }
        
        info!("Cache warming completed");
    }
    
    /// Estimate block size in bytes
    fn estimate_block_size(&self, block: &ConsensusBlock) -> usize {
        // Rough estimate: base size + transaction data
        let base_size = 256; // Headers, metadata, etc.
        let tx_data_size = block.execution_payload.transactions.iter()
            .map(|tx| tx.len())
            .sum::<usize>();
        
        base_size + tx_data_size
    }
    
    /// Calculate total size of block cache
    fn calculate_block_cache_size(&self, cache: &LruCache<BlockHash, CachedBlock>) -> u64 {
        cache.iter()
            .map(|(_, cached_block)| cached_block.size_bytes as u64)
            .sum()
    }
    
    /// Calculate total size of state cache
    fn calculate_state_cache_size(&self, cache: &LruCache<StateKey, CachedStateValue>) -> u64 {
        cache.iter()
            .map(|(key, value)| (key.0.len() + value.value.len()) as u64)
            .sum()
    }
    
    /// Calculate total size of receipt cache
    fn calculate_receipt_cache_size(&self, cache: &LruCache<H256, CachedReceipt>) -> u64 {
        cache.iter()
            .map(|(_, receipt)| {
                // Estimate receipt size
                256 + receipt.receipt.logs.len() * 128
            })
            .sum::<usize>() as u64
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_blocks: 1000,
            max_state_entries: 10000,
            max_receipts: 5000,
            state_ttl: Duration::from_secs(300), // 5 minutes
            receipt_ttl: Duration::from_secs(600), // 10 minutes
            enable_warming: true,
        }
    }
}

impl CacheStats {
    /// Calculate overall hit rate
    pub fn overall_hit_rate(&self) -> f64 {
        let total_hits = self.block_hits + self.state_hits + self.receipt_hits;
        let total_requests = self.block_hits + self.block_misses + 
                           self.state_hits + self.state_misses + 
                           self.receipt_hits + self.receipt_misses;
        
        if total_requests > 0 {
            total_hits as f64 / total_requests as f64
        } else {
            0.0
        }
    }
    
    /// Get memory usage in MB
    pub fn memory_usage_mb(&self) -> f64 {
        self.total_memory_bytes as f64 / (1024.0 * 1024.0)
    }
}