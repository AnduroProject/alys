//! Bridge Workflow Integration Tests
//! 
//! End-to-end testing of complete peg-in and peg-out workflows

use actix::prelude::*;
use std::time::Duration;
use tokio::time::sleep;

use crate::actors::bridge::{
    BridgeActor, PegInActor, PegOutActor, StreamActor,
    BridgeCoordinationMessage, PegInMessage, PegOutMessage, StreamMessage,
    BridgeSystemConfig, BridgeError
};
use crate::actors::bridge::tests::helpers::*;
use crate::types::*;

struct IntegrationTestSetup {
    bridge_actor: Addr<BridgeActor>,
    pegin_actor: Addr<PegInActor>,
    pegout_actor: Addr<PegOutActor>,
    stream_actor: Addr<StreamActor>,
    config: BridgeSystemConfig,
}

impl IntegrationTestSetup {
    async fn new() -> Result<Self, BridgeError> {
        let config = test_bridge_config();

        // Start all actors
        let bridge_actor = BridgeActor::new(config.clone()).start();
        let pegin_actor = PegInActor::new(config.clone()).start();
        let pegout_actor = PegOutActor::new(config.clone()).start(); 
        let stream_actor = StreamActor::new(config.clone()).start();

        // Initialize bridge system
        bridge_actor
            .send(BridgeCoordinationMessage::InitializeSystem)
            .await
            .map_err(|e| BridgeError::ActorCommunication { 
                message: format!("Failed to initialize bridge system: {}", e)
            })??;

        // Initialize individual actors
        pegin_actor
            .send(PegInMessage::Initialize)
            .await
            .map_err(|e| BridgeError::ActorCommunication {
                message: format!("Failed to initialize pegin actor: {}", e)
            })??;

        pegout_actor
            .send(PegOutMessage::Initialize)
            .await
            .map_err(|e| BridgeError::ActorCommunication {
                message: format!("Failed to initialize pegout actor: {}", e)
            })??;

        stream_actor
            .send(StreamMessage::Initialize)
            .await
            .map_err(|e| BridgeError::ActorCommunication {
                message: format!("Failed to initialize stream actor: {}", e)
            })??;

        // Register actors with bridge coordinator
        bridge_actor
            .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor.clone()))
            .await
            .map_err(|e| BridgeError::ActorCommunication {
                message: format!("Failed to register pegin actor: {}", e)
            })??;

        bridge_actor
            .send(BridgeCoordinationMessage::RegisterPegOutActor(pegout_actor.clone()))
            .await
            .map_err(|e| BridgeError::ActorCommunication {
                message: format!("Failed to register pegout actor: {}", e)
            })??;

        bridge_actor
            .send(BridgeCoordinationMessage::RegisterStreamActor(stream_actor.clone()))
            .await
            .map_err(|e| BridgeError::ActorCommunication {
                message: format!("Failed to register stream actor: {}", e)
            })??;

        Ok(Self {
            bridge_actor,
            pegin_actor,
            pegout_actor,
            stream_actor,
            config,
        })
    }

    async fn shutdown(self) -> Result<(), BridgeError> {
        // Shutdown in reverse order
        self.stream_actor
            .send(StreamMessage::Shutdown)
            .await
            .map_err(|e| BridgeError::ActorCommunication {
                message: format!("Failed to shutdown stream actor: {}", e)
            })??;

        self.pegout_actor
            .send(PegOutMessage::Shutdown)
            .await
            .map_err(|e| BridgeError::ActorCommunication {
                message: format!("Failed to shutdown pegout actor: {}", e)
            })??;

        self.pegin_actor
            .send(PegInMessage::Shutdown)
            .await
            .map_err(|e| BridgeError::ActorCommunication {
                message: format!("Failed to shutdown pegin actor: {}", e)
            })??;

        self.bridge_actor
            .send(BridgeCoordinationMessage::ShutdownSystem)
            .await
            .map_err(|e| BridgeError::ActorCommunication {
                message: format!("Failed to shutdown bridge system: {}", e)
            })??;

        Ok(())
    }
}

