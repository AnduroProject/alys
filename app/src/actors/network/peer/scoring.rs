//! Peer Scoring Engine
//! 
//! Advanced peer reputation and scoring system with federation prioritization,
//! performance tracking, and behavioral analysis.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};
use libp2p::PeerId;

use crate::actors::network::peer::config::ScoringConfig;

/// Peer scoring engine
#[derive(Debug)]
pub struct ScoringEngine {
    /// Scoring configuration
    config: ScoringConfig,
    /// Peer scores
    scores: HashMap<PeerId, PeerScore>,
    /// Scoring history for analysis
    score_history: HashMap<PeerId, Vec<ScoreEntry>>,
    /// Performance metrics
    performance_metrics: HashMap<PeerId, PerformanceMetrics>,
    /// Last update time
    last_update: Instant,
}

impl ScoringEngine {
    /// Create a new scoring engine
    pub fn new(config: ScoringConfig) -> Self {
        Self {
            config,
            scores: HashMap::new(),
            score_history: HashMap::new(),
            performance_metrics: HashMap::new(),
            last_update: Instant::now(),
        }
    }

    /// Initialize score for a new peer
    pub fn initialize_peer(&mut self, peer_id: PeerId, is_federation: bool) {
        let initial_score = if is_federation {
            self.config.base_score + self.config.federation_bonus
        } else {
            self.config.base_score
        };

        let peer_score = PeerScore {
            current_score: initial_score,
            base_score: self.config.base_score,
            last_updated: Instant::now(),
            is_federation,
            connection_score: 0.0,
            latency_score: 0.0,
            uptime_score: 0.0,
            protocol_score: 0.0,
            behavioral_score: 0.0,
        };

        self.scores.insert(peer_id, peer_score);
        self.performance_metrics.insert(peer_id, PerformanceMetrics::default());
        
        self.record_score_change(peer_id, initial_score, "peer_initialized");
    }

    /// Update peer score based on connection success/failure
    pub fn record_connection_result(&mut self, peer_id: PeerId, success: bool, latency_ms: u32) {
        if let Some(score) = self.scores.get_mut(&peer_id) {
            if success {
                // Reward successful connections
                score.connection_score += self.config.message_success_bonus;
                score.latency_score = self.calculate_latency_score(latency_ms);
            } else {
                // Penalize failed connections
                score.connection_score -= self.config.connection_failure_penalty;
            }
            
            self.update_peer_score(peer_id);
        }
    }

    /// Record protocol violation
    pub fn record_protocol_violation(&mut self, peer_id: PeerId, violation_type: ProtocolViolation) {
        if let Some(score) = self.scores.get_mut(&peer_id) {
            let penalty = match violation_type {
                ProtocolViolation::InvalidMessage => 5.0,
                ProtocolViolation::SpamBehavior => 15.0,
                ProtocolViolation::MalformedData => 10.0,
                ProtocolViolation::ProtocolMismatch => 3.0,
                ProtocolViolation::TimeoutExcess => 8.0,
            };
            
            score.protocol_score -= penalty;
            score.behavioral_score -= penalty * 0.5; // Additional behavioral penalty
            
            self.update_peer_score(peer_id);
            self.record_score_change(peer_id, score.current_score, &format!("protocol_violation_{:?}", violation_type));
        }
    }

    /// Record successful message handling
    pub fn record_message_success(&mut self, peer_id: PeerId, message_type: &str) {
        if let Some(score) = self.scores.get_mut(&peer_id) {
            score.protocol_score += self.config.message_success_bonus;
            
            // Extra bonus for federation-critical messages
            if message_type == "federation_blocks" || message_type == "consensus" {
                score.protocol_score += self.config.message_success_bonus;
            }
            
            self.update_peer_score(peer_id);
        }
    }

    /// Update uptime score
    pub fn update_uptime(&mut self, peer_id: PeerId, connected_duration: Duration) {
        if let Some(metrics) = self.performance_metrics.get_mut(&peer_id) {
            metrics.total_uptime += connected_duration;
            metrics.session_count += 1;
            
            if let Some(score) = self.scores.get_mut(&peer_id) {
                score.uptime_score = self.calculate_uptime_score(metrics.total_uptime, metrics.session_count);
                self.update_peer_score(peer_id);
            }
        }
    }

    /// Get peer score
    pub fn get_score(&self, peer_id: &PeerId) -> Option<f64> {
        self.scores.get(peer_id).map(|s| s.current_score)
    }

    /// Get detailed peer score breakdown
    pub fn get_score_breakdown(&self, peer_id: &PeerId) -> Option<&PeerScore> {
        self.scores.get(peer_id)
    }

    /// Get top-scored peers
    pub fn get_top_peers(&self, limit: usize) -> Vec<(PeerId, f64)> {
        let mut peers: Vec<_> = self.scores.iter()
            .map(|(&peer_id, score)| (peer_id, score.current_score))
            .collect();
        
        peers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        peers.truncate(limit);
        peers
    }

    /// Check if peer should be banned based on score
    pub fn should_ban_peer(&self, peer_id: &PeerId) -> bool {
        if let Some(score) = self.scores.get(peer_id) {
            // Never ban federation peers
            if score.is_federation {
                return false;
            }
            score.current_score < self.config.min_score
        } else {
            false
        }
    }

