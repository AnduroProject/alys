//! ChainActor V2 Metrics
//!
//! Basic metrics without over-engineering
//! Phase 4: Enhanced with performance tracking
//! Phase 6: Orphan block metrics for node operators

use prometheus::{Histogram, IntCounter, IntCounterVec, IntGauge, Registry};
use std::time::Instant;

use crate::metrics::ALYS_REGISTRY;

use super::monitoring::PerformanceMetrics;

/// Reason for a block becoming orphaned
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrphanReason {
    /// Block was orphaned due to chain reorganization
    Reorg,
    /// Block arrived too late (stale)
    Stale,
    /// Block's parent is invalid
    InvalidParent,
    /// Block's parent is unknown (not yet received)
    UnknownParent,
    /// Block failed validation
    ValidationFailed,
    /// Reason is unknown or unspecified
    Unknown,
}

impl OrphanReason {
    /// Get string representation for Prometheus label
    pub fn as_str(&self) -> &'static str {
        match self {
            OrphanReason::Reorg => "reorg",
            OrphanReason::Stale => "stale",
            OrphanReason::InvalidParent => "invalid_parent",
            OrphanReason::UnknownParent => "unknown_parent",
            OrphanReason::ValidationFailed => "validation_failed",
            OrphanReason::Unknown => "unknown",
        }
    }
}

/// ChainActor metrics (Phase 4: Enhanced with performance tracking)
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

    /// Phase 4: Performance metrics for monitoring and optimization
    pub performance: PerformanceMetrics,

    /// Phase 5: Fork detection and reorganization metrics
    /// Forks detected counter
    pub forks_detected: IntCounter,
    /// Reorganizations performed counter
    pub reorganizations: IntCounter,
    /// Reorganization depth histogram (how many blocks rolled back)
    pub reorganization_depth: Histogram,
    /// Blocks in import queue gauge
    pub import_queue_depth: IntGauge,
    /// Fork choice update failures after reorganization (CRITICAL metric)
    pub fork_choice_failures_after_reorg: IntCounter,
    /// Deep reorganizations detected (exceeding automatic reorg limit)
    pub deep_reorgs_detected: IntCounter,

    /// Phase 6: Orphan block metrics for node operators
    /// Total orphan blocks detected
    pub orphan_blocks_total: IntCounter,
    /// Orphan blocks by reason (reorg, stale, invalid_parent, unknown_parent)
    pub orphan_blocks_by_reason: IntCounterVec,
    /// Valid blocks discarded during reorganization
    pub blocks_discarded_in_reorg: IntCounter,
    /// Time to complete a reorganization
    pub reorg_recovery_duration: Histogram,
    /// Length of orphaned fork chains before discard
    pub orphan_chain_length: Histogram,
    /// Blocks received with unknown parent (potential orphan)
    pub blocks_with_unknown_parent: IntCounter,
    /// Timestamp of last orphan block (as Unix epoch seconds)
    pub last_orphan_timestamp: IntGauge,
}

