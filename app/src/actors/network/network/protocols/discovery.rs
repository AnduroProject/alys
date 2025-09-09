//! Discovery Protocol Implementation
//! 
//! Combined Kademlia DHT and mDNS discovery for robust peer finding
//! with federation peer prioritization and NAT traversal support.

use libp2p::{
    kad::{
        Behaviour as Kademlia, Event as KademliaEvent, Config as KademliaConfig, QueryResult, GetClosestPeersResult,
        BootstrapResult, Record, store::MemoryStore, AddProviderResult, GetProvidersResult,
        GetRecordResult, PutRecordResult, QueryId,
    },
    mdns::{tokio::Behaviour as Mdns, Event as MdnsEvent},
    identity::Keypair,
    PeerId, Multiaddr,
    swarm::{NetworkBehaviour, ToSwarm},
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use std::task::{Context, Poll};

/// Alys discovery protocol combining Kademlia DHT and mDNS
#[derive(NetworkBehaviour)]
pub struct AlysDiscovery {
    /// Kademlia DHT for global peer discovery
    kademlia: Kademlia<MemoryStore>,
    /// mDNS for local network discovery
    mdns: Mdns,
    /// Discovery configuration
    config: DiscoveryConfig,
    /// Known federation peers for prioritization
    federation_peers: HashSet<PeerId>,
    /// Discovery metrics and statistics
    metrics: DiscoveryMetrics,
    /// Active discovery queries
    active_queries: HashMap<QueryId, DiscoveryQuery>,
    /// Bootstrap status tracking
    bootstrap_status: BootstrapStatus,
    /// Peer discovery cache
    peer_cache: HashMap<PeerId, DiscoveredPeer>,
}

impl AlysDiscovery {
    /// Create a new Alys discovery instance
    pub fn new(
        keypair: &Keypair,
        config: DiscoveryConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let local_peer_id = PeerId::from(keypair.public());

        // Configure Kademlia DHT
        let store = MemoryStore::new(local_peer_id);
        let mut kad_config = KademliaConfig::default();
        
        // Optimize for blockchain network characteristics
        kad_config.set_query_timeout(Duration::from_secs(30)); // Longer timeout for reliability
        kad_config.set_replication_factor(config.replication_factor.try_into().unwrap());
        kad_config.set_parallelism(config.kad_parallelism.try_into().unwrap());
        kad_config.disjoint_query_paths(true); // Use disjoint paths for better reliability
        kad_config.set_max_packet_size(4096); // Larger packets for blockchain data
        
        let mut kademlia = Kademlia::with_config(local_peer_id, store, kad_config);

        // Add bootstrap peers
        for (peer_id, addresses) in &config.bootstrap_peers {
            for addr in addresses {
                kademlia.add_address(peer_id, addr.clone());
            }
        }

        // Configure mDNS for local discovery
        let mdns = Mdns::new(libp2p::mdns::Config::default())
            .map_err(|e| format!("Failed to create mDNS: {}", e))?;

        Ok(Self {
            kademlia,
            mdns,
            config,
            federation_peers: HashSet::new(),
            metrics: DiscoveryMetrics::default(),
            active_queries: HashMap::new(),
            bootstrap_status: BootstrapStatus::NotStarted,
            peer_cache: HashMap::new(),
        })
    }

    /// Start bootstrap process to connect to the DHT network
    pub fn bootstrap(&mut self) -> Result<QueryId, libp2p::kad::BootstrapError> {
        tracing::info!("Starting Kademlia bootstrap process");
        self.bootstrap_status = BootstrapStatus::InProgress;
        let query_id = self.kademlia.bootstrap()?;
        
        // Track bootstrap query
        self.active_queries.insert(query_id, DiscoveryQuery {
            query_id,
            query_type: QueryType::Bootstrap,
            started_at: Instant::now(),
            target: None,
        });

        Ok(query_id)
    }

    /// Find closest peers to a specific peer ID
    pub fn get_closest_peers(&mut self, peer_id: PeerId) -> QueryId {
        tracing::debug!("Searching for closest peers to {}", peer_id);
        let query_id = self.kademlia.get_closest_peers(peer_id);
        
        self.active_queries.insert(query_id, DiscoveryQuery {
            query_id,
            query_type: QueryType::GetClosestPeers,
            started_at: Instant::now(),
            target: Some(peer_id.to_string()),
        });

        self.metrics.queries_started += 1;
        query_id
    }

    /// Store a record in the DHT (for federation configuration, etc.)
    pub fn put_record(&mut self, record: Record) -> Result<QueryId, libp2p::kad::store::Error> {
        tracing::debug!("Storing record with key: {:?}", record.key);
        let query_id = self.kademlia.put_record(record, libp2p::kad::Quorum::One)?;
        
        self.active_queries.insert(query_id, DiscoveryQuery {
            query_id,
            query_type: QueryType::PutRecord,
            started_at: Instant::now(),
            target: None,
        });

        Ok(query_id)
    }

    /// Retrieve a record from the DHT
    pub fn get_record(&mut self, key: &[u8]) -> QueryId {
        tracing::debug!("Retrieving record with key: {:?}", key);
        let query_id = self.kademlia.get_record(key.to_vec().into());
        
        self.active_queries.insert(query_id, DiscoveryQuery {
            query_id,
            query_type: QueryType::GetRecord,
            started_at: Instant::now(),
            target: Some(hex::encode(key)),
        });

        query_id
    }

    /// Add a federation peer for priority handling
    pub fn add_federation_peer(&mut self, peer_id: PeerId, addresses: Vec<Multiaddr>) {
        self.federation_peers.insert(peer_id);
        
        // Add federation peer to Kademlia routing table
        for addr in addresses {
            self.kademlia.add_address(&peer_id, addr);
        }
        
        tracing::info!("Added federation peer to discovery: {}", peer_id);
    }

    /// Remove a federation peer
    pub fn remove_federation_peer(&mut self, peer_id: &PeerId) {
        self.federation_peers.remove(peer_id);
        tracing::info!("Removed federation peer from discovery: {}", peer_id);
    }

    /// Get discovered peers filtered by federation status
    pub fn get_discovered_peers(&self, federation_only: bool) -> Vec<&DiscoveredPeer> {
        self.peer_cache.values()
            .filter(|peer| !federation_only || self.federation_peers.contains(&peer.peer_id))
            .collect()
    }

    /// Get current discovery metrics
    pub fn metrics(&self) -> &DiscoveryMetrics {
        &self.metrics
    }

    /// Handle Kademlia events and convert to Alys discovery events
    pub fn handle_kad_event(&mut self, event: KademliaEvent) -> Vec<DiscoveryProtocolEvent> {
        let mut alys_events = Vec::new();

        match event {
            KademliaEvent::OutboundQueryProgressed { id, result, .. } => {
                // Remove completed query from tracking
                let query_info = self.active_queries.remove(&id);
                
                match result {
                    QueryResult::Bootstrap(Ok(BootstrapResult { num_remaining, .. })) => {
                        if num_remaining == 0 {
                            self.bootstrap_status = BootstrapStatus::Completed;
                            self.metrics.successful_bootstraps += 1;
                            tracing::info!("Bootstrap completed successfully");
                            
                            alys_events.push(DiscoveryProtocolEvent::BootstrapCompleted {
                                duration: query_info.map(|q| q.started_at.elapsed())
                                    .unwrap_or(Duration::from_secs(0)),
                            });
                        }
                    }
                    QueryResult::Bootstrap(Err(e)) => {
                        self.bootstrap_status = BootstrapStatus::Failed;
                        self.metrics.failed_bootstraps += 1;
                        tracing::warn!("Bootstrap failed: {}", e);
                        
                        alys_events.push(DiscoveryProtocolEvent::BootstrapFailed {
                            error: e.to_string(),
                        });
                    }
                    QueryResult::GetClosestPeers(Ok(GetClosestPeersResult { peers, .. })) => {
                        self.metrics.successful_queries += 1;
                        tracing::debug!("Found {} closest peers", peers.len());
                        
                        // Cache discovered peers
                        for peer_id in peers.iter() {
                            self.add_to_peer_cache(*peer_id, vec![], DiscoverySource::Kademlia);
                        }
                        
                        alys_events.push(DiscoveryProtocolEvent::PeersDiscovered {
                            peers,
                            source: DiscoverySource::Kademlia,
                        });
                    }
                    QueryResult::GetRecord(Ok(GetRecordResult { records, .. })) => {
                        tracing::debug!("Retrieved {} records from DHT", records.len());
                        alys_events.push(DiscoveryProtocolEvent::RecordsRetrieved { records });
                    }
                    QueryResult::PutRecord(Ok(PutRecordResult { key, .. })) => {
                        tracing::debug!("Successfully stored record: {:?}", key);
                        alys_events.push(DiscoveryProtocolEvent::RecordStored { key });
                    }
                    QueryResult::GetProviders(Ok(GetProvidersResult { providers, .. })) => {
                        tracing::debug!("Found {} providers", providers.len());
                        alys_events.push(DiscoveryProtocolEvent::ProvidersFound { providers });
                    }
                    result => {
                        // Handle other query results or failures
                        if let Some(query_info) = query_info {
                            tracing::debug!("Query {:?} completed: {:?}", query_info.query_type, result);
                        }
                    }
                }
            }
            KademliaEvent::RoutingUpdated { peer, addresses, old_peer, .. } => {
                tracing::debug!("Routing table updated: peer {} with {} addresses", peer, addresses.len());
                
                // Update peer cache
                self.add_to_peer_cache(peer, addresses, DiscoverySource::Kademlia);
                
                alys_events.push(DiscoveryProtocolEvent::RoutingTableUpdated {
                    added_peer: peer,
                    removed_peer: old_peer,
                });
            }
            KademliaEvent::UnroutablePeer { peer } => {
                tracing::debug!("Peer {} is unroutable", peer);
                self.remove_from_peer_cache(&peer);
                
                alys_events.push(DiscoveryProtocolEvent::PeerUnroutable { peer_id: peer });
            }
            KademliaEvent::PendingRoutablePeer { peer, address } => {
                tracing::debug!("Pending routable peer {} at {}", peer, address);
                alys_events.push(DiscoveryProtocolEvent::PeerRoutePending { peer_id: peer, address });
            }
            _ => {
                // Handle other Kademlia events as needed
                tracing::trace!("Unhandled Kademlia event: {:?}", event);
            }
        }

        alys_events
    }

    /// Handle mDNS events and convert to Alys discovery events
    pub fn handle_mdns_event(&mut self, event: MdnsEvent) -> Vec<DiscoveryProtocolEvent> {
        let mut alys_events = Vec::new();

        match event {
            MdnsEvent::Discovered(list) => {
                tracing::debug!("mDNS discovered {} peers", list.len());
                
                let mut discovered_peers = Vec::new();
                for (peer_id, addr) in list {
                    // Add to Kademlia routing table for global discovery
                    self.kademlia.add_address(&peer_id, addr.clone());
                    
                    // Update peer cache
                    self.add_to_peer_cache(peer_id, vec![addr.clone()], DiscoverySource::Mdns);
                    
                    discovered_peers.push(peer_id);
                }
                
                self.metrics.mdns_discoveries += discovered_peers.len() as u64;
                alys_events.push(DiscoveryProtocolEvent::PeersDiscovered {
                    peers: discovered_peers,
                    source: DiscoverySource::Mdns,
                });
            }
            MdnsEvent::Expired(list) => {
                tracing::debug!("mDNS expired {} peer addresses", list.len());
                
                for (peer_id, _addr) in list {
                    // Update peer cache - could remove or mark as stale
                    if let Some(cached_peer) = self.peer_cache.get_mut(&peer_id) {
                        cached_peer.last_seen = Instant::now();
                    }
                }
                
                alys_events.push(DiscoveryProtocolEvent::MdnsExpired);
            }
        }

        alys_events
    }

    /// Cleanup stale peer cache entries
    pub fn cleanup_peer_cache(&mut self) {
        let now = Instant::now();
        let cache_ttl = Duration::from_secs(300); // 5 minutes

        let initial_count = self.peer_cache.len();
        self.peer_cache.retain(|_, peer| {
            now.duration_since(peer.discovered_at) < cache_ttl
        });

        let cleaned_count = initial_count - self.peer_cache.len();
        if cleaned_count > 0 {
            tracing::debug!("Cleaned {} stale peers from cache", cleaned_count);
        }
    }

    // Private helper methods
    
    fn add_to_peer_cache(&mut self, peer_id: PeerId, addresses: Vec<Multiaddr>, source: DiscoverySource) {
        let is_federation = self.federation_peers.contains(&peer_id);
        
        match self.peer_cache.get_mut(&peer_id) {
            Some(cached_peer) => {
                // Update existing entry
                cached_peer.addresses.extend(addresses);
                cached_peer.addresses.dedup();
                cached_peer.last_seen = Instant::now();
                cached_peer.discovery_sources.insert(source);
            }
            None => {
                // Create new entry
                let discovered_peer = DiscoveredPeer {
                    peer_id,
                    addresses,
                    is_federation_peer: is_federation,
                    discovered_at: Instant::now(),
                    last_seen: Instant::now(),
                    discovery_sources: {
                        let mut sources = HashSet::new();
                        sources.insert(source);
                        sources
                    },
                    connection_attempts: 0,
                    successful_connections: 0,
                };
                
                self.peer_cache.insert(peer_id, discovered_peer);
                self.metrics.unique_peers_discovered += 1;
            }
        }
    }
    
    fn remove_from_peer_cache(&mut self, peer_id: &PeerId) {
        if self.peer_cache.remove(peer_id).is_some() {
            tracing::debug!("Removed peer {} from cache", peer_id);
        }
    }
}

// Supporting types and enums

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    pub bootstrap_peers: HashMap<PeerId, Vec<Multiaddr>>,
    pub replication_factor: u8,
    pub kad_parallelism: u8,
    pub enable_mdns: bool,
    pub cache_size: usize,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            bootstrap_peers: HashMap::new(),
            replication_factor: 20,
            kad_parallelism: 3,
            enable_mdns: true,
            cache_size: 1000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapStatus {
    NotStarted,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug)]
