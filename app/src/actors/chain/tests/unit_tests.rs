//! Unit Tests for Chain Actor
//!
//! Core unit tests for individual ChainActor components.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use actix::prelude::*;
use uuid::Uuid;

use crate::actors::chain::{ChainActor, config::*, messages::*, state::*};
use crate::types::*;
use crate::features::FeatureFlagManager;

#[cfg(test)]
mod chain_actor_tests {
    use super::*;
    
    /// Create a test ChainActor with minimal configuration
    async fn create_test_chain_actor() -> Addr<ChainActor> {
        let config = ChainActorConfig::test_config();
        let actor_addresses = create_test_actor_addresses();
        let feature_flags = Arc::new(TestFeatureFlagManager::new());
        
        ChainActor::new(config, actor_addresses, feature_flags)
            .expect("Failed to create test ChainActor")
            .start()
    }

    /// Create test actor addresses with mock actors
    fn create_test_actor_addresses() -> ActorAddresses {
        ActorAddresses {
            engine: TestEngineActor.start(),
            bridge: TestBridgeActor.start(),
            storage: TestStorageActor.start(),
            network: TestNetworkActor.start(),
            sync: Some(TestSyncActor.start()),
            supervisor: TestRootSupervisor.start(),
        }
    }

    /// Create a test block with specified slot and parent
    fn create_test_block(slot: u64, parent: Hash256) -> SignedConsensusBlock {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        
        let block = ConsensusBlock {
            parent_hash: parent,
            slot,
            auxpow_header: None,
            execution_payload: create_test_execution_payload(),
            pegins: Vec::new(),
            pegout_payment_proposal: None,
            finalized_pegouts: Vec::new(),
            lighthouse_metadata: LighthouseMetadata {
                beacon_block_root: None,
                beacon_state_root: None,
                randao_reveal: None,
                graffiti: Some([0u8; 32]),
                proposer_index: None,
                bls_aggregate_signature: None,
                sync_committee_signature: None,
                sync_committee_bits: None,
            },
            timing: BlockTiming {
                production_started_at: std::time::SystemTime::now(),
                produced_at: std::time::SystemTime::now(),
                received_at: None,
                validation_started_at: None,
                validation_completed_at: None,
                import_completed_at: None,
                processing_duration_ms: None,
            },
            validation_info: ValidationInfo {
                status: BlockValidationStatus::Pending,
                validation_errors: Vec::new(),
                checkpoints: Vec::new(),
                gas_validation: GasValidation {
                    expected_gas_limit: 8000000,
                    actual_gas_used: 0,
                    utilization_percent: 0.0,
                    is_valid: true,
                    base_fee_valid: true,
                    priority_fee_valid: true,
                },
                state_validation: StateValidation {
                    pre_state_root: parent,
                    post_state_root: Hash256::zero(),
                    expected_state_root: Hash256::zero(),
                    state_root_valid: true,
                    storage_proofs_valid: true,
                    account_changes: 0,
                    storage_changes: 0,
                },
                consensus_validation: ConsensusValidation {
                    signature_valid: false,
                    proposer_valid: true,
                    slot_valid: true,
                    parent_valid: true,
                    difficulty_valid: true,
                    auxpow_valid: None,
                    committee_signatures_valid: true,
                },
            },
            actor_metadata: ActorBlockMetadata {
                processing_actor: Some("TestActor".to_string()),
                correlation_id: Some(uuid::Uuid::new_v4()),
                trace_context: TraceContext {
                    trace_id: Some(uuid::Uuid::new_v4().to_string()),
                    span_id: Some(uuid::Uuid::new_v4().to_string()),
                    parent_span_id: None,
                    baggage: std::collections::HashMap::new(),
                    sampled: false,
                },
                priority: BlockProcessingPriority::Normal,
                retry_info: RetryInfo {
                    attempt: 0,
                    max_attempts: 1,
                    backoff_strategy: BackoffStrategy::Fixed { delay_ms: 100 },
                    next_retry_at: None,
                    last_failure_reason: None,
                },
                actor_metrics: ActorProcessingMetrics {
                    queue_time_ms: None,
                    processing_time_ms: None,
                    memory_usage_bytes: None,
                    cpu_time_ms: None,
                    messages_sent: 0,
                    messages_received: 0,
                },
            },
        };
        
        SignedConsensusBlock {
            message: block,
            signature: Signature::random(),
        }
    }