#[actix::test]
async fn test_complete_pegin_workflow() {
    let setup = IntegrationTestSetup::new().await.expect("Failed to setup test environment");

    // Create a test peg-in request
    let pegin_request = TestDataBuilder::test_pegin_request();
    let bitcoin_txid = pegin_request.bitcoin_txid;

    // Step 1: Coordinate peg-in through bridge actor
    let coordination_result = setup.bridge_actor
        .send(BridgeCoordinationMessage::CoordinatePegIn {
            pegin_id: "integration_test_pegin_001".to_string(),
            bitcoin_txid,
        })
        .await;

    assert!(coordination_result.is_ok());
    let coordination_response = coordination_result.unwrap();
    assert!(coordination_response.is_ok());

    // Step 2: Process the actual peg-in request
    let process_result = setup.pegin_actor
        .send(PegInMessage::ProcessRequest {
            request: pegin_request,
        })
        .await;

    assert!(process_result.is_ok());
    let process_response = process_result.unwrap();
    BridgeAssertions::assert_pegin_success(&process_response);

    // Step 3: Verify the peg-in status
    let status_result = setup.pegin_actor
        .send(PegInMessage::GetStatus {
            pegin_id: "integration_test_pegin_001".to_string(),
        })
        .await;

    assert!(status_result.is_ok());

    // Cleanup
    setup.shutdown().await.expect("Failed to shutdown test environment");
}

#[actix::test]
async fn test_complete_pegout_workflow() {
    let setup = IntegrationTestSetup::new().await.expect("Failed to setup test environment");

    // Create a test peg-out request
    let pegout_request = TestDataBuilder::test_pegout_request();
    let burn_tx_hash = pegout_request.burn_tx_hash;

    // Step 1: Coordinate peg-out through bridge actor
    let coordination_result = setup.bridge_actor
        .send(BridgeCoordinationMessage::CoordinatePegOut {
            pegout_id: "integration_test_pegout_001".to_string(),
            burn_tx_hash,
        })
        .await;

    assert!(coordination_result.is_ok());
    let coordination_response = coordination_result.unwrap();
    assert!(coordination_response.is_ok());

    // Step 2: Process the actual peg-out request
    let process_result = setup.pegout_actor
        .send(PegOutMessage::ProcessRequest {
            request: pegout_request,
        })
        .await;

    assert!(process_result.is_ok());
    let process_response = process_result.unwrap();
    BridgeAssertions::assert_pegout_success(&process_response);

    // Step 3: Verify the peg-out status
    let status_result = setup.pegout_actor
        .send(PegOutMessage::GetStatus {
            pegout_id: "integration_test_pegout_001".to_string(),
        })
        .await;

    assert!(status_result.is_ok());

    // Cleanup
    setup.shutdown().await.expect("Failed to shutdown test environment");
}

#[actix::test]
async fn test_concurrent_pegin_pegout_operations() {
    let setup = IntegrationTestSetup::new().await.expect("Failed to setup test environment");

    // Create concurrent requests
    let pegin_request = TestDataBuilder::test_pegin_request();
    let pegout_request = TestDataBuilder::test_pegout_request();

    // Start both operations concurrently
    let pegin_future = setup.bridge_actor
        .send(BridgeCoordinationMessage::CoordinatePegIn {
            pegin_id: "concurrent_pegin_001".to_string(),
            bitcoin_txid: pegin_request.bitcoin_txid,
        });

    let pegout_future = setup.bridge_actor
        .send(BridgeCoordinationMessage::CoordinatePegOut {
            pegout_id: "concurrent_pegout_001".to_string(),
            burn_tx_hash: pegout_request.burn_tx_hash,
        });

    // Wait for both to complete
    let (pegin_result, pegout_result) = tokio::join!(pegin_future, pegout_future);

    assert!(pegin_result.is_ok());
    assert!(pegin_result.unwrap().is_ok());

    assert!(pegout_result.is_ok());  
    assert!(pegout_result.unwrap().is_ok());

    // Process the actual requests
    let pegin_process_future = setup.pegin_actor
        .send(PegInMessage::ProcessRequest {
            request: pegin_request,
        });

    let pegout_process_future = setup.pegout_actor
        .send(PegOutMessage::ProcessRequest {
            request: pegout_request,
        });

    let (pegin_process_result, pegout_process_result) = tokio::join!(pegin_process_future, pegout_process_future);

    assert!(pegin_process_result.is_ok());
    BridgeAssertions::assert_pegin_success(&pegin_process_result.unwrap());

    assert!(pegout_process_result.is_ok());
    BridgeAssertions::assert_pegout_success(&pegout_process_result.unwrap());

    // Cleanup
    setup.shutdown().await.expect("Failed to shutdown test environment");
}

