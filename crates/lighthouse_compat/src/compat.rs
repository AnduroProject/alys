//! Core compatibility layer implementation
//!
//! This module provides the main LighthouseCompat struct that abstracts over
//! both Lighthouse v4 and v5, enabling seamless migration and parallel operation.

use crate::{
    config::CompatConfig,
    conversion::{v4_to_v5, v5_to_v4, responses, ConversionContext, ConversionOptions},
    error::{CompatError, CompatResult},
    types::*,
    health::HealthMonitor,
    metrics::MetricsCollector,
};
use async_trait::async_trait;
use futures::future::FutureExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, warn};

/// Migration modes for the compatibility layer
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MigrationMode {
    /// Use only Lighthouse v4
    V4Only,
    
    /// Use only Lighthouse v5
    V5Only,
    
    /// Run both versions in parallel for comparison
    Parallel,
    
    /// Use v4 as primary, v5 as shadow
    V4Primary,
    
    /// Use v5 as primary, v4 as fallback
    V5Primary,
    
    /// Canary deployment with specified percentage to v5
    Canary(u8),
    
    /// A/B testing with traffic splitting
    ABTesting { test_name: String, v5_percentage: u8 },
}

/// Main compatibility layer struct
pub struct LighthouseCompat {
    /// Configuration
    config: CompatConfig,
    
    /// Current migration mode
    mode: Arc<RwLock<MigrationMode>>,
    
    /// V4 client (optional)
    v4_client: Option<Arc<V4Client>>,
    
    /// V5 client (optional)
    v5_client: Option<Arc<V5Client>>,
    
    /// Conversion context for tracking statistics
    conversion_context: Arc<RwLock<ConversionContext>>,
    
    /// Health monitor
    health_monitor: Arc<HealthMonitor>,
    
    /// Metrics collector
    metrics_collector: Arc<MetricsCollector>,
    
    /// Migration statistics
    stats: Arc<RwLock<MigrationStats>>,
    
    /// Session manager for sticky sessions
    session_manager: Arc<SessionManager>,
}

/// Lighthouse v4 client wrapper
pub struct V4Client {
    /// HTTP client for Engine API
    engine_client: Arc<lighthouse_wrapper::execution_layer::HttpJsonRpc>,
    
    /// Public HTTP client
    public_client: Option<Arc<lighthouse_wrapper::execution_layer::HttpJsonRpc>>,
    
    /// Configuration
    config: crate::config::V4Config,
}

/// Lighthouse v5 client wrapper (placeholder for now)
pub struct V5Client {
    /// Configuration
    config: crate::config::V5Config,
    
    /// Mock client (to be replaced with actual v5 client)
    _mock_client: bool,
}

/// Session management for sticky sessions
pub struct SessionManager {
    /// Session to version mapping
    sessions: Arc<RwLock<std::collections::HashMap<String, ClientVersion>>>,
    
    /// Session timeout
    timeout: Duration,
}

impl LighthouseCompat {
    /// Create a new compatibility layer instance
    pub async fn new(config: CompatConfig) -> CompatResult<Self> {
        info!("Initializing Lighthouse compatibility layer");
        
        // Validate configuration
        config.validate()?;
        
        // Initialize clients based on configuration
        let v4_client = if config.versions.v4.enabled {
            Some(Arc::new(V4Client::new(config.versions.v4.clone()).await?))
        } else {
            None
        };
        
        let v5_client = if config.versions.v5.enabled {
            Some(Arc::new(V5Client::new(config.versions.v5.clone()).await?))
        } else {
            None
        };
        
        // Ensure at least one client is available
        if v4_client.is_none() && v5_client.is_none() {
            return Err(CompatError::Configuration {
                parameter: "versions".to_string(),
                reason: "At least one version must be enabled".to_string(),
            });
        }
        
        let conversion_options = ConversionOptions {
            allow_lossy: config.versions.compatibility.allow_lossy_conversions,
            strict_validation: config.versions.compatibility.strict_types,
            use_defaults: !config.versions.compatibility.default_values.is_empty(),
            downgrade_features: false,
        };
        
        let health_monitor = Arc::new(HealthMonitor::new(config.health.clone()).await?);
        let metrics_collector = Arc::new(MetricsCollector::new(config.observability.metrics.clone())?);
        let session_manager = Arc::new(SessionManager::new(config.migration.traffic_splitting.session_timeout));
        
        let compat = Self {
            mode: Arc::new(RwLock::new(config.migration.initial_mode.clone())),
            config,
            v4_client,
            v5_client,
            conversion_context: Arc::new(RwLock::new(ConversionContext::new(conversion_options))),
            health_monitor,
            metrics_collector,
            stats: Arc::new(RwLock::new(MigrationStats::default())),
            session_manager,
        };
        
        // Start health monitoring
        compat.start_health_monitoring().await?;
        
        info!("Lighthouse compatibility layer initialized successfully");
        Ok(compat)
    }
    
