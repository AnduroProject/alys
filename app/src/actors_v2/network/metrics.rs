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