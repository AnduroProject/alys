//! Test Helpers for Network Actor System
//! 
//! Common utilities, fixtures, and helper functions for testing the network
//! actor system components.

#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use tempfile::TempDir;

#[cfg(test)]
use crate::actors::network::*;
#[cfg(test)]
use crate::actors::network::messages::*;

/// Create a test configuration for SyncActor optimized for testing
#[cfg(test)]
pub fn test_sync_config() -> SyncConfig {
    let mut config = SyncConfig::default();
    config.max_parallel_downloads = 2; // Reduce for testing
    config.validation_workers = 1;     // Single worker for predictability
    config.batch_size = 10;           // Small batches
    config.checkpoint_interval = 5;   // Frequent checkpoints for testing
    config.health_check_interval = Duration::from_millis(100);
    config.request_timeout = Duration::from_secs(1);
    config
}

/// Create a test configuration for NetworkActor optimized for testing  
#[cfg(test)]
pub fn test_network_config() -> NetworkConfig {
    NetworkConfig::lightweight() // Use lightweight config for tests
}

/// Create a test configuration for PeerActor optimized for testing
#[cfg(test)]
pub fn test_peer_config() -> PeerConfig {
    let mut config = PeerConfig::default();
    config.max_peers = 10;           // Small number for testing
    config.connection_timeout = Duration::from_secs(1);
    config.health_check_interval = Duration::from_millis(100);
    config.federation_peer_limit = 3;
    config
}

/// Create test supervision configuration
#[cfg(test)]
pub fn test_supervision_config() -> NetworkSupervisionConfig {
    let mut config = NetworkSupervisionConfig::default();
    config.health_check_interval = Duration::from_millis(100);
    config.sync_restart_policy = RestartPolicy::immediate();
    config.network_restart_policy = RestartPolicy::immediate();
    config.peer_restart_policy = RestartPolicy::immediate();
    config
}

/// Create a temporary directory for checkpoint testing
#[cfg(test)]
pub fn create_test_checkpoint_dir() -> TempDir {
    TempDir::new().expect("Failed to create temporary directory for testing")
}

/// Create test block data
#[cfg(test)]
pub fn create_test_block_data(height: u64) -> BlockData {
    BlockData {
        height,
        hash: ethereum_types::H256::random(),
        parent_hash: if height == 0 { 
            ethereum_types::H256::zero() 
        } else { 
            ethereum_types::H256::random() 
        },
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        data: vec![height as u8; 100], // Simple test data
        signature: None,
    }
}

/// Create test chain state for checkpoint testing
#[cfg(test)]
pub fn create_test_chain_state(height: u64) -> ChainState {
    use std::collections::HashMap;
    
    ChainState {
        height,
        state_root: ethereum_types::H256::random(),
        block_hashes: (0..=height).map(|h| (h, ethereum_types::H256::random())).collect(),
        peer_states: HashMap::new(),
        federation_state: FederationCheckpointState {
            current_authorities: vec!["test_authority".to_string()],
            current_slot: height / 2,
            last_finalized_block: height.saturating_sub(1),
            emergency_mode: false,
        },
        block_count: height,
        metadata: HashMap::new(),
    }
}

/// Mock peer ID for testing
#[cfg(test)]
pub fn create_test_peer_id() -> libp2p::PeerId {
    libp2p::PeerId::random()
}

/// Mock multiaddr for testing
#[cfg(test)]
pub fn create_test_multiaddr(port: u16) -> libp2p::Multiaddr {
    format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap()
}

/// Create test peer info
#[cfg(test)]
pub fn create_test_peer_info(peer_id: libp2p::PeerId, is_federation: bool) -> PeerInfo {
    use std::time::SystemTime;
    
    PeerInfo {
        peer_id,
        addresses: vec![create_test_multiaddr(4001)],
        connection_status: ConnectionStatus::Connected,
        protocols: vec!["sync".to_string(), "gossip".to_string()],
        peer_type: if is_federation { PeerType::Federation } else { PeerType::Regular },
        score: PeerScore {
            overall_score: if is_federation { 95.0 } else { 75.0 },
            latency_score: 20.0,
            throughput_score: 80.0,
            reliability_score: 90.0,
            federation_bonus: if is_federation { 20.0 } else { 0.0 },
            last_updated: SystemTime::now(),
        },
        connection_time: Some(SystemTime::now()),
        last_seen: SystemTime::now(),
        statistics: PeerStatistics {
            messages_sent: 100,
            messages_received: 150,
            bytes_sent: 50000,
            bytes_received: 75000,
            average_latency_ms: 25.0,
            success_rate: 0.98,
            last_activity: SystemTime::now(),
            connection_uptime: Duration::from_secs(3600),
        },
    }
}

