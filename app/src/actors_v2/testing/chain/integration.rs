//! ChainActor Integration Tests
//!
//! Integration tests for ChainActor coordination with other actors

#[cfg(test)]
mod tests {
    use crate::actors_v2::testing::chain::fixtures::*;

    #[tokio::test]
    async fn test_configuration_integration() {
        // Test that different configurations work together
        let validator_cfg = validator_config();
        let non_validator_cfg = non_validator_config();

        assert_ne!(validator_cfg.is_validator, non_validator_cfg.is_validator);
        assert_eq!(validator_cfg.enable_auxpow, non_validator_cfg.enable_auxpow);
    }

    #[tokio::test]
    async fn test_mock_data_consistency() {
        // Test that mock data is internally consistent
        let status = mock_chain_status();
        let pegins = mock_multiple_pegins();
        let pegouts = mock_multiple_pegouts();

        // Verify data relationships
        assert!(status.height > 0);
        assert!(!pegins.is_empty());
        assert!(!pegouts.is_empty());

        // Verify amounts are reasonable
        let total_pegin_amount: u64 = pegins.iter().map(|p| p.amount).sum();
        let total_pegout_amount: u64 = pegouts.iter().map(|p| p.amount).sum();

        assert!(total_pegin_amount > 0);
        assert!(total_pegout_amount > 0);
    }

    #[tokio::test]
    async fn test_address_generation() {
        // Test deterministic address generation
        let addr1 = test_address(1);
        let addr2 = test_address(2);
        let addr1_again = test_address(1);

        assert_ne!(addr1, addr2);
        assert_eq!(addr1, addr1_again);
    }

    #[tokio::test]
    async fn test_bitcoin_data_generation() {
        // Test Bitcoin-related test data
        let hash1 = test_bitcoin_block_hash(1);
        let hash2 = test_bitcoin_block_hash(2);
        let txid1 = test_txid(1);
        let txid2 = test_txid(2);

        assert_ne!(hash1, hash2);
        assert_ne!(txid1, txid2);

        // Verify determinism
        assert_eq!(hash1, test_bitcoin_block_hash(1));
        assert_eq!(txid1, test_txid(1));
    }
}