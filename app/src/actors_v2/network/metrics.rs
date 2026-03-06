//! NetworkActor V2 Metrics
//!
//! Simplified metrics collection for two-actor system.
//! Removed complex supervision metrics from V1.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

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

    // Phase 5: Block reception metrics
    /// Blocks received via gossipsub
    pub blocks_received: u64,
    /// Blocks forwarded to ChainActor
    pub blocks_forwarded: u64,
    /// Blocks dropped due to deserialization errors
    pub blocks_deserialization_errors: u64,
    /// Blocks dropped due to cache hits (duplicates)
    pub blocks_duplicate_cached: u64,
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
            // Phase 5: Initialize block reception metrics
            blocks_received: 0,
            blocks_forwarded: 0,
            blocks_deserialization_errors: 0,
            blocks_duplicate_cached: 0,
        }
    }

    pub fn record_connection_established(&mut self) {
        self.connected_peers += 1;
        self.total_connections += 1;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        NETWORK_CONNECTED_PEERS.set(self.connected_peers as i64);
        NETWORK_TOTAL_CONNECTIONS.inc();
    }

    pub fn record_connection_closed(&mut self) {
        if self.connected_peers > 0 {
            self.connected_peers -= 1;
        }
        self.last_updated = SystemTime::now();
        // Update Prometheus
        NETWORK_CONNECTED_PEERS.set(self.connected_peers as i64);
    }

    pub fn record_connection_failed(&mut self) {
        self.failed_connections += 1;
        self.connection_errors += 1;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        NETWORK_FAILED_CONNECTIONS.inc();
        NETWORK_CONNECTION_ERRORS.inc();
    }

    pub fn record_message_sent(&mut self, size: usize) {
        self.messages_sent += 1;
        self.bytes_sent += size as u64;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        NETWORK_MESSAGES_SENT.inc();
        NETWORK_BYTES_SENT.inc_by(size as u64);
    }

    pub fn record_message_received(&mut self, size: usize) {
        self.messages_received += 1;
        self.bytes_received += size as u64;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        NETWORK_MESSAGES_RECEIVED.inc();
        NETWORK_BYTES_RECEIVED.inc_by(size as u64);
    }

    pub fn record_gossip_published(&mut self) {
        self.gossip_messages_published += 1;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        NETWORK_GOSSIP_PUBLISHED.inc();
    }

    pub fn record_gossip_received(&mut self) {
        self.gossip_messages_received += 1;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        NETWORK_GOSSIP_RECEIVED.inc();
    }

    pub fn record_protocol_error(&mut self) {
        self.protocol_errors += 1;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        NETWORK_PROTOCOL_ERRORS.inc();
    }

    // Phase 4: AuxPoW metrics
    pub fn record_auxpow_broadcast(&mut self, bytes: usize) {
        self.auxpow_broadcasts += 1;
        self.auxpow_broadcast_bytes += bytes as u64;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        NETWORK_MESSAGES_SENT.inc();
        NETWORK_BYTES_SENT.inc_by(bytes as u64);
    }

    pub fn record_auxpow_received(&mut self) {
        self.auxpow_received += 1;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        NETWORK_MESSAGES_RECEIVED.inc();
    }

    // Phase 4: Block request metrics
    pub fn record_block_request_sent(&mut self) {
        self.block_requests_sent += 1;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        NETWORK_REQUESTS_SENT.inc();
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
        // Update Prometheus
        NETWORK_RESPONSES_RECEIVED.inc();
    }

    pub fn record_block_response_error(&mut self) {
        self.block_response_errors += 1;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        NETWORK_CONNECTION_ERRORS.inc();
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
        // Update Prometheus
        NETWORK_MDNS_DISCOVERIES.inc();
        NETWORK_CONNECTED_PEERS.set(self.connected_peers as i64);
        NETWORK_TOTAL_CONNECTIONS.inc();
    }

    pub fn record_mdns_expiry(&mut self) {
        self.mdns_expiries += 1;
        if self.connected_peers > 0 {
            self.connected_peers -= 1;
        }
        self.last_updated = SystemTime::now();
        // Update Prometheus
        NETWORK_MDNS_EXPIRIES.inc();
        NETWORK_CONNECTED_PEERS.set(self.connected_peers as i64);
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

        self.peer_reputation_min = peer_reputations
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);

        self.peer_reputation_max = peer_reputations
            .iter()
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
        output.push_str(&format!(
            "network_connected_peers {}\n",
            self.connected_peers
        ));
        output.push_str(&format!("# TYPE network_total_connections counter\n"));
        output.push_str(&format!(
            "network_total_connections {}\n",
            self.total_connections
        ));
        output.push_str(&format!("# TYPE network_failed_connections counter\n"));
        output.push_str(&format!(
            "network_failed_connections {}\n",
            self.failed_connections
        ));

        // Message metrics
        output.push_str(&format!("# TYPE network_messages_sent counter\n"));
        output.push_str(&format!("network_messages_sent {}\n", self.messages_sent));
        output.push_str(&format!("# TYPE network_messages_received counter\n"));
        output.push_str(&format!(
            "network_messages_received {}\n",
            self.messages_received
        ));
        output.push_str(&format!("# TYPE network_bytes_sent counter\n"));
        output.push_str(&format!("network_bytes_sent {}\n", self.bytes_sent));
        output.push_str(&format!("# TYPE network_bytes_received counter\n"));
        output.push_str(&format!("network_bytes_received {}\n", self.bytes_received));

        // Gossip metrics
        output.push_str(&format!(
            "# TYPE network_gossip_messages_published counter\n"
        ));
        output.push_str(&format!(
            "network_gossip_messages_published {}\n",
            self.gossip_messages_published
        ));
        output.push_str(&format!(
            "# TYPE network_gossip_messages_received counter\n"
        ));
        output.push_str(&format!(
            "network_gossip_messages_received {}\n",
            self.gossip_messages_received
        ));

        // Reputation metrics
        output.push_str(&format!("# TYPE network_peer_reputation_average gauge\n"));
        output.push_str(&format!(
            "network_peer_reputation_average {}\n",
            self.peer_reputation_average
        ));
        output.push_str(&format!("# TYPE network_peer_reputation_min gauge\n"));
        output.push_str(&format!(
            "network_peer_reputation_min {}\n",
            self.peer_reputation_min
        ));
        output.push_str(&format!("# TYPE network_peer_reputation_max gauge\n"));
        output.push_str(&format!(
            "network_peer_reputation_max {}\n",
            self.peer_reputation_max
        ));
        output.push_str(&format!("# TYPE network_banned_peers_total counter\n"));
        output.push_str(&format!(
            "network_banned_peers_total {}\n",
            self.banned_peers_total
        ));

        // Rate limiting metrics
        output.push_str(&format!("# TYPE network_rate_limited_messages counter\n"));
        output.push_str(&format!(
            "network_rate_limited_messages {}\n",
            self.rate_limited_messages
        ));
        output.push_str(&format!("# TYPE network_rejected_connections counter\n"));
        output.push_str(&format!(
            "network_rejected_connections {}\n",
            self.rejected_connections
        ));

        // Latency percentiles
        output.push_str(&format!("# TYPE network_message_latency_p50_ms gauge\n"));
        output.push_str(&format!(
            "network_message_latency_p50_ms {}\n",
            self.message_latency_p50_ms
        ));
        output.push_str(&format!("# TYPE network_message_latency_p95_ms gauge\n"));
        output.push_str(&format!(
            "network_message_latency_p95_ms {}\n",
            self.message_latency_p95_ms
        ));
        output.push_str(&format!("# TYPE network_message_latency_p99_ms gauge\n"));
        output.push_str(&format!(
            "network_message_latency_p99_ms {}\n",
            self.message_latency_p99_ms
        ));

        // Success rate
        output.push_str(&format!(
            "# TYPE network_request_response_success_rate gauge\n"
        ));
        output.push_str(&format!(
            "network_request_response_success_rate {}\n",
            self.request_response_success_rate
        ));

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
        // Update Prometheus
        SYNC_TARGET_HEIGHT.set(target_height as i64);
        SYNC_IS_SYNCING.set(1);
    }

    pub fn stop_sync(&mut self) {
        self.is_syncing = false;
        self.sync_start_time = None;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        SYNC_IS_SYNCING.set(0);
    }

    pub fn record_block_request(&mut self, peer_id: &str) {
        self.block_requests_sent += 1;
        *self
            .peer_request_counts
            .entry(peer_id.to_string())
            .or_insert(0) += 1;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        SYNC_BLOCK_REQUESTS_SENT.inc();
        // Update per-peer metric (truncate peer_id to first 16 chars for label cardinality)
        let short_peer_id = truncate_peer_id(peer_id);
        PEER_BLOCK_REQUESTS.with_label_values(&[&short_peer_id]).inc();
    }

    pub fn record_block_response(&mut self, block_count: u32) {
        self.block_responses_received += 1;
        self.blocks_synced += block_count as u64;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        SYNC_BLOCK_RESPONSES_RECEIVED.inc();
        SYNC_BLOCKS_SYNCED.inc_by(block_count as u64);
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
        // Update Prometheus
        SYNC_BLOCKS_PROCESSED.inc();
        SYNC_CURRENT_HEIGHT.set(height as i64);
        SYNC_BLOCK_PROCESSING_DURATION.observe(processing_time.as_secs_f64());
    }

    pub fn record_block_validated(&mut self) {
        self.blocks_validated += 1;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        SYNC_BLOCKS_VALIDATED.inc();
    }

    pub fn record_block_rejected(&mut self, reason: &str) {
        self.blocks_rejected += 1;
        self.validation_errors += 1;
        tracing::warn!("Block rejected: {}", reason);
        self.last_updated = SystemTime::now();
        // Update Prometheus
        SYNC_BLOCKS_REJECTED.inc();
        SYNC_VALIDATION_ERRORS.inc();
    }

    pub fn record_storage_error(&mut self) {
        self.storage_errors += 1;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        SYNC_STORAGE_ERRORS.inc();
    }

    pub fn record_network_error(&mut self) {
        self.network_errors += 1;
        self.last_updated = SystemTime::now();
        // Update Prometheus
        SYNC_NETWORK_ERRORS.inc();
    }

    pub fn update_sync_rate(&mut self) {
        if let Some(start_time) = self.sync_start_time {
            let elapsed = SystemTime::now()
                .duration_since(start_time)
                .unwrap_or_default();
            let elapsed_seconds = elapsed.as_secs_f64();
            if elapsed_seconds > 0.0 {
                self.sync_rate_blocks_per_second = self.blocks_synced as f64 / elapsed_seconds;
                // Update Prometheus
                SYNC_RATE_BPS.set(self.sync_rate_blocks_per_second);
                SYNC_PROGRESS.set(self.get_sync_progress());

                // Calculate and update ETA
                let blocks_remaining = self.target_height.saturating_sub(self.current_height);
                SYNC_BLOCKS_REMAINING.set(blocks_remaining as i64);

                if self.sync_rate_blocks_per_second > 0.0 {
                    let eta_seconds = blocks_remaining as f64 / self.sync_rate_blocks_per_second;
                    SYNC_ETA_SECONDS.set(eta_seconds);
                } else {
                    SYNC_ETA_SECONDS.set(f64::INFINITY);
                }
            }
        }
    }

    pub fn get_sync_progress(&self) -> f64 {
        if self.target_height == 0 {
            return 0.0;
        }
        (self.current_height as f64 / self.target_height as f64).min(1.0)
    }

    pub fn get_sync_duration(&self) -> std::time::Duration {
        if let Some(start_time) = self.sync_start_time {
            SystemTime::now()
                .duration_since(start_time)
                .unwrap_or_default()
        } else {
            std::time::Duration::from_secs(0)
        }
    }

    pub fn record_sync_complete(&mut self, final_height: u64) {
        self.current_height = final_height;
        self.is_syncing = false;
        self.last_updated = SystemTime::now();
        tracing::info!(
            final_height = final_height,
            blocks_synced = self.blocks_synced,
            "Sync completed successfully - metrics recorded"
        );
        // Update Prometheus
        SYNC_CURRENT_HEIGHT.set(final_height as i64);
        SYNC_IS_SYNCING.set(0);
        SYNC_PROGRESS.set(1.0); // 100% complete
    }
}