    /// Get current migration mode
    pub async fn get_migration_mode(&self) -> MigrationMode {
        self.mode.read().await.clone()
    }
    
    /// Set migration mode
    pub async fn set_migration_mode(&self, mode: MigrationMode) -> CompatResult<()> {
        info!("Changing migration mode to: {:?}", mode);
        
        // Validate mode is possible with current configuration
        match &mode {
            MigrationMode::V4Only if self.v4_client.is_none() => {
                return Err(CompatError::Configuration {
                    parameter: "migration_mode".to_string(),
                    reason: "V4Only mode requires v4 client to be enabled".to_string(),
                });
            }
            MigrationMode::V5Only if self.v5_client.is_none() => {
                return Err(CompatError::Configuration {
                    parameter: "migration_mode".to_string(),
                    reason: "V5Only mode requires v5 client to be enabled".to_string(),
                });
            }
            MigrationMode::Parallel | MigrationMode::V4Primary | MigrationMode::V5Primary 
                if self.v4_client.is_none() || self.v5_client.is_none() => {
                return Err(CompatError::Configuration {
                    parameter: "migration_mode".to_string(),
                    reason: "Dual-client modes require both v4 and v5 clients to be enabled".to_string(),
                });
            }
            _ => {}
        }
        
        *self.mode.write().await = mode;
        
        // Update metrics
        self.metrics_collector.record_mode_change().await;
        
        Ok(())
    }
    
    /// Determine which client(s) to use for a request
    async fn route_request(&self, session_id: Option<&str>) -> CompatResult<RequestRouting> {
        let mode = self.get_migration_mode().await;
        
        match mode {
            MigrationMode::V4Only => Ok(RequestRouting::V4Only),
            MigrationMode::V5Only => Ok(RequestRouting::V5Only),
            MigrationMode::Parallel => Ok(RequestRouting::Parallel),
            MigrationMode::V4Primary => Ok(RequestRouting::V4Primary),
            MigrationMode::V5Primary => Ok(RequestRouting::V5Primary),
            MigrationMode::Canary(percentage) => {
                let use_v5 = self.should_use_v5(percentage, session_id).await;
                Ok(if use_v5 { RequestRouting::V5Only } else { RequestRouting::V4Only })
            }
            MigrationMode::ABTesting { test_name, v5_percentage } => {
                let use_v5 = self.should_use_v5_for_test(&test_name, v5_percentage, session_id).await;
                Ok(if use_v5 { RequestRouting::V5Only } else { RequestRouting::V4Only })
            }
        }
    }
    
    /// Determine if a request should use v5 based on percentage and session
    async fn should_use_v5(&self, percentage: u8, session_id: Option<&str>) -> bool {
        if let Some(session_id) = session_id {
            // Check for sticky session
            if let Some(version) = self.session_manager.get_session_version(session_id).await {
                return matches!(version, ClientVersion::V5 { .. });
            }
            
            // Create new sticky session
            let use_v5 = self.calculate_routing_decision(percentage, session_id);
            let version = if use_v5 {
                ClientVersion::V5 { version: "v5.0.0".to_string() }
            } else {
                ClientVersion::V4 { revision: "441fc16".to_string() }
            };
            
            self.session_manager.set_session_version(session_id, version).await;
            use_v5
        } else {
            // Random routing for non-session requests
            use rand::Rng;
            rand::thread_rng().gen_range(0..100) < percentage
        }
    }
    
