//! Integration Tests for EngineActor
//!
//! Tests the complete EngineActor functionality with real or realistic mock clients.

use std::time::Duration;
use actix::prelude::*;
use tracing_test::traced_test;

use lighthouse_wrapper::types::{Hash256, Address};

use crate::types::*;
use super::super::{
    actor::EngineActor,
    messages::*,
    state::ExecutionState,
    config::EngineConfig,
    EngineResult,
};
use super::{
    helpers::*,
    mocks::{MockExecutionClient, MockClientConfig},
    TestConfig,
};

/// Integration test suite for EngineActor
struct EngineActorIntegrationTest {
    helper: EngineActorTestHelper,
    test_timeout: Duration,
}

impl EngineActorIntegrationTest {
    fn new() -> Self {
        let config = TestConfig::integration();
        Self {
            helper: EngineActorTestHelper::with_config(config.clone()),
            test_timeout: config.test_timeout,
        }
    }
    
    async fn setup(&mut self) -> EngineResult<()> {
        self.helper.start_with_mock().await?;
        
        // Wait for actor to initialize
        assert_completes_within(
            || self.helper.wait_for_ready(Duration::from_secs(10)),
            Duration::from_secs(15),
            "Actor initialization",
        ).await;
        
        Ok(())
    }
    
    async fn teardown(&mut self) -> EngineResult<()> {
        self.helper.shutdown(Duration::from_secs(5)).await
    }
}

#[actix_rt::test]
#[traced_test]
async fn test_actor_lifecycle() {
    let mut test = EngineActorIntegrationTest::new();
    
    // Test actor startup
    test.setup().await.expect("Setup should succeed");
    
    // Verify actor is in ready state
    let status = test.helper.get_status(false).await.expect("Should get status");
    assert!(matches!(status.execution_state, ExecutionState::Ready { .. }));
    assert!(status.client_healthy);
    
    // Test graceful shutdown
    test.teardown().await.expect("Teardown should succeed");
}

#[actix_rt::test]
#[traced_test]
async fn test_health_check_flow() {
    let mut test = EngineActorIntegrationTest::new();
    test.setup().await.expect("Setup should succeed");
    
    // Perform health check
    let result = test.helper.health_check().await;
    assert!(result.is_ok(), "Health check should succeed");
    
    // Verify health status in status response
    let status = test.helper.get_status(false).await.expect("Should get status");
    assert!(status.client_healthy, "Client should be healthy");
    
    test.teardown().await.expect("Teardown should succeed");
}

#[actix_rt::test]
#[traced_test]
async fn test_payload_build_and_execute_flow() {
    let mut test = EngineActorIntegrationTest::new();
    test.setup().await.expect("Setup should succeed");
    
    let parent_hash = Hash256::random();
    
    // Test payload building
    let build_result = test.helper.build_payload(parent_hash).await;
    assert!(build_result.is_ok(), "Payload build should succeed");
    
    let build_response = build_result.unwrap();
    assert!(build_response.payload_id.is_some(), "Should have payload ID");
    assert!(matches!(build_response.status, PayloadStatusType::Valid));
    
    // Test payload execution
    if let Some(payload_hash) = build_response.payload.as_ref().map(|p| p.block_hash) {
        let execute_result = test.helper.execute_payload(payload_hash).await;
        assert!(execute_result.is_ok(), "Payload execution should succeed");
        
        let execute_response = execute_result.unwrap();
        assert!(matches!(execute_response.status, PayloadStatusType::Valid));
    }
    
    test.teardown().await.expect("Teardown should succeed");
}

#[actix_rt::test]
#[traced_test]
async fn test_forkchoice_update_flow() {
    let mut test = EngineActorIntegrationTest::new();
    test.setup().await.expect("Setup should succeed");
    
    let head_hash = Hash256::random();
    let safe_hash = Hash256::random();
    let finalized_hash = Hash256::random();
    
    // Create forkchoice update message
    if let Some(actor) = &test.helper.actor {
        let msg = ForkchoiceUpdatedMessage {
            head_block_hash: head_hash,
            safe_block_hash: safe_hash,
            finalized_block_hash: finalized_hash,
            payload_attributes: None,
            correlation_id: Some(create_correlation_id()),
        };
        
        let result = actor.send(msg).await;
        assert!(result.is_ok(), "Mailbox should accept message");
        
        let response = result.unwrap();
        assert!(response.is_ok(), "Forkchoice update should succeed");
        
        let forkchoice_result = response.unwrap();
        assert!(matches!(forkchoice_result.payload_status, PayloadStatusType::Valid));
    }
    
    test.teardown().await.expect("Teardown should succeed");
}

