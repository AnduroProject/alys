//! Prometheus metrics integration for actor system
//!
//! This module provides integration with Prometheus for collecting and exposing
//! actor system metrics in a format compatible with Prometheus monitoring.

use crate::{
    error::{ActorError, ActorResult},
    metrics::{AggregateStats, MetricsCollector, MetricsSnapshot},
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::RwLock;
use tracing::{error, info};

/// Prometheus configuration for metrics collection
#[derive(Debug, Clone)]
pub struct PrometheusConfig {
    /// Enable Prometheus metrics collection
    pub enabled: bool,
    /// Metrics collection interval
    pub collection_interval: Duration,
    /// HTTP server bind address for metrics export
    pub metrics_bind_address: String,
    /// Metrics endpoint path
    pub metrics_path: String,
    /// Include custom metrics
    pub include_custom_metrics: bool,
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval: Duration::from_secs(15),
            metrics_bind_address: "127.0.0.1:9090".to_string(),
            metrics_path: "/metrics".to_string(),
            include_custom_metrics: true,
        }
    }
}

/// Simplified metrics collector for Prometheus integration
#[derive(Debug)]
pub struct PrometheusMetrics {
    config: PrometheusConfig,
    actor_snapshots: Arc<RwLock<HashMap<String, MetricsSnapshot>>>,
    system_stats: Arc<RwLock<Option<AggregateStats>>>,
    system_start_time: SystemTime,
}

impl PrometheusMetrics {
    /// Create new Prometheus metrics collector
    pub fn new(config: PrometheusConfig) -> Self {
        Self {
            config,
            actor_snapshots: Arc::new(RwLock::new(HashMap::new())),
            system_stats: Arc::new(RwLock::new(None)),
            system_start_time: SystemTime::now(),
        }
    }

    /// Update metrics from actor snapshot
    pub async fn update_actor_metrics(&self, actor_id: String, snapshot: MetricsSnapshot) {
        if !self.config.enabled {
            return;
        }

        let mut snapshots = self.actor_snapshots.write().await;
        snapshots.insert(actor_id, snapshot);
    }

    /// Update system-wide metrics
    pub async fn update_system_metrics(&self, stats: AggregateStats) {
        if !self.config.enabled {
            return;
        }

        let mut system_stats = self.system_stats.write().await;
        *system_stats = Some(stats);
    }