/// Test actor startup helper
#[cfg(test)]
pub struct TestActorSystem {
    pub sync_actor: Option<actix::Addr<SyncActor>>,
    pub network_actor: Option<actix::Addr<NetworkActor>>,
    pub peer_actor: Option<actix::Addr<PeerActor>>,
    pub supervisor: Option<actix::Addr<NetworkSupervisor>>,
}

#[cfg(test)]
impl TestActorSystem {
    pub fn new() -> Self {
        Self {
            sync_actor: None,
            network_actor: None,
            peer_actor: None,
            supervisor: None,
        }
    }

    pub async fn start_sync_actor(&mut self) -> Result<(), ActorError> {
        let config = test_sync_config();
        let actor = SyncActor::new(config)?;
        self.sync_actor = Some(actor.start());
        Ok(())
    }

    pub async fn start_network_actor(&mut self) -> Result<(), ActorError> {
        let config = test_network_config();
        let actor = NetworkActor::new(config)?;
        self.network_actor = Some(actor.start());
        Ok(())
    }

    pub async fn start_peer_actor(&mut self) -> Result<(), ActorError> {
        let config = test_peer_config();
        let actor = PeerActor::new(config)?;
        self.peer_actor = Some(actor.start());
        Ok(())
    }

    pub fn start_supervisor(&mut self) -> Result<(), ActorError> {
        let config = test_supervision_config();
        let supervisor = NetworkSupervisor::new(config);
        self.supervisor = Some(supervisor.start());
        Ok(())
    }

    pub async fn start_all(&mut self) -> Result<(), ActorError> {
        self.start_sync_actor().await?;
        self.start_network_actor().await?;
        self.start_peer_actor().await?;
        self.start_supervisor()?;
        Ok(())
    }

    pub async fn verify_all_healthy(&self) -> bool {
        let mut all_healthy = true;

        if let Some(sync_actor) = &self.sync_actor {
            if let Ok(response) = sync_actor.send(GetSyncStatus).await {
                all_healthy &= response.is_ok();
            } else {
                all_healthy = false;
            }
        }

        if let Some(network_actor) = &self.network_actor {
            if let Ok(response) = network_actor.send(GetNetworkStatus).await {
                all_healthy &= response.is_ok();
            } else {
                all_healthy = false;
            }
        }

        if let Some(peer_actor) = &self.peer_actor {
            if let Ok(response) = peer_actor.send(GetPeerStatus { peer_id: None }).await {
                all_healthy &= response.is_ok();
            } else {
                all_healthy = false;
            }
        }

        all_healthy
    }
}

/// Performance measurement helper
#[cfg(test)]
pub struct PerformanceTracker {
    start_time: std::time::Instant,
    measurements: Vec<(String, Duration)>,
}

#[cfg(test)]
impl PerformanceTracker {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            measurements: Vec::new(),
        }
    }

    pub fn measure<F, R>(&mut self, operation_name: &str, operation: F) -> R
    where
        F: FnOnce() -> R,
    {
        let start = std::time::Instant::now();
        let result = operation();
        let duration = start.elapsed();
        self.measurements.push((operation_name.to_string(), duration));
        result
    }

    pub async fn measure_async<F, Fut, R>(&mut self, operation_name: &str, operation: F) -> R
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = R>,
    {
        let start = std::time::Instant::now();
        let result = operation().await;
        let duration = start.elapsed();
        self.measurements.push((operation_name.to_string(), duration));
        result
    }

    pub fn get_measurements(&self) -> &[(String, Duration)] {
        &self.measurements
    }

    pub fn total_time(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn print_report(&self) {
        println!("Performance Report:");
        println!("Total time: {:?}", self.total_time());
        for (name, duration) in &self.measurements {
            println!("  {}: {:?}", name, duration);
        }
    }
}

/// Message envelope helper for testing
#[cfg(test)]
pub fn create_test_message_envelope<T>(message: T, priority: MessagePriority) -> MessageEnvelope<T> {
    MessageEnvelope::new(message)
        .with_priority(priority)
        .with_max_retries(3)
}