pub struct DiscoveryQuery {
    pub query_id: QueryId,
    pub query_type: QueryType,
    pub started_at: Instant,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum QueryType {
    Bootstrap,
    GetClosestPeers,
    GetRecord,
    PutRecord,
    GetProviders,
    StartProviding,
}

#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    pub peer_id: PeerId,
    pub addresses: Vec<Multiaddr>,
    pub is_federation_peer: bool,
    pub discovered_at: Instant,
    pub last_seen: Instant,
    pub discovery_sources: HashSet<DiscoverySource>,
    pub connection_attempts: u32,
    pub successful_connections: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiscoverySource {
    Kademlia,
    Mdns,
    Bootstrap,
    Manual,
}

#[derive(Default)]
pub struct DiscoveryMetrics {
    pub queries_started: u64,
    pub successful_queries: u64,
    pub failed_queries: u64,
    pub successful_bootstraps: u64,
    pub failed_bootstraps: u64,
    pub unique_peers_discovered: u64,
    pub mdns_discoveries: u64,
}

#[derive(Debug)]
pub enum DiscoveryProtocolEvent {
    BootstrapCompleted {
        duration: Duration,
    },
    BootstrapFailed {
        error: String,
    },
    PeersDiscovered {
        peers: Vec<PeerId>,
        source: DiscoverySource,
    },
    RecordsRetrieved {
        records: Vec<Record>,
    },
    RecordStored {
        key: libp2p::kad::RecordKey,
    },
    ProvidersFound {
        providers: HashSet<PeerId>,
    },
    RoutingTableUpdated {
        added_peer: PeerId,
        removed_peer: Option<PeerId>,
    },
    PeerUnroutable {
        peer_id: PeerId,
    },
    PeerRoutePending {
        peer_id: PeerId,
        address: Multiaddr,
    },
    MdnsExpired,
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    #[test]
    fn test_discovery_creation() {
        let keypair = Keypair::generate_ed25519();
        let config = DiscoveryConfig::default();
        
        let discovery = AlysDiscovery::new(&keypair, config);
        assert!(discovery.is_ok());
    }

