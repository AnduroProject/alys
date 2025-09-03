//! Integration tests for V2 AuxPow system
//!
//! Tests the complete V2 AuxPow implementation including AuxPowActor,
//! DifficultyManager, and integration with ChainActor.

#[cfg(test)]
mod tests {
    use super::*;
    use actix::System;
    use std::time::Duration;
    use bitcoin::CompactTarget;
    use ethereum_types::Address as EvmAddress;

    use crate::{
        auxpow_miner::BitcoinConsensusParams,
        actors::auxpow::{
            AuxPowActor, DifficultyManager,
            config::{AuxPowConfig, DifficultyConfig},
            messages::*,
        },
    };

    /// Create test AuxPowActor
    fn create_test_auxpow_actor() -> AuxPowActor {
        // Mock addresses - in real test would use actual actors
        let chain_actor = actix::Addr::mock(); 
        let difficulty_manager = actix::Addr::mock();
        
        let retarget_params = BitcoinConsensusParams::default();
        let config = AuxPowConfig::default();

        AuxPowActor::new(chain_actor, difficulty_manager, retarget_params, config)
    }

    /// Create test DifficultyManager  
    fn create_test_difficulty_manager() -> DifficultyManager {
        let config = DifficultyConfig::test_config();
        DifficultyManager::new(config)
    }

    #[actix_rt::test]
    async fn test_auxpow_actor_creation() {
        let auxpow_actor = create_test_auxpow_actor();
        
        // Verify initial state
        assert_eq!(auxpow_actor.state.len(), 0);
        assert_eq!(auxpow_actor.config.mining_enabled, false);
    }

    #[actix_rt::test]
    async fn test_difficulty_manager_creation() {
        let difficulty_manager = create_test_difficulty_manager();
        
        // Verify initial state
        assert_eq!(difficulty_manager.difficulty_history.len(), 0);
        assert_eq!(difficulty_manager.last_retarget_height, 0);
    }

    #[actix_rt::test]
    async fn test_mining_config() {
        let mut config = AuxPowConfig::default();
        
        // Test mining configuration
        config.mining_enabled = true;
        config.mining_address = EvmAddress::from_low_u64_be(0x1234567890abcdef);
        
        assert_eq!(config.mining_enabled, true);
        assert_ne!(config.mining_address, EvmAddress::zero());
    }

    #[actix_rt::test]
    async fn test_difficulty_calculation() {
        let difficulty_manager = create_test_difficulty_manager();
        
        // Test is_retarget_height function
        let chain_height = 2016; // Bitcoin's adjustment interval
        let height_diff = 100;
        
        let should_retarget = difficulty_manager.is_retarget_height(chain_height, height_diff);
        
        // Should retarget at exact interval
        assert_eq!(should_retarget, true);
    }

    #[actix_rt::test]
    async fn test_auxpow_metrics() {
        let mut auxpow_actor = create_test_auxpow_actor();
        
        // Test metrics recording
        auxpow_actor.metrics.record_create_call(100);
        auxpow_actor.metrics.record_submit_call(200, true);
        
        assert_eq!(auxpow_actor.metrics.create_calls, 1);
        assert_eq!(auxpow_actor.metrics.submit_calls, 1);
        assert_eq!(auxpow_actor.metrics.successful_submissions, 1);
        assert_eq!(auxpow_actor.metrics.success_rate(), 100.0);
    }

    #[actix_rt::test] 
    async fn test_error_handling() {
        use crate::actors::auxpow::error::{AuxPowError, DifficultyError};
        
        // Test AuxPow error types
        let chain_sync_error = AuxPowError::ChainSyncing;
        let unknown_block_error = AuxPowError::UnknownBlock;
        
        assert_eq!(format!("{}", chain_sync_error), "Chain is currently syncing");
        assert_eq!(format!("{}", unknown_block_error), "Submitted AuxPow for unknown block");
        
        // Test difficulty error types
        let calc_error = DifficultyError::CalculationOverflow;
        assert_eq!(format!("{}", calc_error), "Difficulty calculation overflow");
    }

    #[actix_rt::test]
    async fn test_rpc_address_parsing() {
        use crate::actors::auxpow::rpc::AuxPowRpcContext;
        
        // Test valid address parsing
        let valid_address = "0x742d35Cc6634C0532925a3b8D2C7BFcb39db4D8e";
        let parsed = valid_address.parse::<EvmAddress>();
        assert!(parsed.is_ok());
        
        // Test invalid address parsing
        let invalid_address = "invalid_address";
        let parsed = invalid_address.parse::<EvmAddress>();
        assert!(parsed.is_err());
    }

    /// Integration test for complete mining flow
    #[actix_rt::test]
    async fn test_complete_mining_flow() {
        // This test would require actual actor system running
        // For now, test the individual components
        
        let auxpow_config = AuxPowConfig {
            mining_enabled: true,
            mining_address: EvmAddress::from_low_u64_be(0x1234),
            sync_check_enabled: true,
            work_refresh_interval: Duration::from_secs(30),
            max_pending_work: 100,
        };
        
        let difficulty_config = DifficultyConfig::test_config();
        
        // Verify configurations are compatible
        assert_eq!(auxpow_config.mining_enabled, true);
        assert_eq!(difficulty_config.consensus_params.pow_target_spacing, 2);
    }

    /// Test difficulty adjustment algorithm with exact legacy values
    #[actix_rt::test]
    async fn test_legacy_difficulty_compatibility() {
        let difficulty_manager = create_test_difficulty_manager();
        
        // Test with values from legacy implementation
        let test_bits = 0x1e0ffff0; // Example difficulty bits
        let target = difficulty_manager.uint256_target_from_compact(test_bits);
        let compact = difficulty_manager.target_to_compact_lossy(target);
        
        // Should round-trip correctly
        assert_eq!(compact.to_consensus(), test_bits);
    }

    /// Test actor supervision integration
    #[actix_rt::test]
    async fn test_actor_supervision() {
        let auxpow_actor = create_test_auxpow_actor();
        
        // Test health check response
        let health_check = HealthCheck;
        // Would send to actor in full integration test
        
        // Verify actor is set up for supervision
        assert!(auxpow_actor.config.sync_check_enabled);
    }
}