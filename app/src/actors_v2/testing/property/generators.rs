use proptest::prelude::*;
use std::collections::HashMap;

/// Common property test generators for blockchain-related data
pub mod blockchain {
    use super::*;

    /// Generate valid block numbers
    pub fn block_number() -> impl Strategy<Value = u64> {
        1u64..1_000_000
    }

    /// Generate block hashes (as hex strings)
    pub fn block_hash() -> impl Strategy<Value = String> {
        prop::collection::vec(any::<u8>(), 32)
            .prop_map(|bytes| format!("0x{}", hex::encode(bytes)))
    }

    /// Generate gas limits
    pub fn gas_limit() -> impl Strategy<Value = u64> {
        1_000_000u64..50_000_000
    }

    /// Generate gas used (always <= gas limit)
    pub fn gas_used(max_limit: u64) -> impl Strategy<Value = u64> {
        0u64..max_limit
    }

    /// Generate timestamps (recent years)
    pub fn timestamp() -> impl Strategy<Value = u64> {
        1_600_000_000u64..2_000_000_000 // 2020-2033 range
    }

    /// Generate ethereum addresses
    pub fn address() -> impl Strategy<Value = String> {
        prop::collection::vec(any::<u8>(), 20)
            .prop_map(|bytes| format!("0x{}", hex::encode(bytes)))
    }
}

/// Common property test generators for storage-related operations
pub mod storage {
    use super::*;

    /// Generate keys for key-value storage
    pub fn storage_key() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(any::<u8>(), 1..256)
    }

    /// Generate values for key-value storage
    pub fn storage_value() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(any::<u8>(), 0..10240) // Up to 10KB
    }

    /// Generate batch sizes for batch operations
    pub fn batch_size() -> impl Strategy<Value = usize> {
        1usize..1000
    }

    /// Generate cache sizes
    pub fn cache_size() -> impl Strategy<Value = usize> {
        1usize..10000
    }

    /// Generate database paths
    pub fn db_path() -> impl Strategy<Value = String> {
        prop::string::string_regex(r"/tmp/test_db_[a-z0-9]{8}")
            .expect("Valid regex")
    }
}

/// Generators for actor message patterns
pub mod messages {
    use super::*;

    /// Generate message IDs (UUIDs as strings)
    pub fn message_id() -> impl Strategy<Value = String> {
        prop::collection::vec(any::<u8>(), 16)
            .prop_map(|bytes| {
                format!(
                    "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
                    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
                    u16::from_be_bytes([bytes[4], bytes[5]]),
                    u16::from_be_bytes([bytes[6], bytes[7]]),
                    u16::from_be_bytes([bytes[8], bytes[9]]),
                    u64::from_be_bytes([bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15], 0, 0]) >> 16
                )
            })
    }

    /// Generate correlation IDs
    pub fn correlation_id() -> impl Strategy<Value = Option<String>> {
        prop::option::of(message_id())
    }

    /// Generate message priorities
    pub fn priority() -> impl Strategy<Value = u8> {
        1u8..10
    }
}

/// Generators for testing error conditions
pub mod errors {
    use super::*;

    /// Generate failure rates (0.0 to 1.0)
    pub fn failure_rate() -> impl Strategy<Value = f64> {
        0.0..1.0
    }

    /// Generate error types
    pub fn error_type() -> impl Strategy<Value = String> {
        prop::sample::select(vec![
            "NetworkError".to_string(),
            "DatabaseError".to_string(),
            "ValidationError".to_string(),
            "TimeoutError".to_string(),
            "MemoryError".to_string(),
            "DiskError".to_string(),
        ])
    }

    /// Generate retry counts
    pub fn retry_count() -> impl Strategy<Value = u32> {
        0u32..10
    }
}

/// Composite generators for complex test scenarios
pub mod scenarios {
    use super::*;

    /// Generate a sequence of block operations
    pub fn block_sequence(length: impl Into<prop::collection::SizeRange>) -> impl Strategy<Value = Vec<BlockOperation>> {
        prop::collection::vec(block_operation(), length)
    }

    #[derive(Debug, Clone)]
    pub enum BlockOperation {
        Store { slot: u64, canonical: bool },
        Retrieve { slot: u64 },
        Delete { slot: u64 },
        UpdateHead { slot: u64 },
    }

    pub fn block_operation() -> impl Strategy<Value = BlockOperation> {
        prop_oneof![
            (blockchain::block_number(), any::<bool>()).prop_map(|(slot, canonical)| BlockOperation::Store { slot, canonical }),
            blockchain::block_number().prop_map(|slot| BlockOperation::Retrieve { slot }),
            blockchain::block_number().prop_map(|slot| BlockOperation::Delete { slot }),
            blockchain::block_number().prop_map(|slot| BlockOperation::UpdateHead { slot }),
        ]
    }

    /// Generate concurrent operation patterns
    pub fn concurrent_operations(max_concurrent: usize) -> impl Strategy<Value = Vec<Vec<BlockOperation>>> {
        (1..=max_concurrent).prop_flat_map(|thread_count| {
            prop::collection::vec(
                prop::collection::vec(block_operation(), 1..20),
                thread_count
            )
        })
    }
}

/// Configuration generators
pub mod config {
    use super::*;

    /// Generate database configurations
    pub fn database_config() -> impl Strategy<Value = MockDatabaseConfig> {
        (
            storage::db_path(),
            storage::cache_size(),
            storage::batch_size(),
            any::<bool>(), // enable_compression
            any::<bool>(), // enable_statistics
        ).prop_map(|(path, cache_size, batch_size, compression, stats)| MockDatabaseConfig {
            path,
            cache_size,
            write_batch_size: batch_size,
            enable_compression: compression,
            enable_statistics: stats,
        })
    }

    #[derive(Debug, Clone)]
    pub struct MockDatabaseConfig {
        pub path: String,
        pub cache_size: usize,
        pub write_batch_size: usize,
        pub enable_compression: bool,
        pub enable_statistics: bool,
    }
}

/// Utility functions for property test data generation
pub fn generate_test_data_map(size: usize) -> impl Strategy<Value = HashMap<Vec<u8>, Vec<u8>>> {
    prop::collection::hash_map(
        storage::storage_key(),
        storage::storage_value(),
        0..size
    )
}