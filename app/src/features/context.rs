//! Evaluation context for feature flag evaluation
//!
//! This module provides the EvaluationContext that contains all necessary information
//! for feature flag evaluation including node identity, environment, chain state, and custom attributes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::OnceLock;
use chrono::{DateTime, Utc};
use crate::config::Environment;
use super::FeatureFlagResult;

/// Context information used for feature flag evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationContext {
    /// Unique identifier for this node
    pub node_id: String,
    
    /// Current environment (development, testnet, mainnet, etc.)
    pub environment: Environment,
    
    /// Current blockchain height
    pub chain_height: u64,
    
    /// Current sync progress (0.0-1.0)
    pub sync_progress: f64,
    
    /// Optional validator public key
    pub validator_key: Option<String>,
    
    /// Node's IP address
    pub ip_address: Option<IpAddr>,
    
    /// Current timestamp for evaluation
    pub evaluation_time: DateTime<Utc>,
    
    /// Node health metrics
    pub node_health: NodeHealth,
    
    /// Custom attributes for advanced targeting
    pub custom_attributes: HashMap<String, String>,
    
    /// User session information (if applicable)
    pub session_info: Option<SessionInfo>,
}

impl EvaluationContext {
    /// Create a new evaluation context
    pub fn new(node_id: String, environment: Environment) -> Self {
        Self {
            node_id,
            environment,
            chain_height: 0,
            sync_progress: 0.0,
            validator_key: None,
            ip_address: None,
            evaluation_time: Utc::now(),
            node_health: NodeHealth::default(),
            custom_attributes: HashMap::new(),
            session_info: None,
        }
    }
    
    /// Update chain state information
    pub fn with_chain_state(mut self, height: u64, sync_progress: f64) -> Self {
        self.chain_height = height;
        self.sync_progress = sync_progress.clamp(0.0, 1.0);
        self
    }
    
    /// Set validator key
    pub fn with_validator_key(mut self, key: String) -> Self {
        self.validator_key = Some(key);
        self
    }
    
    /// Set IP address
    pub fn with_ip_address(mut self, ip: IpAddr) -> Self {
        self.ip_address = Some(ip);
        self
    }
    
    /// Update node health metrics
    pub fn with_node_health(mut self, health: NodeHealth) -> Self {
        self.node_health = health;
        self
    }
    
    /// Add custom attribute
    pub fn with_custom_attribute(mut self, key: String, value: String) -> Self {
        self.custom_attributes.insert(key, value);
        self
    }
    
    /// Add multiple custom attributes
    pub fn with_custom_attributes(mut self, attributes: HashMap<String, String>) -> Self {
        self.custom_attributes.extend(attributes);
        self
    }
    
    /// Set session information
    pub fn with_session_info(mut self, session: SessionInfo) -> Self {
        self.session_info = Some(session);
        self
    }
    
    /// Update evaluation timestamp
    pub fn touch(&mut self) {
        self.evaluation_time = Utc::now();
    }
    
    /// Create a hash of the context for consistent rollout evaluation
    pub fn hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        self.node_id.hash(&mut hasher);
        // Include environment and validator key for more consistent distribution
        self.environment.hash(&mut hasher);
        if let Some(ref key) = self.validator_key {
            key.hash(&mut hasher);
        }
        hasher.finish()
    }
    
    /// Get a stable identifier for this context (used for percentage rollouts)
    pub fn stable_id(&self) -> String {
        match &self.validator_key {
            Some(key) => format!("{}:{}", self.node_id, key),
            None => self.node_id.clone(),
        }
    }
}

impl Default for EvaluationContext {
    fn default() -> Self {
        Self::new("unknown".to_string(), Environment::Development)
    }
}

/// Node health metrics for condition evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealth {
    /// Number of connected peers
    pub peer_count: u32,
    
    /// Memory usage in MB
    pub memory_usage_mb: u64,
    
    /// CPU usage percentage (0-100)
    pub cpu_usage_percent: u8,
    
    /// Disk usage percentage (0-100)
    pub disk_usage_percent: u8,
    
    /// Network latency in milliseconds
    pub network_latency_ms: u64,
    
    /// Is the node synced?
    pub is_synced: bool,
    
    /// Last block timestamp
    pub last_block_time: Option<DateTime<Utc>>,
    
    /// Additional health metrics
    pub metrics: HashMap<String, f64>,
}

impl Default for NodeHealth {
    fn default() -> Self {
        Self {
            peer_count: 0,
            memory_usage_mb: 0,
            cpu_usage_percent: 0,
            disk_usage_percent: 0,
            network_latency_ms: 0,
            is_synced: false,
            last_block_time: None,
            metrics: HashMap::new(),
        }
    }
}