    fn create_test_execution_payload() -> ExecutionPayload {
        ExecutionPayload {
            parent_hash: Hash256::random(),
            fee_recipient: Address::random(),
            state_root: Hash256::random(),
            receipts_root: Hash256::random(),
            logs_bloom: [0; 256],
            prev_randao: Hash256::random(),
            block_number: 1,
            gas_limit: 30000000,
            gas_used: 0,
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            extra_data: vec![],
            base_fee_per_gas: 1000000000u64.into(),
            block_hash: Hash256::random(),
            transactions: vec![],
            withdrawals: vec![],
        }
    }

    #[actix::test]
    async fn test_chain_actor_startup() {
        let chain_actor = create_test_chain_actor().await;
        
        let status = chain_actor.send(GetChainStatus).await
            .expect("Failed to send GetChainStatus message")
            .expect("GetChainStatus failed");
        
        assert_eq!(status.head_height, 0);
        assert!(status.head_hash.is_zero());
    }

    #[actix::test]
    async fn test_block_production() {
        let chain_actor = create_test_chain_actor().await;
        
        let block = chain_actor.send(ProduceBlock::new(1, Duration::from_secs(1000)))
            .await
            .expect("Failed to send ProduceBlock message");
        
        match block {
            Ok(produced_block) => {
                assert_eq!(produced_block.message.slot, 1);
                assert!(!produced_block.message.hash().is_zero());
            }
            Err(ChainError::NotOurSlot) => {
                // This is expected for non-validator nodes
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[actix::test]
    async fn test_block_import() {
        let chain_actor = create_test_chain_actor().await;
        let test_block = create_test_block(1, Hash256::zero());
        
        let result = chain_actor.send(ImportBlock {
            block: test_block.clone(),
            broadcast: false,
        }).await
            .expect("Failed to send ImportBlock message")
            .expect("ImportBlock failed");
        
        // Verify block was imported
        let status = chain_actor.send(GetChainStatus).await
            .expect("Failed to send GetChainStatus message")
            .expect("GetChainStatus failed");
        
        assert_eq!(status.head_height, 1);
        assert_eq!(status.head_hash, test_block.message.hash());
    }

    #[actix::test]
    async fn test_block_validation() {
        let chain_actor = create_test_chain_actor().await;
        let invalid_block = create_invalid_test_block();
        
        let result = chain_actor.send(ValidateBlock {
            block: invalid_block,
        }).await
            .expect("Failed to send ValidateBlock message");
        
        match result {
            Ok(false) | Err(_) => {
                // Expected for invalid block
            }
            Ok(true) => panic!("Invalid block was validated as correct"),
        }
    }

    #[actix::test]
    async fn test_chain_reorganization() {
        let chain_actor = create_test_chain_actor().await;
        
        // Build initial chain A (height 1-3)
        let mut chain_a = Vec::new();
        let mut parent_hash = Hash256::zero();
        
        for i in 1..=3 {
            let block = create_test_block(i, parent_hash);
            parent_hash = block.message.hash();
            chain_a.push(block.clone());
            
            chain_actor.send(ImportBlock {
                block,
                broadcast: false,
            }).await
                .expect("Failed to send ImportBlock message")
                .expect("ImportBlock failed");
        }
        
        // Verify initial state
        let status = chain_actor.send(GetChainStatus).await
            .expect("Failed to send GetChainStatus message")
            .expect("GetChainStatus failed");
        
        assert_eq!(status.head_height, 3);
        assert_eq!(status.head_hash, chain_a[2].message.hash());
        
        // Create competing chain B (height 1-4, heavier)
        let mut chain_b = Vec::new();
        parent_hash = Hash256::zero();
        
        for i in 1..=4 {
            let mut block = create_test_block(i, parent_hash);
            if i > 1 {
                // Make chain B heavier by increasing difficulty
                block.message.execution_payload.block_number = i + 1000; // Simulate higher total difficulty
            }
            parent_hash = block.message.hash();
            chain_b.push(block);
        }
        
        // Import competing chain - should trigger reorg
        for block in &chain_b {
            chain_actor.send(ImportBlock {
                block: block.clone(),
                broadcast: false,
            }).await
                .expect("Failed to send ImportBlock message")
                .expect("ImportBlock failed");
        }
        
        // Verify reorg happened
        let final_status = chain_actor.send(GetChainStatus).await
            .expect("Failed to send GetChainStatus message")
            .expect("GetChainStatus failed");
        
        assert_eq!(final_status.head_height, 4);
        assert_eq!(final_status.head_hash, chain_b[3].message.hash());
    }

    #[actix::test]
    async fn test_auxpow_finalization() {
        let chain_actor = create_test_chain_actor().await;
        
        // Import some blocks
        let block1 = create_test_block(1, Hash256::zero());
        let block2 = create_test_block(2, block1.message.hash());
        
        chain_actor.send(ImportBlock {
            block: block1,
            broadcast: false,
        }).await
            .expect("Failed to send ImportBlock message")
            .expect("ImportBlock failed");
        
        chain_actor.send(ImportBlock {
            block: block2.clone(),
            broadcast: false,
        }).await
            .expect("Failed to send ImportBlock message")
            .expect("ImportBlock failed");
        
        // Submit AuxPoW header for finalization
        let auxpow_header = create_test_auxpow_header(2, block2.message.hash());
        
        let result = chain_actor.send(SubmitAuxPowHeader {
            pow_header: auxpow_header,
        }).await
            .expect("Failed to send SubmitAuxPowHeader message")
            .expect("SubmitAuxPowHeader failed");
        
        // Verify finalization
        let status = chain_actor.send(GetChainStatus).await
            .expect("Failed to send GetChainStatus message")
            .expect("GetChainStatus failed");
        
        assert_eq!(status.finalized_height, Some(2));
        assert_eq!(status.finalized_hash, Some(block2.message.hash()));
    }

    #[actix::test]
    async fn test_federation_update() {
        let chain_actor = create_test_chain_actor().await;
        
        let new_members = vec![
            FederationMember {
                public_key: PublicKey::random(),
                address: Address::random(),
                weight: 1,
            },
            FederationMember {
                public_key: PublicKey::random(),
                address: Address::random(),
                weight: 1,
            },
        ];
        
        let result = chain_actor.send(UpdateFederation {
            version: 2,
            members: new_members.clone(),
            threshold: 1,
        }).await
            .expect("Failed to send UpdateFederation message")
            .expect("UpdateFederation failed");
        
        let status = chain_actor.send(GetChainStatus).await
            .expect("Failed to send GetChainStatus message")
            .expect("GetChainStatus failed");
        
        assert_eq!(status.federation_version, 2);
    }

    #[actix::test]
    async fn test_block_subscription() {
        let chain_actor = create_test_chain_actor().await;
        let subscriber = TestBlockSubscriber.start();
        
        let result = chain_actor.send(SubscribeToBlocks {
            subscriber: subscriber.clone().recipient(),
            event_types: vec![BlockEventType::NewBlock, BlockEventType::Finalization],
        }).await
            .expect("Failed to send SubscribeToBlocks message")
            .expect("SubscribeToBlocks failed");
        
        let subscription_id = result;
        
        // Import a block - should trigger notification
        let test_block = create_test_block(1, Hash256::zero());
        
        chain_actor.send(ImportBlock {
            block: test_block,
            broadcast: false,
        }).await
            .expect("Failed to send ImportBlock message")
            .expect("ImportBlock failed");
        
        // Wait for notification
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Unsubscribe
        chain_actor.send(UnsubscribeFromBlocks {
            subscription_id,
        }).await
            .expect("Failed to send UnsubscribeFromBlocks message")
            .expect("UnsubscribeFromBlocks failed");
    }

    #[actix::test]
    async fn test_health_monitoring() {
        let chain_actor = create_test_chain_actor().await;
        
        // Wait for initial health check
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Query health status
        let health = chain_actor.send(GetActorHealth).await
            .expect("Failed to send GetActorHealth message")
            .expect("GetActorHealth failed");
        
        assert!(health.health_score > 50); // Should be healthy initially
        assert!(health.is_active);
    }

    #[actix::test]
    async fn test_performance_metrics() {
        let chain_actor = create_test_chain_actor().await;
        
        // Perform some operations
        for i in 1..=10 {
            let block = create_test_block(i, if i == 1 { Hash256::zero() } else { Hash256::random() });
            
            let _ = chain_actor.send(ImportBlock {
                block,
                broadcast: false,
            }).await;
        }
        
        let metrics = chain_actor.send(GetPerformanceMetrics).await
            .expect("Failed to send GetPerformanceMetrics message")
            .expect("GetPerformanceMetrics failed");
        
        assert!(metrics.blocks_imported > 0);
        assert!(metrics.avg_import_time_ms > 0.0);
    }

    #[actix::test]
    async fn test_error_recovery() {
        let chain_actor = create_test_chain_actor().await;
        
        // Send invalid block to trigger error
        let invalid_block = create_invalid_test_block();
        
        let result = chain_actor.send(ImportBlock {
            block: invalid_block,
            broadcast: false,
        }).await
            .expect("Failed to send ImportBlock message");
        
        assert!(result.is_err());
        
        // Verify actor is still functional after error
        let status = chain_actor.send(GetChainStatus).await
            .expect("Failed to send GetChainStatus message")
            .expect("GetChainStatus failed");
        
        assert_eq!(status.head_height, 0); // Should still be at genesis
    }

    // Helper functions for test data creation
    
    fn create_invalid_test_block() -> SignedConsensusBlock {
        let mut block = create_test_block(1, Hash256::zero());
        // Make it invalid by setting slot to 0
        block.message.slot = 0;
        block
    }

    fn create_test_auxpow_header(height: u64, block_hash: Hash256) -> AuxPowHeader {
        AuxPowHeader {
            height,
            block_hash,
            difficulty: U256::from(1000),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
            parent_block_hash: Hash256::random(),
            committed_bundle_hash: Hash256::random(),
            merkle_path: vec![Hash256::random(), Hash256::random()],
        }
    }
}

// Mock actors for testing
struct TestEngineActor;
struct TestBridgeActor;
struct TestStorageActor;
struct TestNetworkActor;
struct TestSyncActor;
struct TestRootSupervisor;
struct TestBlockSubscriber;

impl Actor for TestEngineActor { type Context = Context<Self>; }
impl Actor for TestBridgeActor { type Context = Context<Self>; }
impl Actor for TestStorageActor { type Context = Context<Self>; }
impl Actor for TestNetworkActor { type Context = Context<Self>; }
impl Actor for TestSyncActor { type Context = Context<Self>; }
impl Actor for TestRootSupervisor { type Context = Context<Self>; }
impl Actor for TestBlockSubscriber { type Context = Context<Self>; }

// Mock feature flag manager
struct TestFeatureFlagManager;

impl TestFeatureFlagManager {
    fn new() -> Self {
        Self
    }
}

impl FeatureFlagManager for TestFeatureFlagManager {
    fn is_enabled(&self, _flag: &crate::features::FeatureFlag) -> bool {
        true // Enable all features for testing
    }
}

// Test configuration
impl ChainActorConfig {
    fn test_config() -> Self {
        Self {
            is_validator: false, // Most tests don't need validation
            slot_duration: Duration::from_secs(2),
            max_blocks_without_pow: 10,
            authority_key: None,
            federation_config: None,
            performance_targets: PerformanceTargets::default(),
            max_pending_blocks: 1000,
            validation_cache_size: 100,
        }
    }
}

impl Default for PerformanceTargets {
    fn default() -> Self {
        Self {
            max_production_time_ms: 1000,
            max_import_time_ms: 500,
            max_validation_time_ms: 200,
            max_finalization_time_ms: 100,
            max_queue_depth: 100,
        }
    }
}