    /// Calculate routing decision based on hash
    fn calculate_routing_decision(&self, percentage: u8, session_id: &str) -> bool {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        session_id.hash(&mut hasher);
        let hash = hasher.finish();
        
        let threshold = (u64::MAX / 100) * percentage as u64;
        hash < threshold
    }
    
    /// Determine if request should use v5 for A/B testing
    async fn should_use_v5_for_test(
        &self,
        test_name: &str,
        v5_percentage: u8,
        session_id: Option<&str>,
    ) -> bool {
        // For A/B testing, we might have more sophisticated logic
        // For now, use the same logic as canary deployment
        self.should_use_v5(v5_percentage, session_id).await
    }
    
    /// Start background health monitoring
    async fn start_health_monitoring(&self) -> CompatResult<()> {
        let health_monitor = Arc::clone(&self.health_monitor);
        let v4_client = self.v4_client.clone();
        let v5_client = self.v5_client.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                // Check v4 client health
                if let Some(v4_client) = &v4_client {
                    match v4_client.health_check().await {
                        Ok(status) => health_monitor.update_v4_health(status).await,
                        Err(e) => {
                            warn!("V4 health check failed: {}", e);
                            health_monitor.record_v4_error(e).await;
                        }
                    }
                }
                
                // Check v5 client health
                if let Some(v5_client) = &v5_client {
                    match v5_client.health_check().await {
                        Ok(status) => health_monitor.update_v5_health(status).await,
                        Err(e) => {
                            warn!("V5 health check failed: {}", e);
                            health_monitor.record_v5_error(e).await;
                        }
                    }
                }
            }
        });
        
        Ok(())
    }
}

#[async_trait]
impl LighthouseClient for LighthouseCompat {
    #[instrument(skip(self, payload))]
    async fn new_payload(&self, payload: ExecutionPayload) -> CompatResult<PayloadStatus> {
        let start_time = Instant::now();
        let routing = self.route_request(None).await?;
        
        let result = match routing {
            RequestRouting::V4Only => self.new_payload_v4(payload).await,
            RequestRouting::V5Only => self.new_payload_v5(payload).await,
            RequestRouting::Parallel => self.new_payload_parallel(payload).await,
            RequestRouting::V4Primary => self.new_payload_v4_primary(payload).await,
            RequestRouting::V5Primary => self.new_payload_v5_primary(payload).await,
        };
        
        // Record metrics
        let duration = start_time.elapsed();
        self.metrics_collector.record_request("new_payload", &result, duration).await;
        
        // Update statistics
        let mut stats = self.stats.write().await;
        stats.total_requests += 1;
        if result.is_ok() {
            stats.successful_requests += 1;
        } else {
            stats.failed_requests += 1;
        }
        
        result
    }
    
