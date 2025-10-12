//! NetworkActor V2 Test Fixtures
//!
//! Test data generation for NetworkActor V2 testing.
//! Following StorageActor fixture patterns.

use uuid::Uuid;
use std::time::{SystemTime, Duration};
use std::collections::HashMap;

use crate::actors_v2::network::{
    NetworkConfig, SyncConfig,
    messages::{GossipMessage, NetworkRequest, PeerId, Block},
    behaviour::AlysNetworkBehaviourEvent,
};
use crate::actors_v2::testing::network::{TestPeer, TestBlock, NetworkTestError};

/// Create test NetworkConfig for various scenarios
pub fn create_test_network_config() -> NetworkConfig {
    NetworkConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_peers: vec![
            "/ip4/127.0.0.1/tcp/8001".to_string(),
            "/ip4/127.0.0.1/tcp/8002".to_string(),
        ],
        max_connections: 50,
        connection_timeout: Duration::from_secs(10),
        gossip_topics: vec![
            "test-blocks".to_string(),
            "test-transactions".to_string(),
            "test-mdns".to_string(),
        ],
        message_size_limit: 1024 * 1024,
        discovery_interval: Duration::from_secs(30),
    }
}

/// Create test SyncConfig for various scenarios
pub fn create_test_sync_config() -> SyncConfig {
    SyncConfig {
        max_blocks_per_request: 32,
        sync_timeout: Duration::from_secs(10),
        max_concurrent_requests: 4,
        block_validation_timeout: Duration::from_secs(5),
        max_sync_peers: 8,
    }
}

/// Create minimal test NetworkConfig
pub fn create_minimal_network_config() -> NetworkConfig {
    NetworkConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_peers: vec![],
        max_connections: 10,
        connection_timeout: Duration::from_secs(5),
        gossip_topics: vec!["test-minimal".to_string()],
        message_size_limit: 64 * 1024,
        discovery_interval: Duration::from_secs(60),
    }
}

/// Create performance test NetworkConfig
pub fn create_performance_network_config() -> NetworkConfig {
    NetworkConfig {
        listen_addresses: vec![
            "/ip4/0.0.0.0/tcp/8000".to_string(),
            "/ip4/0.0.0.0/tcp/8001".to_string(),
        ],
        bootstrap_peers: vec![
            "/ip4/127.0.0.1/tcp/9000".to_string(),
            "/ip4/127.0.0.1/tcp/9001".to_string(),
            "/ip4/127.0.0.1/tcp/9002".to_string(),
        ],
        max_connections: 200,
        connection_timeout: Duration::from_secs(30),
        gossip_topics: vec![
            "perf-blocks".to_string(),
            "perf-transactions".to_string(),
            "perf-mdns".to_string(),
            "perf-metadata".to_string(),
        ],
        message_size_limit: 50 * 1024 * 1024, // 50MB for performance tests
        discovery_interval: Duration::from_secs(15),
    }
}

/// Create test peer set for various scenarios
pub fn create_test_peer_set(peer_count: usize, include_mdns: bool) -> HashMap<String, TestPeer> {
    let mut peers = HashMap::new();

    // Bootstrap peers (20% of total)
    let bootstrap_count = (peer_count as f32 * 0.2).ceil() as usize;
    for i in 0..bootstrap_count {
        let peer_id = format!("bootstrap-peer-{}", i);
        let address = format!("/ip4/127.0.0.{}/tcp/800{}", i + 1, i);
        let peer = TestPeer::new_bootstrap(peer_id.clone(), address);
        peers.insert(peer_id, peer);
    }

    // mDNS peers (30% of total if enabled)
    if include_mdns {
        let mdns_count = (peer_count as f32 * 0.3).ceil() as usize;
        for i in 0..mdns_count {
            let peer_id = format!("mdns-peer-{}", i);
            let address = format!("/ip4/192.168.1.{}/tcp/8000", i + 100);
            let peer = TestPeer::new_mdns(peer_id.clone(), address);
            peers.insert(peer_id, peer);
        }
    }

    // Regular network peers (remaining)
    let regular_count = peer_count - peers.len();
    for i in 0..regular_count {
        let peer_id = format!("network-peer-{}", i);
        let address = format!("/ip4/10.0.0.{}/tcp/8000", i + 100);
        let peer = TestPeer::new_regular(peer_id.clone(), address);
        peers.insert(peer_id, peer);
    }

    peers
}