#[actix_rt::test]
#[traced_test]
async fn test_sync_status_monitoring() {
    let mut test = EngineActorIntegrationTest::new();
    
    // Start with syncing client
    let config = TestConfig {
        use_mock_client: true,
        ..Default::default()
    };
    
    test.helper = EngineActorTestHelper::with_config(config);
    test.helper.start_with_mock().await.expect("Setup should succeed");
    
    // Check sync status
    if let Some(actor) = &test.helper.actor {
        let msg = super::super::handlers::sync_handlers::CheckSyncStatusMessage {
            include_details: true,
        };
        
        let result = actor.send(msg).await;
        assert!(result.is_ok(), "Sync status check should work");
        
        if let Ok(sync_status) = result.unwrap() {
            assert!(sync_status.client_healthy, "Client should be healthy");
        }
    }
    
    test.teardown().await.expect("Teardown should succeed");
}

#[actix_rt::test]
#[traced_test]
async fn test_performance_under_load() {
    let mut test = EngineActorIntegrationTest::new();
    test.setup().await.expect("Setup should succeed");
    
    let mut build_times = Vec::new();
    let num_payloads = 10;
    
    // Build multiple payloads and measure performance
    for i in 0..num_payloads {
        let parent_hash = Hash256::random();
        let start = std::time::Instant::now();
        
        let result = test.helper.build_payload(parent_hash).await;
        let duration = start.elapsed();
        
        assert!(result.is_ok(), "Payload build {} should succeed", i);
        build_times.push(duration);
        
        // Ensure we don't exceed reasonable build times
        assert!(
            duration < Duration::from_millis(500),
            "Payload build {} took too long: {:?}",
            i,
            duration
        );
    }
    
    // Calculate performance metrics
    let avg_build_time = build_times.iter().sum::<Duration>() / build_times.len() as u32;
    let max_build_time = build_times.iter().max().unwrap();
    
    println!("Average build time: {:?}", avg_build_time);
    println!("Maximum build time: {:?}", max_build_time);
    
    // Verify performance targets
    assert!(
        avg_build_time < Duration::from_millis(100),
        "Average build time should be under 100ms"
    );
    
    test.teardown().await.expect("Teardown should succeed");
}