    #[instrument(skip(self, forkchoice_state, payload_attributes))]
    async fn forkchoice_updated(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> CompatResult<ForkchoiceUpdatedResponse> {
        let start_time = Instant::now();
        let routing = self.route_request(None).await?;
        
        let result = match routing {
            RequestRouting::V4Only => {
                self.forkchoice_updated_v4(forkchoice_state, payload_attributes).await
            }
            RequestRouting::V5Only => {
                self.forkchoice_updated_v5(forkchoice_state, payload_attributes).await
            }
            RequestRouting::Parallel => {
                self.forkchoice_updated_parallel(forkchoice_state, payload_attributes).await
            }
            RequestRouting::V4Primary => {
                self.forkchoice_updated_v4_primary(forkchoice_state, payload_attributes).await
            }
            RequestRouting::V5Primary => {
                self.forkchoice_updated_v5_primary(forkchoice_state, payload_attributes).await
            }
        };
        
        // Record metrics
        let duration = start_time.elapsed();
        self.metrics_collector.record_request("forkchoice_updated", &result, duration).await;
        
        result
    }
    
    #[instrument(skip(self))]
    async fn get_payload(&self, payload_id: PayloadId) -> CompatResult<GetPayloadResponse> {
        let start_time = Instant::now();
        let routing = self.route_request(None).await?;
        
        let result = match routing {
            RequestRouting::V4Only => self.get_payload_v4(payload_id).await,
            RequestRouting::V5Only => self.get_payload_v5(payload_id).await,
            RequestRouting::Parallel => self.get_payload_parallel(payload_id).await,
            RequestRouting::V4Primary => self.get_payload_v4_primary(payload_id).await,
            RequestRouting::V5Primary => self.get_payload_v5_primary(payload_id).await,
        };
        
        // Record metrics
        let duration = start_time.elapsed();
        self.metrics_collector.record_request("get_payload", &result, duration).await;
        
        result
    }
    
    async fn is_ready(&self) -> CompatResult<bool> {
        let routing = self.route_request(None).await?;
        
        match routing {
            RequestRouting::V4Only => {
                if let Some(v4_client) = &self.v4_client {
                    v4_client.is_ready().await
                } else {
                    Ok(false)
                }
            }
            RequestRouting::V5Only => {
                if let Some(v5_client) = &self.v5_client {
                    v5_client.is_ready().await
                } else {
                    Ok(false)
                }
            }
            _ => {
                // For parallel modes, require both clients to be ready
                let v4_ready = if let Some(v4_client) = &self.v4_client {
                    v4_client.is_ready().await.unwrap_or(false)
                } else {
                    false
                };
                
                let v5_ready = if let Some(v5_client) = &self.v5_client {
                    v5_client.is_ready().await.unwrap_or(false)
                } else {
                    false
                };
                
                Ok(v4_ready && v5_ready)
            }
        }
    }
    
    fn version(&self) -> ClientVersion {
        // Return the compatibility layer version
        ClientVersion::V4 { revision: "compat-layer".to_string() }
    }
    
    async fn health_check(&self) -> CompatResult<HealthStatus> {
        self.health_monitor.get_overall_health().await
    }
}

/// Request routing options
#[derive(Debug, Clone)]
enum RequestRouting {
    /// Route to v4 only
    V4Only,
    
    /// Route to v5 only
    V5Only,
    
    /// Route to both in parallel
    Parallel,
    
    /// Route to v4 as primary, v5 as shadow
    V4Primary,
    
    /// Route to v5 as primary, v4 as fallback
    V5Primary,
}

impl LighthouseCompat {
    /// Execute new_payload with v4 client only
    async fn new_payload_v4(&self, payload: ExecutionPayload) -> CompatResult<PayloadStatus> {
        let v4_client = self.v4_client.as_ref().ok_or_else(|| CompatError::ServiceUnavailable {
            service: "v4_client".to_string(),
        })?;
        
        // Validate v4 compatibility
        crate::conversion::validation::validate_v4_compatibility(&payload)?;
        
        // Convert to v4 format
        let v4_payload = v5_to_v4::convert_execution_payload(payload)?;
        
        // Execute on v4 client
        let result = v4_client.new_payload(v4_payload).await?;
        
        Ok(result)
    }
    
    /// Execute new_payload with v5 client only  
    async fn new_payload_v5(&self, payload: ExecutionPayload) -> CompatResult<PayloadStatus> {
        let v5_client = self.v5_client.as_ref().ok_or_else(|| CompatError::ServiceUnavailable {
            service: "v5_client".to_string(),
        })?;
        
        // V5 client can handle all payloads
        let result = v5_client.new_payload(payload).await?;
        
        Ok(result)
    }
    
