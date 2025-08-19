//! Feature flag caching system
//!
//! This module provides high-performance caching for feature flag evaluations
//! to minimize evaluation overhead and maintain sub-millisecond response times.

use super::context::EvaluationContext;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use std::hash::{Hash, Hasher, DefaultHasher};

/// Cache entry for feature flag evaluation results
#[derive(Debug, Clone)]
struct CacheEntry {
    /// The cached evaluation result
    result: bool,
    
    /// When this entry was created
    created_at: Instant,
    
    /// TTL for this specific entry
    ttl: Duration,
    
    /// Context hash for validation
    context_hash: u64,
    
    /// Number of times this entry has been accessed
    access_count: u64,
}

impl CacheEntry {
    fn new(result: bool, ttl: Duration, context_hash: u64) -> Self {
        Self {
            result,
            created_at: Instant::now(),
            ttl,
            context_hash,
            access_count: 0,
        }
    }
    
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }
    
    fn access(&mut self) -> bool {
        self.access_count += 1;
        self.result
    }
}

/// High-performance feature flag cache
pub struct FeatureFlagCache {
    /// Cache storage: flag_name -> context_key -> entry
    cache: Arc<RwLock<HashMap<String, HashMap<String, CacheEntry>>>>,
    
    /// Default TTL for cache entries
    default_ttl: Duration,
    
    /// Maximum number of entries per flag (to prevent memory bloat)
    max_entries_per_flag: usize,
    
    /// Cache statistics
    stats: Arc<RwLock<CacheStats>>,
}

