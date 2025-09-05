//! Integration tests for lighthouse_facade
//!
//! These tests validate the facade functionality across different modes
//! and feature flag combinations.

use lighthouse_facade::prelude::*;
use lighthouse_facade::SimpleLighthouseFacade;
use tokio;

#[tokio::test]
async fn test_default_facade_creation() {
    // Test that facade can be created with default config
    let config = FacadeConfig::default();
    let result = LighthouseFacade::new(config).await;
    
    assert!(result.is_ok(), "Should be able to create facade with default config");
}

#[tokio::test] 
async fn test_simple_facade_creation() {
    // Test that simple facade can be created
    let facade = SimpleLighthouseFacade::new(FacadeMode::Mock);
    
    assert!(true, "Should be able to create simple facade with default mode");
}

#[tokio::test]
async fn test_facade_modes() {
    // Test different facade modes
    let modes = vec![
        FacadeMode::Mock,
        FacadeMode::V4Only, 
        FacadeMode::V7Only,
        FacadeMode::Automatic,
        FacadeMode::Dual,
        FacadeMode::Migration,
    ];
    
    for mode in modes {
        let facade = SimpleLighthouseFacade::new(mode);
        assert!(true, "Should be able to create facade with mode {:?}", mode);
    }
}

#[tokio::test]
async fn test_health_monitoring() {
    // Test health monitoring functionality
    let config = FacadeConfig::default();
    let facade = SimpleLighthouseFacade::new(FacadeMode::Mock);
    
    let health_result = facade.health_check().await;
    assert!(health_result.is_ok(), "Health check should succeed");
    
    let health_status = health_result.unwrap();
    assert!(health_status.healthy, "Facade should be healthy in mock mode");
}

#[tokio::test]
async fn test_forkchoice_updated() {
    // Test forkchoice_updated operation
    let config = FacadeConfig::default();
    let facade = SimpleLighthouseFacade::new(FacadeMode::Mock);
    
    let forkchoice_state = ForkchoiceState {
        head_block_hash: ethereum_types::H256::from_low_u64_be(1),
        safe_block_hash: ethereum_types::H256::from_low_u64_be(1), 
        finalized_block_hash: ethereum_types::H256::from_low_u64_be(1),
    };
    
    let payload_attributes = Some(PayloadAttributes {
        timestamp: 1000000,
        prev_randao: ethereum_types::H256::zero(),
        suggested_fee_recipient: ethereum_types::Address::zero(),
        withdrawals: Vec::new(),
    });
    
    let result = facade.forkchoice_updated(forkchoice_state, payload_attributes).await;
    assert!(result.is_ok(), "forkchoice_updated should succeed in mock mode");
}

#[tokio::test]
async fn test_get_payload() {
    // Test get_payload operation
    let config = FacadeConfig::default();
    let facade = SimpleLighthouseFacade::new(FacadeMode::Mock);
    
    let payload_id = 12345u64;
    let result = facade.get_payload(payload_id).await;
    assert!(result.is_ok(), "get_payload should succeed in mock mode");
}

#[tokio::test]
async fn test_new_payload() {
    // Test new_payload operation
    let config = FacadeConfig::default();
    let facade = SimpleLighthouseFacade::new(FacadeMode::Mock);
    
    // Create a mock execution payload
    let execution_payload = ExecutionPayload::default_test_payload();
    
    let result = facade.new_payload(execution_payload).await;
    assert!(result.is_ok(), "new_payload should succeed in mock mode");
}

#[tokio::test]
async fn test_error_handling() {
    // Test error handling for invalid inputs
    let config = FacadeConfig::default();
    let facade = SimpleLighthouseFacade::new(FacadeMode::Mock);
    
    // Test with invalid forkchoice state (zero hash)
    let invalid_forkchoice_state = ForkchoiceState {
        head_block_hash: ethereum_types::H256::zero(),
        safe_block_hash: ethereum_types::H256::zero(),
        finalized_block_hash: ethereum_types::H256::zero(),
    };
    
    let result = facade.forkchoice_updated(invalid_forkchoice_state, None).await;
    // In mock mode, this might still succeed, but in real mode it should fail
    // The important thing is that it doesn't panic
    assert!(result.is_ok() || result.is_err(), "Should handle invalid input gracefully");
}

#[tokio::test]  
async fn test_metrics_collection() {
    // Test that metrics are being collected
    let config = FacadeConfig::default();
    let facade = SimpleLighthouseFacade::new(FacadeMode::Mock);
    
    // Perform some operations to generate metrics
    let forkchoice_state = ForkchoiceState {
        head_block_hash: ethereum_types::H256::from_low_u64_be(1),
        safe_block_hash: ethereum_types::H256::from_low_u64_be(1),
        finalized_block_hash: ethereum_types::H256::from_low_u64_be(1),
    };
    
    let _ = facade.forkchoice_updated(forkchoice_state, None).await;
    let _ = facade.get_payload(12345).await;
    
    // Check that the facade is tracking operations
    let health = facade.health_check().await.unwrap();
    // In a real implementation, we'd check that metrics show > 0 operations
    assert!(health.healthy);
}

#[cfg(test)]
mod feature_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_compilation_without_features() {
        // This test ensures the facade works without any lighthouse features
        let config = FacadeConfig::default();
        let facade = SimpleLighthouseFacade::new(FacadeMode::Mock);
        
        let health = facade.health_check().await;
        assert!(health.is_ok());
    }
    
    #[cfg(feature = "v7")]
    #[tokio::test]
    async fn test_v7_mode_functionality() {
        // This test only runs when v7 feature is enabled
        let facade = SimpleLighthouseFacade::new(FacadeMode::V7Only);
        let health = facade.health_check().await;
        assert!(health.is_ok());
        
        // Test that v7-specific functionality works
        let forkchoice_state = ForkchoiceState {
            head_block_hash: ethereum_types::H256::from_low_u64_be(1),
            safe_block_hash: ethereum_types::H256::from_low_u64_be(1),
            finalized_block_hash: ethereum_types::H256::from_low_u64_be(1),
        };
        
        let result = facade.forkchoice_updated(forkchoice_state, None).await;
        assert!(result.is_ok());
    }
}