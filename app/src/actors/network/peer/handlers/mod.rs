//! PeerActor Message Handlers
//! 
//! This module contains all message handlers for PeerActor, managing peer connections,
//! discovery, scoring, and lifecycle operations with federation prioritization.

use actix::{Handler, Context, ResponseFuture};
use libp2p::PeerId;
use std::time::Duration;

use actor_system::{ActorResult, ActorError};
use crate::actors::network::messages::*;

// Type alias for peer-specific actor results
type PeerActorResult<T> = ActorResult<T>;
use crate::actors::network::peer::{
    connection::{DisconnectionReason, ConnectionEvent},
    scoring::ProtocolViolation,
};
use super::PeerActor;

/// Peer lifecycle and management handlers
impl Handler<StartPeerManager> for PeerActor {
    type Result = ResponseFuture<PeerActorResult<PeerManagerStatus>>;

    fn handle(&mut self, msg: StartPeerManager, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::info!("Starting peer manager with config: max_peers={}, federation_priority={}", 
                      self.config.max_peers, self.config.federation_priority);

        // Initialize discovery if enabled
        if self.config.discovery_config.enabled {
            self.discovery_service.start_discovery();
        }

        // Start health monitoring
        self.health_monitor.start_monitoring();

        let status = PeerManagerStatus {
            is_running: true,
            connected_peers: 0,
            pending_connections: 0,
            discovery_enabled: self.config.discovery_config.enabled,
            max_peers: self.config.max_peers,
        };

        Box::pin(async move {
            Ok(Ok(status))
        })
    }
}

impl Handler<StopPeerManager> for PeerActor {
    type Result = ResponseFuture<PeerActorResult<()>>;

    fn handle(&mut self, msg: StopPeerManager, ctx: &mut Context<Self>) -> Self::Result {
        tracing::info!("Stopping peer manager (graceful: {})", msg.graceful);

        let graceful = msg.graceful;
        let connected_peers = self.connection_manager.get_connected_peers();

        if graceful {
            // Gracefully close all connections
            for peer_id in connected_peers {
                self.connection_manager.remove_connection(&peer_id, DisconnectionReason::LocalShutdown);
            }
        }

        self.shutdown_requested = true;
        ctx.stop();

        Box::pin(async move {
            Ok(Ok(()))
        })
    }
}

impl Handler<GetPeerManagerStatus> for PeerActor {
    type Result = PeerActorResult<PeerManagerStatus>;

    fn handle(&mut self, _msg: GetPeerManagerStatus, _ctx: &mut Context<Self>) -> Self::Result {
        let status = PeerManagerStatus {
            is_running: !self.shutdown_requested,
            connected_peers: self.connection_manager.get_stats().total_connections,
            pending_connections: self.connection_manager.get_pending_connections().len() as u32,
            discovery_enabled: self.config.discovery_config.enabled,
            max_peers: self.config.max_peers,
        };

        Ok(Ok(status))
    }
}

/// Connection management handlers
impl Handler<ConnectToPeer> for PeerActor {
    type Result = ResponseFuture<PeerActorResult<ConnectionResult>>;

    fn handle(&mut self, msg: ConnectToPeer, _ctx: &mut Context<Self>) -> Self::Result {
        let peer_id = msg.peer_id;
        let addresses = msg.addresses;
        let is_federation = msg.is_federation_peer.unwrap_or(false);

        tracing::info!("Initiating connection to peer {} (federation: {})", peer_id, is_federation);

        // Check if already connected
        if self.connection_manager.is_connected(&peer_id) {
            return Box::pin(async move {
                Ok(Ok(ConnectionResult {
                    success: true,
                    peer_id,
                    message: "Already connected".to_string(),
                }))
            });
        }

        // Add peer to store if not exists
        if let Err(e) = self.peer_store.add_peer(peer_id, addresses.clone()) {
            tracing::error!("Failed to add peer to store: {:?}", e);
        }

        // Initialize scoring for peer
        self.scoring_engine.initialize_peer(peer_id, is_federation);

        // Initiate connection
        match self.connection_manager.initiate_connection(peer_id, addresses) {
            Ok(_) => {
                Box::pin(async move {
                    // In a real implementation, this would wait for the actual connection
                    // For now, return success immediately
                    Ok(Ok(ConnectionResult {
                        success: true,
                        peer_id,
                        message: "Connection initiated".to_string(),
                    }))
                })
            },
            Err(e) => {
                Box::pin(async move {
                    Ok(Ok(ConnectionResult {
                        success: false,
                        peer_id,
                        message: format!("Failed to initiate connection: {:?}", e),
                    }))
                })
            }
        }
    }
}

impl Handler<DisconnectFromPeer> for PeerActor {
    type Result = PeerActorResult<()>;