impl NodeHealth {
    /// Create new node health with basic metrics
    pub fn new(peer_count: u32, memory_mb: u64, cpu_percent: u8) -> Self {
        Self {
            peer_count,
            memory_usage_mb: memory_mb,
            cpu_usage_percent: cpu_percent,
            ..Default::default()
        }
    }
    
    /// Update sync status
    pub fn with_sync_status(mut self, is_synced: bool, last_block_time: Option<DateTime<Utc>>) -> Self {
        self.is_synced = is_synced;
        self.last_block_time = last_block_time;
        self
    }
    
    /// Add custom health metric
    pub fn with_metric(mut self, name: String, value: f64) -> Self {
        self.metrics.insert(name, value);
        self
    }
}

/// User session information (if applicable for node operations)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Session ID
    pub session_id: String,
    
    /// User/operator ID
    pub user_id: Option<String>,
    
    /// Session start time
    pub started_at: DateTime<Utc>,
    
    /// Session metadata
    pub metadata: HashMap<String, String>,
}

impl SessionInfo {
    /// Create new session info
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            user_id: None,
            started_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }
    
    /// Set user ID
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }
    
    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Global evaluation context provider
static CONTEXT_PROVIDER: OnceLock<Box<dyn EvaluationContextProvider + Send + Sync>> = OnceLock::new();

/// Trait for providing evaluation context
pub trait EvaluationContextProvider {
    /// Get the current evaluation context
    async fn get_context(&self) -> FeatureFlagResult<EvaluationContext>;
    
    /// Update context with current node state
    async fn refresh_context(&self) -> FeatureFlagResult<()>;
}

/// Default evaluation context provider implementation
pub struct DefaultEvaluationContextProvider {
    base_context: std::sync::RwLock<EvaluationContext>,
}

impl DefaultEvaluationContextProvider {
    /// Create new provider with base context
    pub fn new(node_id: String, environment: Environment) -> Self {
        let context = EvaluationContext::new(node_id, environment);
        Self {
            base_context: std::sync::RwLock::new(context),
        }
    }
    
    /// Update the base context
    pub fn update_context<F>(&self, updater: F) -> FeatureFlagResult<()>
    where
        F: FnOnce(&mut EvaluationContext),
    {
        let mut context = self.base_context.write()
            .map_err(|_| super::FeatureFlagError::EvaluationError {
                reason: "Failed to acquire write lock on context".to_string()
            })?;
        updater(&mut *context);
        context.touch();
        Ok(())
    }
}

impl EvaluationContextProvider for DefaultEvaluationContextProvider {
    async fn get_context(&self) -> FeatureFlagResult<EvaluationContext> {
        let context = self.base_context.read()
            .map_err(|_| super::FeatureFlagError::EvaluationError {
                reason: "Failed to acquire read lock on context".to_string()
            })?;
        let mut ctx = context.clone();
        ctx.touch(); // Update evaluation time
        Ok(ctx)
    }
    
    async fn refresh_context(&self) -> FeatureFlagResult<()> {
        // In a real implementation, this would fetch current node state
        // For now, we just update the timestamp
        self.update_context(|ctx| {
            ctx.touch();
            // Here we could fetch:
            // - Current chain height from the chain actor
            // - Sync progress from the sync actor  
            // - Node health metrics from monitoring
            // - Peer count from network layer
        })
    }
}

/// Initialize the global evaluation context provider
pub fn init_evaluation_context(provider: Box<dyn EvaluationContextProvider + Send + Sync>) -> Result<(), String> {
    CONTEXT_PROVIDER.set(provider)
        .map_err(|_| "Evaluation context provider already initialized".to_string())
}

/// Get the current evaluation context from the global provider
pub async fn get_evaluation_context() -> FeatureFlagResult<EvaluationContext> {
    match CONTEXT_PROVIDER.get() {
        Some(provider) => provider.get_context().await,
        None => {
            // Fallback to a basic context if no provider is set
            Ok(EvaluationContext::default())
        }
    }
}

/// Refresh the global evaluation context
pub async fn refresh_evaluation_context() -> FeatureFlagResult<()> {
    match CONTEXT_PROVIDER.get() {
        Some(provider) => provider.refresh_context().await,
        None => Ok(()), // No-op if no provider is set
    }
}

/// Create a context for a specific environment and node
pub fn create_context_for_node(
    node_id: String, 
    environment: Environment, 
    chain_height: u64, 
    sync_progress: f64
) -> EvaluationContext {
    EvaluationContext::new(node_id, environment)
        .with_chain_state(chain_height, sync_progress)
}

/// Create a test context for unit testing
pub fn create_test_context() -> EvaluationContext {
    EvaluationContext::new("test-node".to_string(), Environment::Testing)
        .with_chain_state(1000, 1.0)
        .with_custom_attribute("test".to_string(), "true".to_string())
}