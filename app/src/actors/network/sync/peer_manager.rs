//! Peer Manager for Sync Coordination
//! 
//! Manages peer selection, performance tracking, and block request coordination
//! for optimal synchronization performance with federation-aware prioritization.

use std::collections::{HashMap, VecDeque, BinaryHeap};
use std::time::{Duration, Instant};
use std::cmp::Ordering;
use libp2p::PeerId;
use crate::actors::network::messages::{NetworkError, NetworkResult};
use crate::actors::network::sync::state::{PeerSyncInfo, PeerRanking, BlockRange, BlacklistInfo};

/// Peer management for sync operations
pub struct PeerManager {
    /// Active sync peers with performance tracking
    active_peers: HashMap<PeerId, SyncPeerInfo>,
    /// Peer performance rankings
    peer_rankings: BinaryHeap<RankedPeer>,
    /// Blacklisted peers
    blacklisted_peers: HashMap<PeerId, BlacklistInfo>,
    /// Block assignment tracking
    block_assignments: HashMap<PeerId, Vec<BlockAssignment>>,
    /// Assignment history for optimization
    assignment_history: VecDeque<AssignmentResult>,
    /// Configuration
    config: PeerManagerConfig,
}

impl PeerManager {
    /// Create a new peer manager
    pub fn new(config: PeerManagerConfig) -> Self {
        Self {
            active_peers: HashMap::new(),
            peer_rankings: BinaryHeap::new(),
            blacklisted_peers: HashMap::new(),
            block_assignments: HashMap::new(),
            assignment_history: VecDeque::new(),
            config,
        }
    }

    /// Add or update a peer for sync operations
    pub fn add_peer(&mut self, peer_id: PeerId, peer_info: PeerSyncData) -> NetworkResult<()> {
        // Check if peer is blacklisted
        if let Some(blacklist_info) = self.blacklisted_peers.get(&peer_id) {
            if blacklist_info.blacklisted_at.elapsed() < blacklist_info.duration {
                return Err(NetworkError::PeerNotFound {
                    peer_id: peer_id.to_string(),
                });
            } else {
                // Remove expired blacklist entry
                self.blacklisted_peers.remove(&peer_id);
            }
        }

        let sync_peer_info = SyncPeerInfo {
            peer_id: peer_id.clone(),
            peer_data: peer_info,
            performance_metrics: PeerPerformanceMetrics::default(),
            assignment_state: AssignmentState::Available,
            last_activity: Instant::now(),
            consecutive_failures: 0,
            total_blocks_downloaded: 0,
        };

        self.active_peers.insert(peer_id.clone(), sync_peer_info);
        self.update_peer_ranking(peer_id);

        tracing::debug!("Added peer {} for sync operations", peer_id);
        Ok(())
    }

    /// Remove a peer from sync operations
    pub fn remove_peer(&mut self, peer_id: &PeerId) -> NetworkResult<()> {
        if let Some(peer_info) = self.active_peers.remove(peer_id) {
            // Cancel any active assignments
            if let Some(assignments) = self.block_assignments.remove(peer_id) {
                for assignment in assignments {
                    // Reassign blocks to other peers
                    self.reassign_blocks(assignment.range)?;
                }
            }

            // Remove from rankings
            self.peer_rankings.retain(|ranked_peer| ranked_peer.peer_id != *peer_id);

            tracing::debug!("Removed peer {} from sync operations", peer_id);
        }

        Ok(())
    }

    /// Select best peers for block download
    pub fn select_peers_for_sync(
        &self,
        block_range: BlockRange,
        required_peers: usize,
    ) -> NetworkResult<Vec<PeerId>> {
        let mut selected_peers = Vec::new();
        let mut considered_peers: Vec<_> = self.peer_rankings
            .iter()
            .filter(|peer| {
                // Filter available peers
                if let Some(peer_info) = self.active_peers.get(&peer.peer_id) {
                    matches!(peer_info.assignment_state, AssignmentState::Available) &&
                    peer_info.peer_data.height >= block_range.end_height
                } else {
                    false
                }
            })
            .take(required_peers * 2) // Consider more peers for better selection
            .cloned()
            .collect();

        // Sort by composite score (higher is better)
        considered_peers.sort_by(|a, b| b.composite_score.partial_cmp(&a.composite_score).unwrap_or(Ordering::Equal));

        // Prioritize federation peers
        let (federation_peers, regular_peers): (Vec<_>, Vec<_>) = considered_peers
            .into_iter()
            .partition(|peer| peer.is_federation_peer);

        // Select federation peers first
        for peer in federation_peers.into_iter().take(required_peers) {
            selected_peers.push(peer.peer_id);
        }

        // Fill remaining slots with regular peers
        let remaining = required_peers.saturating_sub(selected_peers.len());
        for peer in regular_peers.into_iter().take(remaining) {
            selected_peers.push(peer.peer_id);
        }

        if selected_peers.is_empty() {
            return Err(NetworkError::ResourceExhausted {
                resource: "Available sync peers".to_string(),
            });
        }

        Ok(selected_peers)
    }

