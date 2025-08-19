//! Actor system metrics integration with Prometheus
//! 
//! This module bridges the actor_system::ActorMetrics with the global Prometheus registry,
//! providing real-time actor performance monitoring and health tracking.

use crate::metrics::*;
use actor_system::metrics::{ActorMetrics, MetricsSnapshot, AggregateStats};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, Instant};
use tokio::time::interval;
use tracing::{debug, warn, error, trace};

/// Actor types for consistent labeling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorType {
    Chain,
    Engine,
    Network,
    Bridge,
    Storage,
    Sync,
    Stream,
    Supervisor,
    System,
}

impl ActorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorType::Chain => "chain",
            ActorType::Engine => "engine", 
            ActorType::Network => "network",
            ActorType::Bridge => "bridge",
            ActorType::Storage => "storage",
            ActorType::Sync => "sync",
            ActorType::Stream => "stream",
            ActorType::Supervisor => "supervisor",
            ActorType::System => "system",
        }
    }
    
    pub fn from_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            s if s.contains("chain") => ActorType::Chain,
            s if s.contains("engine") => ActorType::Engine,
            s if s.contains("network") => ActorType::Network,
            s if s.contains("bridge") => ActorType::Bridge,
            s if s.contains("storage") => ActorType::Storage,
            s if s.contains("sync") => ActorType::Sync,
            s if s.contains("stream") => ActorType::Stream,
            s if s.contains("supervisor") => ActorType::Supervisor,
            _ => ActorType::System,
        }
    }
}

/// Message types for detailed message categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Lifecycle,      // Start, Stop, Restart, HealthCheck
    Sync,           // Block sync, peer coordination
    Network,        // P2P messages, broadcasts
    Mining,         // Block template, submission
    Governance,     // Proposal, voting
    Bridge,         // Peg operations
    Storage,        // Database operations
    System,         // Internal system messages
    Custom(u16),    // Custom message types
}

impl MessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageType::Lifecycle => "lifecycle",
            MessageType::Sync => "sync",
            MessageType::Network => "network",
            MessageType::Mining => "mining",
            MessageType::Governance => "governance",
            MessageType::Bridge => "bridge",
            MessageType::Storage => "storage",
            MessageType::System => "system",
            MessageType::Custom(_) => "custom",
        }
    }
}

/// Actor metrics bridge that collects from actor_system::ActorMetrics
/// and exports to Prometheus
#[derive(Debug)]
pub struct ActorMetricsBridge {
    /// Registered actors with their metrics
    actors: Arc<dashmap::DashMap<String, RegisteredActor>>,
    /// Collection interval for metrics updates
    collection_interval: Duration,
    /// Last collection time for calculating rates
    last_collection: Arc<parking_lot::RwLock<SystemTime>>,
    /// Performance tracking
    start_time: Instant,
}

/// Registered actor information
#[derive(Debug)]
struct RegisteredActor {
    actor_type: ActorType,
    metrics: Arc<ActorMetrics>,
    last_snapshot: Option<MetricsSnapshot>,
    registration_time: SystemTime,
}

impl ActorMetricsBridge {
    /// Create new actor metrics bridge
    pub fn new(collection_interval: Duration) -> Self {
        debug!("Initializing ActorMetricsBridge with {:?} collection interval", collection_interval);
        
        Self {
            actors: Arc::new(dashmap::DashMap::new()),
            collection_interval,
            last_collection: Arc::new(parking_lot::RwLock::new(SystemTime::now())),
            start_time: Instant::now(),
        }
    }
    
    /// Register an actor for metrics collection
    pub fn register_actor(&self, actor_name: String, actor_type: ActorType, metrics: Arc<ActorMetrics>) {
        debug!("Registering actor '{}' of type '{}'", actor_name, actor_type.as_str());
        
        let registered = RegisteredActor {
            actor_type,
            metrics,
            last_snapshot: None,
            registration_time: SystemTime::now(),
        };
        
        self.actors.insert(actor_name.clone(), registered);
        
        // Update actor lifecycle metrics
        ACTOR_LIFECYCLE_EVENTS
            .with_label_values(&[actor_type.as_str(), "spawn"])
            .inc();
    }
    
    /// Unregister an actor from metrics collection
    pub fn unregister_actor(&self, actor_name: &str) {
        if let Some((_, registered)) = self.actors.remove(actor_name) {
            debug!("Unregistering actor '{}'", actor_name);
            
            // Update actor lifecycle metrics
            ACTOR_LIFECYCLE_EVENTS
                .with_label_values(&[registered.actor_type.as_str(), "stop"])
                .inc();
        }
    }
    
