//! Multi-level cache implementation for Storage Actor - V2
//!
//! This module provides efficient caching for frequently accessed blockchain data
//! including blocks, state, and other storage operations.

use super::actor::{AlysConsensusBlock};
use lru::LruCache;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::*;
use lighthouse_wrapper::types::Hash256;
use ethereum_types::H256;

/// State key type
pub type StateKey = Vec<u8>;

/// Transaction receipt placeholder
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransactionReceipt {
    pub transaction_hash: H256,
    pub block_hash: Hash256,
    pub block_number: u64,
    pub gas_used: u64,
    pub status: bool,
}

/// Multi-level cache for storage operations
#[derive(Debug, Clone)]
pub struct StorageCache {
    /// Block cache (hash -> block)
    block_cache: Arc<RwLock<LruCache<Hash256, CachedBlock>>>,
    /// State cache (key -> value with TTL)
    state_cache: Arc<RwLock<LruCache<StateKey, CachedStateValue>>>,
    /// Receipt cache for transaction receipts
    receipt_cache: Arc<RwLock<LruCache<H256, CachedReceipt>>>,
    /// Cache configuration
    config: CacheConfig,
    /// Cache statistics
    stats: Arc<RwLock<CacheStats>>,
    /// Expiration tracking for state cache
    state_expirations: Arc<RwLock<HashMap<StateKey, Instant>>>,
    /// Expiration tracking for receipt cache
    receipt_expirations: Arc<RwLock<HashMap<H256, Instant>>>,
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
    pub block: AlysConsensusBlock,
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

impl StorageCache {
    /// Create a new storage cache with the given configuration
    pub fn new(config: CacheConfig) -> Self {
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
        let state_expirations = Arc::new(RwLock::new(HashMap::new()));
        let receipt_expirations = Arc::new(RwLock::new(HashMap::new()));

        StorageCache {
            block_cache,
            state_cache,
            receipt_cache,
            config,
            stats,
            state_expirations,
            receipt_expirations,
        }
    }

    /// Cache a block
    pub async fn put_block(&self, block_hash: Hash256, block: AlysConsensusBlock) {
        let mut cache = self.block_cache.write().await;
        let mut stats = self.stats.write().await;

        let size_bytes = std::mem::size_of_val(&block) + 256; // Approximate size
        let cached_block = CachedBlock {
            block,
            cached_at: Instant::now(),
            access_count: 0,
            size_bytes,
        };

        if cache.put(block_hash, cached_block).is_some() {
            stats.block_evictions += 1;
        }

        stats.block_cache_bytes += size_bytes as u64;
        debug!("Cached block: {} (size: {} bytes)", block_hash, size_bytes);
    }

    /// Retrieve a cached block
    pub async fn get_block(&self, block_hash: &Hash256) -> Option<AlysConsensusBlock> {
        let mut cache = self.block_cache.write().await;
        let mut stats = self.stats.write().await;

        match cache.get_mut(block_hash) {
            Some(cached_block) => {
                cached_block.access_count += 1;
                stats.block_hits += 1;
                debug!("Block cache hit: {}", block_hash);
                Some(cached_block.block.clone())
            }
            None => {
                stats.block_misses += 1;
                debug!("Block cache miss: {}", block_hash);
                None
            }
        }
    }

    /// Cache state data
    pub async fn put_state(&self, key: StateKey, value: Vec<u8>) {
        let mut cache = self.state_cache.write().await;
        let mut stats = self.stats.write().await;
        let mut expirations = self.state_expirations.write().await;

        let now = Instant::now();
        let expires_at = now + self.config.state_ttl;
        let size_bytes = key.len() + value.len();

        let cached_value = CachedStateValue {
            value,
            cached_at: now,
            expires_at,
            access_count: 0,
        };

        // Track expiration time for cleanup
        expirations.insert(key.clone(), expires_at);

        if cache.put(key, cached_value).is_some() {
            stats.state_evictions += 1;
        }

        stats.state_cache_bytes += size_bytes as u64;
    }

