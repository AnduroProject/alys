//! Legacy Integration & Adapters - Phase 4 Implementation (ALYS-006-16 to ALYS-006-20)
//! 
//! Provides gradual migration patterns from Arc<RwLock<T>> to actor-based systems
//! with feature flag integration, dual-path execution, and comprehensive metrics
//! collection for the Alys V2 sidechain migration.

use crate::actors::foundation::{
    ActorRegistry, FeatureFlagManager, constants::{adapter, migration}
};
use crate::chain::Chain;
use crate::engine::Engine;
use crate::features::FeatureFlagManager as CoreFeatureFlagManager;
use crate::actors::{ChainActor, EngineActor};
use crate::messages::chain_messages::*;
use crate::types::*;
use lighthouse_wrapper::types::{
    Address, ExecutionBlockHash, ExecutionPayload, MainnetEthSpec
};
use actix::{Actor, Addr, Message};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn, instrument};
use uuid::Uuid;

/// Adapter-specific error types for migration operations
#[derive(Error, Debug, Clone)]
pub enum AdapterError {
    #[error("Legacy system error: {0}")]
    LegacyError(String),

    #[error("Actor system error: {0}")]
    ActorError(String),

    #[error("Feature flag error: {flag} - {details}")]
    FeatureFlagError { flag: String, details: String },

    #[error("Migration state error: {operation} in state {state}")]
    MigrationStateError { operation: String, state: String },

    #[error("Performance threshold exceeded: {metric} = {value}, threshold = {threshold}")]
    PerformanceThresholdExceeded { metric: String, value: f64, threshold: f64 },

    #[error("Dual-path inconsistency detected: legacy={legacy:?}, actor={actor:?}")]
    DualPathInconsistency { legacy: String, actor: String },

    #[error("Adapter not initialized: {component}")]
    AdapterNotInitialized { component: String },

    #[error("Migration rollback required: {reason}")]
    MigrationRollbackRequired { reason: String },

    #[error("Adapter metrics collection failed: {source}")]
    MetricsCollectionFailed { source: String },
}

/// Migration state tracking for adapters
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationState {
    /// Legacy system only
    LegacyOnly,
    /// Both systems running, preferring legacy
    DualPathLegacyPreferred,
    /// Both systems running, preferring actor
    DualPathActorPreferred,
    /// Actor system only
    ActorOnly,
    /// Migration rolled back to legacy
    RolledBack { reason: String },
    /// Migration failed
    Failed { error: String },
}

/// Adapter performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMetrics {
    /// Operation name
    pub operation: String,
    /// Legacy execution time
    pub legacy_duration: Option<Duration>,
    /// Actor execution time
    pub actor_duration: Option<Duration>,
    /// Execution timestamp
    pub timestamp: SystemTime,
    /// Success status
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Custom metadata
    pub metadata: HashMap<String, String>,
}

/// Configuration for adapter behavior
#[derive(Debug, Clone)]
pub struct AdapterConfig {
    /// Feature flag manager reference
    pub feature_flag_manager: Arc<CoreFeatureFlagManager>,
    /// Performance monitoring enabled
    pub enable_performance_monitoring: bool,
    /// Dual-path consistency checking enabled
    pub enable_consistency_checking: bool,
    /// Metrics collection interval
    pub metrics_collection_interval: Duration,
    /// Performance threshold for migration decisions
    pub performance_threshold: f64,
    /// Maximum allowed inconsistency rate
    pub max_inconsistency_rate: f64,
    /// Migration timeout
    pub migration_timeout: Duration,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        Self {
            feature_flag_manager: Arc::new(CoreFeatureFlagManager::new()),
            enable_performance_monitoring: true,
            enable_consistency_checking: true,
            metrics_collection_interval: Duration::from_secs(60),
            performance_threshold: 1.5, // Actor should be within 1.5x legacy performance
            max_inconsistency_rate: 0.01, // 1% max inconsistency rate
            migration_timeout: Duration::from_secs(30),
        }
    }
}