    /// Export metrics in Prometheus format
    pub async fn export_metrics(&self) -> ActorResult<String> {
        if !self.config.enabled {
            return Ok("# Metrics collection disabled\n".to_string());
        }

        let mut output = String::new();

        // System uptime
        let uptime = self.system_start_time.elapsed().unwrap_or_default();
        output.push_str(&format!(
            "# HELP alys_system_uptime_seconds System uptime in seconds\n\
             # TYPE alys_system_uptime_seconds gauge\n\
             alys_system_uptime_seconds {}\n\n",
            uptime.as_secs()
        ));

        // System-wide metrics
        if let Some(stats) = self.system_stats.read().await.as_ref() {
            output.push_str(&format!(
                "# HELP alys_system_health_score Overall system health score (0-1)\n\
                 # TYPE alys_system_health_score gauge\n\
                 alys_system_health_score {:.3}\n\n",
                if stats.total_actors > 0 {
                    let health_ratio = stats.healthy_actors as f64 / stats.total_actors as f64;
                    (stats.overall_success_rate + health_ratio) / 2.0
                } else {
                    1.0
                }
            ));

            output.push_str(&format!(
                "# HELP alys_active_actors Number of currently active actors\n\
                 # TYPE alys_active_actors gauge\n\
                 alys_active_actors{{state=\"total\"}} {}\n\
                 alys_active_actors{{state=\"healthy\"}} {}\n\n",
                stats.total_actors, stats.healthy_actors
            ));

            output.push_str(&format!(
                "# HELP alys_messages_processed_total Total number of messages processed\n\
                 # TYPE alys_messages_processed_total counter\n\
                 alys_messages_processed_total {}\n\n",
                stats.total_messages_processed
            ));

            output.push_str(&format!(
                "# HELP alys_messages_failed_total Total number of failed messages\n\
                 # TYPE alys_messages_failed_total counter\n\
                 alys_messages_failed_total {}\n\n",
                stats.total_messages_failed
            ));

            output.push_str(&format!(
                "# HELP alys_actor_restarts_total Total number of actor restarts\n\
                 # TYPE alys_actor_restarts_total counter\n\
                 alys_actor_restarts_total {}\n\n",
                stats.total_restarts
            ));

            output.push_str(&format!(
                "# HELP alys_system_success_rate Overall system success rate\n\
                 # TYPE alys_system_success_rate gauge\n\
                 alys_system_success_rate {:.3}\n\n",
                stats.overall_success_rate
            ));

            output.push_str(&format!(
                "# HELP alys_message_processing_duration_seconds Average message processing duration\n\
                 # TYPE alys_message_processing_duration_seconds gauge\n\
                 alys_message_processing_duration_seconds {:.6}\n\n",
                stats.avg_response_time.as_secs_f64()
            ));

            output.push_str(&format!(
                "# HELP alys_memory_usage_bytes Total memory usage by actors\n\
                 # TYPE alys_memory_usage_bytes gauge\n\
                 alys_memory_usage_bytes {}\n\n",
                stats.total_memory_usage
            ));
        }

        // Per-actor metrics
        let snapshots = self.actor_snapshots.read().await;
        for (actor_id, snapshot) in snapshots.iter() {
            // Parse actor type from actor_id if it follows the pattern "type:id"
            let (actor_type, actor_instance) = if let Some(pos) = actor_id.find(':') {
                (&actor_id[..pos], &actor_id[pos + 1..])
            } else {
                ("unknown", actor_id.as_str())
            };

            output.push_str(&format!(
                "alys_actor_messages_processed_total{{actor_type=\"{}\",actor_id=\"{}\"}} {}\n",
                actor_type, actor_instance, snapshot.messages_processed
            ));

            output.push_str(&format!(
                "alys_actor_messages_failed_total{{actor_type=\"{}\",actor_id=\"{}\"}} {}\n",
                actor_type, actor_instance, snapshot.messages_failed
            ));

            output.push_str(&format!(
                "alys_actor_mailbox_size{{actor_type=\"{}\",actor_id=\"{}\"}} {}\n",
                actor_type, actor_instance, snapshot.mailbox_size
            ));

            output.push_str(&format!(
                "alys_actor_restarts_total{{actor_type=\"{}\",actor_id=\"{}\"}} {}\n",
                actor_type, actor_instance, snapshot.restarts
            ));

            output.push_str(&format!(
                "alys_actor_memory_usage_bytes{{actor_type=\"{}\",actor_id=\"{}\"}} {}\n",
                actor_type, actor_instance, snapshot.peak_memory_usage
            ));

            output.push_str(&format!(
                "alys_actor_processing_duration_seconds{{actor_type=\"{}\",actor_id=\"{}\"}} {:.6}\n",
                actor_type, actor_instance, snapshot.avg_processing_time.as_secs_f64()
            ));

            // Custom counters
            for (counter_name, value) in &snapshot.custom_counters {
                output.push_str(&format!(
                    "alys_custom_counter_{}{{actor_type=\"{}\",actor_id=\"{}\"}} {}\n",
                    counter_name, actor_type, actor_instance, value
                ));
            }

            // Custom gauges
            for (gauge_name, value) in &snapshot.custom_gauges {
                output.push_str(&format!(
                    "alys_custom_gauge_{}{{actor_type=\"{}\",actor_id=\"{}\"}} {}\n",
                    gauge_name, actor_type, actor_instance, value
                ));
            }
        }

        Ok(output)
    }

    /// Start metrics collection from MetricsCollector
    pub fn start_collection_from_collector(
        self: Arc<Self>,
        collector: Arc<MetricsCollector>,
    ) -> tokio::task::JoinHandle<()> {
        let interval = self.config.collection_interval;
        
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            
            loop {
                interval_timer.tick().await;
                
                // Collect metrics from all actors
                let all_metrics = collector.get_all_metrics();
                for (actor_name, snapshot) in all_metrics {
                    self.update_actor_metrics(actor_name, snapshot).await;
                }
                
                // Update system-wide metrics
                let aggregate_stats = collector.get_aggregate_stats();
                self.update_system_metrics(aggregate_stats).await;
                
                info!("Prometheus metrics collection completed");
            }
        })
    }

    /// Get current configuration
    pub fn config(&self) -> &PrometheusConfig {
        &self.config
    }

    /// Check if metrics collection is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