/// Assert that a result contains a network error of specific type
#[cfg(test)]
pub fn assert_network_error(result: &NetworkResult<()>, expected_error_type: &str) {
    match result {
        Err(error) => {
            let error_string = format!("{:?}", error);
            assert!(error_string.contains(expected_error_type), 
                   "Expected error type '{}' but got: {:?}", expected_error_type, error);
        }
        Ok(_) => panic!("Expected error but got success"),
    }
}

/// Wait for a condition with timeout
#[cfg(test)]
pub async fn wait_for_condition<F>(mut condition: F, timeout: Duration) -> bool
where
    F: FnMut() -> bool,
{
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

/// Create mock sync operation for testing
#[cfg(test)]
pub fn create_test_sync_operation(operation_id: String, height_range: (u64, u64)) -> SyncOperation {
    SyncOperation {
        operation_id,
        start_height: height_range.0,
        end_height: height_range.1,
        mode: SyncMode::Fast,
        started_at: std::time::Instant::now(),
        progress: 0.0,
        assigned_peers: vec!["test_peer".to_string()],
        blocks_downloaded: 0,
        blocks_validated: 0,
        blocks_applied: 0,
        status: SyncStatus::Discovery,
        error_count: 0,
    }
}

/// Network event simulator for testing
#[cfg(test)]
pub struct NetworkEventSimulator {
    events: Vec<SimulatedNetworkEvent>,
    current_time: std::time::Instant,
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub enum SimulatedNetworkEvent {
    PeerConnected(libp2p::PeerId),
    PeerDisconnected(libp2p::PeerId),
    MessageReceived { from: libp2p::PeerId, data: Vec<u8> },
    NetworkPartition(Duration),
    NetworkRecovery,
}

#[cfg(test)]
impl NetworkEventSimulator {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            current_time: std::time::Instant::now(),
        }
    }

    pub fn add_event(&mut self, event: SimulatedNetworkEvent) {
        self.events.push(event);
    }

    pub fn simulate_peer_churn(&mut self, peer_count: usize, duration: Duration) {
        for i in 0..peer_count {
            let peer_id = libp2p::PeerId::random();
            self.add_event(SimulatedNetworkEvent::PeerConnected(peer_id));
            
            // Simulate some activity
            self.add_event(SimulatedNetworkEvent::MessageReceived {
                from: peer_id,
                data: vec![i as u8; 100],
            });
            
            // Some peers disconnect
            if i % 3 == 0 {
                self.add_event(SimulatedNetworkEvent::PeerDisconnected(peer_id));
            }
        }
    }

    pub fn simulate_network_partition(&mut self, duration: Duration) {
        self.add_event(SimulatedNetworkEvent::NetworkPartition(duration));
        self.add_event(SimulatedNetworkEvent::NetworkRecovery);
    }

    pub fn get_events(&self) -> &[SimulatedNetworkEvent] {
        &self.events
    }
}

// Assertions and validation helpers

#[cfg(test)]
pub fn assert_sync_status_valid(status: &SyncStatus) {
    assert!(status.sync_progress >= 0.0 && status.sync_progress <= 1.0);
    assert!(status.blocks_per_second >= 0.0);
    
    if status.target_height.is_some() {
        let target = status.target_height.unwrap();
        assert!(status.current_height <= target);
    }
    
    if status.can_produce_blocks {
        assert!(status.sync_progress >= 0.995); // Must meet 99.5% threshold
    }
}

#[cfg(test)]
pub fn assert_network_status_valid(status: &NetworkStatus) {
    assert!(status.local_peer_id.to_string().len() > 0);
    assert!(status.connected_peers >= 0);
    assert!(status.pending_connections >= 0);
    assert!(!status.active_protocols.is_empty());
}

#[cfg(test)]
pub fn assert_peer_status_valid(status: &PeerStatus) {
    assert!(status.total_peers >= status.peers.len() as u32);
    assert!(status.federation_peers <= status.total_peers);
    
    for peer in &status.peers {
        assert!(!peer.addresses.is_empty());
        assert!(peer.score.overall_score >= 0.0 && peer.score.overall_score <= 100.0);
        assert!(peer.statistics.success_rate >= 0.0 && peer.statistics.success_rate <= 1.0);
    }
}

// Test data builders

#[cfg(test)]
pub struct TestDataBuilder;

#[cfg(test)]
impl TestDataBuilder {
    pub fn sync_status() -> TestSyncStatusBuilder {
        TestSyncStatusBuilder::new()
    }