/// Create test gossip message
pub fn create_test_gossip_message(topic: &str, message_content: &str) -> GossipMessage {
    GossipMessage {
        topic: topic.to_string(),
        data: message_content.as_bytes().to_vec(),
        message_id: Uuid::new_v4().to_string(),
    }
}

/// Create test block gossip message
pub fn create_test_block_gossip_message(block_height: u64) -> GossipMessage {
    let block_data = format!("{{\"height\":{},\"data\":\"test-block-{}\"}}", block_height, block_height);
    create_test_gossip_message("test-blocks", &block_data)
}

/// Create test transaction gossip message
pub fn create_test_transaction_gossip_message(tx_hash: &str) -> GossipMessage {
    let tx_data = format!("{{\"hash\":\"{}\",\"data\":\"test-transaction\"}}", tx_hash);
    create_test_gossip_message("test-transactions", &tx_data)
}

/// Create test mDNS gossip message
pub fn create_test_mdns_gossip_message(peer_id: &str, addresses: &[String]) -> GossipMessage {
    let announcement_data = format!(
        "{{\"peer_id\":\"{}\",\"addresses\":{:?}}}",
        peer_id, addresses
    );
    create_test_gossip_message("test-mdns", &announcement_data)
}

/// Create test block sequence for sync testing
pub fn create_test_block_sequence(start_height: u64, count: u32) -> Vec<TestBlock> {
    (0..count)
        .map(|i| TestBlock::new(start_height + i as u64))
        .collect()
}

/// Create test network request
pub fn create_test_block_request(start_height: u64, count: u32) -> NetworkRequest {
    NetworkRequest::GetBlocks { start_height, count }
}

/// Create test network requests for various scenarios
pub fn create_test_network_requests() -> Vec<NetworkRequest> {
    vec![
        NetworkRequest::GetBlocks { start_height: 100, count: 10 },
        NetworkRequest::GetBlocks { start_height: 200, count: 50 },
        NetworkRequest::GetChainStatus,
        NetworkRequest::GetPeers,
        NetworkRequest::GetStatus,
    ]
}

/// Create test behaviour events for various scenarios
pub fn create_test_behaviour_events() -> Vec<AlysNetworkBehaviourEvent> {
    vec![
        AlysNetworkBehaviourEvent::GossipMessage {
            topic: "test-blocks".to_string(),
            data: b"test block data".to_vec(),
            source_peer: "test-peer-1".to_string(),
            message_id: Uuid::new_v4().to_string(),
        },
        AlysNetworkBehaviourEvent::PeerConnected {
            peer_id: "test-peer-2".to_string(),
            address: "/ip4/127.0.0.1/tcp/8000".to_string(),
        },
        AlysNetworkBehaviourEvent::PeerIdentified {
            peer_id: "test-peer-3".to_string(),
            protocols: vec!["/alys/block/1.0.0".to_string()],
            addresses: vec!["/ip4/127.0.0.1/tcp/8001".to_string()],
        },
        AlysNetworkBehaviourEvent::MdnsPeerDiscovered {
            peer_id: "mdns-test-peer".to_string(),
            addresses: vec!["/ip4/192.168.1.100/tcp/8000".to_string()],
        },
        AlysNetworkBehaviourEvent::PeerDisconnected {
            peer_id: "test-peer-4".to_string(),
            reason: "Connection timeout".to_string(),
        },
    ]
}