    /// Start the metrics collection background task
    pub async fn start_collection(&self) -> tokio::task::JoinHandle<()> {
        let actors = self.actors.clone();
        let interval_duration = self.collection_interval;
        let last_collection = self.last_collection.clone();
        
        debug!("Starting actor metrics collection background task");
        
        tokio::spawn(async move {
            let mut interval_timer = interval(interval_duration);
            
            loop {
                interval_timer.tick().await;
                
                let collection_start = Instant::now();
                let current_time = SystemTime::now();
                
                // Update collection timestamp
                *last_collection.write() = current_time;
                
                // Collect metrics from all registered actors
                let mut total_actors = 0;
                let mut healthy_actors = 0;
                let mut total_message_count = 0;
                let mut total_restarts = 0;
                
                for mut actor_entry in actors.iter_mut() {
                    let actor_name = actor_entry.key();
                    let registered = actor_entry.value_mut();
                    
                    total_actors += 1;
                    let snapshot = registered.metrics.snapshot();
                    
                    // Update Prometheus metrics
                    Self::update_prometheus_metrics(actor_name, &registered.actor_type, &snapshot);
                    
                    // Calculate rates if we have a previous snapshot
                    if let Some(last_snapshot) = &registered.last_snapshot {
                        Self::update_rate_metrics(actor_name, &registered.actor_type, last_snapshot, &snapshot);
                    }
                    
                    // Health tracking
                    if snapshot.is_healthy() {
                        healthy_actors += 1;
                    }
                    
                    total_message_count += snapshot.messages_processed + snapshot.messages_failed;
                    total_restarts += snapshot.restarts;
                    
                    // Update last snapshot
                    registered.last_snapshot = Some(snapshot);
                }
                
                let collection_duration = collection_start.elapsed();
                
                trace!(
                    total_actors = total_actors,
                    healthy_actors = healthy_actors,
                    total_messages = total_message_count,
                    total_restarts = total_restarts,
                    collection_time_ms = collection_duration.as_millis(),
                    "Actor metrics collection completed"
                );
                
                // Update aggregate metrics
                Self::update_aggregate_metrics(total_actors, healthy_actors, total_message_count);
            }
        })
    }
    
    /// Update Prometheus metrics for a specific actor
    fn update_prometheus_metrics(actor_name: &str, actor_type: &ActorType, snapshot: &MetricsSnapshot) {
        let type_label = actor_type.as_str();
        
        // ALYS-003-11: Actor message metrics with counters and latency histograms
        ACTOR_MESSAGE_COUNT
            .with_label_values(&[type_label, "processed"])
            .inc_by(snapshot.messages_processed);
            
        ACTOR_MESSAGE_COUNT
            .with_label_values(&[type_label, "failed"])
            .inc_by(snapshot.messages_failed);
        
        // Record latency (convert from average to individual observations for histogram)
        if snapshot.avg_processing_time.as_nanos() > 0 {
            ACTOR_MESSAGE_LATENCY
                .with_label_values(&[type_label])
                .observe(snapshot.avg_processing_time.as_secs_f64());
        }
        
        // ALYS-003-12: Mailbox size monitoring per actor type
        ACTOR_MAILBOX_SIZE
            .with_label_values(&[type_label])
            .set(snapshot.mailbox_size as i64);
        
        // ALYS-003-13: Actor restart tracking
        ACTOR_RESTARTS
            .with_label_values(&[type_label, "failure"])
            .inc_by(snapshot.restarts);
        
        // ALYS-003-15: Actor performance metrics - throughput calculation
        let messages_per_second = if snapshot.avg_processing_time.as_secs_f64() > 0.0 {
            1.0 / snapshot.avg_processing_time.as_secs_f64()
        } else {
            0.0
        };
        
        ACTOR_MESSAGE_THROUGHPUT
            .with_label_values(&[type_label])
            .set(messages_per_second);
        
        // Update error counts with detailed categorization
        for (error_type, count) in &snapshot.error_counts {
            let sanitized_error = MetricLabels::sanitize_label_value(error_type);
            
            // Record errors in migration errors if they're migration-related
            if error_type.contains("migration") {
                MIGRATION_ERRORS
                    .with_label_values(&["actor_system", &sanitized_error])
                    .inc_by(*count);
            }
        }
        
        // Custom metrics from actor
        for (metric_name, value) in &snapshot.custom_counters {
            // These could be exposed as actor-specific metrics
            trace!(
                actor = actor_name,
                actor_type = type_label,
                metric = metric_name,
                value = value,
                "Custom counter metric"
            );
        }
        
        for (metric_name, value) in &snapshot.custom_gauges {
            trace!(
                actor = actor_name,
                actor_type = type_label,
                metric = metric_name,
                value = value,
                "Custom gauge metric"
            );
        }
    }
    