    pub fn network_status() -> TestNetworkStatusBuilder {
        TestNetworkStatusBuilder::new()
    }

    pub fn peer_info() -> TestPeerInfoBuilder {
        TestPeerInfoBuilder::new()
    }
}

#[cfg(test)]
pub struct TestSyncStatusBuilder {
    status: SyncStatus,
}

#[cfg(test)]
impl TestSyncStatusBuilder {
    pub fn new() -> Self {
        Self {
            status: SyncStatus {
                is_syncing: false,
                current_height: 0,
                target_height: None,
                sync_progress: 0.0,
                blocks_per_second: 0.0,
                eta_seconds: None,
                connected_peers: 0,
                active_downloads: 0,
                validation_queue_size: 0,
                can_produce_blocks: false,
                last_block_hash: None,
                sync_mode: SyncMode::Fast,
                checkpoint_info: None,
            },
        }
    }

    pub fn syncing(mut self) -> Self {
        self.status.is_syncing = true;
        self
    }

    pub fn progress(mut self, progress: f64) -> Self {
        self.status.sync_progress = progress;
        self.status.can_produce_blocks = progress >= 0.995;
        self
    }

    pub fn height(mut self, current: u64, target: Option<u64>) -> Self {
        self.status.current_height = current;
        self.status.target_height = target;
        self
    }

    pub fn throughput(mut self, bps: f64) -> Self {
        self.status.blocks_per_second = bps;
        self
    }

    pub fn build(self) -> SyncStatus {
        self.status
    }
}

#[cfg(test)]
pub struct TestNetworkStatusBuilder {
    status: NetworkStatus,
}

#[cfg(test)]
impl TestNetworkStatusBuilder {
    pub fn new() -> Self {
        Self {
            status: NetworkStatus {
                is_active: false,
                local_peer_id: libp2p::PeerId::random(),
                listening_addresses: vec![],
                connected_peers: 0,
                pending_connections: 0,
                total_bandwidth_in: 0,
                total_bandwidth_out: 0,
                active_protocols: vec![],
                gossip_topics: vec![],
                discovery_status: DiscoveryStatus {
                    mdns_enabled: false,
                    kad_routing_table_size: 0,
                    bootstrap_peers_connected: 0,
                    total_discovered_peers: 0,
                },
            },
        }
    }

    pub fn active(mut self) -> Self {
        self.status.is_active = true;
        self
    }

    pub fn peers(mut self, connected: u32) -> Self {
        self.status.connected_peers = connected;
        self
    }

    pub fn protocols(mut self, protocols: Vec<String>) -> Self {
        self.status.active_protocols = protocols;
        self
    }

    pub fn build(self) -> NetworkStatus {
        self.status
    }
}

#[cfg(test)]
pub struct TestPeerInfoBuilder {
    peer_info: PeerInfo,
}

#[cfg(test)]
impl TestPeerInfoBuilder {
    pub fn new() -> Self {
        use std::time::SystemTime;
        
        Self {
            peer_info: PeerInfo {
                peer_id: libp2p::PeerId::random(),
                addresses: vec![],
                connection_status: ConnectionStatus::Disconnected,
                protocols: vec![],
                peer_type: PeerType::Regular,
                score: PeerScore {
                    overall_score: 50.0,
                    latency_score: 50.0,
                    throughput_score: 50.0,
                    reliability_score: 50.0,
                    federation_bonus: 0.0,
                    last_updated: SystemTime::now(),
                },
                connection_time: None,
                last_seen: SystemTime::now(),
                statistics: PeerStatistics {
                    messages_sent: 0,
                    messages_received: 0,
                    bytes_sent: 0,
                    bytes_received: 0,
                    average_latency_ms: 0.0,
                    success_rate: 1.0,
                    last_activity: SystemTime::now(),
                    connection_uptime: Duration::from_secs(0),
                },
            },
        }
    }

    pub fn federation(mut self) -> Self {
        self.peer_info.peer_type = PeerType::Federation;
        self.peer_info.score.federation_bonus = 20.0;
        self.peer_info.score.overall_score = 90.0;
        self
    }

    pub fn connected(mut self) -> Self {
        self.peer_info.connection_status = ConnectionStatus::Connected;
        self.peer_info.connection_time = Some(std::time::SystemTime::now());
        self
    }

    pub fn score(mut self, score: f64) -> Self {
        self.peer_info.score.overall_score = score;
        self
    }

    pub fn build(self) -> PeerInfo {
        self.peer_info
    }
}