    fn handle(&mut self, msg: DisconnectFromPeer, _ctx: &mut Context<Self>) -> Self::Result {
        let peer_id = msg.peer_id;
        let reason = DisconnectionReason::LocalShutdown; // Default reason

        if let Some(_) = self.connection_manager.remove_connection(&peer_id, reason) {
            tracing::info!("Disconnected from peer {}", peer_id);
            
            // Update scoring
            self.scoring_engine.cleanup_peer(&peer_id);
            
            Ok(Ok(()))
        } else {
            tracing::warn!("Attempted to disconnect from unknown peer {}", peer_id);
            Ok(Ok(()))
        }
    }
}

impl Handler<GetConnectedPeers> for PeerActor {
    type Result = PeerActorResult<Vec<ConnectedPeerInfo>>;

    fn handle(&mut self, _msg: GetConnectedPeers, _ctx: &mut Context<Self>) -> Self::Result {
        let connected_peers = self.connection_manager.get_connected_peers();
        let mut peer_info_list = Vec::new();

        for peer_id in connected_peers {
            if let Some(connection) = self.connection_manager.get_connection(&peer_id) {
                let peer_info = ConnectedPeerInfo {
                    peer_id,
                    addresses: vec![], // Would extract from connection endpoint
                    connection_type: match connection.connection_type {
                        crate::actors::network::peer::connection::ConnectionType::Inbound => "inbound".to_string(),
                        crate::actors::network::peer::connection::ConnectionType::Outbound => "outbound".to_string(),
                    },
                    is_federation: connection.is_federation,
                    uptime: connection.established_at.elapsed(),
                    bytes_sent: connection.bytes_sent,
                    bytes_received: connection.bytes_received,
                    supported_protocols: connection.supported_protocols.clone(),
                };
                peer_info_list.push(peer_info);
            }
        }

        Ok(Ok(peer_info_list))
    }
}

/// Peer discovery handlers
impl Handler<StartDiscovery> for PeerActor {
    type Result = PeerActorResult<()>;

    fn handle(&mut self, _msg: StartDiscovery, _ctx: &mut Context<Self>) -> Self::Result {
        if !self.config.discovery_config.enabled {
            return Ok(Err(ActorError::ConfigurationError {
                reason: "Discovery is disabled in configuration".to_string(),
            }));
        }

        tracing::info!("Starting peer discovery");
        self.discovery_service.start_discovery();
        Ok(Ok(()))
    }
}

impl Handler<StopDiscovery> for PeerActor {
    type Result = PeerActorResult<()>;

    fn handle(&mut self, _msg: StopDiscovery, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::info!("Stopping peer discovery");
        self.discovery_service.stop_discovery();
        Ok(Ok(()))
    }
}

impl Handler<DiscoverPeers> for PeerActor {
    type Result = ResponseFuture<PeerActorResult<DiscoveryResult>>;

    fn handle(&mut self, msg: DiscoverPeers, _ctx: &mut Context<Self>) -> Self::Result {
        let target_count = msg.target_count.unwrap_or(10);
        
        tracing::info!("Running peer discovery to find {} peers", target_count);

        // In a real implementation, this would trigger actual discovery
        Box::pin(async move {
            Ok(Ok(DiscoveryResult {
                peers_discovered: 0,
                peers_connected: 0,
                discovery_duration: Duration::from_millis(100),
                errors: Vec::new(),
            }))
        })
    }
}

/// Peer scoring handlers
impl Handler<UpdatePeerScore> for PeerActor {
    type Result = PeerActorResult<()>;

    fn handle(&mut self, msg: UpdatePeerScore, _ctx: &mut Context<Self>) -> Self::Result {
        let peer_id = msg.peer_id;
        
        match msg.score_event {
            PeerScoreEvent::ConnectionSuccess { latency_ms } => {
                self.scoring_engine.record_connection_result(peer_id, true, latency_ms);
                self.peer_store.record_connection_attempt(&peer_id, true);
            },
            PeerScoreEvent::ConnectionFailure => {
                self.scoring_engine.record_connection_result(peer_id, false, 0);
                self.peer_store.record_connection_attempt(&peer_id, false);
            },
            PeerScoreEvent::ProtocolViolation { violation_type } => {
                let violation = match violation_type.as_str() {
                    "invalid_message" => ProtocolViolation::InvalidMessage,
                    "spam_behavior" => ProtocolViolation::SpamBehavior,
                    "malformed_data" => ProtocolViolation::MalformedData,
                    "protocol_mismatch" => ProtocolViolation::ProtocolMismatch,
                    "timeout_excess" => ProtocolViolation::TimeoutExcess,
                    _ => ProtocolViolation::InvalidMessage,
                };
                self.scoring_engine.record_protocol_violation(peer_id, violation);
            },
            PeerScoreEvent::MessageSuccess { message_type } => {
                self.scoring_engine.record_message_success(peer_id, &message_type);
            },
            PeerScoreEvent::UptimeUpdate { connected_duration } => {
                self.scoring_engine.update_uptime(peer_id, connected_duration);
            },
        }

        // Check if peer should be banned based on score
        if self.scoring_engine.should_ban_peer(&peer_id) {
            tracing::warn!("Peer {} score too low, banning", peer_id);
            self.peer_store.ban_peer(&peer_id, Some(Duration::from_secs(3600))); // 1 hour ban
            self.connection_manager.remove_connection(&peer_id, DisconnectionReason::Banned);
        }

        Ok(Ok(()))
    }
}