impl ChainMetrics {
    /// Create new metrics instance (Phase 4: Enhanced with performance metrics)
    pub fn new() -> Self {
        Self {
            blocks_produced: IntCounter::new(
                "alys_chain_blocks_produced_total",
                "Total blocks produced",
            )
            .unwrap(),
            blocks_imported: IntCounter::new(
                "alys_chain_blocks_imported_total",
                "Total blocks imported",
            )
            .unwrap(),
            block_production_failures: IntCounter::new(
                "alys_chain_block_production_failures_total",
                "Block production failures",
            )
            .unwrap(),
            block_import_failures: IntCounter::new(
                "alys_chain_block_import_failures_total",
                "Block import failures",
            )
            .unwrap(),
            auxpow_processed: IntCounter::new("alys_chain_auxpow_processed_total", "AuxPoW processed")
                .unwrap(),
            auxpow_failures: IntCounter::new(
                "alys_chain_auxpow_failures_total",
                "AuxPoW validation failures",
            )
            .unwrap(),
            pegins_processed: IntCounter::new(
                "alys_chain_pegins_processed_total",
                "Peg-in operations processed",
            )
            .unwrap(),
            pegouts_processed: IntCounter::new(
                "alys_chain_pegouts_processed_total",
                "Peg-out operations processed",
            )
            .unwrap(),
            chain_height: IntGauge::new("alys_chain_height", "Current chain height").unwrap(),
            sync_status: IntGauge::new("alys_chain_sync_status", "Sync status (1=synced, 0=not synced)")
                .unwrap(),
            network_peers: IntGauge::new("alys_chain_network_peers", "Number of network peers").unwrap(),
            block_production_duration: Histogram::with_opts(prometheus::histogram_opts!(
                "alys_chain_block_production_duration_seconds",
                "Block production duration"
            ))
            .unwrap(),
            block_validation_duration: Histogram::with_opts(prometheus::histogram_opts!(
                "alys_chain_block_validation_duration_seconds",
                "Block validation duration"
            ))
            .unwrap(),
            last_activity: Instant::now(),
            performance: PerformanceMetrics::new(), // Phase 4: Performance tracking
            // Phase 5: Fork and reorganization metrics
            forks_detected: IntCounter::new("alys_chain_forks_detected_total", "Total forks detected")
                .unwrap(),
            reorganizations: IntCounter::new(
                "alys_chain_reorganizations_total",
                "Total reorganizations performed",
            )
            .unwrap(),
            reorganization_depth: Histogram::with_opts(prometheus::histogram_opts!(
                "alys_chain_reorganization_depth",
                "Depth of chain reorganizations (blocks rolled back)"
            ))
            .unwrap(),
            import_queue_depth: IntGauge::new(
                "alys_chain_import_queue_depth",
                "Number of blocks in import queue",
            )
            .unwrap(),
            fork_choice_failures_after_reorg: IntCounter::new(
                "alys_chain_fork_choice_failures_after_reorg_total",
                "Failed fork choice updates after reorganization",
            )
            .unwrap(),
            deep_reorgs_detected: IntCounter::new(
                "alys_chain_deep_reorgs_detected_total",
                "Deep reorganizations detected (exceeding automatic limit)",
            )
            .unwrap(),
            // Phase 6: Orphan block metrics
            orphan_blocks_total: IntCounter::new(
                "alys_chain_orphan_blocks_total",
                "Total orphan blocks detected",
            )
            .unwrap(),
            orphan_blocks_by_reason: IntCounterVec::new(
                prometheus::opts!(
                    "alys_chain_orphan_blocks_by_reason_total",
                    "Orphan blocks categorized by reason"
                ),
                &["reason"],
            )
            .unwrap(),
            blocks_discarded_in_reorg: IntCounter::new(
                "alys_chain_blocks_discarded_in_reorg_total",
                "Valid blocks discarded during chain reorganization",
            )
            .unwrap(),
            reorg_recovery_duration: Histogram::with_opts(prometheus::histogram_opts!(
                "alys_chain_reorg_recovery_duration_seconds",
                "Time to complete chain reorganization",
                vec![0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0]
            ))
            .unwrap(),
            orphan_chain_length: Histogram::with_opts(prometheus::histogram_opts!(
                "alys_chain_orphan_chain_length",
                "Length of orphaned fork chains",
                vec![1.0, 2.0, 3.0, 5.0, 10.0, 20.0, 50.0, 100.0]
            ))
            .unwrap(),
            blocks_with_unknown_parent: IntCounter::new(
                "alys_chain_blocks_with_unknown_parent_total",
                "Blocks received with unknown parent hash",
            )
            .unwrap(),
            last_orphan_timestamp: IntGauge::new(
                "alys_chain_last_orphan_timestamp_seconds",
                "Unix timestamp of last orphan block detection",
            )
            .unwrap(),
        }
    }