#[actix::test]
async fn test_governance_coordination_workflow() {
    let setup = IntegrationTestSetup::new().await.expect("Failed to setup test environment");

    // Establish connections for governance communication
    let connection_result = setup.stream_actor
        .send(StreamMessage::EstablishConnection {
            peer_id: "governance_peer_001".to_string(),
            endpoint: "ws://localhost:9944".to_string(),
        })
        .await;

    assert!(connection_result.is_ok());
    assert!(connection_result.unwrap().is_ok());

    // Send a governance message
    use crate::actors::bridge::GovernanceMessage;
    let governance_msg = GovernanceMessage {
        msg_type: "bridge_proposal".to_string(),
        proposal_id: "bridge_prop_001".to_string(),
        data: serde_json::json!({
            "title": "Increase Bridge Security",
            "description": "Proposal to increase minimum confirmations",
            "new_confirmations": 12
        }),
        timestamp: std::time::SystemTime::now(),
    };

    let send_result = setup.stream_actor
        .send(StreamMessage::SendGovernanceMessage {
            message: governance_msg,
            target_peers: vec!["governance_peer_001".to_string()],
        })
        .await;

    assert!(send_result.is_ok());
    assert!(send_result.unwrap().is_ok());

    // Verify connection status
    let status_result = setup.stream_actor
        .send(StreamMessage::GetConnectionStatus)
        .await;

    assert!(status_result.is_ok());

    // Cleanup
    setup.shutdown().await.expect("Failed to shutdown test environment");
}

#[actix::test]
async fn test_system_metrics_collection() {
    let setup = IntegrationTestSetup::new().await.expect("Failed to setup test environment");

    // Perform some operations to generate metrics
    let pegin_request = TestDataBuilder::test_pegin_request();
    let _process_result = setup.pegin_actor
        .send(PegInMessage::ProcessRequest {
            request: pegin_request,
        })
        .await;

    let pegout_request = TestDataBuilder::test_pegout_request();
    let _process_result = setup.pegout_actor
        .send(PegOutMessage::ProcessRequest {
            request: pegout_request,
        })
        .await;

    // Small delay to allow metrics to update
    sleep(Duration::from_millis(100)).await;

    // Collect metrics from all actors
    let bridge_metrics = setup.bridge_actor
        .send(BridgeCoordinationMessage::GetSystemMetrics)
        .await;

    let pegin_metrics = setup.pegin_actor
        .send(PegInMessage::GetMetrics)
        .await;

    let pegout_metrics = setup.pegout_actor
        .send(PegOutMessage::GetMetrics)
        .await;

    let stream_metrics = setup.stream_actor
        .send(StreamMessage::GetMetrics)
        .await;

    // Verify all metrics are accessible
    assert!(bridge_metrics.is_ok());
    assert!(pegin_metrics.is_ok());
    assert!(pegout_metrics.is_ok());
    assert!(stream_metrics.is_ok());

    // Cleanup
    setup.shutdown().await.expect("Failed to shutdown test environment");
}

#[actix::test]
async fn test_full_system_status_check() {
    let setup = IntegrationTestSetup::new().await.expect("Failed to setup test environment");

    // Get comprehensive system status
    let status_result = setup.bridge_actor
        .send(BridgeCoordinationMessage::GetSystemStatus)
        .await;

    assert!(status_result.is_ok());
    
    // The status should include information about all registered actors
    let status_response = status_result.unwrap();
    assert!(status_response.is_ok());

    // Verify individual actor statuses
    let pegin_status = setup.pegin_actor
        .send(PegInMessage::GetStatus {
            pegin_id: "status_check".to_string(),
        })
        .await;

    let pegout_status = setup.pegout_actor
        .send(PegOutMessage::GetStatus {
            pegout_id: "status_check".to_string(),
        })
        .await;

    let stream_status = setup.stream_actor
        .send(StreamMessage::GetConnectionStatus)
        .await;

    assert!(pegin_status.is_ok());
    assert!(pegout_status.is_ok());
    assert!(stream_status.is_ok());

    // Cleanup
    setup.shutdown().await.expect("Failed to shutdown test environment");
}

#[actix::test]
async fn test_graceful_system_shutdown() {
    let setup = IntegrationTestSetup::new().await.expect("Failed to setup test environment");

    // Start some operations
    let pegin_request = TestDataBuilder::test_pegin_request();
    let _coordination_result = setup.bridge_actor
        .send(BridgeCoordinationMessage::CoordinatePegIn {
            pegin_id: "shutdown_test_pegin".to_string(),
            bitcoin_txid: pegin_request.bitcoin_txid,
        })
        .await;

    // Allow some processing time
    sleep(Duration::from_millis(50)).await;

    // Perform graceful shutdown
    let shutdown_result = setup.shutdown().await;
    assert!(shutdown_result.is_ok());
}