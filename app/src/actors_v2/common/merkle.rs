//! Merkle tree verification for transaction roots
//!
//! Placeholder implementation for Phase 5.
//!
//! Note: Full Keccak256-based Merkle tree implementation deferred pending:
//! 1. Addition of tiny_keccak dependency to Cargo.toml
//! 2. Verification that ExecutionPayloadCapella has transactions_root field
//!
//! As noted in the implementation plan, Merkle verification can be skipped
//! if the field is not available.

use ethereum_types::H256;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Calculate Merkle root of transactions
///
/// **Placeholder implementation** using simple hash for now.
/// Will be replaced with proper Keccak256 Merkle tree when dependencies are added.
///
/// # Arguments
/// * `transactions` - Slice of serialized transactions
///
/// # Returns
/// The Merkle root hash. Returns H256::zero() for empty transaction list.
///
/// # Examples
/// ```
/// use ethereum_types::H256;
/// use alys::actors_v2::common::merkle::calculate_transaction_root;
///
/// let txs = vec![vec![1, 2, 3], vec![4, 5, 6]];
/// let root = calculate_transaction_root(&txs);
/// assert_ne!(root, H256::zero());
/// ```
///
pub fn calculate_transaction_root(transactions: &[Vec<u8>]) -> H256 {
    if transactions.is_empty() {
        return H256::zero();
    }

    // Placeholder: Simple hash of concatenated transactions
    // TODO: Replace with proper Keccak256 Merkle tree implementation
    let mut hasher = DefaultHasher::new();

    for tx in transactions {
        tx.hash(&mut hasher);
    }

    let hash_value = hasher.finish();

    // Convert u64 hash to H256 (expanding to 32 bytes)
    H256::from_low_u64_be(hash_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_transactions() {
        let root = calculate_transaction_root(&[]);
        assert_eq!(
            root,
            H256::zero(),
            "Empty transaction list should return zero hash"
        );
    }

    #[test]
    fn test_single_transaction() {
        let txs = vec![vec![1, 2, 3, 4]];
        let root = calculate_transaction_root(&txs);
        assert_ne!(
            root,
            H256::zero(),
            "Single transaction should produce non-zero root"
        );

        // Verify determinism
        let root2 = calculate_transaction_root(&txs);
        assert_eq!(root, root2, "Same input should produce same root");
    }

    #[test]
    fn test_multiple_transactions() {
        let txs = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];
        let root = calculate_transaction_root(&txs);
        assert_ne!(
            root,
            H256::zero(),
            "Multiple transactions should produce non-zero root"
        );

        // Verify determinism
        let root2 = calculate_transaction_root(&txs);
        assert_eq!(root, root2, "Same input should produce same root");
    }

    #[test]
    fn test_different_transactions_different_roots() {
        let txs1 = vec![vec![1, 2, 3], vec![4, 5, 6]];
        let txs2 = vec![vec![1, 2, 3], vec![4, 5, 7]]; // One byte different

        let root1 = calculate_transaction_root(&txs1);
        let root2 = calculate_transaction_root(&txs2);

        assert_ne!(
            root1, root2,
            "Different transactions should produce different roots"
        );
    }

    #[test]
    fn test_order_matters() {
        let txs1 = vec![vec![1, 2, 3], vec![4, 5, 6]];
        let txs2 = vec![vec![4, 5, 6], vec![1, 2, 3]]; // Swapped order

        let root1 = calculate_transaction_root(&txs1);
        let root2 = calculate_transaction_root(&txs2);

        assert_ne!(root1, root2, "Order should affect Merkle root");
    }

    #[test]
    fn test_odd_number_transactions() {
        // Test with 3 transactions (odd number)
        let txs = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];
        let root = calculate_transaction_root(&txs);
        assert_ne!(root, H256::zero(), "Odd number of transactions should work");
    }

    #[test]
    fn test_power_of_two_transactions() {
        // Test with 4 transactions (power of 2)
        let txs = vec![vec![1], vec![2], vec![3], vec![4]];
        let root = calculate_transaction_root(&txs);
        assert_ne!(root, H256::zero(), "Power of 2 transactions should work");
    }

    #[test]
    fn test_hash_consistency() {
        let data = vec![vec![1u8, 2, 3]];
        let root1 = calculate_transaction_root(&data);
        let root2 = calculate_transaction_root(&data);
        assert_eq!(root1, root2, "Hash should be deterministic");
    }
}