    /// Assign block range to peers
    pub fn assign_blocks(
        &mut self,
        range: BlockRange,
        peers: Vec<PeerId>,
    ) -> NetworkResult<Vec<BlockAssignment>> {
        if peers.is_empty() {
            return Err(NetworkError::ValidationError {
                reason: "No peers provided for block assignment".to_string(),
            });
        }

        let blocks_per_peer = (range.end_height - range.start_height + 1) / peers.len() as u64;
        let mut assignments = Vec::new();
        let mut current_height = range.start_height;

        for (i, peer_id) in peers.iter().enumerate() {
            let end_height = if i == peers.len() - 1 {
                range.end_height // Last peer gets remaining blocks
            } else {
                current_height + blocks_per_peer - 1
            };

            let assignment = BlockAssignment {
                assignment_id: format!("{}_{}_{}_{}", peer_id, current_height, end_height, Instant::now().elapsed().as_millis()),
                peer_id: peer_id.clone(),
                range: BlockRange {
                    start_height: current_height,
                    end_height,
                    assigned_at: Instant::now(),
                    priority: range.priority,
                },
                status: AssignmentStatus::Active,
                assigned_at: Instant::now(),
                deadline: Instant::now() + self.config.assignment_timeout,
                retry_count: 0,
            };

            assignments.push(assignment.clone());

            // Update peer state
            if let Some(peer_info) = self.active_peers.get_mut(peer_id) {
                peer_info.assignment_state = AssignmentState::Downloading;
            }

            // Track assignment
            self.block_assignments.entry(peer_id.clone()).or_default().push(assignment);

            current_height = end_height + 1;
        }

        tracing::debug!("Assigned {} block ranges to {} peers", assignments.len(), peers.len());
        Ok(assignments)
    }

    /// Update peer performance metrics
    pub fn update_peer_performance(
        &mut self,
        peer_id: &PeerId,
        blocks_downloaded: u64,
        download_time: Duration,
        success: bool,
    ) -> NetworkResult<()> {
        let peer_info = self.active_peers.get_mut(peer_id).ok_or_else(|| {
            NetworkError::PeerNotFound {
                peer_id: peer_id.to_string(),
            }
        })?;

        // Update performance metrics
        if success {
            peer_info.performance_metrics.successful_downloads += 1;
            peer_info.performance_metrics.total_blocks_downloaded += blocks_downloaded;
            peer_info.total_blocks_downloaded += blocks_downloaded;
            peer_info.consecutive_failures = 0;

            // Calculate blocks per second
            if !download_time.is_zero() {
                let bps = blocks_downloaded as f64 / download_time.as_secs_f64();
                peer_info.performance_metrics.update_throughput(bps);
            }
        } else {
            peer_info.performance_metrics.failed_downloads += 1;
            peer_info.consecutive_failures += 1;

            // Consider blacklisting if too many failures
            if peer_info.consecutive_failures >= self.config.max_consecutive_failures {
                self.blacklist_peer(peer_id.clone(), "Too many consecutive failures".to_string())?;
                return Ok(());
            }
        }

        peer_info.last_activity = Instant::now();
        peer_info.assignment_state = AssignmentState::Available;

        // Update ranking
        self.update_peer_ranking(peer_id.clone());

        // Record assignment result
        let result = AssignmentResult {
            peer_id: peer_id.clone(),
            blocks_assigned: blocks_downloaded,
            success,
            duration: download_time,
            timestamp: Instant::now(),
        };
        
        self.assignment_history.push_back(result);
        if self.assignment_history.len() > self.config.max_assignment_history {
            self.assignment_history.pop_front();
        }

        Ok(())
    }

