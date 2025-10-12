//! NetworkActor V2 Metrics
//!
//! Simplified metrics collection for two-actor system.
//! Removed complex supervision metrics from V1.

use serde::{Serialize, Deserialize};
use std::time::{Duration, Instant, SystemTime};
use std::collections::HashMap;

/// NetworkActor metrics - P2P protocols only
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    // Connection metrics
    pub connected_peers: u32,
    pub total_connections: u64,
    pub failed_connections: u64,

    // Message metrics
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,

    // Gossip metrics
    pub gossip_messages_published: u64,
    pub gossip_messages_received: u64,
    pub gossip_subscription_count: u32,

    // Request-response metrics
    pub requests_sent: u64,
    pub requests_received: u64,
    pub responses_sent: u64,
    pub responses_received: u64,

    // Error metrics
    pub protocol_errors: u64,
    pub connection_errors: u64,

    // Performance metrics
    pub average_latency_ms: f64,
    pub last_updated: SystemTime,

    // Phase 4: AuxPoW metrics
    pub auxpow_broadcasts: u64,
    pub auxpow_broadcast_bytes: u64,
    pub auxpow_received: u64,

    // Phase 4: Block request metrics
    pub block_requests_sent: u64,
    pub block_request_latency_ms: Vec<u64>,
    pub block_responses_received: u64,
    pub block_response_errors: u64,

    // Phase 2 Task 2.4: mDNS discovery metrics
    pub mdns_discoveries: u64,
    pub mdns_expiries: u64,

    // Phase 4: Advanced metrics
    pub peer_reputation_average: f64,
    pub peer_reputation_min: f64,
    pub peer_reputation_max: f64,
    pub banned_peers_total: u64,
    pub rate_limited_messages: u64,
    pub rejected_connections: u64,
    pub connection_duration_p50_ms: u64,
    pub connection_duration_p95_ms: u64,
    pub connection_duration_p99_ms: u64,
    pub message_latency_p50_ms: u64,
    pub message_latency_p95_ms: u64,
    pub message_latency_p99_ms: u64,
    pub gossipsub_mesh_size: u32,
    pub gossipsub_topics_active: u32,
    pub request_response_success_rate: f64,
    pub uptime_seconds: u64,
    pub last_peer_discovered: Option<SystemTime>,
}

impl NetworkMetrics {
    pub fn new() -> Self {
        Self {
            connected_peers: 0,
            total_connections: 0,
            failed_connections: 0,
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            gossip_messages_published: 0,
            gossip_messages_received: 0,
            gossip_subscription_count: 0,
            requests_sent: 0,
            requests_received: 0,
            responses_sent: 0,
            responses_received: 0,
            protocol_errors: 0,
            connection_errors: 0,
            average_latency_ms: 0.0,
            last_updated: SystemTime::now(),
            auxpow_broadcasts: 0,
            auxpow_broadcast_bytes: 0,
            auxpow_received: 0,
            block_requests_sent: 0,
            block_request_latency_ms: Vec::new(),
            block_responses_received: 0,
            block_response_errors: 0,
            mdns_discoveries: 0,
            mdns_expiries: 0,
            // Phase 4: Initialize advanced metrics
            peer_reputation_average: 50.0,
            peer_reputation_min: 50.0,
            peer_reputation_max: 50.0,
            banned_peers_total: 0,
            rate_limited_messages: 0,
            rejected_connections: 0,
            connection_duration_p50_ms: 0,
            connection_duration_p95_ms: 0,
            connection_duration_p99_ms: 0,
            message_latency_p50_ms: 0,
            message_latency_p95_ms: 0,
            message_latency_p99_ms: 0,
            gossipsub_mesh_size: 0,
            gossipsub_topics_active: 0,
            request_response_success_rate: 0.0,
            uptime_seconds: 0,
            last_peer_discovered: None,
        }
    }

    pub fn record_connection_established(&mut self) {
        self.connected_peers += 1;
        self.total_connections += 1;
        self.last_updated = SystemTime::now();
    }

    pub fn record_connection_closed(&mut self) {
        if self.connected_peers > 0 {
            self.connected_peers -= 1;
        }
        self.last_updated = SystemTime::now();
    }

    pub fn record_connection_failed(&mut self) {
        self.failed_connections += 1;
        self.connection_errors += 1;
        self.last_updated = SystemTime::now();
    }

    pub fn record_message_sent(&mut self, size: usize) {
        self.messages_sent += 1;
        self.bytes_sent += size as u64;
        self.last_updated = SystemTime::now();
    }