    /// Update rate-based metrics by comparing snapshots
    fn update_rate_metrics(
        actor_name: &str, 
        actor_type: &ActorType, 
        last: &MetricsSnapshot, 
        current: &MetricsSnapshot
    ) {
        let type_label = actor_type.as_str();
        
        // Calculate message processing rate
        let messages_delta = current.messages_processed.saturating_sub(last.messages_processed);
        let failures_delta = current.messages_failed.saturating_sub(last.messages_failed);
        
        if messages_delta > 0 || failures_delta > 0 {
            trace!(
                actor = actor_name,
                actor_type = type_label,
                messages_processed = messages_delta,
                messages_failed = failures_delta,
                "Actor activity detected"
            );
        }
        
        // Detect restart events
        let restarts_delta = current.restarts.saturating_sub(last.restarts);
        if restarts_delta > 0 {
            warn!(
                actor = actor_name,
                actor_type = type_label,
                restart_count = restarts_delta,
                "Actor restart detected"
            );
            
            // Record restart in lifecycle events
            ACTOR_LIFECYCLE_EVENTS
                .with_label_values(&[type_label, "restart"])
                .inc_by(restarts_delta);
        }
        
        // Monitor health changes
        let was_healthy = last.is_healthy();
        let is_healthy = current.is_healthy();
        
        if was_healthy && !is_healthy {
            warn!(
                actor = actor_name,
                actor_type = type_label,
                success_rate = %format!("{:.2}%", current.success_rate() * 100.0),
                error_rate = %format!("{:.2}%", current.error_rate() * 100.0),
                "Actor health degraded"
            );
        } else if !was_healthy && is_healthy {
            debug!(
                actor = actor_name,
                actor_type = type_label,
                "Actor health recovered"
            );
            
            // Record recovery event
            ACTOR_LIFECYCLE_EVENTS
                .with_label_values(&[type_label, "recover"])
                .inc();
        }
    }
    
    /// Update aggregate system metrics
    fn update_aggregate_metrics(total_actors: usize, healthy_actors: usize, total_messages: u64) {
        // Update actor count by type (this would need more detailed tracking)
        // For now, we'll update a general actor health ratio
        if total_actors > 0 {
            let health_ratio = healthy_actors as f64 / total_actors as f64;
            debug!(
                total_actors = total_actors,
                healthy_actors = healthy_actors,
                health_ratio = %format!("{:.2}%", health_ratio * 100.0),
                total_messages = total_messages,
                "System health metrics updated"
            );
        }
    }
    
    /// Get current aggregate statistics
    pub fn get_aggregate_stats(&self) -> AggregateStats {
        let snapshots: Vec<_> = self.actors.iter()
            .map(|entry| entry.value().metrics.snapshot())
            .collect();
        
        if snapshots.is_empty() {
            return AggregateStats::default();
        }
        
        let total_messages: u64 = snapshots.iter().map(|s| s.messages_processed).sum();
        let total_failed: u64 = snapshots.iter().map(|s| s.messages_failed).sum();
        let total_restarts: u64 = snapshots.iter().map(|s| s.restarts).sum();
        let total_memory: u64 = snapshots.iter().map(|s| s.peak_memory_usage).sum();
        
        let avg_response_time = if !snapshots.is_empty() {
            let total_nanos: u64 = snapshots.iter()
                .map(|s| s.avg_processing_time.as_nanos() as u64)
                .sum();
            Duration::from_nanos(total_nanos / snapshots.len() as u64)
        } else {
            Duration::from_millis(0)
        };
        
        let healthy_actors = snapshots.iter().filter(|s| s.is_healthy()).count();
        
        AggregateStats {
            total_actors: snapshots.len(),
            healthy_actors,
            total_messages_processed: total_messages,
            total_messages_failed: total_failed,
            total_restarts,
            avg_response_time,
            total_memory_usage: total_memory,
            overall_success_rate: if total_messages + total_failed > 0 {
                total_messages as f64 / (total_messages + total_failed) as f64
            } else {
                1.0
            },
        }
    }
    