    /// Update last activity timestamp
    pub fn record_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Record block production success
    pub fn record_block_produced(&mut self, duration: std::time::Duration) {
        self.blocks_produced.inc();
        self.block_production_duration
            .observe(duration.as_secs_f64());
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
        self.block_validation_duration
            .observe(duration.as_secs_f64());
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

    // Phase 6: Orphan block recording methods

    /// Record an orphan block detection
    pub fn record_orphan_block(&mut self, reason: OrphanReason) {
        self.orphan_blocks_total.inc();
        self.orphan_blocks_by_reason
            .with_label_values(&[reason.as_str()])
            .inc();
        self.last_orphan_timestamp.set(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        );
        self.record_activity();
    }

    /// Record a block with unknown parent (potential future orphan)
    pub fn record_unknown_parent_block(&mut self) {
        self.blocks_with_unknown_parent.inc();
        self.record_activity();
    }

    /// Record blocks discarded during a reorganization
    pub fn record_reorg_discarded_blocks(&mut self, count: u64) {
        for _ in 0..count {
            self.blocks_discarded_in_reorg.inc();
        }
        self.record_activity();
    }

    /// Record a completed reorganization with timing and depth
    pub fn record_reorganization(&mut self, depth: u64, duration: std::time::Duration) {
        self.reorganizations.inc();
        self.reorganization_depth.observe(depth as f64);
        self.reorg_recovery_duration.observe(duration.as_secs_f64());
        self.record_activity();
    }

    /// Record the length of an orphaned fork chain
    pub fn record_orphan_chain_length(&mut self, length: u64) {
        self.orphan_chain_length.observe(length as f64);
        self.record_activity();
    }

    /// Record a fork detection
    pub fn record_fork_detected(&mut self) {
        self.forks_detected.inc();
        self.record_activity();
    }

    /// Get total orphan blocks count
    pub fn get_orphan_blocks_total(&self) -> u64 {
        self.orphan_blocks_total.get()
    }

    /// Get orphan blocks count by reason
    pub fn get_orphan_blocks_by_reason(&self, reason: OrphanReason) -> u64 {
        self.orphan_blocks_by_reason
            .with_label_values(&[reason.as_str()])
            .get()
    }

    // Getter methods for testing

    /// Get activity count (total operations performed)
    pub fn get_activity_count(&self) -> u64 {
        // Sum of various operations as a proxy for activity count
        (self.blocks_produced.get()
            + self.blocks_imported.get()
            + self.auxpow_processed.get()
            + self.pegins_processed.get()
            + self.pegouts_processed.get()) as u64
    }

    /// Get current chain height
    pub fn get_chain_height(&self) -> u64 {
        self.chain_height.get() as u64
    }

    /// Get sync status
    pub fn get_sync_status(&self) -> bool {
        self.sync_status.get() == 1
    }

    /// Get network peers count
    pub fn get_network_peers(&self) -> usize {
        self.network_peers.get() as usize
    }

    /// Register all metrics with the ALYS_REGISTRY for Prometheus exposure
    /// Call this once after creating ChainMetrics to ensure metrics appear in /metrics
    pub fn register(&self) -> Result<(), prometheus::Error> {
        self.register_with_registry(&ALYS_REGISTRY)
    }

    /// Register all metrics with a specific registry
    pub fn register_with_registry(&self, registry: &Registry) -> Result<(), prometheus::Error> {
        // Core block metrics
        registry.register(Box::new(self.blocks_produced.clone()))?;
        registry.register(Box::new(self.blocks_imported.clone()))?;
        registry.register(Box::new(self.block_production_failures.clone()))?;
        registry.register(Box::new(self.block_import_failures.clone()))?;

        // AuxPoW metrics
        registry.register(Box::new(self.auxpow_processed.clone()))?;
        registry.register(Box::new(self.auxpow_failures.clone()))?;

        // Peg operations
        registry.register(Box::new(self.pegins_processed.clone()))?;
        registry.register(Box::new(self.pegouts_processed.clone()))?;

        // Chain state metrics
        registry.register(Box::new(self.chain_height.clone()))?;
        registry.register(Box::new(self.sync_status.clone()))?;
        registry.register(Box::new(self.network_peers.clone()))?;

        // Duration histograms
        registry.register(Box::new(self.block_production_duration.clone()))?;
        registry.register(Box::new(self.block_validation_duration.clone()))?;

        // Fork and reorganization metrics (Phase 5)
        registry.register(Box::new(self.forks_detected.clone()))?;
        registry.register(Box::new(self.reorganizations.clone()))?;
        registry.register(Box::new(self.reorganization_depth.clone()))?;
        registry.register(Box::new(self.import_queue_depth.clone()))?;
        registry.register(Box::new(self.fork_choice_failures_after_reorg.clone()))?;
        registry.register(Box::new(self.deep_reorgs_detected.clone()))?;

        // Orphan block metrics (Phase 6)
        registry.register(Box::new(self.orphan_blocks_total.clone()))?;
        registry.register(Box::new(self.orphan_blocks_by_reason.clone()))?;
        registry.register(Box::new(self.blocks_discarded_in_reorg.clone()))?;
        registry.register(Box::new(self.reorg_recovery_duration.clone()))?;
        registry.register(Box::new(self.orphan_chain_length.clone()))?;
        registry.register(Box::new(self.blocks_with_unknown_parent.clone()))?;
        registry.register(Box::new(self.last_orphan_timestamp.clone()))?;

        Ok(())
    }
}

impl Default for ChainMetrics {
    fn default() -> Self {
        Self::new()
    }
}