#[actix_rt::test]
#[traced_test]
async fn test_error_recovery() {
    let config = TestConfig {
        use_mock_client: true,
        simulate_failures: true,
        failure_rate: 0.3, // 30% failure rate
        ..Default::default()
    };
    
    let mut test = EngineActorIntegrationTest {
        helper: EngineActorTestHelper::with_config(config.clone()),
        test_timeout: config.test_timeout,
    };
    
    test.setup().await.expect("Setup should succeed despite failures");
    
    // Attempt multiple operations, some should succeed despite failures
    let mut successes = 0;
    let mut failures = 0;
    
    for i in 0..20 {
        let parent_hash = Hash256::random();
        let result = test.helper.build_payload(parent_hash).await;
        
        match result {
            Ok(_) => {
                successes += 1;
                println!("Operation {} succeeded", i);
            },
            Err(e) => {
                failures += 1;
                println!("Operation {} failed: {}", i, e);
            }
        }
        
        // Small delay between operations
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    
    println!("Successes: {}, Failures: {}", successes, failures);
    
    // We should have some successes even with failures
    assert!(successes > 0, "Should have some successful operations");
    
    // Actor should still be responsive
    let status = test.helper.get_status(true).await.expect("Should get status");
    println!("Final actor state: {:?}", status.execution_state);
    
    test.teardown().await.expect("Teardown should succeed");
}

#[actix_rt::test]
#[traced_test]
async fn test_concurrent_operations() {
    let mut test = EngineActorIntegrationTest::new();
    test.setup().await.expect("Setup should succeed");
    
    let num_concurrent = 5;
    let mut handles = Vec::new();
    
    // Launch concurrent payload builds
    for i in 0..num_concurrent {
        let parent_hash = Hash256::random();
        
        if let Some(actor) = &test.helper.actor {
            let actor_clone = actor.clone();
            let handle = tokio::spawn(async move {
                let msg = BuildPayloadMessage {
                    parent_hash,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    fee_recipient: Address::zero(),
                    prev_randao: Hash256::random(),
                    withdrawals: vec![],
                    correlation_id: Some(format!("concurrent_{}", i)),
                };
                
                actor_clone.send(msg).await
            });
            
            handles.push(handle);
        }
    }
    
    // Wait for all operations to complete
    let mut successes = 0;
    for (i, handle) in handles.into_iter().enumerate() {
        match handle.await {
            Ok(Ok(Ok(_))) => {
                successes += 1;
                println!("Concurrent operation {} succeeded", i);
            },
            Ok(Ok(Err(e))) => {
                println!("Concurrent operation {} failed: {}", i, e);
            },
            Ok(Err(e)) => {
                println!("Concurrent operation {} mailbox error: {}", i, e);
            },
            Err(e) => {
                println!("Concurrent operation {} join error: {}", i, e);
            }
        }
    }
    
    println!("Concurrent successes: {}/{}", successes, num_concurrent);
    
    // Should handle concurrent operations successfully
    assert!(successes >= num_concurrent / 2, "Should handle concurrent operations");
    
    test.teardown().await.expect("Teardown should succeed");
}

#[actix_rt::test]
#[traced_test]
async fn test_state_transitions() {
    let mut test = EngineActorIntegrationTest::new();
    test.setup().await.expect("Setup should succeed");
    
    // Test transition to degraded state through configuration update
    if let Some(actor) = &test.helper.actor {
        // Send restart message to trigger state transition
        let restart_msg = RestartEngineMessage {
            reason: "Test state transition".to_string(),
            preserve_state: false,
        };
        
        let result = actor.send(restart_msg).await;
        assert!(result.is_ok(), "Restart message should be accepted");
        
        // Wait a bit for restart to process
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Check that actor recovered to ready state
        let recovered = test.helper.wait_for_ready(Duration::from_secs(5)).await;
        assert!(recovered, "Actor should recover to ready state");
    }
    
    test.teardown().await.expect("Teardown should succeed");
}

#[actix_rt::test]
#[traced_test]
async fn test_metrics_collection() {
    let mut test = EngineActorIntegrationTest::new();
    test.setup().await.expect("Setup should succeed");
    
    // Perform some operations to generate metrics
    for _ in 0..3 {
        let parent_hash = Hash256::random();
        let _ = test.helper.build_payload(parent_hash).await;
        let _ = test.helper.health_check().await;
    }
    
    // Get status with metrics
    let status = test.helper.get_status(true).await.expect("Should get status");
    
    if let Some(metrics) = status.metrics {
        println!("Collected metrics: {:?}", metrics);
        
        // Verify metrics are being collected
        assert!(metrics.payloads_built > 0, "Should have payload build metrics");
        assert!(status.uptime > Duration::ZERO, "Should have uptime metric");
    }
    
    test.teardown().await.expect("Teardown should succeed");
}

/// Test scenario using the scenario builder
#[actix_rt::test]
#[traced_test]
async fn test_complex_scenario() {
    let mut helper = EngineActorTestHelper::new();
    
    let scenario = TestScenarioBuilder::new()
        .with_timeout(Duration::from_secs(30))
        .step("Initialize actor", |helper| async move {
            helper.start_with_mock().await.map(|_| ())
        })
        .step("Wait for ready state", |helper| async move {
            let ready = helper.wait_for_ready(Duration::from_secs(10)).await;
            if ready {
                Ok(())
            } else {
                Err(super::super::EngineError::ActorError("Not ready".to_string()))
            }
        })
        .step("Build payload", |helper| async move {
            let parent_hash = Hash256::random();
            helper.build_payload(parent_hash).await.map(|_| ())
        })
        .step("Check health", |helper| async move {
            helper.health_check().await
        })
        .step("Get status with metrics", |helper| async move {
            helper.get_status(true).await.map(|_| ())
        });
    
    let result = scenario.execute(&mut helper).await.expect("Scenario should execute");
    
    println!("Scenario result: {:?}", result);
    assert!(result.success, "Complex scenario should succeed");
    assert!(result.failed_step.is_none(), "No step should fail");
    
    // Clean up
    helper.shutdown(Duration::from_secs(5)).await.expect("Cleanup should succeed");
}

#[cfg(test)]
mod load_tests {
    use super::*;
    
    #[actix_rt::test]
    #[traced_test]
    async fn test_sustained_load() {
        let mut test = EngineActorIntegrationTest::new();
        test.setup().await.expect("Setup should succeed");
        
        let duration = Duration::from_secs(10);
        let start = std::time::Instant::now();
        let mut operation_count = 0;
        
        // Run operations for specified duration
        while start.elapsed() < duration {
            let parent_hash = Hash256::random();
            
            match test.helper.build_payload(parent_hash).await {
                Ok(_) => operation_count += 1,
                Err(e) => println!("Load test operation failed: {}", e),
            }
            
            // Small delay to prevent overwhelming
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        
        println!(
            "Completed {} operations in {:?} ({:.2} ops/sec)",
            operation_count,
            duration,
            operation_count as f64 / duration.as_secs_f64()
        );
        
        // Verify minimum throughput
        let ops_per_second = operation_count as f64 / duration.as_secs_f64();
        assert!(
            ops_per_second > 10.0,
            "Should maintain at least 10 operations per second"
        );
        
        test.teardown().await.expect("Teardown should succeed");
    }
}