    /// Record a specific message processing event
    pub fn record_message_event(
        &self,
        actor_name: &str,
        message_type: MessageType,
        processing_time: Duration,
        success: bool,
    ) {
        if let Some(actor_entry) = self.actors.get(actor_name) {
            let actor_type = actor_entry.actor_type;
            let type_label = actor_type.as_str();
            let msg_type_label = message_type.as_str();
            
            // Update detailed message metrics
            ACTOR_MESSAGE_COUNT
                .with_label_values(&[type_label, msg_type_label])
                .inc();
            
            ACTOR_MESSAGE_LATENCY
                .with_label_values(&[type_label])
                .observe(processing_time.as_secs_f64());
            
            if success {
                trace!(
                    actor = actor_name,
                    actor_type = type_label,
                    message_type = msg_type_label,
                    processing_time_ms = processing_time.as_millis(),
                    "Message processed successfully"
                );
            } else {
                debug!(
                    actor = actor_name,
                    actor_type = type_label,
                    message_type = msg_type_label,
                    processing_time_ms = processing_time.as_millis(),
                    "Message processing failed"
                );
            }
        }
    }
    
    /// Record actor lifecycle event
    pub fn record_lifecycle_event(&self, actor_name: &str, event: &str) {
        if let Some(actor_entry) = self.actors.get(actor_name) {
            let actor_type = actor_entry.actor_type;
            
            ACTOR_LIFECYCLE_EVENTS
                .with_label_values(&[actor_type.as_str(), event])
                .inc();
            
            debug!(
                actor = actor_name,
                actor_type = actor_type.as_str(),
                event = event,
                "Actor lifecycle event recorded"
            );
        }
    }
    
    /// Get metrics for a specific actor
    pub fn get_actor_metrics(&self, actor_name: &str) -> Option<MetricsSnapshot> {
        self.actors.get(actor_name)
            .map(|entry| entry.metrics.snapshot())
    }
    
    /// Get all registered actor names and types
    pub fn get_registered_actors(&self) -> HashMap<String, ActorType> {
        self.actors.iter()
            .map(|entry| (entry.key().clone(), entry.value().actor_type))
            .collect()
    }
    
    /// Check overall system health based on actor health
    pub fn is_system_healthy(&self) -> bool {
        let stats = self.get_aggregate_stats();
        
        if stats.total_actors == 0 {
            return true; // No actors to monitor
        }
        
        let health_ratio = stats.healthy_actors as f64 / stats.total_actors as f64;
        let system_healthy = health_ratio >= 0.8 && stats.overall_success_rate >= 0.95;
        
        debug!(
            total_actors = stats.total_actors,
            healthy_actors = stats.healthy_actors,
            health_ratio = %format!("{:.2}%", health_ratio * 100.0),
            success_rate = %format!("{:.2}%", stats.overall_success_rate * 100.0),
            system_healthy = system_healthy,
            "System health check completed"
        );
        
        system_healthy
    }
    
    /// Get uptime since bridge creation
    pub fn get_uptime(&self) -> Duration {
        self.start_time.elapsed()
    }
}

impl Default for ActorMetricsBridge {
    fn default() -> Self {
        Self::new(Duration::from_secs(5))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actor_system::metrics::ActorMetrics;
    
    #[tokio::test]
    async fn test_actor_metrics_bridge() {
        let bridge = ActorMetricsBridge::new(Duration::from_millis(100));
        let metrics = Arc::new(ActorMetrics::new());
        
        // Register an actor
        bridge.register_actor("test_chain_actor".to_string(), ActorType::Chain, metrics.clone());
        
        // Simulate some activity
        metrics.record_message_processed(Duration::from_millis(50));
        metrics.record_message_processed(Duration::from_millis(75));
        metrics.record_message_failed("timeout");
        
        // Check stats
        let stats = bridge.get_aggregate_stats();
        assert_eq!(stats.total_actors, 1);
        assert_eq!(stats.total_messages_processed, 2);
        assert_eq!(stats.total_messages_failed, 1);
        
        // Unregister actor
        bridge.unregister_actor("test_chain_actor");
        let stats = bridge.get_aggregate_stats();
        assert_eq!(stats.total_actors, 0);
    }
    
    #[test]
    fn test_actor_type_classification() {
        assert_eq!(ActorType::from_name("chain_actor"), ActorType::Chain);
        assert_eq!(ActorType::from_name("NetworkActor"), ActorType::Network);
        assert_eq!(ActorType::from_name("bridge_supervisor"), ActorType::Bridge);
        assert_eq!(ActorType::from_name("unknown_actor"), ActorType::System);
    }
    
    #[test]
    fn test_message_type_labels() {
        assert_eq!(MessageType::Lifecycle.as_str(), "lifecycle");
        assert_eq!(MessageType::Network.as_str(), "network");
        assert_eq!(MessageType::Custom(42).as_str(), "custom");
    }
}