    /// Complete block assignment
    pub fn complete_assignment(
        &mut self,
        assignment_id: &str,
        success: bool,
    ) -> NetworkResult<()> {
        // Find and remove assignment
        let mut found_assignment = None;
        for (peer_id, assignments) in self.block_assignments.iter_mut() {
            if let Some(pos) = assignments.iter().position(|a| a.assignment_id == assignment_id) {
                let mut assignment = assignments.remove(pos);
                assignment.status = if success {
                    AssignmentStatus::Completed
                } else {
                    AssignmentStatus::Failed
                };
                found_assignment = Some((peer_id.clone(), assignment));
                break;
            }
        }

        if let Some((peer_id, assignment)) = found_assignment {
            let download_time = assignment.assigned_at.elapsed();
            let blocks_downloaded = assignment.range.end_height - assignment.range.start_height + 1;

            self.update_peer_performance(&peer_id, blocks_downloaded, download_time, success)?;

            if !success && assignment.retry_count < self.config.max_retries {
                // Retry assignment with different peer
                self.reassign_blocks(assignment.range)?;
            }
        }

        Ok(())
    }

    /// Get peer performance rankings
    pub fn get_peer_rankings(&self) -> Vec<PeerRanking> {
        self.peer_rankings
            .iter()
            .map(|ranked_peer| PeerRanking {
                peer_id: ranked_peer.peer_id.to_string(),
                composite_score: ranked_peer.composite_score,
                latency_ms: ranked_peer.latency_ms,
                throughput_score: ranked_peer.throughput_score,
                reliability_score: ranked_peer.reliability_score,
                is_federation_peer: ranked_peer.is_federation_peer,
            })
            .collect()
    }

    /// Get sync statistics
    pub fn get_sync_stats(&self) -> SyncStats {
        let total_assignments = self.assignment_history.len();
        let successful_assignments = self.assignment_history.iter().filter(|r| r.success).count();
        let success_rate = if total_assignments > 0 {
            successful_assignments as f64 / total_assignments as f64
        } else {
            0.0
        };

        let average_bps = if !self.assignment_history.is_empty() {
            let total_blocks: u64 = self.assignment_history.iter().map(|r| r.blocks_assigned).sum();
            let total_time: Duration = self.assignment_history.iter().map(|r| r.duration).sum();
            if !total_time.is_zero() {
                total_blocks as f64 / total_time.as_secs_f64()
            } else {
                0.0
            }
        } else {
            0.0
        };

        SyncStats {
            active_peers: self.active_peers.len(),
            blacklisted_peers: self.blacklisted_peers.len(),
            total_assignments,
            successful_assignments,
            success_rate,
            average_blocks_per_second: average_bps,
            federation_peers: self.active_peers.values().filter(|p| p.peer_data.is_federation_peer).count(),
        }
    }

    /// Blacklist a problematic peer
    fn blacklist_peer(&mut self, peer_id: PeerId, reason: String) -> NetworkResult<()> {
        let blacklist_info = BlacklistInfo {
            blacklisted_at: Instant::now(),
            duration: self.config.blacklist_duration,
            reason,
            strike_count: 1, // Could be incremented for repeat offenders
        };

        self.blacklisted_peers.insert(peer_id.clone(), blacklist_info);
        self.remove_peer(&peer_id)?;

        tracing::warn!("Blacklisted peer {} for {}", peer_id, 
            self.blacklisted_peers.get(&peer_id).unwrap().reason);

        Ok(())
    }

    /// Reassign blocks to different peers
    fn reassign_blocks(&mut self, range: BlockRange) -> NetworkResult<()> {
        // Find available peers for reassignment
        let available_peers: Vec<PeerId> = self.active_peers
            .iter()
            .filter(|(_, info)| matches!(info.assignment_state, AssignmentState::Available))
            .map(|(peer_id, _)| peer_id.clone())
            .collect();

        if !available_peers.is_empty() {
            let selected_peers = self.select_peers_for_sync(range.clone(), 1)?;
            if !selected_peers.is_empty() {
                self.assign_blocks(range, selected_peers)?;
                tracing::debug!("Reassigned blocks to different peer");
            }
        }

        Ok(())
    }

    /// Update peer ranking in the heap
    fn update_peer_ranking(&mut self, peer_id: PeerId) {
        // Remove existing ranking
        self.peer_rankings.retain(|ranked_peer| ranked_peer.peer_id != peer_id);

        // Calculate new ranking
        if let Some(peer_info) = self.active_peers.get(&peer_id) {
            let composite_score = self.calculate_composite_score(&peer_info);
            
            let ranked_peer = RankedPeer {
                peer_id: peer_id.clone(),
                composite_score,
                latency_ms: peer_info.performance_metrics.average_latency_ms,
                throughput_score: peer_info.performance_metrics.average_blocks_per_second,
                reliability_score: peer_info.performance_metrics.success_rate,
                is_federation_peer: peer_info.peer_data.is_federation_peer,
                last_updated: Instant::now(),
            };

            self.peer_rankings.push(ranked_peer);
        }
    }