/// Create large test data for performance testing
pub fn create_large_test_block(height: u64, size_mb: usize) -> TestBlock {
    let data_size = size_mb * 1024 * 1024;
    let mut data = Vec::with_capacity(data_size);

    // Fill with pseudo-random data
    for i in 0..data_size {
        data.push((i % 256) as u8);
    }

    TestBlock {
        height,
        data,
        hash: format!("large-block-hash-{}", height),
        parent_hash: format!("large-block-parent-{}", height.saturating_sub(1)),
        timestamp: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    }
}

/// Create test blocks for chaos testing
pub fn create_chaos_test_blocks(count: usize, variable_sizes: bool) -> Vec<TestBlock> {
    (0..count)
        .map(|i| {
            if variable_sizes {
                // Variable size blocks for chaos testing
                let size_kb = 1 + (i % 100); // 1KB to 100KB
                let mut data = Vec::with_capacity(size_kb * 1024);
                for j in 0..size_kb * 1024 {
                    data.push((j % 256) as u8);
                }

                TestBlock {
                    height: i as u64,
                    data,
                    hash: format!("chaos-block-{}", i),
                    parent_hash: if i == 0 {
                        "genesis".to_string()
                    } else {
                        format!("chaos-block-{}", i - 1)
                    },
                    timestamp: SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                }
            } else {
                TestBlock::new(i as u64)
            }
        })
        .collect()
}

/// Create invalid test configurations for validation testing
pub fn create_invalid_network_configs() -> Vec<(NetworkConfig, &'static str)> {
    vec![
        (
            NetworkConfig {
                listen_addresses: vec![], // Invalid: empty
                ..create_test_network_config()
            },
            "empty listen addresses"
        ),
        (
            NetworkConfig {
                max_connections: 0, // Invalid: zero connections
                ..create_test_network_config()
            },
            "zero max connections"
        ),
        (
            NetworkConfig {
                message_size_limit: 0, // Invalid: zero message size
                ..create_test_network_config()
            },
            "zero message size limit"
        ),
    ]
}

/// Create invalid test configurations for sync validation testing
pub fn create_invalid_sync_configs() -> Vec<(SyncConfig, &'static str)> {
    vec![
        (
            SyncConfig {
                max_blocks_per_request: 0, // Invalid: zero blocks
                ..create_test_sync_config()
            },
            "zero max blocks per request"
        ),
        (
            SyncConfig {
                max_concurrent_requests: 0, // Invalid: zero requests
                ..create_test_sync_config()
            },
            "zero max concurrent requests"
        ),
        (
            SyncConfig {
                max_sync_peers: 0, // Invalid: zero peers
                ..create_test_sync_config()
            },
            "zero max sync peers"
        ),
    ]
}

/// Create test scenario data for property testing
pub struct NetworkPropertyTestData {
    pub peer_scenarios: Vec<PeerScenario>,
    pub message_scenarios: Vec<MessageScenario>,
    pub sync_scenarios: Vec<SyncScenario>,
}

#[derive(Debug, Clone)]
pub struct PeerScenario {
    pub peer_count: usize,
    pub mdns_ratio: f32,
    pub bootstrap_ratio: f32,
    pub connection_success_rate: f32,
}

#[derive(Debug, Clone)]
pub struct MessageScenario {
    pub message_count: usize,
    pub topics: Vec<String>,
    pub message_sizes: Vec<usize>,
    pub failure_rate: f32,
}

#[derive(Debug, Clone)]
pub struct SyncScenario {
    pub start_height: u64,
    pub target_height: u64,
    pub block_sizes: Vec<usize>,
    pub peer_count: usize,
    pub request_pattern: RequestPattern,
}

#[derive(Debug, Clone)]
pub enum RequestPattern {
    Sequential,
    Parallel,
    RandomOrder,
    ChunkedParallel(u32),
}

impl NetworkPropertyTestData {
    pub fn new() -> Self {
        Self {
            peer_scenarios: Self::create_peer_scenarios(),
            message_scenarios: Self::create_message_scenarios(),
            sync_scenarios: Self::create_sync_scenarios(),
        }
    }