    pub fn record_message_received(&mut self, size: usize) {
        self.messages_received += 1;
        self.bytes_received += size as u64;
        self.last_updated = SystemTime::now();
    }

    pub fn record_gossip_published(&mut self) {
        self.gossip_messages_published += 1;
        self.last_updated = SystemTime::now();
    }

    pub fn record_gossip_received(&mut self) {
        self.gossip_messages_received += 1;
        self.last_updated = SystemTime::now();
    }

    pub fn record_protocol_error(&mut self) {
        self.protocol_errors += 1;
        self.last_updated = SystemTime::now();
    }

    // Phase 4: AuxPoW metrics
    pub fn record_auxpow_broadcast(&mut self, bytes: usize) {
        self.auxpow_broadcasts += 1;
        self.auxpow_broadcast_bytes += bytes as u64;
        self.last_updated = SystemTime::now();
    }

    pub fn record_auxpow_received(&mut self) {
        self.auxpow_received += 1;
        self.last_updated = SystemTime::now();
    }

    // Phase 4: Block request metrics
    pub fn record_block_request_sent(&mut self) {
        self.block_requests_sent += 1;
        self.last_updated = SystemTime::now();
    }

    pub fn record_block_response(&mut self, latency: Duration) {
        self.block_responses_received += 1;
        let latency_ms = latency.as_millis() as u64;
        self.block_request_latency_ms.push(latency_ms);

        // Keep only last 100 latencies to avoid unbounded growth
        if self.block_request_latency_ms.len() > 100 {
            self.block_request_latency_ms.remove(0);
        }

        self.last_updated = SystemTime::now();
    }

    pub fn record_block_response_error(&mut self) {
        self.block_response_errors += 1;
        self.last_updated = SystemTime::now();
    }

    pub fn get_average_block_request_latency_ms(&self) -> f64 {
        if self.block_request_latency_ms.is_empty() {
            return 0.0;
        }
        let sum: u64 = self.block_request_latency_ms.iter().sum();
        sum as f64 / self.block_request_latency_ms.len() as f64
    }

    // Phase 2 Task 2.4: mDNS discovery metrics
    pub fn record_mdns_discovery(&mut self) {
        self.mdns_discoveries += 1;
        self.connected_peers += 1;
        self.total_connections += 1;
        self.last_updated = SystemTime::now();
    }

    pub fn record_mdns_expiry(&mut self) {
        self.mdns_expiries += 1;
        if self.connected_peers > 0 {
            self.connected_peers -= 1;
        }
        self.last_updated = SystemTime::now();
    }

    // Phase 4: Advanced metric methods

    /// Calculate percentiles from a sorted list of values
    pub fn calculate_percentiles(&self, values: &[u64]) -> (u64, u64, u64) {
        if values.is_empty() {
            return (0, 0, 0);
        }

        let mut sorted = values.to_vec();
        sorted.sort_unstable();

        let p50_idx = (sorted.len() as f64 * 0.50) as usize;
        let p95_idx = (sorted.len() as f64 * 0.95) as usize;
        let p99_idx = (sorted.len() as f64 * 0.99) as usize;

        let p50 = sorted[p50_idx.min(sorted.len() - 1)];
        let p95 = sorted[p95_idx.min(sorted.len() - 1)];
        let p99 = sorted[p99_idx.min(sorted.len() - 1)];

        (p50, p95, p99)
    }

    /// Update reputation statistics from peer manager
    pub fn update_reputation_stats(&mut self, peer_reputations: Vec<f64>) {
        if peer_reputations.is_empty() {
            self.peer_reputation_average = 50.0;
            self.peer_reputation_min = 50.0;
            self.peer_reputation_max = 50.0;
            return;
        }

        let sum: f64 = peer_reputations.iter().sum();
        self.peer_reputation_average = sum / peer_reputations.len() as f64;

        self.peer_reputation_min = peer_reputations.iter()
            .copied()
            .fold(f64::INFINITY, f64::min);

        self.peer_reputation_max = peer_reputations.iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
    }

    /// Record rate limited message
    pub fn record_rate_limited(&mut self) {
        self.rate_limited_messages += 1;
        self.last_updated = SystemTime::now();
    }

    /// Record rejected connection
    pub fn record_rejected_connection(&mut self) {
        self.rejected_connections += 1;
        self.last_updated = SystemTime::now();
    }