    /// Calculate composite performance score for peer
    fn calculate_composite_score(&self, peer_info: &SyncPeerInfo) -> f64 {
        let base_score = 
            (peer_info.performance_metrics.success_rate * 40.0) +
            (peer_info.performance_metrics.average_blocks_per_second.min(1000.0) / 1000.0 * 30.0) +
            ((1000.0 - peer_info.performance_metrics.average_latency_ms.min(1000.0)) / 1000.0 * 20.0) +
            (if peer_info.total_blocks_downloaded > 1000 { 10.0 } else { 0.0 });

        // Federation bonus
        if peer_info.peer_data.is_federation_peer {
            base_score + 20.0
        } else {
            base_score
        }
    }
}

/// Peer manager configuration
pub struct PeerManagerConfig {
    pub max_concurrent_assignments: usize,
    pub assignment_timeout: Duration,
    pub max_consecutive_failures: u32,
    pub max_retries: u32,
    pub blacklist_duration: Duration,
    pub max_assignment_history: usize,
}

impl Default for PeerManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_assignments: 16,
            assignment_timeout: Duration::from_secs(30),
            max_consecutive_failures: 3,
            max_retries: 2,
            blacklist_duration: Duration::from_secs(300), // 5 minutes
            max_assignment_history: 1000,
        }
    }
}

/// Internal sync peer information
struct SyncPeerInfo {
    peer_id: PeerId,
    peer_data: PeerSyncData,
    performance_metrics: PeerPerformanceMetrics,
    assignment_state: AssignmentState,
    last_activity: Instant,
    consecutive_failures: u32,
    total_blocks_downloaded: u64,
}

/// Peer data provided during registration
#[derive(Clone)]
pub struct PeerSyncData {
    pub height: u64,
    pub is_federation_peer: bool,
    pub protocols: Vec<String>,
    pub advertised_capabilities: Vec<String>,
}

/// Peer performance metrics
#[derive(Default)]
struct PeerPerformanceMetrics {
    successful_downloads: u64,
    failed_downloads: u64,
    total_blocks_downloaded: u64,
    average_blocks_per_second: f64,
    average_latency_ms: f64,
    success_rate: f64,
}

impl PeerPerformanceMetrics {
    fn update_throughput(&mut self, new_bps: f64) {
        let total_downloads = self.successful_downloads as f64;
        if total_downloads > 0.0 {
            self.average_blocks_per_second = 
                ((self.average_blocks_per_second * (total_downloads - 1.0)) + new_bps) / total_downloads;
        } else {
            self.average_blocks_per_second = new_bps;
        }

        // Update success rate
        let total_attempts = self.successful_downloads + self.failed_downloads;
        if total_attempts > 0 {
            self.success_rate = self.successful_downloads as f64 / total_attempts as f64;
        }
    }
}

/// Peer assignment state
#[derive(Clone, PartialEq)]
enum AssignmentState {
    Available,
    Downloading,
    Blacklisted,
}

/// Block assignment to peer
#[derive(Clone)]
pub struct BlockAssignment {
    pub assignment_id: String,
    pub peer_id: PeerId,
    pub range: BlockRange,
    pub status: AssignmentStatus,
    pub assigned_at: Instant,
    pub deadline: Instant,
    pub retry_count: u32,
}

/// Assignment status
#[derive(Clone)]
pub enum AssignmentStatus {
    Active,
    Completed,
    Failed,
    Cancelled,
}

/// Assignment result for history tracking
struct AssignmentResult {
    peer_id: PeerId,
    blocks_assigned: u64,
    success: bool,
    duration: Duration,
    timestamp: Instant,
}

/// Ranked peer for selection
#[derive(Clone)]
struct RankedPeer {
    peer_id: PeerId,
    composite_score: f64,
    latency_ms: f64,
    throughput_score: f64,
    reliability_score: f64,
    is_federation_peer: bool,
    last_updated: Instant,
}

impl PartialEq for RankedPeer {
    fn eq(&self, other: &Self) -> bool {
        self.peer_id == other.peer_id
    }
}