    fn create_peer_scenarios() -> Vec<PeerScenario> {
        vec![
            PeerScenario {
                peer_count: 5,
                mdns_ratio: 0.6, // High mDNS ratio
                bootstrap_ratio: 0.2,
                connection_success_rate: 0.9,
            },
            PeerScenario {
                peer_count: 20,
                mdns_ratio: 0.3, // Balanced
                bootstrap_ratio: 0.2,
                connection_success_rate: 0.8,
            },
            PeerScenario {
                peer_count: 50,
                mdns_ratio: 0.1, // Low mDNS ratio
                bootstrap_ratio: 0.1,
                connection_success_rate: 0.7,
            },
        ]
    }

    fn create_message_scenarios() -> Vec<MessageScenario> {
        vec![
            MessageScenario {
                message_count: 100,
                topics: vec!["test-blocks".to_string()],
                message_sizes: vec![1024, 2048, 4096],
                failure_rate: 0.05,
            },
            MessageScenario {
                message_count: 500,
                topics: vec![
                    "test-blocks".to_string(),
                    "test-transactions".to_string(),
                ],
                message_sizes: vec![512, 1024, 8192, 16384],
                failure_rate: 0.1,
            },
            MessageScenario {
                message_count: 1000,
                topics: vec![
                    "test-blocks".to_string(),
                    "test-transactions".to_string(),
                    "test-mdns".to_string(),
                ],
                message_sizes: vec![256, 512, 1024, 2048, 32768],
                failure_rate: 0.15,
            },
        ]
    }

    fn create_sync_scenarios() -> Vec<SyncScenario> {
        vec![
            SyncScenario {
                start_height: 0,
                target_height: 100,
                block_sizes: vec![1024, 2048],
                peer_count: 3,
                request_pattern: RequestPattern::Sequential,
            },
            SyncScenario {
                start_height: 100,
                target_height: 500,
                block_sizes: vec![2048, 4096, 8192],
                peer_count: 5,
                request_pattern: RequestPattern::Parallel,
            },
            SyncScenario {
                start_height: 500,
                target_height: 1000,
                block_sizes: vec![4096, 8192, 16384],
                peer_count: 8,
                request_pattern: RequestPattern::ChunkedParallel(32),
            },
        ]
    }
}

/// Create test data for chaos testing
pub struct NetworkChaosTestData {
    pub failure_scenarios: Vec<FailureScenario>,
    pub recovery_scenarios: Vec<RecoveryScenario>,
    pub load_scenarios: Vec<LoadScenario>,
}

#[derive(Debug, Clone)]
pub struct FailureScenario {
    pub scenario_name: String,
    pub failure_type: FailureType,
    pub failure_duration: Duration,
    pub failure_intensity: f32, // 0.0 to 1.0
}

#[derive(Debug, Clone)]
pub enum FailureType {
    NetworkPartition,
    PeerChurn,
    MessageLoss,
    SlowNetwork,
    MemoryPressure,
}

#[derive(Debug, Clone)]
pub struct RecoveryScenario {
    pub scenario_name: String,
    pub recovery_type: RecoveryType,
    pub expected_recovery_time: Duration,
}

#[derive(Debug, Clone)]
pub enum RecoveryType {
    AutomaticRecovery,
    ManualRecovery,
    PartialRecovery,
    GradualRecovery,
}

#[derive(Debug, Clone)]
pub struct LoadScenario {
    pub scenario_name: String,
    pub concurrent_operations: usize,
    pub operation_rate: f32, // operations per second
    pub duration: Duration,
}

impl NetworkChaosTestData {
    pub fn new() -> Self {
        Self {
            failure_scenarios: Self::create_failure_scenarios(),
            recovery_scenarios: Self::create_recovery_scenarios(),
            load_scenarios: Self::create_load_scenarios(),
        }
    }