    #[test]
    fn test_federation_peer_management() {
        let keypair = Keypair::generate_ed25519();
        let config = DiscoveryConfig::default();
        let mut discovery = AlysDiscovery::new(&keypair, config).unwrap();
        
        let federation_peer = PeerId::random();
        let addresses = vec!["/ip4/127.0.0.1/tcp/8000".parse().unwrap()];
        
        discovery.add_federation_peer(federation_peer, addresses);
        assert!(discovery.federation_peers.contains(&federation_peer));
        
        discovery.remove_federation_peer(&federation_peer);
        assert!(!discovery.federation_peers.contains(&federation_peer));
    }

    #[test]
    fn test_peer_cache_management() {
        let keypair = Keypair::generate_ed25519();
        let config = DiscoveryConfig::default();
        let mut discovery = AlysDiscovery::new(&keypair, config).unwrap();
        
        let peer_id = PeerId::random();
        let addresses = vec!["/ip4/127.0.0.1/tcp/8001".parse().unwrap()];
        
        discovery.add_to_peer_cache(peer_id, addresses, DiscoverySource::Kademlia);
        assert!(discovery.peer_cache.contains_key(&peer_id));
        
        discovery.remove_from_peer_cache(&peer_id);
        assert!(!discovery.peer_cache.contains_key(&peer_id));
    }

    #[test]
    fn test_discovery_source_tracking() {
        let keypair = Keypair::generate_ed25519();
        let config = DiscoveryConfig::default();
        let mut discovery = AlysDiscovery::new(&keypair, config).unwrap();
        
        let peer_id = PeerId::random();
        let addresses = vec!["/ip4/127.0.0.1/tcp/8002".parse().unwrap()];
        
        // Add peer via Kademlia
        discovery.add_to_peer_cache(peer_id, addresses.clone(), DiscoverySource::Kademlia);
        assert!(discovery.peer_cache[&peer_id].discovery_sources.contains(&DiscoverySource::Kademlia));
        
        // Add same peer via mDNS
        discovery.add_to_peer_cache(peer_id, addresses, DiscoverySource::Mdns);
        assert!(discovery.peer_cache[&peer_id].discovery_sources.contains(&DiscoverySource::Kademlia));
        assert!(discovery.peer_cache[&peer_id].discovery_sources.contains(&DiscoverySource::Mdns));
    }
}