impl Default for SyncMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Prometheus Metrics for SyncActor (exposed via /metrics endpoint)
// ============================================================================

use lazy_static::lazy_static;
use prometheus::{
    register_gauge_vec_with_registry, register_gauge_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Gauge, GaugeVec, Histogram, IntCounter, IntCounterVec, IntGauge,
};

use crate::metrics::ALYS_REGISTRY;

lazy_static! {
    // Sync progress metrics
    pub static ref SYNC_CURRENT_HEIGHT: IntGauge = register_int_gauge_with_registry!(
        "sync_current_height",
        "Current synced blockchain height",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref SYNC_TARGET_HEIGHT: IntGauge = register_int_gauge_with_registry!(
        "sync_target_height",
        "Target height to sync to",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref SYNC_BLOCKS_SYNCED: IntCounter = register_int_counter_with_registry!(
        "sync_blocks_synced_total",
        "Total blocks synced from peers",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref SYNC_PROGRESS: Gauge = register_gauge_with_registry!(
        "sync_progress_ratio",
        "Sync progress as ratio (0.0 to 1.0)",
        ALYS_REGISTRY
    )
    .unwrap();

    // Request metrics
    pub static ref SYNC_BLOCK_REQUESTS_SENT: IntCounter = register_int_counter_with_registry!(
        "sync_block_requests_sent_total",
        "Total block requests sent to peers",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref SYNC_BLOCK_RESPONSES_RECEIVED: IntCounter = register_int_counter_with_registry!(
        "sync_block_responses_received_total",
        "Total block responses received from peers",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref SYNC_BLOCK_REQUEST_FAILURES: IntCounter = register_int_counter_with_registry!(
        "sync_block_request_failures_total",
        "Total failed block requests",
        ALYS_REGISTRY
    )
    .unwrap();

    // Processing metrics
    pub static ref SYNC_BLOCKS_PROCESSED: IntCounter = register_int_counter_with_registry!(
        "sync_blocks_processed_total",
        "Total blocks processed during sync",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref SYNC_BLOCKS_VALIDATED: IntCounter = register_int_counter_with_registry!(
        "sync_blocks_validated_total",
        "Total blocks validated during sync",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref SYNC_BLOCKS_REJECTED: IntCounter = register_int_counter_with_registry!(
        "sync_blocks_rejected_total",
        "Total blocks rejected during sync",
        ALYS_REGISTRY
    )
    .unwrap();

    // Performance metrics
    pub static ref SYNC_BLOCK_PROCESSING_DURATION: Histogram = register_histogram_with_registry!(
        "sync_block_processing_duration_seconds",
        "Block processing duration during sync",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref SYNC_RATE_BPS: Gauge = register_gauge_with_registry!(
        "sync_rate_blocks_per_second",
        "Current sync rate in blocks per second",
        ALYS_REGISTRY
    )
    .unwrap();

    // Peer metrics
    pub static ref SYNC_ACTIVE_PEERS: IntGauge = register_int_gauge_with_registry!(
        "sync_active_peers",
        "Number of peers actively used for sync",
        ALYS_REGISTRY
    )
    .unwrap();

    // Error metrics
    pub static ref SYNC_VALIDATION_ERRORS: IntCounter = register_int_counter_with_registry!(
        "sync_validation_errors_total",
        "Total validation errors during sync",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref SYNC_STORAGE_ERRORS: IntCounter = register_int_counter_with_registry!(
        "sync_storage_errors_total",
        "Total storage errors during sync",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref SYNC_NETWORK_ERRORS: IntCounter = register_int_counter_with_registry!(
        "sync_network_errors_total",
        "Total network errors during sync",
        ALYS_REGISTRY
    )
    .unwrap();

    // State metrics
    pub static ref SYNC_IS_SYNCING: IntGauge = register_int_gauge_with_registry!(
        "sync_is_syncing",
        "Whether sync is active (1) or not (0)",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref SYNC_STATE: IntGauge = register_int_gauge_with_registry!(
        "sync_state",
        "Current sync state (0=Stopped, 1=Starting, 2=DiscoveringPeers, 3=QueryingNetworkHeight, 4=RequestingBlocks, 5=ProcessingBlocks, 6=Synced, 7=Error)",
        ALYS_REGISTRY
    )
    .unwrap();

    /// Estimated time to complete sync in seconds
    pub static ref SYNC_ETA_SECONDS: Gauge = register_gauge_with_registry!(
        "sync_eta_seconds",
        "Estimated time to complete sync in seconds",
        ALYS_REGISTRY
    )
    .unwrap();

    /// Blocks remaining to sync
    pub static ref SYNC_BLOCKS_REMAINING: IntGauge = register_int_gauge_with_registry!(
        "sync_blocks_remaining",
        "Number of blocks remaining to sync",
        ALYS_REGISTRY
    )
    .unwrap();

    // ============================================================================
    // Per-Peer Metrics (labeled by peer_id)
    // ============================================================================

    /// Block requests sent to each peer
    pub static ref PEER_BLOCK_REQUESTS: IntCounterVec = register_int_counter_vec_with_registry!(
        "peer_block_requests_total",
        "Total block requests sent to each peer",
        &["peer_id"],
        ALYS_REGISTRY
    )
    .unwrap();

    /// Block responses received from each peer
    pub static ref PEER_BLOCK_RESPONSES: IntCounterVec = register_int_counter_vec_with_registry!(
        "peer_block_responses_total",
        "Total block responses received from each peer",
        &["peer_id"],
        ALYS_REGISTRY
    )
    .unwrap();

    /// Bytes received from each peer
    pub static ref PEER_BYTES_RECEIVED: IntCounterVec = register_int_counter_vec_with_registry!(
        "peer_bytes_received_total",
        "Total bytes received from each peer",
        &["peer_id"],
        ALYS_REGISTRY
    )
    .unwrap();

    /// Bytes sent to each peer
    pub static ref PEER_BYTES_SENT: IntCounterVec = register_int_counter_vec_with_registry!(
        "peer_bytes_sent_total",
        "Total bytes sent to each peer",
        &["peer_id"],
        ALYS_REGISTRY
    )
    .unwrap();

    /// Messages received from each peer
    pub static ref PEER_MESSAGES_RECEIVED: IntCounterVec = register_int_counter_vec_with_registry!(
        "peer_messages_received_total",
        "Total messages received from each peer",
        &["peer_id"],
        ALYS_REGISTRY
    )
    .unwrap();

    /// Messages sent to each peer
    pub static ref PEER_MESSAGES_SENT: IntCounterVec = register_int_counter_vec_with_registry!(
        "peer_messages_sent_total",
        "Total messages sent to each peer",
        &["peer_id"],
        ALYS_REGISTRY
    )
    .unwrap();

    /// Errors encountered with each peer
    pub static ref PEER_ERRORS: IntCounterVec = register_int_counter_vec_with_registry!(
        "peer_errors_total",
        "Total errors encountered with each peer",
        &["peer_id", "error_type"],
        ALYS_REGISTRY
    )
    .unwrap();

    /// Reputation score for each connected peer
    pub static ref PEER_REPUTATION: GaugeVec = register_gauge_vec_with_registry!(
        "peer_reputation",
        "Current reputation score for each connected peer",
        &["peer_id"],
        ALYS_REGISTRY
    )
    .unwrap();
}

/// Helper function to update Prometheus metrics from SyncMetrics
pub fn update_prometheus_sync_metrics(metrics: &SyncMetrics) {
    SYNC_CURRENT_HEIGHT.set(metrics.current_height as i64);
    SYNC_TARGET_HEIGHT.set(metrics.target_height as i64);
    SYNC_PROGRESS.set(metrics.get_sync_progress());
    SYNC_RATE_BPS.set(metrics.sync_rate_blocks_per_second);
    SYNC_ACTIVE_PEERS.set(metrics.sync_peers_active as i64);
    SYNC_IS_SYNCING.set(if metrics.is_syncing { 1 } else { 0 });
}

/// Helper function to update sync state metric
pub fn update_prometheus_sync_state(state: &super::sync_actor::SyncState) {
    let state_value = match state {
        super::sync_actor::SyncState::Stopped => 0,
        super::sync_actor::SyncState::Starting => 1,
        super::sync_actor::SyncState::DiscoveringPeers => 2,
        super::sync_actor::SyncState::QueryingNetworkHeight => 3,
        super::sync_actor::SyncState::RequestingBlocks => 4,
        super::sync_actor::SyncState::ProcessingBlocks => 5,
        super::sync_actor::SyncState::Synced => 6,
        super::sync_actor::SyncState::Error(_) => 7,
    };
    SYNC_STATE.set(state_value);
}

// ============================================================================
// Prometheus Metrics for NetworkActor (exposed via /metrics endpoint)
// ============================================================================

lazy_static! {
    // Connection metrics
    pub static ref NETWORK_CONNECTED_PEERS: IntGauge = register_int_gauge_with_registry!(
        "network_connected_peers",
        "Number of currently connected peers",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_TOTAL_CONNECTIONS: IntCounter = register_int_counter_with_registry!(
        "network_total_connections",
        "Total number of peer connections established",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_FAILED_CONNECTIONS: IntCounter = register_int_counter_with_registry!(
        "network_failed_connections_total",
        "Total number of failed connection attempts",
        ALYS_REGISTRY
    )
    .unwrap();

    // Message metrics
    pub static ref NETWORK_MESSAGES_SENT: IntCounter = register_int_counter_with_registry!(
        "network_messages_sent_total",
        "Total number of P2P messages sent",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_MESSAGES_RECEIVED: IntCounter = register_int_counter_with_registry!(
        "network_messages_received_total",
        "Total number of P2P messages received",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_BYTES_SENT: IntCounter = register_int_counter_with_registry!(
        "network_bytes_sent_total",
        "Total bytes sent over P2P network",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_BYTES_RECEIVED: IntCounter = register_int_counter_with_registry!(
        "network_bytes_received_total",
        "Total bytes received over P2P network",
        ALYS_REGISTRY
    )
    .unwrap();

    // Gossip metrics
    pub static ref NETWORK_GOSSIP_PUBLISHED: IntCounter = register_int_counter_with_registry!(
        "network_gossip_messages_published_total",
        "Total gossipsub messages published",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_GOSSIP_RECEIVED: IntCounter = register_int_counter_with_registry!(
        "network_gossip_messages_received_total",
        "Total gossipsub messages received",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_GOSSIP_SUBSCRIPTIONS: IntGauge = register_int_gauge_with_registry!(
        "network_gossip_subscriptions",
        "Number of active gossipsub subscriptions",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_GOSSIPSUB_MESH_SIZE: IntGauge = register_int_gauge_with_registry!(
        "network_gossipsub_mesh_size",
        "Current gossipsub mesh size",
        ALYS_REGISTRY
    )
    .unwrap();

    // Request-response metrics
    pub static ref NETWORK_REQUESTS_SENT: IntCounter = register_int_counter_with_registry!(
        "network_requests_sent_total",
        "Total request-response requests sent",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_REQUESTS_RECEIVED: IntCounter = register_int_counter_with_registry!(
        "network_requests_received_total",
        "Total request-response requests received",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_RESPONSES_SENT: IntCounter = register_int_counter_with_registry!(
        "network_responses_sent_total",
        "Total request-response responses sent",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_RESPONSES_RECEIVED: IntCounter = register_int_counter_with_registry!(
        "network_responses_received_total",
        "Total request-response responses received",
        ALYS_REGISTRY
    )
    .unwrap();

    // Error metrics
    pub static ref NETWORK_PROTOCOL_ERRORS: IntCounter = register_int_counter_with_registry!(
        "network_protocol_errors_total",
        "Total protocol errors",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_CONNECTION_ERRORS: IntCounter = register_int_counter_with_registry!(
        "network_connection_errors_total",
        "Total connection errors",
        ALYS_REGISTRY
    )
    .unwrap();

    // Reputation metrics
    pub static ref NETWORK_PEER_REPUTATION_AVG: Gauge = register_gauge_with_registry!(
        "network_peer_reputation_average",
        "Average peer reputation score",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_PEER_REPUTATION_MIN: Gauge = register_gauge_with_registry!(
        "network_peer_reputation_min",
        "Minimum peer reputation score",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_PEER_REPUTATION_MAX: Gauge = register_gauge_with_registry!(
        "network_peer_reputation_max",
        "Maximum peer reputation score",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_BANNED_PEERS: IntCounter = register_int_counter_with_registry!(
        "network_banned_peers_total",
        "Total peers banned",
        ALYS_REGISTRY
    )
    .unwrap();

    // Rate limiting metrics
    pub static ref NETWORK_RATE_LIMITED: IntCounter = register_int_counter_with_registry!(
        "network_rate_limited_messages_total",
        "Total messages rate limited",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_REJECTED_CONNECTIONS: IntCounter = register_int_counter_with_registry!(
        "network_rejected_connections_total",
        "Total connections rejected",
        ALYS_REGISTRY
    )
    .unwrap();

    // Latency metrics (gauges for percentiles)
    pub static ref NETWORK_LATENCY_P50: Gauge = register_gauge_with_registry!(
        "network_message_latency_p50_ms",
        "Message latency 50th percentile in milliseconds",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_LATENCY_P95: Gauge = register_gauge_with_registry!(
        "network_message_latency_p95_ms",
        "Message latency 95th percentile in milliseconds",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_LATENCY_P99: Gauge = register_gauge_with_registry!(
        "network_message_latency_p99_ms",
        "Message latency 99th percentile in milliseconds",
        ALYS_REGISTRY
    )
    .unwrap();

    // mDNS discovery metrics
    pub static ref NETWORK_MDNS_DISCOVERIES: IntCounter = register_int_counter_with_registry!(
        "network_mdns_discoveries_total",
        "Total peers discovered via mDNS",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_MDNS_EXPIRIES: IntCounter = register_int_counter_with_registry!(
        "network_mdns_expiries_total",
        "Total mDNS peer expirations",
        ALYS_REGISTRY
    )
    .unwrap();

    // Block reception metrics (Phase 5)
    pub static ref NETWORK_BLOCKS_RECEIVED: IntCounter = register_int_counter_with_registry!(
        "network_blocks_received_total",
        "Total blocks received via gossipsub",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_BLOCKS_FORWARDED: IntCounter = register_int_counter_with_registry!(
        "network_blocks_forwarded_total",
        "Total blocks forwarded to ChainActor",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_BLOCKS_DESER_ERRORS: IntCounter = register_int_counter_with_registry!(
        "network_blocks_deserialization_errors_total",
        "Total blocks dropped due to deserialization errors",
        ALYS_REGISTRY
    )
    .unwrap();

    pub static ref NETWORK_BLOCKS_DUPLICATE: IntCounter = register_int_counter_with_registry!(
        "network_blocks_duplicate_total",
        "Total duplicate blocks cached/dropped",
        ALYS_REGISTRY
    )
    .unwrap();

    // Uptime metric
    pub static ref NETWORK_UPTIME: IntGauge = register_int_gauge_with_registry!(
        "network_uptime_seconds",
        "Network actor uptime in seconds",
        ALYS_REGISTRY
    )
    .unwrap();
}

/// Truncate peer ID to first 16 characters for Prometheus label cardinality control
pub fn truncate_peer_id(peer_id: &str) -> String {
    if peer_id.len() > 16 {
        peer_id[..16].to_string()
    } else {
        peer_id.to_string()
    }
}

/// Record per-peer message sent
pub fn record_peer_message_sent(peer_id: &str, bytes: usize) {
    let short_id = truncate_peer_id(peer_id);
    PEER_MESSAGES_SENT.with_label_values(&[&short_id]).inc();
    PEER_BYTES_SENT.with_label_values(&[&short_id]).inc_by(bytes as u64);
}

/// Record per-peer message received
pub fn record_peer_message_received(peer_id: &str, bytes: usize) {
    let short_id = truncate_peer_id(peer_id);
    PEER_MESSAGES_RECEIVED.with_label_values(&[&short_id]).inc();
    PEER_BYTES_RECEIVED.with_label_values(&[&short_id]).inc_by(bytes as u64);
}

/// Record per-peer block response received
pub fn record_peer_block_response(peer_id: &str) {
    let short_id = truncate_peer_id(peer_id);
    PEER_BLOCK_RESPONSES.with_label_values(&[&short_id]).inc();
}

/// Record per-peer error
pub fn record_peer_error(peer_id: &str, error_type: &str) {
    let short_id = truncate_peer_id(peer_id);
    PEER_ERRORS.with_label_values(&[&short_id, error_type]).inc();
}

/// Helper function to update all NetworkMetrics Prometheus gauges
pub fn update_prometheus_network_metrics(metrics: &NetworkMetrics) {
    NETWORK_CONNECTED_PEERS.set(metrics.connected_peers as i64);
    NETWORK_GOSSIP_SUBSCRIPTIONS.set(metrics.gossip_subscription_count as i64);
    NETWORK_GOSSIPSUB_MESH_SIZE.set(metrics.gossipsub_mesh_size as i64);
    NETWORK_PEER_REPUTATION_AVG.set(metrics.peer_reputation_average);
    NETWORK_PEER_REPUTATION_MIN.set(metrics.peer_reputation_min);
    NETWORK_PEER_REPUTATION_MAX.set(metrics.peer_reputation_max);
    NETWORK_LATENCY_P50.set(metrics.message_latency_p50_ms as f64);
    NETWORK_LATENCY_P95.set(metrics.message_latency_p95_ms as f64);
    NETWORK_LATENCY_P99.set(metrics.message_latency_p99_ms as f64);
    NETWORK_UPTIME.set(metrics.uptime_seconds as i64);
}

/// Helper function to update per-peer reputation scores
/// Takes a slice of (peer_id, reputation_score) tuples
pub fn update_prometheus_peer_reputations(peer_reputations: &[(String, f64)]) {
    // Reset existing peer reputation metrics to handle disconnected peers
    // Note: This clears all labels, then sets new values
    PEER_REPUTATION.reset();

    for (peer_id, reputation) in peer_reputations {
        // Use shortened peer ID for readability (first 8 chars)
        let short_peer_id = if peer_id.len() > 16 {
            format!("{}...", &peer_id[..16])
        } else {
            peer_id.clone()
        };
        PEER_REPUTATION.with_label_values(&[&short_peer_id]).set(*reputation);
    }
}