    /// Record peer ban
    pub fn record_peer_banned(&mut self) {
        self.banned_peers_total += 1;
        self.last_updated = SystemTime::now();
    }

    /// Update latency percentiles from current data
    pub fn update_latency_percentiles(&mut self) {
        if !self.block_request_latency_ms.is_empty() {
            let (p50, p95, p99) = self.calculate_percentiles(&self.block_request_latency_ms);
            self.message_latency_p50_ms = p50;
            self.message_latency_p95_ms = p95;
            self.message_latency_p99_ms = p99;
        }
    }

    /// Calculate request-response success rate
    pub fn calculate_success_rate(&mut self) {
        let total_responses = self.block_responses_received + self.block_response_errors;
        if total_responses > 0 {
            self.request_response_success_rate =
                self.block_responses_received as f64 / total_responses as f64;
        }
    }

    /// Export metrics in Prometheus format
    pub fn export_prometheus(&self) -> String {
        let mut output = String::new();

        // Connection metrics
        output.push_str(&format!("# TYPE network_connected_peers gauge\n"));
        output.push_str(&format!("network_connected_peers {}\n", self.connected_peers));
        output.push_str(&format!("# TYPE network_total_connections counter\n"));
        output.push_str(&format!("network_total_connections {}\n", self.total_connections));
        output.push_str(&format!("# TYPE network_failed_connections counter\n"));
        output.push_str(&format!("network_failed_connections {}\n", self.failed_connections));

        // Message metrics
        output.push_str(&format!("# TYPE network_messages_sent counter\n"));
        output.push_str(&format!("network_messages_sent {}\n", self.messages_sent));
        output.push_str(&format!("# TYPE network_messages_received counter\n"));
        output.push_str(&format!("network_messages_received {}\n", self.messages_received));
        output.push_str(&format!("# TYPE network_bytes_sent counter\n"));
        output.push_str(&format!("network_bytes_sent {}\n", self.bytes_sent));
        output.push_str(&format!("# TYPE network_bytes_received counter\n"));
        output.push_str(&format!("network_bytes_received {}\n", self.bytes_received));

        // Gossip metrics
        output.push_str(&format!("# TYPE network_gossip_messages_published counter\n"));
        output.push_str(&format!("network_gossip_messages_published {}\n", self.gossip_messages_published));
        output.push_str(&format!("# TYPE network_gossip_messages_received counter\n"));
        output.push_str(&format!("network_gossip_messages_received {}\n", self.gossip_messages_received));

        // Reputation metrics
        output.push_str(&format!("# TYPE network_peer_reputation_average gauge\n"));
        output.push_str(&format!("network_peer_reputation_average {}\n", self.peer_reputation_average));
        output.push_str(&format!("# TYPE network_peer_reputation_min gauge\n"));
        output.push_str(&format!("network_peer_reputation_min {}\n", self.peer_reputation_min));
        output.push_str(&format!("# TYPE network_peer_reputation_max gauge\n"));
        output.push_str(&format!("network_peer_reputation_max {}\n", self.peer_reputation_max));
        output.push_str(&format!("# TYPE network_banned_peers_total counter\n"));
        output.push_str(&format!("network_banned_peers_total {}\n", self.banned_peers_total));

        // Rate limiting metrics
        output.push_str(&format!("# TYPE network_rate_limited_messages counter\n"));
        output.push_str(&format!("network_rate_limited_messages {}\n", self.rate_limited_messages));
        output.push_str(&format!("# TYPE network_rejected_connections counter\n"));
        output.push_str(&format!("network_rejected_connections {}\n", self.rejected_connections));

        // Latency percentiles
        output.push_str(&format!("# TYPE network_message_latency_p50_ms gauge\n"));
        output.push_str(&format!("network_message_latency_p50_ms {}\n", self.message_latency_p50_ms));
        output.push_str(&format!("# TYPE network_message_latency_p95_ms gauge\n"));
        output.push_str(&format!("network_message_latency_p95_ms {}\n", self.message_latency_p95_ms));
        output.push_str(&format!("# TYPE network_message_latency_p99_ms gauge\n"));
        output.push_str(&format!("network_message_latency_p99_ms {}\n", self.message_latency_p99_ms));

        // Success rate
        output.push_str(&format!("# TYPE network_request_response_success_rate gauge\n"));
        output.push_str(&format!("network_request_response_success_rate {}\n", self.request_response_success_rate));

        // Uptime
        output.push_str(&format!("# TYPE network_uptime_seconds counter\n"));
        output.push_str(&format!("network_uptime_seconds {}\n", self.uptime_seconds));

        output
    }
}