    /// Execute new_payload with parallel execution
    async fn new_payload_parallel(&self, payload: ExecutionPayload) -> CompatResult<PayloadStatus> {
        let v4_client = self.v4_client.as_ref().ok_or_else(|| CompatError::ServiceUnavailable {
            service: "v4_client".to_string(),
        })?;
        let v5_client = self.v5_client.as_ref().ok_or_else(|| CompatError::ServiceUnavailable {
            service: "v5_client".to_string(),
        })?;
        
        // Execute both in parallel
        let (v4_result, v5_result) = tokio::join!(
            self.new_payload_v4(payload.clone()),
            self.new_payload_v5(payload.clone())
        );
        
        // Compare results
        match (&v4_result, &v5_result) {
            (Ok(v4_status), Ok(v5_status)) => {
                if v4_status.status == v5_status.status {
                    self.metrics_collector.record_consensus_match("new_payload").await;
                } else {
                    self.metrics_collector.record_consensus_mismatch("new_payload", 
                        &format!("v4={:?}, v5={:?}", v4_status.status, v5_status.status)).await;
                    warn!("Consensus mismatch in new_payload: v4={:?}, v5={:?}", 
                        v4_status.status, v5_status.status);
                }
            }
            (Ok(_), Err(e)) => {
                warn!("V5 failed while V4 succeeded in new_payload: {}", e);
                self.metrics_collector.record_v5_only_error("new_payload").await;
            }
            (Err(e), Ok(_)) => {
                warn!("V4 failed while V5 succeeded in new_payload: {}", e);
                self.metrics_collector.record_v4_only_error("new_payload").await;
            }
            (Err(e4), Err(e5)) => {
                error!("Both versions failed in new_payload: v4={}, v5={}", e4, e5);
                self.metrics_collector.record_both_errors("new_payload").await;
            }
        }
        
        // Return v4 result (primary) during parallel testing
        v4_result
    }
    
    /// Execute new_payload with v4 as primary, v5 as shadow
    async fn new_payload_v4_primary(&self, payload: ExecutionPayload) -> CompatResult<PayloadStatus> {
        let v4_result = self.new_payload_v4(payload.clone()).await;
        
        // Execute v5 in background (fire and forget)
        let v5_payload = payload.clone();
        let v5_client = self.v5_client.clone();
        let metrics = Arc::clone(&self.metrics_collector);
        
        tokio::spawn(async move {
            if let Some(v5_client) = v5_client {
                match v5_client.new_payload(v5_payload).await {
                    Ok(_) => metrics.record_shadow_success("new_payload").await,
                    Err(e) => {
                        warn!("Shadow v5 execution failed: {}", e);
                        metrics.record_shadow_error("new_payload").await;
                    }
                }
            }
        });
        
        v4_result
    }
    
    /// Execute new_payload with v5 as primary, v4 as fallback
    async fn new_payload_v5_primary(&self, payload: ExecutionPayload) -> CompatResult<PayloadStatus> {
        match self.new_payload_v5(payload.clone()).await {
            Ok(result) => Ok(result),
            Err(e) => {
                warn!("V5 primary failed, falling back to v4: {}", e);
                self.metrics_collector.record_fallback("new_payload").await;
                self.new_payload_v4(payload).await
            }
        }
    }
    
