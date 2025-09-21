//! ChainActor V2 Metrics
//!
//! Basic metrics without over-engineering

use prometheus::{Histogram, IntCounter, IntGauge};
use std::time::Instant;

/// ChainActor metrics
#[derive(Debug, Clone)]
pub struct ChainMetrics {
    /// Blocks produced counter
    pub blocks_produced: IntCounter,

    /// Blocks imported counter
    pub blocks_imported: IntCounter,

    /// Block production failures counter
    pub block_production_failures: IntCounter,

    /// Block import failures counter
    pub block_import_failures: IntCounter,

    /// AuxPoW processed counter
    pub auxpow_processed: IntCounter,

    /// AuxPoW validation failures counter
    pub auxpow_failures: IntCounter,

    /// Peg-in operations counter
    pub pegins_processed: IntCounter,

    /// Peg-out operations counter
    pub pegouts_processed: IntCounter,

    /// Current chain height gauge
    pub chain_height: IntGauge,

    /// Sync status gauge (1 = synced, 0 = not synced)
    pub sync_status: IntGauge,

    /// Network peers count gauge
    pub network_peers: IntGauge,

    /// Block production duration histogram
    pub block_production_duration: Histogram,

    /// Block validation duration histogram
    pub block_validation_duration: Histogram,

    /// Last activity timestamp
    pub last_activity: Instant,
}

impl ChainMetrics {
    /// Create new metrics instance
    pub fn new() -> Self {
        Self {
            blocks_produced: IntCounter::new("chain_blocks_produced_total", "Total blocks produced").unwrap(),
            blocks_imported: IntCounter::new("chain_blocks_imported_total", "Total blocks imported").unwrap(),
            block_production_failures: IntCounter::new("chain_block_production_failures_total", "Block production failures").unwrap(),
            block_import_failures: IntCounter::new("chain_block_import_failures_total", "Block import failures").unwrap(),
            auxpow_processed: IntCounter::new("chain_auxpow_processed_total", "AuxPoW processed").unwrap(),
            auxpow_failures: IntCounter::new("chain_auxpow_failures_total", "AuxPoW validation failures").unwrap(),
            pegins_processed: IntCounter::new("chain_pegins_processed_total", "Peg-in operations processed").unwrap(),
            pegouts_processed: IntCounter::new("chain_pegouts_processed_total", "Peg-out operations processed").unwrap(),
            chain_height: IntGauge::new("chain_height", "Current chain height").unwrap(),
            sync_status: IntGauge::new("chain_sync_status", "Sync status (1=synced, 0=not synced)").unwrap(),
            network_peers: IntGauge::new("chain_network_peers", "Number of network peers").unwrap(),
            block_production_duration: Histogram::with_opts(prometheus::histogram_opts!("chain_block_production_duration_seconds", "Block production duration")).unwrap(),
            block_validation_duration: Histogram::with_opts(prometheus::histogram_opts!("chain_block_validation_duration_seconds", "Block validation duration")).unwrap(),
            last_activity: Instant::now(),
        }
    }

    /// Update last activity timestamp
    pub fn record_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Record block production success
    pub fn record_block_produced(&mut self, duration: std::time::Duration) {
        self.blocks_produced.inc();
        self.block_production_duration.observe(duration.as_secs_f64());
        self.record_activity();
    }

    /// Record block production failure
    pub fn record_block_production_failure(&mut self) {
        self.block_production_failures.inc();
        self.record_activity();
    }

    /// Record block import success
    pub fn record_block_imported(&mut self, duration: std::time::Duration) {
        self.blocks_imported.inc();
        self.block_validation_duration.observe(duration.as_secs_f64());
        self.record_activity();
    }

    /// Record block import failure
    pub fn record_block_import_failure(&mut self) {
        self.block_import_failures.inc();
        self.record_activity();
    }

    /// Update chain height
    pub fn set_chain_height(&mut self, height: u64) {
        self.chain_height.set(height as i64);
        self.record_activity();
    }

    /// Update sync status
    pub fn set_sync_status(&mut self, is_synced: bool) {
        self.sync_status.set(if is_synced { 1 } else { 0 });
        self.record_activity();
    }

    /// Update network peers count
    pub fn set_network_peers(&mut self, count: usize) {
        self.network_peers.set(count as i64);
        self.record_activity();
    }
}

impl Default for ChainMetrics {
    fn default() -> Self {
        Self::new()
    }
}