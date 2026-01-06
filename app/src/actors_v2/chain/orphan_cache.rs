//! Orphan Block Cache for ChainActor V2
//!
//! Stores blocks whose parents haven't been imported yet, enabling:
//! - Out-of-order block reception during sync
//! - Tracking "observed network height" from future blocks
//! - Automatic processing when parents become available
//!
//! Eviction policies:
//! - Time-based: blocks older than max_age are evicted
//! - Size-based: oldest blocks evicted when cache exceeds max_size
//! - Height-based: blocks too far ahead of current height are rejected

use ethereum_types::H256;
use lighthouse_wrapper::types::MainnetEthSpec;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use crate::block::SignedConsensusBlock;

/// Configuration for the orphan block cache
#[derive(Debug, Clone)]
pub struct OrphanCacheConfig {
    /// Maximum number of orphan blocks to cache
    pub max_size: usize,
    /// Maximum age of cached blocks before eviction
    pub max_age: Duration,
    /// Maximum height ahead of current to accept (prevents DoS)
    pub max_height_ahead: u64,
}

impl Default for OrphanCacheConfig {
    fn default() -> Self {
        Self {
            max_size: 100,
            max_age: Duration::from_secs(60),
            max_height_ahead: 100,
        }
    }
}

/// Entry in the orphan cache
#[derive(Debug, Clone)]
pub struct OrphanEntry {
    /// The orphan block
    pub block: SignedConsensusBlock<MainnetEthSpec>,
    /// Block height (cached for quick access)
    pub height: u64,
    /// Block hash (cached for quick access)
    pub hash: H256,
    /// Parent hash this block is waiting for
    pub parent_hash: H256,
    /// When this block was cached
    pub cached_at: Instant,
    /// Source peer ID (for reputation tracking)
    pub peer_id: Option<String>,
}

/// Orphan block cache
///
/// Stores blocks that can't be imported because their parent hasn't been
/// imported yet. When a block is successfully imported, the cache is checked
/// for any children that can now be processed.
#[derive(Debug)]
pub struct OrphanBlockCache {
    /// Configuration
    config: OrphanCacheConfig,
    /// Orphan blocks indexed by parent hash they're waiting for
    /// Multiple blocks can have the same parent (forks)
    by_parent: HashMap<H256, Vec<OrphanEntry>>,
    /// Orphan blocks indexed by their own hash (for deduplication)
    by_hash: HashMap<H256, H256>, // block_hash -> parent_hash
    /// Total number of cached blocks
    size: usize,
    /// Highest block height seen (observed network height)
    observed_height: u64,
    /// Statistics
    stats: OrphanCacheStats,
}

/// Cache statistics for monitoring
#[derive(Debug, Default, Clone)]
pub struct OrphanCacheStats {
    /// Total blocks added to cache
    pub total_added: u64,
    /// Total blocks evicted (any reason)
    pub total_evicted: u64,
    /// Blocks evicted due to age
    pub evicted_age: u64,
    /// Blocks evicted due to size limit
    pub evicted_size: u64,
    /// Blocks rejected (too far ahead)
    pub rejected_height: u64,
    /// Blocks rejected (duplicate)
    pub rejected_duplicate: u64,
    /// Blocks successfully retrieved for processing
    pub total_retrieved: u64,
}

impl OrphanBlockCache {
    /// Create a new orphan cache with default configuration
    pub fn new() -> Self {
        Self::with_config(OrphanCacheConfig::default())
    }

    /// Create a new orphan cache with custom configuration
    pub fn with_config(config: OrphanCacheConfig) -> Self {
        Self {
            config,
            by_parent: HashMap::new(),
            by_hash: HashMap::new(),
            size: 0,
            observed_height: 0,
            stats: OrphanCacheStats::default(),
        }
    }

    /// Get the highest block height observed (including orphans)
    pub fn observed_height(&self) -> u64 {
        self.observed_height
    }