impl FeatureFlagCache {
    /// Create a new cache with default settings
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            default_ttl: Duration::from_secs(ttl_seconds),
            max_entries_per_flag: 1000, // Reasonable limit for memory usage
            stats: Arc::new(RwLock::new(CacheStats::new())),
        }
    }
    
    /// Create cache with custom settings
    pub fn with_settings(ttl_seconds: u64, max_entries_per_flag: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            default_ttl: Duration::from_secs(ttl_seconds),
            max_entries_per_flag,
            stats: Arc::new(RwLock::new(CacheStats::new())),
        }
    }
    
    /// Get cached result for a flag and context
    pub async fn get(&self, flag_name: &str, context: &EvaluationContext) -> Option<bool> {
        let context_key = self.context_key(context);
        let context_hash = context.hash();
        
        let mut cache_guard = self.cache.write().await;
        
        if let Some(flag_cache) = cache_guard.get_mut(flag_name) {
            if let Some(entry) = flag_cache.get_mut(&context_key) {
                // Validate context hasn't changed
                if entry.context_hash != context_hash {
                    // Context changed, remove stale entry
                    flag_cache.remove(&context_key);
                    self.update_stats(|s| s.context_mismatches += 1).await;
                    return None;
                }
                
                // Check if expired
                if entry.is_expired() {
                    flag_cache.remove(&context_key);
                    self.update_stats(|s| s.expired_entries += 1).await;
                    return None;
                }
                
                // Valid cache hit
                let result = entry.access();
                self.update_stats(|s| {
                    s.hits += 1;
                    s.total_accesses += 1;
                }).await;
                
                return Some(result);
            }
        }
        
        // Cache miss
        self.update_stats(|s| {
            s.misses += 1;
            s.total_accesses += 1;
        }).await;
        
        None
    }
    
    /// Cache a result for a flag and context
    pub async fn put(&self, flag_name: String, context: EvaluationContext, result: bool) {
        let context_key = self.context_key(&context);
        let context_hash = context.hash();
        let entry = CacheEntry::new(result, self.default_ttl, context_hash);
        
        let mut cache_guard = self.cache.write().await;
        
        // Get or create flag cache
        let flag_cache = cache_guard.entry(flag_name.clone()).or_insert_with(HashMap::new);
        
        // Check if we need to evict old entries
        if flag_cache.len() >= self.max_entries_per_flag {
            self.evict_oldest_entries(flag_cache).await;
        }
        
        // Insert new entry
        flag_cache.insert(context_key, entry);
        
        self.update_stats(|s| s.insertions += 1).await;
    }
    
    /// Invalidate cache for a specific flag
    pub async fn invalidate_flag(&self, flag_name: &str) {
        let mut cache_guard = self.cache.write().await;
        if let Some(flag_cache) = cache_guard.remove(flag_name) {
            let entries_removed = flag_cache.len();
            self.update_stats(|s| {
                s.invalidations += 1;
                s.entries_evicted += entries_removed as u64;
            }).await;
        }
    }
    
    /// Clear all cached entries
    pub async fn clear(&self) {
        let mut cache_guard = self.cache.write().await;
        let flags_cleared = cache_guard.len();
        let entries_cleared: usize = cache_guard.values().map(|v| v.len()).sum();
        
        cache_guard.clear();
        
        self.update_stats(|s| {
            s.full_clears += 1;
            s.entries_evicted += entries_cleared as u64;
        }).await;
        
        tracing::debug!("Cache cleared: {} flags, {} entries", flags_cleared, entries_cleared);
    }
    
    /// Clean up expired entries
    pub async fn cleanup_expired(&self) {
        let mut cache_guard = self.cache.write().await;
        let mut total_removed = 0;
        
        for (flag_name, flag_cache) in cache_guard.iter_mut() {
            let initial_size = flag_cache.len();
            flag_cache.retain(|_, entry| !entry.is_expired());
            let removed = initial_size - flag_cache.len();
            total_removed += removed;
            
            if removed > 0 {
                tracing::debug!("Removed {} expired entries for flag '{}'", removed, flag_name);
            }
        }
        
        // Remove empty flag caches
        cache_guard.retain(|_, flag_cache| !flag_cache.is_empty());
        
        if total_removed > 0 {
            self.update_stats(|s| {
                s.cleanup_runs += 1;
                s.expired_entries += total_removed as u64;
            }).await;
        }
    }
    
    /// Get cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        let stats = self.stats.read().await;
        stats.clone()
    }
    
    /// Get cache size information
    pub async fn get_size_info(&self) -> CacheSizeInfo {
        let cache_guard = self.cache.read().await;
        let total_flags = cache_guard.len();
        let total_entries: usize = cache_guard.values().map(|v| v.len()).sum();
        let largest_flag_cache = cache_guard.values().map(|v| v.len()).max().unwrap_or(0);
        
        CacheSizeInfo {
            total_flags,
            total_entries,
            largest_flag_cache,
            max_entries_per_flag: self.max_entries_per_flag,
        }
    }
    
    // Private helper methods
    
    /// Create a context key for caching
    fn context_key(&self, context: &EvaluationContext) -> String {
        // Use stable ID and relevant context fields
        format!(
            "{}:{}:{}:{}",
            context.stable_id(),
            context.environment as u32,
            context.chain_height,
            (context.sync_progress * 100.0) as u32
        )
    }
    
    /// Evict oldest entries when cache is full
    async fn evict_oldest_entries(&self, flag_cache: &mut HashMap<String, CacheEntry>) {
        let eviction_count = (flag_cache.len() / 4).max(1); // Evict 25% when full
        
        // Find oldest entries
        let mut entries: Vec<_> = flag_cache.iter().collect();
        entries.sort_by_key(|(_, entry)| entry.created_at);
        
        // Remove oldest entries
        for (context_key, _) in entries.into_iter().take(eviction_count) {
            flag_cache.remove(context_key);
        }
        
        self.update_stats(|s| s.entries_evicted += eviction_count as u64).await;
    }
    
    /// Update cache statistics
    async fn update_stats<F>(&self, updater: F)
    where
        F: FnOnce(&mut CacheStats),
    {
        if let Ok(mut stats) = self.stats.write().await {
            updater(&mut *stats);
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub total_accesses: u64,
    pub insertions: u64,
    pub invalidations: u64,
    pub full_clears: u64,
    pub cleanup_runs: u64,
    pub expired_entries: u64,
    pub entries_evicted: u64,
    pub context_mismatches: u64,
}

impl CacheStats {
    pub fn new() -> Self {
        Self {
            hits: 0,
            misses: 0,
            total_accesses: 0,
            insertions: 0,
            invalidations: 0,
            full_clears: 0,
            cleanup_runs: 0,
            expired_entries: 0,
            entries_evicted: 0,
            context_mismatches: 0,
        }
    }
    
    /// Calculate cache hit rate
    pub fn hit_rate(&self) -> f64 {
        if self.total_accesses == 0 {
            0.0
        } else {
            self.hits as f64 / self.total_accesses as f64
        }
    }
    
    /// Calculate miss rate
    pub fn miss_rate(&self) -> f64 {
        1.0 - self.hit_rate()
    }
}

/// Cache size information
#[derive(Debug, Clone)]
pub struct CacheSizeInfo {
    pub total_flags: usize,
    pub total_entries: usize,
    pub largest_flag_cache: usize,
    pub max_entries_per_flag: usize,
}

impl CacheSizeInfo {
    /// Calculate memory usage estimate (rough approximation)
    pub fn estimated_memory_kb(&self) -> usize {
        // Rough estimate: each entry ~200 bytes (including HashMap overhead)
        (self.total_entries * 200) / 1024
    }
    
    /// Check if any flag cache is near the limit
    pub fn has_large_caches(&self) -> bool {
        self.largest_flag_cache > (self.max_entries_per_flag * 3 / 4)
    }
}

/// Background cache maintenance task
pub struct CacheMaintenance {
    cache: Arc<FeatureFlagCache>,
    cleanup_interval: Duration,
}

impl CacheMaintenance {
    pub fn new(cache: Arc<FeatureFlagCache>, cleanup_interval_seconds: u64) -> Self {
        Self {
            cache,
            cleanup_interval: Duration::from_secs(cleanup_interval_seconds),
        }
    }
    
    /// Start background maintenance task
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.cleanup_interval);
            
            loop {
                interval.tick().await;
                
                // Cleanup expired entries
                self.cache.cleanup_expired().await;
                
                // Log cache statistics periodically
                let stats = self.cache.get_stats().await;
                let size_info = self.cache.get_size_info().await;
                
                tracing::debug!(
                    "Cache stats: {} total accesses, {:.2}% hit rate, {} flags, {} entries",
                    stats.total_accesses,
                    stats.hit_rate() * 100.0,
                    size_info.total_flags,
                    size_info.total_entries
                );
                
                // Warn about large caches
                if size_info.has_large_caches() {
                    tracing::warn!(
                        "Large cache detected: {} entries in largest flag cache (limit: {})",
                        size_info.largest_flag_cache,
                        size_info.max_entries_per_flag
                    );
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Environment;
    
    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache = FeatureFlagCache::new(5);
        let context = EvaluationContext::new("test-node".to_string(), Environment::Development);
        
        // Test cache miss
        assert!(cache.get("test_flag", &context).await.is_none());
        
        // Test cache put and hit
        cache.put("test_flag".to_string(), context.clone(), true).await;
        assert_eq!(cache.get("test_flag", &context).await, Some(true));
        
        // Test stats
        let stats = cache.get_stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.insertions, 1);
    }
    
    #[tokio::test]
    async fn test_cache_expiration() {
        let cache = FeatureFlagCache::new(1); // 1 second TTL
        let context = EvaluationContext::new("test-node".to_string(), Environment::Development);
        
        // Insert entry
        cache.put("test_flag".to_string(), context.clone(), true).await;
        assert_eq!(cache.get("test_flag", &context).await, Some(true));
        
        // Wait for expiration
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(cache.get("test_flag", &context).await.is_none());
    }
    
    #[tokio::test]
    async fn test_cache_context_sensitivity() {
        let cache = FeatureFlagCache::new(60);
        let context1 = EvaluationContext::new("node1".to_string(), Environment::Development);
        let context2 = EvaluationContext::new("node2".to_string(), Environment::Development);
        
        // Different contexts should have separate cache entries
        cache.put("test_flag".to_string(), context1.clone(), true).await;
        cache.put("test_flag".to_string(), context2.clone(), false).await;
        
        assert_eq!(cache.get("test_flag", &context1).await, Some(true));
        assert_eq!(cache.get("test_flag", &context2).await, Some(false));
    }
    
    #[tokio::test]
    async fn test_cache_invalidation() {
        let cache = FeatureFlagCache::new(60);
        let context = EvaluationContext::new("test-node".to_string(), Environment::Development);
        
        cache.put("test_flag".to_string(), context.clone(), true).await;
        assert_eq!(cache.get("test_flag", &context).await, Some(true));
        
        cache.invalidate_flag("test_flag").await;
        assert!(cache.get("test_flag", &context).await.is_none());
    }
}