    /// Placeholder implementations for forkchoice_updated variants
    async fn forkchoice_updated_v4(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> CompatResult<ForkchoiceUpdatedResponse> {
        // Implementation similar to new_payload_v4
        todo!("Implement forkchoice_updated_v4")
    }
    
    async fn forkchoice_updated_v5(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> CompatResult<ForkchoiceUpdatedResponse> {
        // Implementation similar to new_payload_v5
        todo!("Implement forkchoice_updated_v5")
    }
    
    async fn forkchoice_updated_parallel(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> CompatResult<ForkchoiceUpdatedResponse> {
        // Implementation similar to new_payload_parallel
        todo!("Implement forkchoice_updated_parallel")
    }
    
    async fn forkchoice_updated_v4_primary(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> CompatResult<ForkchoiceUpdatedResponse> {
        // Implementation similar to new_payload_v4_primary
        todo!("Implement forkchoice_updated_v4_primary")
    }
    
    async fn forkchoice_updated_v5_primary(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> CompatResult<ForkchoiceUpdatedResponse> {
        // Implementation similar to new_payload_v5_primary
        todo!("Implement forkchoice_updated_v5_primary")
    }
    
    /// Placeholder implementations for get_payload variants
    async fn get_payload_v4(&self, payload_id: PayloadId) -> CompatResult<GetPayloadResponse> {
        todo!("Implement get_payload_v4")
    }
    
    async fn get_payload_v5(&self, payload_id: PayloadId) -> CompatResult<GetPayloadResponse> {
        todo!("Implement get_payload_v5")
    }
    
    async fn get_payload_parallel(&self, payload_id: PayloadId) -> CompatResult<GetPayloadResponse> {
        todo!("Implement get_payload_parallel")
    }
    
    async fn get_payload_v4_primary(&self, payload_id: PayloadId) -> CompatResult<GetPayloadResponse> {
        todo!("Implement get_payload_v4_primary")
    }
    
    async fn get_payload_v5_primary(&self, payload_id: PayloadId) -> CompatResult<GetPayloadResponse> {
        todo!("Implement get_payload_v5_primary")
    }
}

impl V4Client {
    /// Create new v4 client
    async fn new(config: crate::config::V4Config) -> CompatResult<Self> {
        use lighthouse_wrapper::execution_layer::auth::{Auth, JwtKey};
        use lighthouse_wrapper::sensitive_url::SensitiveUrl;
        
        // Read JWT secret
        let jwt_secret = std::fs::read_to_string(&config.jwt_secret_file)
            .map_err(|e| CompatError::Configuration {
                parameter: "jwt_secret_file".to_string(),
                reason: format!("Failed to read JWT secret: {}", e),
            })?;
        
        let jwt_key = JwtKey::from_hex(&jwt_secret.trim())
            .map_err(|e| CompatError::Configuration {
                parameter: "jwt_secret".to_string(),
                reason: format!("Invalid JWT secret: {}", e),
            })?;
        
        // Create engine client
        let rpc_auth = Auth::new(jwt_key, None, None);
        let engine_url = SensitiveUrl::parse(&config.engine_endpoint)
            .map_err(|e| CompatError::Configuration {
                parameter: "engine_endpoint".to_string(),
                reason: format!("Invalid engine endpoint URL: {}", e),
            })?;
        
        let engine_client = lighthouse_wrapper::execution_layer::HttpJsonRpc::new_with_auth(
            engine_url, rpc_auth, Some(3)
        ).map_err(|e| CompatError::Connection {
            endpoint: config.engine_endpoint.clone(),
            reason: format!("Failed to create engine client: {}", e),
        })?;
        
        // Create public client if specified
        let public_client = if let Some(public_endpoint) = &config.public_endpoint {
            let public_url = SensitiveUrl::parse(public_endpoint)
                .map_err(|e| CompatError::Configuration {
                    parameter: "public_endpoint".to_string(),
                    reason: format!("Invalid public endpoint URL: {}", e),
                })?;
            
            let client = lighthouse_wrapper::execution_layer::HttpJsonRpc::new(
                public_url, Some(3)
            ).map_err(|e| CompatError::Connection {
                endpoint: public_endpoint.clone(),
                reason: format!("Failed to create public client: {}", e),
            })?;
            
            Some(Arc::new(client))
        } else {
            None
        };
        
        Ok(Self {
            engine_client: Arc::new(engine_client),
            public_client,
            config,
        })
    }
    
    /// Execute new payload on v4 client
    async fn new_payload(&self, payload: lighthouse_wrapper::types::ExecutionPayloadCapella<lighthouse_wrapper::types::MainnetEthSpec>) -> CompatResult<PayloadStatus> {
        use lighthouse_wrapper::types::MainnetEthSpec;
        
        let result = self.engine_client.new_payload::<MainnetEthSpec>(
            lighthouse_wrapper::types::ExecutionPayload::Capella(payload)
        ).await.map_err(|e| CompatError::EngineApi {
            operation: "new_payload".to_string(),
            details: format!("V4 client error: {}", e),
        })?;
        
        Ok(responses::convert_payload_status_from_v4(result))
    }
    
    /// Check if v4 client is ready
    async fn is_ready(&self) -> CompatResult<bool> {
        // Simple ping to engine API
        match self.engine_client.rpc_request::<String>(
            "web3_clientVersion",
            serde_json::Value::Array(vec![]),
            Duration::from_secs(5),
        ).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    
    /// Get health status
    async fn health_check(&self) -> CompatResult<HealthStatus> {
        let start_time = Instant::now();
        let ready = self.is_ready().await?;
        let response_time = start_time.elapsed();
        
        Ok(HealthStatus {
            healthy: ready,
            sync_status: if ready { SyncStatus::Synced } else { SyncStatus::NotSyncing },
            peer_count: 0, // Would query actual peer count
            last_success: if ready { Some(std::time::SystemTime::now()) } else { None },
            error_details: if !ready { Some("Client not responding".to_string()) } else { None },
            metrics: HealthMetrics {
                avg_response_time: response_time,
                error_rate: if ready { 0.0 } else { 1.0 },
                request_count: 1,
                memory_usage_mb: 100, // Would query actual metrics
                cpu_usage: 10.0,
            },
        })
    }
}

impl V5Client {
    /// Create new v5 client (placeholder)
    async fn new(config: crate::config::V5Config) -> CompatResult<Self> {
        info!("Creating V5 client (mock implementation)");
        
        Ok(Self {
            config,
            _mock_client: true,
        })
    }
    
    /// Execute new payload on v5 client (mock)
    async fn new_payload(&self, _payload: ExecutionPayload) -> CompatResult<PayloadStatus> {
        // Mock implementation
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        Ok(PayloadStatus {
            status: PayloadStatusType::Valid,
            latest_valid_hash: Some(ethereum_types::H256::zero()),
            validation_error: None,
        })
    }
    
    /// Check if v5 client is ready (mock)
    async fn is_ready(&self) -> CompatResult<bool> {
        Ok(true)
    }
    
    /// Get health status (mock)
    async fn health_check(&self) -> CompatResult<HealthStatus> {
        Ok(HealthStatus {
            healthy: true,
            sync_status: SyncStatus::Synced,
            peer_count: 5,
            last_success: Some(std::time::SystemTime::now()),
            error_details: None,
            metrics: HealthMetrics {
                avg_response_time: Duration::from_millis(25),
                error_rate: 0.0,
                request_count: 100,
                memory_usage_mb: 150,
                cpu_usage: 15.0,
            },
        })
    }
}

impl SessionManager {
    /// Create new session manager
    pub fn new(timeout: Duration) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            timeout,
        }
    }
    
    /// Get version for a session
    async fn get_session_version(&self, session_id: &str) -> Option<ClientVersion> {
        self.sessions.read().await.get(session_id).cloned()
    }
    
    /// Set version for a session
    async fn set_session_version(&self, session_id: &str, version: ClientVersion) {
        self.sessions.write().await.insert(session_id.to_string(), version);
        
        // TODO: Implement session cleanup after timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CompatConfig;
    
    #[tokio::test]
    async fn test_migration_mode_validation() {
        let mut config = CompatConfig::default();
        config.versions.v4.enabled = true;
        config.versions.v5.enabled = false;
        
        // This should work since only v4 is enabled
        let compat = LighthouseCompat::new(config.clone()).await.unwrap();
        
        // V4Only mode should work
        assert!(compat.set_migration_mode(MigrationMode::V4Only).await.is_ok());
        
        // V5Only mode should fail
        assert!(compat.set_migration_mode(MigrationMode::V5Only).await.is_err());
        
        // Parallel mode should fail
        assert!(compat.set_migration_mode(MigrationMode::Parallel).await.is_err());
    }
    
    #[tokio::test]
    async fn test_session_routing() {
        let compat = create_test_compat().await;
        
        // Test consistent routing for same session
        let session_id = "test_session_123";
        let use_v5_1 = compat.should_use_v5(50, Some(session_id)).await;
        let use_v5_2 = compat.should_use_v5(50, Some(session_id)).await;
        
        // Should be consistent due to sticky sessions
        assert_eq!(use_v5_1, use_v5_2);
    }
    
    #[test]
    fn test_routing_decision_consistency() {
        let compat = create_mock_compat();
        
        // Same session should always get same result
        let session_id = "consistent_test";
        let decision1 = compat.calculate_routing_decision(50, session_id);
        let decision2 = compat.calculate_routing_decision(50, session_id);
        
        assert_eq!(decision1, decision2);
    }
    
    async fn create_test_compat() -> LighthouseCompat {
        let mut config = CompatConfig::default();
        config.versions.v4.enabled = true;
        config.versions.v5.enabled = false;
        
        LighthouseCompat::new(config).await.unwrap()
    }
    
    fn create_mock_compat() -> LighthouseCompat {
        // Create a mock compat instance for testing
        // This would need proper mocking infrastructure in a real implementation
        todo!("Implement mock compat for testing")
    }
}