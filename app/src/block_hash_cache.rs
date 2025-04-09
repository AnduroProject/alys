use async_trait::async_trait;
use bitcoin::BlockHash;
use eyre::eyre;
use std::sync::Arc;
use tracing::trace;
// TODO: Make use optional for nodes not submitting aux pow

/// A cache for storing block hashes in an ordered array.
///
/// This provides a simple and efficient mechanism to track new blocks
/// and retrieve or flush them as needed. Optimized for:
/// - Fast sequential addition of new block hashes (O(1) append)
/// - Fast reads of all stored hashes
/// - Fast clearing of all data by returning the stored contents
#[derive(Default, Debug, Clone)]
pub struct BlockHashCache {
    // Using Vec for O(1) append and sequential read
    hashes: Vec<BlockHash>,
}

#[async_trait]
pub trait BlockHashCacheInit {
    /// Initializes the cache with a given vector of hashes.
    async fn init_block_hash_cache(self: &Arc<Self>) -> eyre::Result<()>;
}

impl BlockHashCache {
    /// Creates a new empty BlockHashCache.
    pub fn new(aggregate_hashes: Option<Vec<BlockHash>>) -> Self {
        // Initialize with the provided hashes or an empty vector
        Self {
            hashes: aggregate_hashes.unwrap_or_default(),
        }
    }

    /// Initializes the cache with a given vector of hashes.
    pub fn init(&mut self, hashes: Vec<BlockHash>) -> eyre::Result<()> {
        trace!("BlockHashCache::init");
        // trace!("BlockHashCache::init: hashes: {:#?}", hashes);
        self.hashes = hashes;
        Ok(())
    }

    /// Adds a block hash to the cache.
    pub fn add(&mut self, hash: BlockHash) {
        self.hashes.push(hash);
        trace!("BlockHashCache::add: Added hash {}", hash);
    }

    /// Returns a reference to all stored hashes.
    pub fn get(&self) -> Vec<BlockHash> {
        trace!("BlockHashCache::get");
        // trace!("{:#?}", self.hashes);
        self.hashes.clone()
    }

    /// Searches for the hash in the cache and removes all entries up to and including it.
    /// Returns Ok(()) if the hash was found and removed, or Err(()) if the hash was not found.
    pub fn reset_with(&mut self, hash: BlockHash) -> eyre::Result<()> {
        trace!("BlockHashCache::reset_with");

        // Find the index of the hash in the vector
        if let Some(index) = self.hashes.iter().position(|&h| h == hash) {
            // Remove all elements up to and including the found hash
            self.hashes = self.hashes.split_off(index + 1);
            Ok(())
        } else {
            // Hash not found
            Err(eyre!("Hash not found in cache"))
        }
    }

    /// Clears the cache and returns all stored hashes.
    #[allow(dead_code)]
    pub fn flush(&mut self) -> Vec<BlockHash> {
        std::mem::take(&mut self.hashes)
    }

    /// Returns the number of hashes in the cache.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    /// Returns true if the cache is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::hashes::{sha256d, Hash}; // For BlockHash creation

    #[test]
    fn test_new_cache_is_empty() {
        let cache = BlockHashCache::new(None);
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(cache.get().is_empty());
    }

    #[test]
    fn test_new_cache_with_initial_hashes() {
        let hash1 = BlockHash::from_byte_array([1; 32]);
        let hash2 = BlockHash::from_byte_array([2; 32]);
        let initial_hashes = vec![hash1, hash2];

        let cache = BlockHashCache::new(Some(initial_hashes.clone()));

        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(), initial_hashes);
    }

    #[test]
    fn test_init_method() {
        let mut cache = BlockHashCache::new(None);
        let hash1 = BlockHash::from_byte_array([1; 32]);
        let hash2 = BlockHash::from_byte_array([2; 32]);
        let hashes = vec![hash1, hash2];

        let result = cache.init(hashes.clone());

        assert!(result.is_ok());
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(), hashes);
    }

    #[test]
    fn test_add_hash() {
        let mut cache = BlockHashCache::new(None);
        let hash1 = BlockHash::from_byte_array([1; 32]);

        cache.add(hash1);

        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get()[0], hash1);
    }

    #[test]
    fn test_add_multiple_hashes() {
        let mut cache = BlockHashCache::new(None);
        let hash1 = BlockHash::from_byte_array([1; 32]);
        let hash2 = BlockHash::from_byte_array([2; 32]);
        let hash3 = BlockHash::from_byte_array([3; 32]);

        cache.add(hash1);
        cache.add(hash2);
        cache.add(hash3);

        assert_eq!(cache.len(), 3);

        let expected = vec![hash1, hash2, hash3];
        assert_eq!(cache.get(), expected);
    }

    #[test]
    fn test_reset_with_found_hash() {
        let mut cache = BlockHashCache::new(None);
        let hash1 = BlockHash::from_byte_array([1; 32]);
        let hash2 = BlockHash::from_byte_array([2; 32]);
        let hash3 = BlockHash::from_byte_array([3; 32]);
        let hash4 = BlockHash::from_byte_array([4; 32]);

        // Add hashes
        cache.add(hash1);
        cache.add(hash2);
        cache.add(hash3);
        cache.add(hash4);
        assert_eq!(cache.len(), 4);

        // Reset with hash2 - should remove hash1 and hash2
        let result = cache.reset_with(hash2);

        // Should return Ok
        assert!(result.is_ok());

        // Cache should now contain only hash3 and hash4
        assert_eq!(cache.len(), 2);
        let expected = vec![hash3, hash4];
        assert_eq!(cache.get(), expected);
    }

    #[test]
    fn test_reset_with_not_found_hash() {
        let mut cache = BlockHashCache::new(None);
        let hash1 = BlockHash::from_byte_array([1; 32]);
        let hash2 = BlockHash::from_byte_array([2; 32]);
        let missing_hash = BlockHash::from_byte_array([99; 32]);

        // Add hashes
        cache.add(hash1);
        cache.add(hash2);
        assert_eq!(cache.len(), 2);

        // Reset with missing hash
        let result = cache.reset_with(missing_hash);

        // Should return Err
        assert!(result.is_err());

        // Cache should remain unchanged
        assert_eq!(cache.len(), 2);
        let expected = vec![hash1, hash2];
        assert_eq!(cache.get(), expected);
    }

    #[test]
    fn test_get_returns_hashes_in_order() {
        let mut cache = BlockHashCache::new(None);
        let hashes: Vec<BlockHash> = (0..5)
            .map(|i| BlockHash::from_byte_array([i; 32]))
            .collect();

        for hash in &hashes {
            cache.add(*hash);
        }

        assert_eq!(cache.get(), hashes);
    }

    #[test]
    fn test_flush() {
        let mut cache = BlockHashCache::new(None);
        let hash1 = BlockHash::from_byte_array([1; 32]);
        let hash2 = BlockHash::from_byte_array([2; 32]);

        cache.add(hash1);
        cache.add(hash2);

        let flushed = cache.flush();

        // Check returned hashes
        let expected = vec![hash1, hash2];
        assert_eq!(flushed, expected);

        // Check cache is now empty
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(cache.get().is_empty());
    }

    #[test]
    fn test_default() {
        let cache = BlockHashCache::default();
        assert!(cache.is_empty());
    }
}