/// ALYS-006-16: Design LegacyAdapter pattern for gradual migration from Arc<RwLock<T>> to actor model
/// 
/// Generic adapter trait that bridges legacy Arc<RwLock<T>> code with actor systems,
/// providing feature flag integration, performance monitoring, and gradual migration support.
#[async_trait]
pub trait LegacyAdapter<T, A> 
where 
    T: Send + Sync + 'static,
    A: Actor + Send + 'static,
{
    type Request: Send + Sync + 'static;
    type Response: Send + Sync + 'static;
    type Error: std::error::Error + Send + Sync + 'static;

    /// Execute operation via legacy system
    async fn execute_legacy(
        &self,
        legacy: &Arc<RwLock<T>>,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error>;

    /// Execute operation via actor system
    async fn execute_actor(
        &self,
        actor: &Addr<A>,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error>;

    /// Get feature flag name for this adapter
    fn feature_flag_name(&self) -> &str;

    /// Compare responses for consistency checking
    fn compare_responses(
        &self,
        legacy_response: &Self::Response,
        actor_response: &Self::Response,
    ) -> bool;

    /// Get performance metric name
    fn performance_metric_name(&self) -> &str;
}

/// Generic adapter implementation providing dual-path execution with feature flags
pub struct GenericAdapter<T, A> 
where 
    T: Send + Sync + 'static,
    A: Actor + Send + 'static,
{
    /// Legacy system reference
    legacy: Arc<RwLock<T>>,
    /// Actor system reference
    actor: Option<Addr<A>>,
    /// Adapter configuration
    config: AdapterConfig,
    /// Migration state tracking
    state: Arc<RwLock<MigrationState>>,
    /// Performance metrics collection
    metrics: Arc<RwLock<Vec<AdapterMetrics>>>,
    /// Adapter name for logging
    name: String,
}

impl<T, A> GenericAdapter<T, A> 
where 
    T: Send + Sync + 'static,
    A: Actor + Send + 'static,
{
    /// Create a new generic adapter
    pub fn new(
        name: String,
        legacy: Arc<RwLock<T>>,
        config: AdapterConfig,
    ) -> Self {
        info!("Initializing generic adapter: {}", name);
        
        Self {
            legacy,
            actor: None,
            config,
            state: Arc::new(RwLock::new(MigrationState::LegacyOnly)),
            metrics: Arc::new(RwLock::new(Vec::new())),
            name,
        }
    }

    /// Set the actor for dual-path execution
    pub async fn set_actor(&mut self, actor: Addr<A>) -> Result<(), AdapterError> {
        info!("Setting actor for adapter: {}", self.name);
        self.actor = Some(actor);
        
        // Update migration state
        let mut state = self.state.write().await;
        *state = MigrationState::DualPathLegacyPreferred;
        
        Ok(())
    }

    /// Execute operation with dual-path support
    #[instrument(skip(self, adapter_impl, request))]
    pub async fn execute<L, Request, Response, Error>(
        &self,
        adapter_impl: &L,
        request: Request,
    ) -> Result<Response, AdapterError>
    where
        L: LegacyAdapter<T, A, Request = Request, Response = Response, Error = Error>,
        Error: std::error::Error + Send + Sync + 'static,
    {
        let state = self.state.read().await;
        let start_time = Instant::now();
        
        match &*state {
            MigrationState::LegacyOnly => {
                debug!("Executing via legacy system only: {}", self.name);
                self.execute_legacy_only(adapter_impl, request, start_time).await
            }
            MigrationState::DualPathLegacyPreferred => {
                debug!("Executing dual-path with legacy preference: {}", self.name);
                self.execute_dual_path_legacy_preferred(adapter_impl, request, start_time).await
            }
            MigrationState::DualPathActorPreferred => {
                debug!("Executing dual-path with actor preference: {}", self.name);
                self.execute_dual_path_actor_preferred(adapter_impl, request, start_time).await
            }
            MigrationState::ActorOnly => {
                debug!("Executing via actor system only: {}", self.name);
                self.execute_actor_only(adapter_impl, request, start_time).await
            }
            MigrationState::RolledBack { reason } => {
                warn!("Adapter rolled back, using legacy: {} - {}", self.name, reason);
                self.execute_legacy_only(adapter_impl, request, start_time).await
            }
            MigrationState::Failed { error } => {
                error!("Adapter in failed state: {} - {}", self.name, error);
                Err(AdapterError::MigrationStateError {
                    operation: "execute".to_string(),
                    state: format!("Failed: {}", error),
                })
            }
        }
    }

    /// Execute using legacy system only
    async fn execute_legacy_only<L, Request, Response, Error>(
        &self,
        adapter_impl: &L,
        request: Request,
    ) -> Result<Response, AdapterError>
    where
        L: LegacyAdapter<T, A, Request = Request, Response = Response, Error = Error>,
        Error: std::error::Error + Send + Sync + 'static,
    {
        let start = Instant::now();
        
        match adapter_impl.execute_legacy(&self.legacy, request).await {
            Ok(response) => {
                self.record_metrics("legacy_only", Some(start.elapsed()), None, true, None).await;
                Ok(response)
            }
            Err(e) => {
                self.record_metrics("legacy_only", Some(start.elapsed()), None, false, Some(e.to_string())).await;
                Err(AdapterError::LegacyError(e.to_string()))
            }
        }
    }

    /// Execute using actor system only
    async fn execute_actor_only<L, Request, Response, Error>(
        &self,
        adapter_impl: &L,
        request: Request,
        start_time: Instant,
    ) -> Result<Response, AdapterError>
    where
        L: LegacyAdapter<T, A, Request = Request, Response = Response, Error = Error>,
        Error: std::error::Error + Send + Sync + 'static,
    {
        let actor = self.actor.as_ref().ok_or_else(|| AdapterError::AdapterNotInitialized {
            component: self.name.clone(),
        })?;

        let start = Instant::now();
        
        match adapter_impl.execute_actor(actor, request).await {
            Ok(response) => {
                self.record_metrics("actor_only", None, Some(start.elapsed()), true, None).await;
                Ok(response)
            }
            Err(e) => {
                self.record_metrics("actor_only", None, Some(start.elapsed()), false, Some(e.to_string())).await;
                Err(AdapterError::ActorError(e.to_string()))
            }
        }
    }

    /// Execute dual-path with legacy preference
    async fn execute_dual_path_legacy_preferred<L, Request, Response, Error>(
        &self,
        adapter_impl: &L,
        request: Request,
        start_time: Instant,
    ) -> Result<Response, AdapterError>
    where
        L: LegacyAdapter<T, A, Request = Request, Response = Response, Error = Error>,
        Error: std::error::Error + Send + Sync + 'static,
        Request: Clone,
    {
        // Execute legacy first
        let legacy_start = Instant::now();
        let legacy_result = adapter_impl.execute_legacy(&self.legacy, request.clone()).await;
        let legacy_duration = legacy_start.elapsed();

        // If feature flag enabled, also execute via actor for comparison
        if self.should_use_actor(adapter_impl).await {
            if let Some(actor) = &self.actor {
                let actor_start = Instant::now();
                let actor_result = adapter_impl.execute_actor(actor, request).await;
                let actor_duration = actor_start.elapsed();

                // Check consistency if both succeeded
                if let (Ok(ref legacy_resp), Ok(ref actor_resp)) = (&legacy_result, &actor_result) {
                    if self.config.enable_consistency_checking {
                        if !adapter_impl.compare_responses(legacy_resp, actor_resp) {
                            warn!("Dual-path inconsistency detected in adapter: {}", self.name);
                            self.record_metrics(
                                "dual_path_inconsistent", 
                                Some(legacy_duration), 
                                Some(actor_duration), 
                                false,
                                Some("Inconsistent results".to_string())
                            ).await;
                        }
                    }
                }

                self.record_metrics("dual_path_legacy_preferred", Some(legacy_duration), Some(actor_duration), legacy_result.is_ok(), None).await;
            }
        }

        match legacy_result {
            Ok(response) => Ok(response),
            Err(e) => Err(AdapterError::LegacyError(e.to_string())),
        }
    }

    /// Execute dual-path with actor preference
    async fn execute_dual_path_actor_preferred<L, Request, Response, Error>(
        &self,
        adapter_impl: &L,
        request: Request,
        start_time: Instant,
    ) -> Result<Response, AdapterError>
    where
        L: LegacyAdapter<T, A, Request = Request, Response = Response, Error = Error>,
        Error: std::error::Error + Send + Sync + 'static,
        Request: Clone,
    {
        let actor = self.actor.as_ref().ok_or_else(|| AdapterError::AdapterNotInitialized {
            component: self.name.clone(),
        })?;

        // Execute actor first
        let actor_start = Instant::now();
        let actor_result = adapter_impl.execute_actor(actor, request.clone()).await;
        let actor_duration = actor_start.elapsed();

        match actor_result {
            Ok(response) => {
                // Execute legacy for comparison if enabled
                if self.config.enable_consistency_checking {
                    let legacy_start = Instant::now();
                    if let Ok(legacy_resp) = adapter_impl.execute_legacy(&self.legacy, request).await {
                        let legacy_duration = legacy_start.elapsed();
                        
                        if !adapter_impl.compare_responses(&legacy_resp, &response) {
                            warn!("Dual-path inconsistency detected in adapter: {}", self.name);
                        }
                        
                        self.record_metrics("dual_path_actor_preferred", Some(legacy_duration), Some(actor_duration), true, None).await;
                    }
                }
                
                Ok(response)
            }
            Err(e) => {
                warn!("Actor execution failed, falling back to legacy: {} - {}", self.name, e);
                
                // Fallback to legacy
                let legacy_start = Instant::now();
                match adapter_impl.execute_legacy(&self.legacy, request).await {
                    Ok(response) => {
                        let legacy_duration = legacy_start.elapsed();
                        self.record_metrics("dual_path_fallback", Some(legacy_duration), Some(actor_duration), true, Some("Actor failed, legacy succeeded".to_string())).await;
                        Ok(response)
                    }
                    Err(legacy_err) => {
                        let legacy_duration = legacy_start.elapsed();
                        self.record_metrics("dual_path_both_failed", Some(legacy_duration), Some(actor_duration), false, Some(format!("Both failed: actor={}, legacy={}", e, legacy_err))).await;
                        Err(AdapterError::ActorError(e.to_string()))
                    }
                }
            }
        }
    }

    /// Check if actor system should be used based on feature flags
    async fn should_use_actor<L>(&self, adapter_impl: &L) -> bool
    where
        L: LegacyAdapter<T, A>,
    {
        self.config
            .feature_flag_manager
            .is_enabled(adapter_impl.feature_flag_name())
            .await
            .unwrap_or(false)
    }

    /// Record performance metrics
    async fn record_metrics(
        &self,
        operation: &str,
        legacy_duration: Option<Duration>,
        actor_duration: Option<Duration>,
        success: bool,
        error: Option<String>,
    ) {
        if !self.config.enable_performance_monitoring {
            return;
        }

        let metric = AdapterMetrics {
            operation: operation.to_string(),
            legacy_duration,
            actor_duration,
            timestamp: SystemTime::now(),
            success,
            error,
            metadata: HashMap::new(),
        };

        let mut metrics = self.metrics.write().await;
        metrics.push(metric);

        // Limit metrics history size
        if metrics.len() > adapter::MAX_METRICS_HISTORY {
            metrics.drain(0..adapter::METRICS_CLEANUP_BATCH_SIZE);
        }
    }

    /// Get current migration state
    pub async fn get_migration_state(&self) -> MigrationState {
        self.state.read().await.clone()
    }

    /// Update migration state
    pub async fn set_migration_state(&self, new_state: MigrationState) -> Result<(), AdapterError> {
        info!("Updating migration state for {}: {:?}", self.name, new_state);
        let mut state = self.state.write().await;
        *state = new_state;
        Ok(())
    }

    /// Get collected metrics
    pub async fn get_metrics(&self) -> Vec<AdapterMetrics> {
        self.metrics.read().await.clone()
    }

    /// Clear metrics history
    pub async fn clear_metrics(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.clear();
    }

    /// Get performance summary
    pub async fn get_performance_summary(&self) -> AdapterPerformanceSummary {
        let metrics = self.metrics.read().await;
        
        let mut legacy_times = Vec::new();
        let mut actor_times = Vec::new();
        let mut success_count = 0;
        let mut total_count = 0;

        for metric in metrics.iter() {
            total_count += 1;
            if metric.success {
                success_count += 1;
            }

            if let Some(duration) = metric.legacy_duration {
                legacy_times.push(duration);
            }

            if let Some(duration) = metric.actor_duration {
                actor_times.push(duration);
            }
        }

        let legacy_avg = if legacy_times.is_empty() {
            None
        } else {
            Some(Duration::from_nanos(
                legacy_times.iter().map(|d| d.as_nanos() as u64).sum::<u64>() / legacy_times.len() as u64
            ))
        };

        let actor_avg = if actor_times.is_empty() {
            None
        } else {
            Some(Duration::from_nanos(
                actor_times.iter().map(|d| d.as_nanos() as u64).sum::<u64>() / actor_times.len() as u64
            ))
        };

        let success_rate = if total_count > 0 {
            success_count as f64 / total_count as f64
        } else {
            0.0
        };

        AdapterPerformanceSummary {
            adapter_name: self.name.clone(),
            total_operations: total_count,
            success_rate,
            legacy_avg_duration: legacy_avg,
            actor_avg_duration: actor_avg,
            performance_ratio: match (legacy_avg, actor_avg) {
                (Some(legacy), Some(actor)) => Some(actor.as_nanos() as f64 / legacy.as_nanos() as f64),
                _ => None,
            },
            last_updated: SystemTime::now(),
        }
    }
}

/// Performance summary for an adapter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterPerformanceSummary {
    pub adapter_name: String,
    pub total_operations: usize,
    pub success_rate: f64,
    pub legacy_avg_duration: Option<Duration>,
    pub actor_avg_duration: Option<Duration>,
    pub performance_ratio: Option<f64>, // actor_time / legacy_time
    pub last_updated: SystemTime,
}

/// ALYS-006-17: ChainAdapter - Migration adapter for Chain consensus operations
/// 
/// Bridges between legacy Arc<RwLock<Chain>> and ChainActor for consensus operations,
/// block processing, and state management with feature flag integration.
pub struct ChainAdapter {
    /// Feature flag name for chain migration
    feature_flag: String,
}

impl ChainAdapter {
    pub fn new() -> Self {
        Self {
            feature_flag: "migration.chain_actor".to_string(),
        }
    }
}

#[async_trait]
impl LegacyAdapter<Chain, ChainActor> for ChainAdapter {
    type Request = ChainAdapterRequest;
    type Response = ChainAdapterResponse;
    type Error = AdapterError;

    async fn execute_legacy(
        &self,
        legacy: &Arc<RwLock<Chain>>,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error> {
        let chain = legacy.read().await;
        
        match request {
            ChainAdapterRequest::GetHead => {
                let head = chain.get_head()
                    .map(|block_ref| ChainAdapterResponse::Head(Some(block_ref)))
                    .unwrap_or(ChainAdapterResponse::Head(None));
                Ok(head)
            }
            ChainAdapterRequest::ProcessBlock { block } => {
                drop(chain);
                let mut chain = legacy.write().await;
                chain.process_block(block).await
                    .map(|_| ChainAdapterResponse::BlockProcessed)
                    .map_err(|e| AdapterError::LegacyError(format!("Chain process block failed: {}", e)))
            }
            ChainAdapterRequest::ProduceBlock => {
                drop(chain);
                let mut chain = legacy.write().await;
                chain.produce_block().await
                    .map(|block| ChainAdapterResponse::BlockProduced(block))
                    .map_err(|e| AdapterError::LegacyError(format!("Chain produce block failed: {}", e)))
            }
            ChainAdapterRequest::UpdateHead { new_head } => {
                drop(chain);
                let mut chain = legacy.write().await;
                chain.update_head(new_head);
                Ok(ChainAdapterResponse::HeadUpdated)
            }
        }
    }

    async fn execute_actor(
        &self,
        actor: &Addr<ChainActor>,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error> {
        match request {
            ChainAdapterRequest::GetHead => {
                let head = actor.send(GetHeadMessage).await
                    .map_err(|e| AdapterError::ActorError(format!("Chain actor mailbox error: {}", e)))?;
                Ok(ChainAdapterResponse::Head(head))
            }
            ChainAdapterRequest::ProcessBlock { block } => {
                actor.send(ProcessBlockMessage { block }).await
                    .map_err(|e| AdapterError::ActorError(format!("Chain actor mailbox error: {}", e)))?
                    .map(|_| ChainAdapterResponse::BlockProcessed)
                    .map_err(|e| AdapterError::ActorError(format!("Chain actor process block failed: {:?}", e)))
            }
            ChainAdapterRequest::ProduceBlock => {
                let block = actor.send(ProduceBlockMessage).await
                    .map_err(|e| AdapterError::ActorError(format!("Chain actor mailbox error: {}", e)))?
                    .map_err(|e| AdapterError::ActorError(format!("Chain actor produce block failed: {:?}", e)))?;
                Ok(ChainAdapterResponse::BlockProduced(block))
            }
            ChainAdapterRequest::UpdateHead { new_head } => {
                actor.send(UpdateHeadMessage { new_head }).await
                    .map_err(|e| AdapterError::ActorError(format!("Chain actor mailbox error: {}", e)))?;
                Ok(ChainAdapterResponse::HeadUpdated)
            }
        }
    }

    fn feature_flag_name(&self) -> &str {
        &self.feature_flag
    }

    fn compare_responses(
        &self,
        legacy_response: &Self::Response,
        actor_response: &Self::Response,
    ) -> bool {
        match (legacy_response, actor_response) {
            (ChainAdapterResponse::Head(legacy_head), ChainAdapterResponse::Head(actor_head)) => {
                legacy_head == actor_head
            }
            (ChainAdapterResponse::BlockProcessed, ChainAdapterResponse::BlockProcessed) => true,
            (ChainAdapterResponse::HeadUpdated, ChainAdapterResponse::HeadUpdated) => true,
            (ChainAdapterResponse::BlockProduced(legacy_block), ChainAdapterResponse::BlockProduced(actor_block)) => {
                legacy_block.hash() == actor_block.hash()
            }
            _ => false,
        }
    }

    fn performance_metric_name(&self) -> &str {
        "chain_adapter_performance"
    }
}

/// Chain adapter request types
#[derive(Debug, Clone)]
pub enum ChainAdapterRequest {
    GetHead,
    ProcessBlock { block: ConsensusBlock },
    ProduceBlock,
    UpdateHead { new_head: BlockRef },
}

/// Chain adapter response types
#[derive(Debug, Clone, PartialEq)]
pub enum ChainAdapterResponse {
    Head(Option<BlockRef>),
    BlockProcessed,
    BlockProduced(ConsensusBlock),
    HeadUpdated,
}

/// ALYS-006-18: EngineAdapter - Migration adapter for EVM execution operations
/// 
/// Bridges between legacy Arc<RwLock<Engine>> and EngineActor for payload building,
/// block execution, and EVM integration with backward compatibility.
pub struct EngineAdapter {
    /// Feature flag name for engine migration
    feature_flag: String,
}

impl EngineAdapter {
    pub fn new() -> Self {
        Self {
            feature_flag: "migration.engine_actor".to_string(),
        }
    }
}

#[async_trait]
impl LegacyAdapter<Engine, EngineActor> for EngineAdapter {
    type Request = EngineAdapterRequest;
    type Response = EngineAdapterResponse;
    type Error = AdapterError;

    async fn execute_legacy(
        &self,
        legacy: &Arc<RwLock<Engine>>,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error> {
        let engine = legacy.read().await;
        
        match request {
            EngineAdapterRequest::BuildPayload { parent_hash, timestamp, fee_recipient } => {
                let payload = engine.build_block(
                    timestamp,
                    Some(parent_hash),
                    vec![] // No balance additions in this example
                ).await
                    .map_err(|e| AdapterError::LegacyError(format!("Engine build block failed: {}", e)))?;
                
                Ok(EngineAdapterResponse::PayloadBuilt { 
                    payload_id: format!("legacy_payload_{}", timestamp.as_secs()) 
                })
            }
            EngineAdapterRequest::GetPayload { payload_id } => {
                // Legacy engine doesn't have explicit payload retrieval
                // This would need to be implemented based on the actual legacy interface
                Err(AdapterError::LegacyError("Legacy payload retrieval not implemented".to_string()))
            }
            EngineAdapterRequest::ExecutePayload { payload } => {
                let block_hash = engine.commit_block(payload).await
                    .map_err(|e| AdapterError::LegacyError(format!("Engine commit block failed: {}", e)))?;
                
                Ok(EngineAdapterResponse::PayloadExecuted { 
                    block_hash,
                    status: ExecutionStatus::Valid 
                })
            }
            EngineAdapterRequest::SetFinalized { block_hash } => {
                engine.set_finalized(block_hash).await;
                Ok(EngineAdapterResponse::FinalizedSet)
            }
        }
    }

    async fn execute_actor(
        &self,
        actor: &Addr<EngineActor>,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error> {
        match request {
            EngineAdapterRequest::BuildPayload { parent_hash, timestamp, fee_recipient } => {
                let payload_id = actor.send(BuildPayloadMessage {
                    parent_hash,
                    timestamp: timestamp.as_secs(),
                    fee_recipient,
                }).await
                    .map_err(|e| AdapterError::ActorError(format!("Engine actor mailbox error: {}", e)))?
                    .map_err(|e| AdapterError::ActorError(format!("Engine actor build payload failed: {:?}", e)))?;
                
                Ok(EngineAdapterResponse::PayloadBuilt { payload_id })
            }
            EngineAdapterRequest::GetPayload { payload_id } => {
                let _payload = actor.send(GetPayloadMessage { payload_id }).await
                    .map_err(|e| AdapterError::ActorError(format!("Engine actor mailbox error: {}", e)))?
                    .map_err(|e| AdapterError::ActorError(format!("Engine actor get payload failed: {:?}", e)))?;
                
                Ok(EngineAdapterResponse::PayloadRetrieved)
            }
            EngineAdapterRequest::ExecutePayload { payload } => {
                let result = actor.send(ExecutePayloadMessage { payload }).await
                    .map_err(|e| AdapterError::ActorError(format!("Engine actor mailbox error: {}", e)))?
                    .map_err(|e| AdapterError::ActorError(format!("Engine actor execute payload failed: {:?}", e)))?;
                
                Ok(EngineAdapterResponse::PayloadExecuted {
                    block_hash: result.latest_valid_hash.unwrap_or_default(),
                    status: result.status,
                })
            }
            EngineAdapterRequest::SetFinalized { block_hash: _ } => {
                // Engine actor doesn't have explicit finalization message in current design
                // This would need to be added to the actor interface
                Ok(EngineAdapterResponse::FinalizedSet)
            }
        }
    }

    fn feature_flag_name(&self) -> &str {
        &self.feature_flag
    }

    fn compare_responses(
        &self,
        legacy_response: &Self::Response,
        actor_response: &Self::Response,
    ) -> bool {
        match (legacy_response, actor_response) {
            (EngineAdapterResponse::PayloadBuilt { .. }, EngineAdapterResponse::PayloadBuilt { .. }) => true,
            (EngineAdapterResponse::PayloadRetrieved, EngineAdapterResponse::PayloadRetrieved) => true,
            (
                EngineAdapterResponse::PayloadExecuted { status: legacy_status, .. },
                EngineAdapterResponse::PayloadExecuted { status: actor_status, .. }
            ) => {
                std::mem::discriminant(legacy_status) == std::mem::discriminant(actor_status)
            }
            (EngineAdapterResponse::FinalizedSet, EngineAdapterResponse::FinalizedSet) => true,
            _ => false,
        }
    }

    fn performance_metric_name(&self) -> &str {
        "engine_adapter_performance"
    }
}

/// Engine adapter request types
#[derive(Debug, Clone)]
pub enum EngineAdapterRequest {
    BuildPayload {
        parent_hash: ExecutionBlockHash,
        timestamp: Duration,
        fee_recipient: Address,
    },
    GetPayload {
        payload_id: String,
    },
    ExecutePayload {
        payload: ExecutionPayload<MainnetEthSpec>,
    },
    SetFinalized {
        block_hash: ExecutionBlockHash,
    },
}

/// Engine adapter response types
#[derive(Debug, Clone)]
pub enum EngineAdapterResponse {
    PayloadBuilt { payload_id: String },
    PayloadRetrieved,
    PayloadExecuted { 
        block_hash: ExecutionBlockHash,
        status: ExecutionStatus,
    },
    FinalizedSet,
}

/// Adapter manager for coordinating multiple adapters and migration phases
pub struct AdapterManager {
    /// Chain adapter for consensus operations
    pub chain_adapter: GenericAdapter<Chain, ChainActor>,
    /// Engine adapter for EVM execution
    pub engine_adapter: GenericAdapter<Engine, EngineActor>,
    /// Global migration configuration
    config: AdapterConfig,
    /// Migration state tracking across all adapters
    global_migration_state: Arc<RwLock<GlobalMigrationState>>,
}

/// Global migration state across all system components
#[derive(Debug, Clone)]
pub struct GlobalMigrationState {
    /// Overall migration phase
    pub phase: MigrationPhase,
    /// Per-component migration states
    pub component_states: HashMap<String, MigrationState>,
    /// Migration start time
    pub started_at: SystemTime,
    /// Last state change time
    pub last_updated: SystemTime,
    /// Migration metrics summary
    pub metrics: GlobalMigrationMetrics,
}

/// Migration phase tracking
#[derive(Debug, Clone, PartialEq)]
pub enum MigrationPhase {
    /// Planning phase - feature flags disabled
    Planning,
    /// Gradual rollout - dual path with legacy preference
    GradualRollout,
    /// Performance validation - dual path with actor preference
    PerformanceValidation,
    /// Final cutover - actor only
    FinalCutover,
    /// Rollback - reverted to legacy
    Rollback { reason: String },
    /// Complete - migration finished
    Complete,
}

/// Global migration metrics
#[derive(Debug, Clone, Default)]
pub struct GlobalMigrationMetrics {
    pub total_operations: u64,
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub inconsistency_count: u64,
    pub average_performance_ratio: f64,
    pub migration_progress_percentage: f64,
}

impl AdapterManager {
    /// Create a new adapter manager with legacy systems
    pub fn new(
        chain: Arc<RwLock<Chain>>,
        engine: Arc<RwLock<Engine>>,
        config: AdapterConfig,
    ) -> Self {
        let chain_adapter = GenericAdapter::new(
            "chain_adapter".to_string(),
            chain,
            config.clone(),
        );

        let engine_adapter = GenericAdapter::new(
            "engine_adapter".to_string(),
            engine,
            config.clone(),
        );

        let global_state = GlobalMigrationState {
            phase: MigrationPhase::Planning,
            component_states: HashMap::new(),
            started_at: SystemTime::now(),
            last_updated: SystemTime::now(),
            metrics: GlobalMigrationMetrics::default(),
        };

        Self {
            chain_adapter,
            engine_adapter,
            config,
            global_migration_state: Arc::new(RwLock::new(global_state)),
        }
    }

    /// Set actors for dual-path execution
    pub async fn set_actors(
        &mut self,
        chain_actor: Addr<ChainActor>,
        engine_actor: Addr<EngineActor>,
    ) -> Result<(), AdapterError> {
        info!("Setting actors for dual-path migration");
        
        self.chain_adapter.set_actor(chain_actor).await?;
        self.engine_adapter.set_actor(engine_actor).await?;

        // Update global migration state
        let mut global_state = self.global_migration_state.write().await;
        global_state.phase = MigrationPhase::GradualRollout;
        global_state.last_updated = SystemTime::now();
        
        Ok(())
    }

    /// Advance migration phase
    pub async fn advance_migration_phase(&self) -> Result<MigrationPhase, AdapterError> {
        let mut global_state = self.global_migration_state.write().await;
        
        let new_phase = match global_state.phase {
            MigrationPhase::Planning => MigrationPhase::GradualRollout,
            MigrationPhase::GradualRollout => {
                // Check performance before advancing
                if self.should_advance_to_performance_validation().await {
                    MigrationPhase::PerformanceValidation
                } else {
                    return Ok(global_state.phase.clone());
                }
            }
            MigrationPhase::PerformanceValidation => {
                if self.should_advance_to_final_cutover().await {
                    MigrationPhase::FinalCutover
                } else {
                    return Ok(global_state.phase.clone());
                }
            }
            MigrationPhase::FinalCutover => {
                if self.should_complete_migration().await {
                    MigrationPhase::Complete
                } else {
                    return Ok(global_state.phase.clone());
                }
            }
            _ => return Ok(global_state.phase.clone()),
        };

        info!("Advancing migration phase: {:?} -> {:?}", global_state.phase, new_phase);
        global_state.phase = new_phase.clone();
        global_state.last_updated = SystemTime::now();

        Ok(new_phase)
    }

    /// Check if ready to advance to performance validation phase
    async fn should_advance_to_performance_validation(&self) -> bool {
        // Check that both adapters are successfully running dual-path
        let chain_state = self.chain_adapter.get_migration_state().await;
        let engine_state = self.engine_adapter.get_migration_state().await;

        matches!(chain_state, MigrationState::DualPathLegacyPreferred) &&
        matches!(engine_state, MigrationState::DualPathLegacyPreferred)
    }

    /// Check if ready to advance to final cutover
    async fn should_advance_to_final_cutover(&self) -> bool {
        // Check performance metrics and success rates
        let chain_metrics = self.chain_adapter.get_performance_summary().await;
        let engine_metrics = self.engine_adapter.get_performance_summary().await;

        chain_metrics.success_rate > 0.99 && 
        engine_metrics.success_rate > 0.99 &&
        chain_metrics.performance_ratio.map_or(true, |ratio| ratio <= self.config.performance_threshold) &&
        engine_metrics.performance_ratio.map_or(true, |ratio| ratio <= self.config.performance_threshold)
    }

    /// Check if migration should be completed
    async fn should_complete_migration(&self) -> bool {
        // Check that both adapters are running actor-only successfully
        let chain_state = self.chain_adapter.get_migration_state().await;
        let engine_state = self.engine_adapter.get_migration_state().await;

        matches!(chain_state, MigrationState::ActorOnly) &&
        matches!(engine_state, MigrationState::ActorOnly)
    }

    /// Get comprehensive migration status
    pub async fn get_migration_status(&self) -> GlobalMigrationState {
        self.global_migration_state.read().await.clone()
    }

    /// Force rollback migration
    pub async fn rollback_migration(&self, reason: String) -> Result<(), AdapterError> {
        warn!("Rolling back migration: {}", reason);
        
        // Set all adapters to rolled back state
        self.chain_adapter.set_migration_state(MigrationState::RolledBack { 
            reason: reason.clone() 
        }).await?;
        
        self.engine_adapter.set_migration_state(MigrationState::RolledBack { 
            reason: reason.clone() 
        }).await?;

        // Update global state
        let mut global_state = self.global_migration_state.write().await;
        global_state.phase = MigrationPhase::Rollback { reason };
        global_state.last_updated = SystemTime::now();

        Ok(())
    }
}