impl Default for PrometheusMetrics {
    fn default() -> Self {
        Self::new(PrometheusConfig::default())
    }
}

/// Simple HTTP server for metrics export
pub struct MetricsServer {
    metrics: Arc<PrometheusMetrics>,
    bind_address: String,
}

impl MetricsServer {
    /// Create new metrics server
    pub fn new(metrics: Arc<PrometheusMetrics>) -> Self {
        let bind_address = metrics.config().metrics_bind_address.clone();
        Self {
            metrics,
            bind_address,
        }
    }

    /// Start HTTP server for metrics export
    pub async fn start(&self) -> ActorResult<()> {
        use std::convert::Infallible;
        use std::net::SocketAddr;

        let metrics = self.metrics.clone();

        let make_svc = hyper::service::make_service_fn(move |_conn| {
            let metrics = metrics.clone();
            async move {
                Ok::<_, Infallible>(hyper::service::service_fn(move |req| {
                    let metrics = metrics.clone();
                    async move {
                        match req.uri().path() {
                            "/metrics" => {
                                match metrics.export_metrics().await {
                                    Ok(metrics_text) => {
                                        Ok(hyper::Response::builder()
                                            .header("content-type", "text/plain; version=0.0.4; charset=utf-8")
                                            .body(hyper::Body::from(metrics_text))
                                            .unwrap())
                                    }
                                    Err(e) => {
                                        error!("Failed to export metrics: {}", e);
                                        Ok(hyper::Response::builder()
                                            .status(500)
                                            .body(hyper::Body::from(format!("Error: {}", e)))
                                            .unwrap())
                                    }
                                }
                            }
                            "/health" => {
                                Ok(hyper::Response::builder()
                                    .body(hyper::Body::from("OK"))
                                    .unwrap())
                            }
                            _ => {
                                Ok(hyper::Response::builder()
                                    .status(404)
                                    .body(hyper::Body::from("Not Found"))
                                    .unwrap())
                            }
                        }
                    }
                }))
            }
        });

        let addr: SocketAddr = self.bind_address.parse()
            .map_err(|e| ActorError::ConfigurationError {
                field: "bind_address".to_string(),
                reason: format!("Invalid address format: {}", e),
            })?;

        info!("Starting metrics server on http://{}/metrics", addr);

        let server = hyper::Server::bind(&addr).serve(make_svc);

        if let Err(e) = server.await {
            return Err(ActorError::SystemFailure {
                reason: format!("Metrics server failed: {}", e),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prometheus_config_default() {
        let config = PrometheusConfig::default();
        assert!(config.enabled);
        assert_eq!(config.collection_interval, Duration::from_secs(15));
        assert_eq!(config.metrics_bind_address, "127.0.0.1:9090");
    }

    #[tokio::test]
    async fn test_prometheus_metrics_creation() {
        let config = PrometheusConfig::default();
        let metrics = PrometheusMetrics::new(config);
        
        assert!(metrics.is_enabled());
        
        // Test metrics export
        let exported = metrics.export_metrics().await.unwrap();
        assert!(exported.contains("alys_system_uptime_seconds"));
    }

    #[tokio::test]
    async fn test_metrics_update() {
        let config = PrometheusConfig::default();
        let metrics = PrometheusMetrics::new(config);
        
        // Create a sample snapshot
        let snapshot = MetricsSnapshot {
            enabled: true,
            messages_processed: 100,
            messages_failed: 5,
            avg_processing_time: Duration::from_millis(50),
            mailbox_size: 10,
            restarts: 1,
            state_transitions: 5,
            last_activity: SystemTime::now(),
            peak_memory_usage: 1024 * 1024, // 1MB
            total_cpu_time: Duration::from_secs(10),
            error_counts: HashMap::new(),
            custom_counters: HashMap::new(),
            custom_gauges: HashMap::new(),
        };
        
        metrics.update_actor_metrics("TestActor:test_instance".to_string(), snapshot).await;
        
        let exported = metrics.export_metrics().await.unwrap();
        assert!(exported.contains("TestActor"));
        assert!(exported.contains("test_instance"));
        assert!(exported.contains("100")); // messages processed
    }
}