    /// Apply periodic score decay
    pub fn apply_decay(&mut self) {
        let now = Instant::now();
        let time_since_last_update = now.duration_since(self.last_update);
        
        if time_since_last_update < self.config.update_interval {
            return;
        }
        
        let decay_factor = self.calculate_decay_factor(time_since_last_update);
        
        for (peer_id, score) in self.scores.iter_mut() {
            let old_score = score.current_score;
            
            // Apply decay to non-base components
            score.connection_score *= decay_factor;
            score.latency_score *= decay_factor;
            score.protocol_score *= decay_factor;
            score.behavioral_score *= decay_factor;
            
            // Uptime score decays slower
            score.uptime_score *= decay_factor.powf(0.5);
            
            score.last_updated = now;
            self.update_peer_score_internal(score);
            
            if (old_score - score.current_score).abs() > 0.1 {
                self.record_score_change(*peer_id, score.current_score, "periodic_decay");
            }
        }
        
        self.last_update = now;
    }

    /// Update peer's overall score
    fn update_peer_score(&mut self, peer_id: PeerId) {
        if let Some(score) = self.scores.get_mut(&peer_id) {
            self.update_peer_score_internal(score);
        }
    }

    /// Internal score calculation
    fn update_peer_score_internal(&mut self, score: &mut PeerScore) {
        let base = if score.is_federation {
            score.base_score + self.config.federation_bonus
        } else {
            score.base_score
        };

        let weighted_score = base 
            + score.connection_score
            + (score.latency_score * self.config.latency_weight)
            + (score.uptime_score * self.config.uptime_weight)
            + score.protocol_score
            + score.behavioral_score;

        score.current_score = weighted_score.min(self.config.max_score).max(self.config.min_score);
        score.last_updated = Instant::now();
    }

    /// Calculate latency score (lower latency = higher score)
    fn calculate_latency_score(&self, latency_ms: u32) -> f64 {
        match latency_ms {
            0..=50 => 5.0,      // Excellent
            51..=100 => 3.0,    // Good
            101..=200 => 1.0,   // Average
            201..=500 => -1.0,  // Poor
            _ => -3.0,          // Very poor
        }
    }

    /// Calculate uptime score based on total uptime and session stability
    fn calculate_uptime_score(&self, total_uptime: Duration, session_count: u32) -> f64 {
        if session_count == 0 {
            return 0.0;
        }

        let uptime_hours = total_uptime.as_secs_f64() / 3600.0;
        let avg_session_hours = uptime_hours / session_count as f64;

        // Reward both total uptime and session stability
        let uptime_component = (uptime_hours / 24.0).min(10.0); // Max 10 points for 24+ hours
        let stability_component = (avg_session_hours / 2.0).min(5.0); // Max 5 points for 2+ hour sessions

        uptime_component + stability_component
    }

    /// Calculate decay factor based on time elapsed
    fn calculate_decay_factor(&self, elapsed: Duration) -> f64 {
        let hours_elapsed = elapsed.as_secs_f64() / 3600.0;
        let decay_per_hour = self.config.decay_rate;
        (1.0 - decay_per_hour).powf(hours_elapsed).max(0.1) // Minimum 10% retention
    }

    /// Record score change for historical analysis
    fn record_score_change(&mut self, peer_id: PeerId, new_score: f64, reason: &str) {
        let entry = ScoreEntry {
            score: new_score,
            timestamp: SystemTime::now(),
            reason: reason.to_string(),
        };

        self.score_history.entry(peer_id)
            .or_insert_with(Vec::new)
            .push(entry);

        // Keep only recent history (last 100 entries per peer)
        if let Some(history) = self.score_history.get_mut(&peer_id) {
            if history.len() > 100 {
                history.drain(0..history.len() - 100);
            }
        }
    }

    /// Get score history for a peer
    pub fn get_score_history(&self, peer_id: &PeerId) -> Option<&Vec<ScoreEntry>> {
        self.score_history.get(peer_id)
    }

    /// Clean up data for disconnected peers
    pub fn cleanup_peer(&mut self, peer_id: &PeerId) {
        // Keep scores but mark as offline
        if let Some(score) = self.scores.get_mut(peer_id) {
            score.behavioral_score -= 1.0; // Small penalty for disconnection
            self.update_peer_score_internal(score);
        }
    }

    /// Remove peer completely
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        self.scores.remove(peer_id);
        self.score_history.remove(peer_id);
        self.performance_metrics.remove(peer_id);
    }
}

/// Detailed peer score breakdown
#[derive(Debug, Clone)]
pub struct PeerScore {
    /// Current overall score
    pub current_score: f64,
    /// Base score (starting point)
    pub base_score: f64,
    /// Last update timestamp
    pub last_updated: Instant,
    /// Whether this is a federation peer
    pub is_federation: bool,
    /// Score based on connection reliability
    pub connection_score: f64,
    /// Score based on network latency
    pub latency_score: f64,
    /// Score based on uptime/availability
    pub uptime_score: f64,
    /// Score based on protocol compliance
    pub protocol_score: f64,
    /// Score based on behavioral patterns
    pub behavioral_score: f64,
}

/// Historical score entry
#[derive(Debug, Clone)]
pub struct ScoreEntry {
    /// Score value
    pub score: f64,
    /// When this score was recorded
    pub timestamp: SystemTime,
    /// Reason for score change
    pub reason: String,
}

/// Performance metrics for scoring
#[derive(Debug, Default)]
pub struct PerformanceMetrics {
    /// Total time connected
    pub total_uptime: Duration,
    /// Number of connection sessions
    pub session_count: u32,
    /// Average message latency
    pub avg_latency_ms: u32,
    /// Messages successfully processed
    pub successful_messages: u64,
    /// Messages failed/rejected
    pub failed_messages: u64,
}

/// Types of protocol violations
#[derive(Debug, Clone, Copy)]
pub enum ProtocolViolation {
    /// Invalid message format
    InvalidMessage,
    /// Sending too many messages
    SpamBehavior,
    /// Malformed data in messages
    MalformedData,
    /// Unsupported protocol version
    ProtocolMismatch,
    /// Excessive timeouts
    TimeoutExcess,
}