    fn create_failure_scenarios() -> Vec<FailureScenario> {
        vec![
            FailureScenario {
                scenario_name: "Network Partition".to_string(),
                failure_type: FailureType::NetworkPartition,
                failure_duration: Duration::from_secs(30),
                failure_intensity: 0.5,
            },
            FailureScenario {
                scenario_name: "High Peer Churn".to_string(),
                failure_type: FailureType::PeerChurn,
                failure_duration: Duration::from_secs(60),
                failure_intensity: 0.7,
            },
            FailureScenario {
                scenario_name: "Message Loss".to_string(),
                failure_type: FailureType::MessageLoss,
                failure_duration: Duration::from_secs(45),
                failure_intensity: 0.3,
            },
            FailureScenario {
                scenario_name: "Slow Network".to_string(),
                failure_type: FailureType::SlowNetwork,
                failure_duration: Duration::from_secs(120),
                failure_intensity: 0.4,
            },
        ]
    }

    fn create_recovery_scenarios() -> Vec<RecoveryScenario> {
        vec![
            RecoveryScenario {
                scenario_name: "Network Healing".to_string(),
                recovery_type: RecoveryType::AutomaticRecovery,
                expected_recovery_time: Duration::from_secs(15),
            },
            RecoveryScenario {
                scenario_name: "Peer Reconnection".to_string(),
                recovery_type: RecoveryType::GradualRecovery,
                expected_recovery_time: Duration::from_secs(30),
            },
            RecoveryScenario {
                scenario_name: "Sync State Recovery".to_string(),
                recovery_type: RecoveryType::AutomaticRecovery,
                expected_recovery_time: Duration::from_secs(20),
            },
        ]
    }

    fn create_load_scenarios() -> Vec<LoadScenario> {
        vec![
            LoadScenario {
                scenario_name: "Low Load".to_string(),
                concurrent_operations: 5,
                operation_rate: 10.0,
                duration: Duration::from_secs(30),
            },
            LoadScenario {
                scenario_name: "Medium Load".to_string(),
                concurrent_operations: 20,
                operation_rate: 50.0,
                duration: Duration::from_secs(60),
            },
            LoadScenario {
                scenario_name: "High Load".to_string(),
                concurrent_operations: 50,
                operation_rate: 100.0,
                duration: Duration::from_secs(120),
            },
        ]
    }
}

/// Helper functions for test validation
pub fn validate_test_peer(peer: &TestPeer) -> Result<(), NetworkTestError> {
    if peer.peer_id.is_empty() {
        return Err(NetworkTestError::Validation("Peer ID cannot be empty".to_string()));
    }

    if peer.address.is_empty() || !peer.address.starts_with('/') {
        return Err(NetworkTestError::Validation(
            format!("Invalid peer address: {}", peer.address)
        ));
    }

    if peer.reputation < 0.0 || peer.reputation > 100.0 {
        return Err(NetworkTestError::Validation(
            format!("Invalid peer reputation: {}", peer.reputation)
        ));
    }

    Ok(())
}

/// Validate test block data
pub fn validate_test_block(block: &TestBlock) -> Result<(), NetworkTestError> {
    if block.data.is_empty() {
        return Err(NetworkTestError::Validation("Block data cannot be empty".to_string()));
    }

    if block.hash.is_empty() {
        return Err(NetworkTestError::Validation("Block hash cannot be empty".to_string()));
    }

    if block.data.len() > 100 * 1024 * 1024 { // 100MB max
        return Err(NetworkTestError::Validation(
            format!("Block too large: {} bytes", block.data.len())
        ));
    }

    Ok(())
}

/// Create test configuration variants for edge case testing
pub fn create_edge_case_configs() -> Vec<(NetworkConfig, &'static str)> {
    vec![
        (
            NetworkConfig {
                max_connections: 1, // Minimal connections
                ..create_test_network_config()
            },
            "minimal connections"
        ),
        (
            NetworkConfig {
                connection_timeout: Duration::from_millis(100), // Very short timeout
                ..create_test_network_config()
            },
            "short timeout"
        ),
        (
            NetworkConfig {
                message_size_limit: 1024, // Small message limit
                ..create_test_network_config()
            },
            "small message limit"
        ),
        (
            NetworkConfig {
                discovery_interval: Duration::from_secs(1), // Very frequent discovery
                ..create_test_network_config()
            },
            "frequent discovery"
        ),
    ]
}