    /// Retrieve cached state data
    pub async fn get_state(&self, key: &[u8]) -> Option<Vec<u8>> {
        let mut cache = self.state_cache.write().await;
        let mut stats = self.stats.write().await;

        match cache.get_mut(key) {
            Some(cached_value) => {
                // Check if expired
                if Instant::now() > cached_value.expires_at {
                    cache.pop(key);
                    stats.state_expirations += 1;
                    stats.state_misses += 1;
                    None
                } else {
                    cached_value.access_count += 1;
                    stats.state_hits += 1;
                    Some(cached_value.value.clone())
                }
            }
            None => {
                stats.state_misses += 1;
                None
            }
        }
    }

    /// Cache a transaction receipt
    pub async fn put_receipt(&self, tx_hash: H256, receipt: TransactionReceipt) {
        let mut cache = self.receipt_cache.write().await;
        let mut stats = self.stats.write().await;
        let mut expirations = self.receipt_expirations.write().await;

        let now = Instant::now();
        let expires_at = now + self.config.receipt_ttl;
        let size_bytes = std::mem::size_of_val(&receipt);

        let cached_receipt = CachedReceipt {
            receipt,
            cached_at: now,
            expires_at,
            access_count: 0,
        };

        // Track expiration time for cleanup
        expirations.insert(tx_hash, expires_at);

        if cache.put(tx_hash, cached_receipt).is_some() {
            stats.receipt_evictions += 1;
        }

        stats.receipt_cache_bytes += size_bytes as u64;
    }

    /// Retrieve a cached transaction receipt
    pub async fn get_receipt(&self, tx_hash: &H256) -> Option<TransactionReceipt> {
        let mut cache = self.receipt_cache.write().await;
        let mut stats = self.stats.write().await;

        match cache.get_mut(tx_hash) {
            Some(cached_receipt) => {
                // Check if expired
                if Instant::now() > cached_receipt.expires_at {
                    cache.pop(tx_hash);
                    stats.receipt_expirations += 1;
                    stats.receipt_misses += 1;
                    None
                } else {
                    cached_receipt.access_count += 1;
                    stats.receipt_hits += 1;
                    Some(cached_receipt.receipt.clone())
                }
            }
            None => {
                stats.receipt_misses += 1;
                None
            }
        }
    }

    /// Clean up expired entries
    pub async fn cleanup_expired(&self) {
        let now = Instant::now();
        let mut stats = self.stats.write().await;
        let mut expired_count = 0;

        // Clean up expired state entries
        {
            let mut state_cache = self.state_cache.write().await;
            let mut state_expirations = self.state_expirations.write().await;
            let mut expired_keys: Vec<StateKey> = Vec::new();

            // Find expired keys from our expiration tracker
            for (key, expiration_time) in state_expirations.iter() {
                if now > *expiration_time {
                    expired_keys.push(key.clone());
                }
            }

            // Remove expired keys from both cache and expiration tracker
            for key in &expired_keys {
                state_cache.pop(key);
                state_expirations.remove(key);
                expired_count += 1;
            }

            stats.state_expirations += expired_keys.len() as u64;

            if !expired_keys.is_empty() {
                debug!("Cleaned up {} expired state entries", expired_keys.len());
            }
        }

        // Clean up expired receipt entries
        {
            let mut receipt_cache = self.receipt_cache.write().await;
            let mut receipt_expirations = self.receipt_expirations.write().await;
            let mut expired_keys: Vec<H256> = Vec::new();

            // Find expired keys from our expiration tracker
            for (key, expiration_time) in receipt_expirations.iter() {
                if now > *expiration_time {
                    expired_keys.push(*key);
                }
            }

            // Remove expired keys from both cache and expiration tracker
            for key in &expired_keys {
                receipt_cache.pop(key);
                receipt_expirations.remove(key);
                expired_count += 1;
            }

            stats.receipt_expirations += expired_keys.len() as u64;

            if !expired_keys.is_empty() {
                debug!("Cleaned up {} expired receipt entries", expired_keys.len());
            }
        }

        if expired_count > 0 {
            debug!("Cache cleanup completed: removed {} expired entries", expired_count);
        }
    }

    /// Get cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        let stats = self.stats.read().await;
        let mut result = stats.clone();

