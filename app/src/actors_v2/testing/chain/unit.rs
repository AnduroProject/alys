//! ChainActor Unit Tests
//!
//! Basic unit tests for ChainActor V2 functionality

#[cfg(test)]
mod tests {
    use crate::actors_v2::testing::chain::fixtures::*;

    #[tokio::test]
    async fn test_chain_config_validation() {
        // Test configuration validation
        let config = validator_config();
        assert!(config.validate().is_ok());

        let mut invalid_config = config.clone();
        invalid_config.max_blocks_without_pow = 0;
        assert!(invalid_config.validate().is_err());
    }

    #[tokio::test]
    async fn test_chain_status_creation() {
        // Test chain status message creation
        let status = mock_chain_status();
        assert_eq!(status.height, 100);
        assert_eq!(status.peer_count, 5);
        assert!(status.is_synced);
        assert!(status.is_validator);
        assert!(status.auxpow_enabled);
    }

    #[tokio::test]
    async fn test_pegin_fixtures() {
        // Test peg-in data fixtures
        let pegin = mock_pegin_info();
        assert_eq!(pegin.amount, 100000000); // 1 BTC

        let multiple_pegins = mock_multiple_pegins();
        assert_eq!(multiple_pegins.len(), 3);
        assert_eq!(multiple_pegins.iter().map(|p| p.amount).sum::<u64>(), 350000000); // 3.5 BTC total
    }

    #[tokio::test]
    async fn test_pegout_fixtures() {
        // Test peg-out data fixtures
        let pegout = mock_pegout_request();
        assert_eq!(pegout.amount, 50000000); // 0.5 BTC

        let multiple_pegouts = mock_multiple_pegouts();
        assert_eq!(multiple_pegouts.len(), 2);
        assert_eq!(multiple_pegouts.iter().map(|p| p.amount).sum::<u64>(), 100000000); // 1 BTC total
    }
}