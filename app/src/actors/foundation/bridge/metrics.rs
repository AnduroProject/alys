use prometheus::{
    Counter, Histogram, Gauge, IntCounter, IntGauge, 
    register_counter, register_histogram, register_gauge,
    register_int_counter, register_int_gauge
};
use std::time::Instant;

#[derive(Clone)]
pub struct BridgeMetrics {
    // Peg-in metrics
    pub pegin_attempts: IntCounter,
    pub pegins_processed: IntCounter,
    pub pegin_processing_time: Histogram,
    pub pegin_confirmations: Histogram,
    pub pegin_volume: Counter,

    // Peg-out metrics  
    pub pegout_attempts: IntCounter,
    pub pegouts_processed: IntCounter,
    pub pegouts_broadcast: IntCounter,
    pub pegout_processing_time: Histogram,
    pub pegout_volume: Counter,

    // Operation state metrics
    pub pending_pegins: IntGauge,
    pub pending_pegouts: IntGauge,
    pub failed_operations: IntCounter,
    pub retry_attempts: IntCounter,

    // UTXO metrics
    pub available_utxos: IntGauge,
    pub total_utxo_value: Gauge,
    pub utxo_refresh_time: Histogram,
    pub utxo_selection_time: Histogram,

    // Transaction metrics
    pub tx_build_time: Histogram,
    pub tx_broadcast_time: Histogram,
    pub signature_collection_time: Histogram,
    pub average_fee_rate: Gauge,

    // Federation metrics
    pub federation_updates: IntCounter,
    pub governance_requests: IntCounter,
    pub signature_requests: IntCounter,
    pub signatures_received: IntCounter,

    // Health metrics
    pub uptime: Gauge,
    pub last_activity: Gauge,
    pub bitcoin_connection_status: IntGauge,
    pub governance_connection_status: IntGauge,

    // Error metrics
    pub error_count: IntCounter,
    pub error_rate: Gauge,
    pub critical_errors: IntCounter,
}