impl Eq for RankedPeer {}

impl PartialOrd for RankedPeer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedPeer {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher composite score is better
        self.composite_score.partial_cmp(&other.composite_score).unwrap_or(Ordering::Equal)
    }
}

/// Sync performance statistics
pub struct SyncStats {
    pub active_peers: usize,
    pub blacklisted_peers: usize,
    pub total_assignments: usize,
    pub successful_assignments: usize,
    pub success_rate: f64,
    pub average_blocks_per_second: f64,
    pub federation_peers: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_peer_data(height: u64, is_federation: bool) -> PeerSyncData {
        PeerSyncData {
            height,
            is_federation_peer: is_federation,
            protocols: vec!["sync".to_string()],
            advertised_capabilities: vec!["fast_sync".to_string()],
        }
    }

    #[test]
    fn peer_manager_creation() {
        let manager = PeerManager::new(PeerManagerConfig::default());
        assert_eq!(manager.active_peers.len(), 0);
        assert_eq!(manager.blacklisted_peers.len(), 0);
    }

    #[test]
    fn peer_addition_and_removal() {
        let mut manager = PeerManager::new(PeerManagerConfig::default());
        let peer_id = PeerId::random();
        let peer_data = create_test_peer_data(1000, false);

        // Add peer
        manager.add_peer(peer_id.clone(), peer_data).unwrap();
        assert_eq!(manager.active_peers.len(), 1);
        assert_eq!(manager.peer_rankings.len(), 1);

        // Remove peer
        manager.remove_peer(&peer_id).unwrap();
        assert_eq!(manager.active_peers.len(), 0);
    }

    #[test]
    fn peer_selection() {
        let mut manager = PeerManager::new(PeerManagerConfig::default());
        
        // Add multiple peers
        let peers: Vec<_> = (0..5).map(|i| {
            let peer_id = PeerId::random();
            let peer_data = create_test_peer_data(1000, i < 2); // First 2 are federation
            manager.add_peer(peer_id.clone(), peer_data).unwrap();
            peer_id
        }).collect();

        let range = BlockRange {
            start_height: 900,
            end_height: 950,
            assigned_at: Instant::now(),
            priority: 0,
        };

        // Select peers for sync
        let selected = manager.select_peers_for_sync(range, 3).unwrap();
        assert_eq!(selected.len(), 3);
    }

    #[test]
    fn block_assignment() {
        let mut manager = PeerManager::new(PeerManagerConfig::default());
        
        let peers: Vec<_> = (0..3).map(|_| {
            let peer_id = PeerId::random();
            let peer_data = create_test_peer_data(1000, false);
            manager.add_peer(peer_id.clone(), peer_data).unwrap();
            peer_id
        }).collect();

        let range = BlockRange {
            start_height: 900,
            end_height: 950,
            assigned_at: Instant::now(),
            priority: 0,
        };

        let assignments = manager.assign_blocks(range, peers).unwrap();
        assert_eq!(assignments.len(), 3);
        
        // Check that blocks are distributed
        let total_blocks: u64 = assignments.iter()
            .map(|a| a.range.end_height - a.range.start_height + 1)
            .sum();
        assert_eq!(total_blocks, 51); // 900-950 inclusive
    }

    #[test]
    fn peer_performance_update() {
        let mut manager = PeerManager::new(PeerManagerConfig::default());
        let peer_id = PeerId::random();
        let peer_data = create_test_peer_data(1000, false);
        
        manager.add_peer(peer_id.clone(), peer_data).unwrap();

        // Update with successful download
        manager.update_peer_performance(
            &peer_id,
            100,
            Duration::from_secs(1),
            true
        ).unwrap();

        let stats = manager.get_sync_stats();
        assert_eq!(stats.active_peers, 1);
    }

    #[test]
    fn peer_blacklisting() {
        let mut manager = PeerManager::new(PeerManagerConfig::default());
        let peer_id = PeerId::random();
        let peer_data = create_test_peer_data(1000, false);
        
        manager.add_peer(peer_id.clone(), peer_data).unwrap();

        // Simulate multiple failures to trigger blacklisting
        for _ in 0..4 {
            let _ = manager.update_peer_performance(
                &peer_id,
                0,
                Duration::from_secs(30),
                false
            );
        }

        let stats = manager.get_sync_stats();
        assert_eq!(stats.active_peers, 0);
        assert_eq!(stats.blacklisted_peers, 1);
    }
}