impl Handler<GetPeerScore> for PeerActor {
    type Result = PeerActorResult<Option<PeerScoreInfo>>;

    fn handle(&mut self, msg: GetPeerScore, _ctx: &mut Context<Self>) -> Self::Result {
        let peer_id = msg.peer_id;
        
        if let Some(score) = self.scoring_engine.get_score(&peer_id) {
            let score_info = PeerScoreInfo {
                peer_id,
                score,
                is_federation: self.peer_store.get_peer(&peer_id)
                    .map(|p| p.is_federation).unwrap_or(false),
                last_updated: std::time::SystemTime::now(), // Would use actual timestamp
            };
            Ok(Ok(Some(score_info)))
        } else {
            Ok(Ok(None))
        }
    }
}

impl Handler<GetTopPeers> for PeerActor {
    type Result = PeerActorResult<Vec<PeerScoreInfo>>;

    fn handle(&mut self, msg: GetTopPeers, _ctx: &mut Context<Self>) -> Self::Result {
        let limit = msg.limit.unwrap_or(10) as usize;
        let top_peers = self.scoring_engine.get_top_peers(limit);
        
        let peer_scores: Vec<PeerScoreInfo> = top_peers.into_iter()
            .map(|(peer_id, score)| {
                let is_federation = self.peer_store.get_peer(&peer_id)
                    .map(|p| p.is_federation).unwrap_or(false);
                
                PeerScoreInfo {
                    peer_id,
                    score,
                    is_federation,
                    last_updated: std::time::SystemTime::now(),
                }
            })
            .collect();

        Ok(Ok(peer_scores))
    }
}

/// Peer banning handlers
impl Handler<BanPeer> for PeerActor {
    type Result = PeerActorResult<()>;

    fn handle(&mut self, msg: BanPeer, _ctx: &mut Context<Self>) -> Self::Result {
        let peer_id = msg.peer_id;
        let duration = msg.duration;

        tracing::warn!("Banning peer {} for {:?}", peer_id, duration);

        // Ban in peer store
        self.peer_store.ban_peer(&peer_id, duration);

        // Disconnect if currently connected
        self.connection_manager.remove_connection(&peer_id, DisconnectionReason::Banned);

        // Cancel pending connections
        self.connection_manager.cancel_pending_connection(&peer_id);

        Ok(Ok(()))
    }
}

impl Handler<UnbanPeer> for PeerActor {
    type Result = PeerActorResult<()>;

    fn handle(&mut self, msg: UnbanPeer, _ctx: &mut Context<Self>) -> Self::Result {
        let peer_id = msg.peer_id;

        tracing::info!("Unbanning peer {}", peer_id);
        self.peer_store.unban_peer(&peer_id);

        Ok(Ok(()))
    }
}

impl Handler<GetBannedPeers> for PeerActor {
    type Result = PeerActorResult<Vec<BannedPeerInfo>>;

    fn handle(&mut self, _msg: GetBannedPeers, _ctx: &mut Context<Self>) -> Self::Result {
        // This would iterate through banned peers in the store
        // For now, return empty list
        Ok(Ok(Vec::new()))
    }
}

/// Health monitoring and cleanup handlers
impl Handler<PerformHealthCheck> for PeerActor {
    type Result = PeerActorResult<HealthCheckResult>;

    fn handle(&mut self, _msg: PerformHealthCheck, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::debug!("Performing peer health check");

        // Check for idle connections
        let max_idle = Duration::from_secs(300); // 5 minutes
        let idle_peers = self.connection_manager.check_idle_connections(max_idle);
        
        for peer_id in idle_peers {
            tracing::info!("Disconnecting idle peer {}", peer_id);
            self.connection_manager.remove_connection(&peer_id, DisconnectionReason::Timeout);
        }

        // Check for pending connection timeouts
        let timed_out_peers = self.connection_manager.check_pending_timeouts();
        for peer_id in &timed_out_peers {
            tracing::warn!("Connection timeout for peer {}", peer_id);
            self.scoring_engine.record_connection_result(*peer_id, false, 0);
        }

        // Apply score decay
        self.scoring_engine.apply_decay();

        // Clean up expired peer data
        self.peer_store.cleanup_expired();

        let result = HealthCheckResult {
            healthy_peers: self.connection_manager.get_stats().total_connections,
            idle_connections_closed: 0, // Would track actual count
            timed_out_connections: timed_out_peers.len() as u32,
            expired_peers_cleaned: 0, // Would track actual count
        };

        Ok(Ok(result))
    }
}

impl Handler<CleanupPeerData> for PeerActor {
    type Result = PeerActorResult<u32>;

    fn handle(&mut self, _msg: CleanupPeerData, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::info!("Performing peer data cleanup");

        // Clean up expired data
        self.peer_store.cleanup_expired();

        // This would return actual count of cleaned up entries
        Ok(Ok(0))
    }
}