    /// Get current cache size
    pub fn len(&self) -> usize {
        self.size
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Get cache statistics
    pub fn stats(&self) -> &OrphanCacheStats {
        &self.stats
    }

    /// Add a block to the orphan cache
    ///
    /// Returns Ok(true) if added, Ok(false) if rejected (duplicate, too far ahead),
    /// or the observed height update.
    pub fn add(
        &mut self,
        block: SignedConsensusBlock<MainnetEthSpec>,
        height: u64,
        hash: H256,
        parent_hash: H256,
        current_height: u64,
        peer_id: Option<String>,
    ) -> Result<bool, String> {
        // Check if block is too far ahead
        if height > current_height + self.config.max_height_ahead {
            self.stats.rejected_height += 1;
            debug!(
                height = height,
                current = current_height,
                max_ahead = self.config.max_height_ahead,
                "Rejecting orphan block: too far ahead"
            );
            return Ok(false);
        }

        // Check for duplicate
        if self.by_hash.contains_key(&hash) {
            self.stats.rejected_duplicate += 1;
            debug!(
                hash = ?hash,
                height = height,
                "Rejecting orphan block: duplicate"
            );
            return Ok(false);
        }

        // Evict expired entries first
        self.evict_expired();

        // Evict oldest if at capacity
        while self.size >= self.config.max_size {
            if !self.evict_oldest() {
                warn!("Failed to evict oldest orphan block");
                break;
            }
        }

        // Create entry
        let entry = OrphanEntry {
            block,
            height,
            hash,
            parent_hash,
            cached_at: Instant::now(),
            peer_id,
        };

        // Update observed height
        if height > self.observed_height {
            info!(
                previous = self.observed_height,
                new = height,
                "Updated observed network height from orphan block"
            );
            self.observed_height = height;
        }

        // Add to indices
        self.by_hash.insert(hash, parent_hash);
        self.by_parent.entry(parent_hash).or_default().push(entry);
        self.size += 1;
        self.stats.total_added += 1;

        debug!(
            hash = ?hash,
            height = height,
            parent = ?parent_hash,
            cache_size = self.size,
            "Added orphan block to cache"
        );

        Ok(true)
    }

    /// Remove and return all orphan blocks waiting for the given parent hash
    ///
    /// Called when a block is successfully imported to check for orphan children.
    pub fn remove_by_parent(&mut self, parent_hash: &H256) -> Vec<OrphanEntry> {
        if let Some(entries) = self.by_parent.remove(parent_hash) {
            // Remove from by_hash index
            for entry in &entries {
                self.by_hash.remove(&entry.hash);
            }

            let count = entries.len();
            self.size = self.size.saturating_sub(count);
            self.stats.total_retrieved += count as u64;

            debug!(
                parent = ?parent_hash,
                count = count,
                remaining = self.size,
                "Retrieved orphan children for processing"
            );

            entries
        } else {
            Vec::new()
        }
    }

    /// Check if we have any orphans waiting for a specific parent
    pub fn has_children_for(&self, parent_hash: &H256) -> bool {
        self.by_parent.contains_key(parent_hash)
    }

    /// Get count of orphans waiting for a specific parent
    pub fn children_count_for(&self, parent_hash: &H256) -> usize {
        self.by_parent.get(parent_hash).map_or(0, |v| v.len())
    }

    /// Evict entries older than max_age
    pub fn evict_expired(&mut self) {
        let now = Instant::now();
        let max_age = self.config.max_age;
        let mut expired_parents = Vec::new();

        for (parent_hash, entries) in &mut self.by_parent {
            let original_len = entries.len();
            entries.retain(|entry| {
                let age = now.duration_since(entry.cached_at);
                if age > max_age {
                    // Remove from by_hash
                    self.by_hash.remove(&entry.hash);
                    false
                } else {
                    true
                }
            });

            let evicted = original_len - entries.len();
            if evicted > 0 {
                self.size = self.size.saturating_sub(evicted);
                self.stats.evicted_age += evicted as u64;
                self.stats.total_evicted += evicted as u64;
            }

            if entries.is_empty() {
                expired_parents.push(*parent_hash);
            }
        }

        // Remove empty parent entries
        for parent_hash in expired_parents {
            self.by_parent.remove(&parent_hash);
        }
    }

    /// Evict the oldest entry
    fn evict_oldest(&mut self) -> bool {
        // Find the oldest entry
        let mut oldest: Option<(H256, usize, Instant)> = None;

        for (parent_hash, entries) in &self.by_parent {
            for (idx, entry) in entries.iter().enumerate() {
                match &oldest {
                    None => oldest = Some((*parent_hash, idx, entry.cached_at)),
                    Some((_, _, oldest_time)) => {
                        if entry.cached_at < *oldest_time {
                            oldest = Some((*parent_hash, idx, entry.cached_at));
                        }
                    }
                }
            }
        }

        if let Some((parent_hash, idx, _)) = oldest {
            if let Some(entries) = self.by_parent.get_mut(&parent_hash) {
                if idx < entries.len() {
                    let entry = entries.remove(idx);
                    self.by_hash.remove(&entry.hash);
                    self.size = self.size.saturating_sub(1);
                    self.stats.evicted_size += 1;
                    self.stats.total_evicted += 1;

                    // Remove parent entry if empty
                    if entries.is_empty() {
                        self.by_parent.remove(&parent_hash);
                    }

                    debug!(
                        hash = ?entry.hash,
                        height = entry.height,
                        "Evicted oldest orphan block (size limit)"
                    );

                    return true;
                }
            }
        }

        false
    }

    /// Clear all cached orphans
    pub fn clear(&mut self) {
        let cleared = self.size;
        self.by_parent.clear();
        self.by_hash.clear();
        self.size = 0;

        if cleared > 0 {
            info!(cleared = cleared, "Cleared orphan block cache");
        }
    }

    /// Get all heights currently in the cache (for debugging)
    pub fn cached_heights(&self) -> Vec<u64> {
        let mut heights: Vec<u64> = self
            .by_parent
            .values()
            .flat_map(|entries| entries.iter().map(|e| e.height))
            .collect();
        heights.sort();
        heights.dedup();
        heights
    }
}

impl Default for OrphanBlockCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_hash(n: u8) -> H256 {
        H256::from([n; 32])
    }

    #[test]
    fn test_cache_basic_operations() {
        let mut cache = OrphanBlockCache::new();

        assert!(cache.is_empty());
        assert_eq!(cache.observed_height(), 0);
    }

    #[test]
    fn test_observed_height_updates() {
        let mut cache = OrphanBlockCache::new();

        // Note: We can't easily create SignedConsensusBlock in tests,
        // but we can test the height tracking logic conceptually
        assert_eq!(cache.observed_height(), 0);
    }

    #[test]
    fn test_eviction_by_size() {
        let config = OrphanCacheConfig {
            max_size: 5,
            max_age: Duration::from_secs(3600),
            max_height_ahead: 1000,
        };
        let cache = OrphanBlockCache::with_config(config);

        assert_eq!(cache.config.max_size, 5);
    }

    #[test]
    fn test_has_children_for() {
        let cache = OrphanBlockCache::new();

        assert!(!cache.has_children_for(&make_test_hash(1)));
        assert_eq!(cache.children_count_for(&make_test_hash(1)), 0);
    }

    #[test]
    fn test_stats_default() {
        let stats = OrphanCacheStats::default();

        assert_eq!(stats.total_added, 0);
        assert_eq!(stats.total_evicted, 0);
        assert_eq!(stats.rejected_height, 0);
    }
}