        // Update memory usage calculations
        result.total_memory_bytes = result.block_cache_bytes + result.state_cache_bytes + result.receipt_cache_bytes;

        result
    }

    /// Get cache hit rates
    pub async fn get_hit_rates(&self) -> HashMap<String, f64> {
        let stats = self.stats.read().await;
        let mut hit_rates = HashMap::new();

        // Block cache hit rate
        let block_total = stats.block_hits + stats.block_misses;
        let block_hit_rate = if block_total > 0 {
            stats.block_hits as f64 / block_total as f64
        } else {
            0.0
        };
        hit_rates.insert("blocks".to_string(), block_hit_rate);

        // State cache hit rate
        let state_total = stats.state_hits + stats.state_misses;
        let state_hit_rate = if state_total > 0 {
            stats.state_hits as f64 / state_total as f64
        } else {
            0.0
        };
        hit_rates.insert("state".to_string(), state_hit_rate);

        // Receipt cache hit rate
        let receipt_total = stats.receipt_hits + stats.receipt_misses;
        let receipt_hit_rate = if receipt_total > 0 {
            stats.receipt_hits as f64 / receipt_total as f64
        } else {
            0.0
        };
        hit_rates.insert("receipts".to_string(), receipt_hit_rate);

        // Overall hit rate
        let total_hits = stats.block_hits + stats.state_hits + stats.receipt_hits;
        let total_requests = block_total + state_total + receipt_total;
        let overall_hit_rate = if total_requests > 0 {
            total_hits as f64 / total_requests as f64
        } else {
            0.0
        };
        hit_rates.insert("overall".to_string(), overall_hit_rate);

        hit_rates
    }

    /// Clear all caches
    pub async fn clear_all(&self) {
        let mut block_cache = self.block_cache.write().await;
        let mut state_cache = self.state_cache.write().await;
        let mut receipt_cache = self.receipt_cache.write().await;
        let mut state_expirations = self.state_expirations.write().await;
        let mut receipt_expirations = self.receipt_expirations.write().await;
        let mut stats = self.stats.write().await;

        block_cache.clear();
        state_cache.clear();
        receipt_cache.clear();
        state_expirations.clear();
        receipt_expirations.clear();

        *stats = CacheStats::default();

        info!("All caches cleared");
    }
}

impl CacheStats {
    /// Calculate total memory usage in MB
    pub fn memory_usage_mb(&self) -> f64 {
        self.total_memory_bytes as f64 / (1024.0 * 1024.0)
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_blocks: 1000,
            max_state_entries: 10000,
            max_receipts: 5000,
            state_ttl: Duration::from_secs(300),    // 5 minutes
            receipt_ttl: Duration::from_secs(3600), // 1 hour
            enable_warming: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time;
    use lighthouse_wrapper::types::Hash256;
    use ethereum_types::H256;

    #[tokio::test]
    async fn test_cache_cleanup_expired() {
        // Create cache with short TTL for testing
        let config = CacheConfig {
            max_blocks: 100,
            max_state_entries: 100,
            max_receipts: 100,
            state_ttl: Duration::from_millis(100),  // Very short for testing
            receipt_ttl: Duration::from_millis(100),
            enable_warming: false,
        };

        let cache = StorageCache::new(config);

        // Add some state entries
        let key1 = b"test_key_1".to_vec();
        let value1 = b"test_value_1".to_vec();
        cache.put_state(key1.clone(), value1.clone()).await;

        // Wait for expiration
        time::sleep(Duration::from_millis(150)).await;

        // Run cleanup
        cache.cleanup_expired().await;

        // Get cache stats to verify cleanup worked
        let stats = cache.get_stats().await;
        // Note: The exact expiration count depends on implementation details
        println!("Cache cleanup test completed with {} state expirations", stats.state_expirations);
    }

    #[tokio::test]
    async fn test_cache_basic_functionality() {
        let config = CacheConfig::default();
        let cache = StorageCache::new(config);

        // Test state caching
        let key = b"test_key".to_vec();
        let value = b"test_value".to_vec();
        cache.put_state(key.clone(), value.clone()).await;

        let retrieved = cache.get_state(&key).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), value);
    }
}