impl BridgeMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        Ok(Self {
            // Peg-in metrics
            pegin_attempts: register_int_counter!(
                "bridge_pegin_attempts_total",
                "Total number of peg-in attempts"
            )?,
            pegins_processed: register_int_counter!(
                "bridge_pegins_processed_total", 
                "Total number of successfully processed peg-ins"
            )?,
            pegin_processing_time: register_histogram!(
                "bridge_pegin_processing_duration_seconds",
                "Time taken to process peg-in operations"
            )?,
            pegin_confirmations: register_histogram!(
                "bridge_pegin_confirmations",
                "Number of confirmations for processed peg-ins"
            )?,
            pegin_volume: register_counter!(
                "bridge_pegin_volume_btc",
                "Total volume of BTC pegged in"
            )?,

            // Peg-out metrics
            pegout_attempts: register_int_counter!(
                "bridge_pegout_attempts_total",
                "Total number of peg-out attempts"  
            )?,
            pegouts_processed: register_int_counter!(
                "bridge_pegouts_processed_total",
                "Total number of successfully processed peg-outs"
            )?,
            pegouts_broadcast: register_int_counter!(
                "bridge_pegouts_broadcast_total",
                "Total number of peg-out transactions broadcast"
            )?,
            pegout_processing_time: register_histogram!(
                "bridge_pegout_processing_duration_seconds",
                "Time taken to process peg-out operations"
            )?,
            pegout_volume: register_counter!(
                "bridge_pegout_volume_btc",
                "Total volume of BTC pegged out"
            )?,

            // Operation state metrics
            pending_pegins: register_int_gauge!(
                "bridge_pending_pegins",
                "Number of pending peg-in operations"
            )?,
            pending_pegouts: register_int_gauge!(
                "bridge_pending_pegouts", 
                "Number of pending peg-out operations"
            )?,
            failed_operations: register_int_counter!(
                "bridge_failed_operations_total",
                "Total number of failed bridge operations"
            )?,
            retry_attempts: register_int_counter!(
                "bridge_retry_attempts_total",
                "Total number of operation retry attempts"
            )?,

            // UTXO metrics
            available_utxos: register_int_gauge!(
                "bridge_available_utxos",
                "Number of available UTXOs"
            )?,
            total_utxo_value: register_gauge!(
                "bridge_total_utxo_value_btc",
                "Total value of available UTXOs in BTC"
            )?,
            utxo_refresh_time: register_histogram!(
                "bridge_utxo_refresh_duration_seconds",
                "Time taken to refresh UTXO set"
            )?,
            utxo_selection_time: register_histogram!(
                "bridge_utxo_selection_duration_seconds",
                "Time taken to select UTXOs for transactions"
            )?,

            // Transaction metrics
            tx_build_time: register_histogram!(
                "bridge_tx_build_duration_seconds",
                "Time taken to build transactions"
            )?,
            tx_broadcast_time: register_histogram!(
                "bridge_tx_broadcast_duration_seconds", 
                "Time taken to broadcast transactions"
            )?,
            signature_collection_time: register_histogram!(
                "bridge_signature_collection_duration_seconds",
                "Time taken to collect signatures"
            )?,
            average_fee_rate: register_gauge!(
                "bridge_average_fee_rate_sat_per_byte",
                "Average fee rate in satoshis per byte"
            )?,

            // Federation metrics
            federation_updates: register_int_counter!(
                "bridge_federation_updates_total",
                "Total number of federation updates"
            )?,
            governance_requests: register_int_counter!(
                "bridge_governance_requests_total",
                "Total number of governance requests"
            )?,
            signature_requests: register_int_counter!(
                "bridge_signature_requests_total",
                "Total number of signature requests"
            )?,
            signatures_received: register_int_counter!(
                "bridge_signatures_received_total",
                "Total number of signatures received"
            )?,

            // Health metrics
            uptime: register_gauge!(
                "bridge_uptime_seconds",
                "Bridge actor uptime in seconds"
            )?,
            last_activity: register_gauge!(
                "bridge_last_activity_timestamp",
                "Timestamp of last bridge activity"
            )?,
            bitcoin_connection_status: register_int_gauge!(
                "bridge_bitcoin_connection_status",
                "Bitcoin node connection status (1=connected, 0=disconnected)"
            )?,
            governance_connection_status: register_int_gauge!(
                "bridge_governance_connection_status",
                "Governance connection status (1=connected, 0=disconnected)"
            )?,

            // Error metrics
            error_count: register_int_counter!(
                "bridge_errors_total",
                "Total number of bridge errors"
            )?,
            error_rate: register_gauge!(
                "bridge_error_rate",
                "Current error rate (errors per minute)"
            )?,
            critical_errors: register_int_counter!(
                "bridge_critical_errors_total",
                "Total number of critical errors"
            )?,
        })
    }

    /// Record a peg-in processing event
    pub fn record_pegin(&self, amount: u64, processing_time: std::time::Duration) {
        self.pegins_processed.inc();
        self.pegin_volume.inc_by(amount as f64 / 100_000_000.0); // Convert to BTC
        self.pegin_processing_time.observe(processing_time.as_secs_f64());
        self.update_activity();
    }

    /// Record a peg-out processing event  
    pub fn record_pegout(&self, amount: u64, processing_time: std::time::Duration) {
        self.pegouts_processed.inc();
        self.pegout_volume.inc_by(amount as f64 / 100_000_000.0); // Convert to BTC
        self.pegout_processing_time.observe(processing_time.as_secs_f64());
        self.update_activity();
    }

    /// Record an error occurrence
    pub fn record_error(&self, error: &super::errors::BridgeError) {
        self.error_count.inc();
        
        if error.severity() == super::errors::ErrorSeverity::Critical {
            self.critical_errors.inc();
        }
        
        self.update_activity();
    }

    /// Update UTXO metrics
    pub fn update_utxo_metrics(&self, count: usize, total_value: u64) {
        self.available_utxos.set(count as i64);
        self.total_utxo_value.set(total_value as f64 / 100_000_000.0); // Convert to BTC
    }

    /// Record UTXO refresh time
    pub fn record_utxo_refresh(&self, duration: std::time::Duration) {
        self.utxo_refresh_time.observe(duration.as_secs_f64());
    }

    /// Update connection status
    pub fn set_bitcoin_connection(&self, connected: bool) {
        self.bitcoin_connection_status.set(if connected { 1 } else { 0 });
    }

    pub fn set_governance_connection(&self, connected: bool) {
        self.governance_connection_status.set(if connected { 1 } else { 0 });
    }

    /// Update last activity timestamp
    fn update_activity(&self) {
        self.last_activity.set(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as f64
        );
    }

    /// Calculate and update success rate
    pub fn update_success_rate(&self) {
        let total = self.pegin_attempts.get() + self.pegout_attempts.get();
        let successful = self.pegins_processed.get() + self.pegouts_processed.get();
        
        if total > 0 {
            let rate = successful as f64 / total as f64;
            // Note: You'd need a success_rate gauge if you want to track this
        }
    }
}

impl Default for BridgeMetrics {
    fn default() -> Self {
        Self::new().expect("Failed to create BridgeMetrics")
    }
}

/// Timer helper for measuring operation durations
pub struct MetricsTimer {
    start: Instant,
}

impl MetricsTimer {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> std::time::Duration {
        self.start.elapsed()
    }

    pub fn observe_and_reset(&mut self, histogram: &Histogram) {
        histogram.observe(self.elapsed().as_secs_f64());
        self.start = Instant::now();
    }
}