impl Default for NetworkMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// SyncActor metrics - blockchain sync only
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMetrics {
    // Sync progress
    pub current_height: u64,
    pub target_height: u64,
    pub blocks_synced: u64,

    // Request metrics
    pub block_requests_sent: u64,
    pub block_responses_received: u64,
    pub block_request_failures: u64,

    // Processing metrics
    pub blocks_processed: u64,
    pub blocks_validated: u64,
    pub blocks_rejected: u64,

    // Performance metrics
    pub average_block_processing_time_ms: f64,
    pub sync_rate_blocks_per_second: f64,

    // Peer metrics
    pub sync_peers_active: u32,
    pub peer_request_counts: HashMap<String, u64>,

    // Error metrics
    pub validation_errors: u64,
    pub storage_errors: u64,
    pub network_errors: u64,

    // State
    pub is_syncing: bool,
    pub sync_start_time: Option<SystemTime>,
    pub last_updated: SystemTime,
}

impl SyncMetrics {
    pub fn new() -> Self {
        Self {
            current_height: 0,
            target_height: 0,
            blocks_synced: 0,
            block_requests_sent: 0,
            block_responses_received: 0,
            block_request_failures: 0,
            blocks_processed: 0,
            blocks_validated: 0,
            blocks_rejected: 0,
            average_block_processing_time_ms: 0.0,
            sync_rate_blocks_per_second: 0.0,
            sync_peers_active: 0,
            peer_request_counts: HashMap::new(),
            validation_errors: 0,
            storage_errors: 0,
            network_errors: 0,
            is_syncing: false,
            sync_start_time: None,
            last_updated: SystemTime::now(),
        }
    }

    pub fn start_sync(&mut self, target_height: u64) {
        self.is_syncing = true;
        self.target_height = target_height;
        self.sync_start_time = Some(SystemTime::now());
        self.last_updated = SystemTime::now();
    }

    pub fn stop_sync(&mut self) {
        self.is_syncing = false;
        self.sync_start_time = None;
        self.last_updated = SystemTime::now();
    }

    pub fn record_block_request(&mut self, peer_id: &str) {
        self.block_requests_sent += 1;
        *self.peer_request_counts.entry(peer_id.to_string()).or_insert(0) += 1;
        self.last_updated = SystemTime::now();
    }

    pub fn record_block_response(&mut self, block_count: u32) {
        self.block_responses_received += 1;
        self.blocks_synced += block_count as u64;
        self.last_updated = SystemTime::now();
    }

    pub fn record_block_processed(&mut self, height: u64, processing_time: Duration) {
        self.blocks_processed += 1;
        self.current_height = height;

        // Update average processing time
        let processing_ms = processing_time.as_millis() as f64;
        if self.average_block_processing_time_ms == 0.0 {
            self.average_block_processing_time_ms = processing_ms;
        } else {
            self.average_block_processing_time_ms =
                (self.average_block_processing_time_ms * 0.9) + (processing_ms * 0.1);
        }

        self.last_updated = SystemTime::now();
    }

    pub fn record_block_validated(&mut self) {
        self.blocks_validated += 1;
        self.last_updated = SystemTime::now();
    }

    pub fn record_block_rejected(&mut self, reason: &str) {
        self.blocks_rejected += 1;
        self.validation_errors += 1;
        tracing::warn!("Block rejected: {}", reason);
        self.last_updated = SystemTime::now();
    }

    pub fn record_storage_error(&mut self) {
        self.storage_errors += 1;
        self.last_updated = SystemTime::now();
    }

    pub fn record_network_error(&mut self) {
        self.network_errors += 1;
        self.last_updated = SystemTime::now();
    }

    pub fn update_sync_rate(&mut self) {
        if let Some(start_time) = self.sync_start_time {
            let elapsed = SystemTime::now().duration_since(start_time).unwrap_or_default();
            let elapsed_seconds = elapsed.as_secs_f64();
            if elapsed_seconds > 0.0 {
                self.sync_rate_blocks_per_second = self.blocks_synced as f64 / elapsed_seconds;
            }
        }
    }

    pub fn get_sync_progress(&self) -> f64 {
        if self.target_height == 0 {
            return 0.0;
        }
        (self.current_height as f64 / self.target_height as f64).min(1.0)
    }
}

impl Default for SyncMetrics {
    fn default() -> Self {
        Self::new()
    }
}