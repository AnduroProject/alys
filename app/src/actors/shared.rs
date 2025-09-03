//! Shared structures for V2 Actor System
//!
//! Contains common types and utilities used across multiple actors,
//! including actor addresses, communication patterns, and shared state.

use actix::prelude::*;
use std::sync::Arc;

use crate::actors::{
    chain::actor::ChainActor,
    engine::actor::EngineActor,
    storage::actor::StorageActor,
    auxpow::{AuxPowActor, DifficultyManager},
    supervisor::RootSupervisor,
};

/// Actor addresses for cross-actor communication
/// 
/// This struct provides a centralized way to access all actors in the system,
/// enabling message passing between different components while maintaining
/// loose coupling.
#[derive(Clone)]
pub struct ActorAddresses {
    /// Reference to the chain consensus actor
    pub chain: Option<Addr<ChainActor>>,
    
    /// Reference to the execution engine actor
    pub engine: Addr<EngineActor>,
    
    /// Reference to the bridge actor for peg operations
    pub bridge: Addr<BridgeActor>,
    
    /// Reference to the storage actor
    pub storage: Addr<StorageActor>,
    
    /// Reference to the network actor
    pub network: Addr<NetworkActor>,
    
    /// Reference to the sync actor (optional)
    pub sync: Option<Addr<SyncActor>>,
    
    /// Reference to the AuxPow mining actor (optional)
    pub auxpow: Option<Addr<AuxPowActor>>,
    
    /// Reference to the difficulty manager actor (optional)
    pub difficulty_manager: Option<Addr<DifficultyManager>>,
    
    /// Reference to the root supervisor
    pub supervisor: Addr<RootSupervisor>,
}

impl ActorAddresses {
    /// Create new actor addresses (used during system initialization)
    pub fn new(
        engine: Addr<EngineActor>,
        bridge: Addr<BridgeActor>,
        storage: Addr<StorageActor>,
        network: Addr<NetworkActor>,
        supervisor: Addr<RootSupervisor>,
    ) -> Self {
        Self {
            chain: None,
            engine,
            bridge,
            storage,
            network,
            sync: None,
            auxpow: None,
            difficulty_manager: None,
            supervisor,
        }
    }

    /// Set the chain actor address (called after chain actor is created)
    pub fn set_chain_actor(&mut self, chain: Addr<ChainActor>) {
        self.chain = Some(chain);
    }

    /// Set the sync actor address (optional)
    pub fn set_sync_actor(&mut self, sync: Addr<SyncActor>) {
        self.sync = Some(sync);
    }

    /// Set the AuxPow actor address (optional, for mining)
    pub fn set_auxpow_actor(&mut self, auxpow: Addr<AuxPowActor>) {
        self.auxpow = Some(auxpow);
    }

    /// Set the difficulty manager actor address (optional, for mining)
    pub fn set_difficulty_manager(&mut self, difficulty_manager: Addr<DifficultyManager>) {
        self.difficulty_manager = Some(difficulty_manager);
    }
}

/// Actor system configuration
#[derive(Debug, Clone)]
pub struct ActorSystemConfig {
    /// Whether to enable actor supervision
    pub enable_supervision: bool,
    
    /// Maximum message queue size per actor
    pub max_queue_size: usize,
    
    /// Actor startup timeout
    pub startup_timeout_ms: u64,
    
    /// Health check interval
    pub health_check_interval_ms: u64,
    
    /// Test mode flag
    pub test_mode: bool,
}

impl Default for ActorSystemConfig {
    fn default() -> Self {
        Self {
            enable_supervision: true,
            max_queue_size: 1000,
            startup_timeout_ms: 30000,
            health_check_interval_ms: 30000,
            test_mode: false,
        }
    }
}

impl ActorSystemConfig {
    /// Create test configuration
    pub fn test_default() -> Self {
        Self {
            enable_supervision: true,
            max_queue_size: 100,
            startup_timeout_ms: 5000,
            health_check_interval_ms: 10000,
            test_mode: true,
        }
    }
}

/// Actor health status
#[derive(Debug, Clone)]
pub struct ActorHealth {
    /// Actor identifier
    pub actor_id: String,
    
    /// Whether actor is healthy
    pub is_healthy: bool,
    
    /// Last health check timestamp
    pub last_check: std::time::SystemTime,
    
    /// Error message if unhealthy
    pub error_message: Option<String>,
    
    /// Performance metrics
    pub metrics: ActorMetrics,
}

/// Actor performance metrics
#[derive(Debug, Clone, Default)]
pub struct ActorMetrics {
    /// Messages processed per second
    pub messages_per_second: f64,
    
    /// Average message processing time (ms)
    pub avg_processing_time_ms: f64,
    
    /// Current message queue depth
    pub queue_depth: u32,
    
    /// Total messages processed
    pub total_messages: u64,
    
    /// Memory usage (bytes)
    pub memory_usage_bytes: u64,
}

/// Common actor lifecycle events
#[derive(Debug, Clone)]
pub enum ActorLifecycleEvent {
    /// Actor started successfully
    Started { actor_id: String, timestamp: std::time::SystemTime },
    
    /// Actor stopped (normal shutdown)
    Stopped { actor_id: String, timestamp: std::time::SystemTime },
    
    /// Actor failed with error
    Failed { 
        actor_id: String, 
        error: String, 
        timestamp: std::time::SystemTime 
    },
    
    /// Actor restarted after failure
    Restarted { 
        actor_id: String, 
        restart_count: u32, 
        timestamp: std::time::SystemTime 
    },
}

/// Message priority levels for actor communication
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    /// Critical system messages (highest priority)
    Critical = 0,
    
    /// High priority messages (important operations)
    High = 1,
    
    /// Normal priority messages (default)
    Normal = 2,
    
    /// Low priority messages (background tasks)
    Low = 3,
}

impl Default for MessagePriority {
    fn default() -> Self {
        MessagePriority::Normal
    }
}

/// Actor communication pattern types
#[derive(Debug, Clone)]
pub enum CommunicationPattern {
    /// Fire-and-forget message
    FireAndForget,
    
    /// Request-response with timeout
    RequestResponse { timeout_ms: u64 },
    
    /// Broadcast to multiple actors
    Broadcast { target_actors: Vec<String> },
    
    /// Publish-subscribe pattern
    PubSub { topic: String },
}

/// Error types for actor operations
#[derive(Debug, thiserror::Error)]
pub enum ActorError {
    #[error("Actor not found: {actor_id}")]
    ActorNotFound { actor_id: String },
    
    #[error("Actor is not responding (timeout after {timeout_ms}ms)")]
    ActorTimeout { timeout_ms: u64 },
    
    #[error("Actor mailbox is full (max size: {max_size})")]
    MailboxFull { max_size: usize },
    
    #[error("Actor initialization failed: {reason}")]
    InitializationFailed { reason: String },
    
    #[error("Message serialization error: {message}")]
    SerializationError { message: String },
    
    #[error("System shutdown in progress")]
    SystemShuttingDown,
}

/// Result type for actor operations
pub type ActorResult<T> = Result<T, ActorError>;

// Forward declarations for actors not yet implemented
// These would be properly implemented in their respective modules

/// Bridge actor for two-way peg operations
pub struct BridgeActor;

impl Actor for BridgeActor {
    type Context = Context<Self>;
}

/// Network actor for P2P communications
pub struct NetworkActor;

impl Actor for NetworkActor {
    type Context = Context<Self>;
}

/// Sync actor for blockchain synchronization
pub struct SyncActor;

impl Actor for SyncActor {
    type Context = Context<Self>;
}

/// Utility functions for actor management
pub mod utils {
    use super::*;
    
    /// Create a standardized actor ID
    pub fn create_actor_id(actor_type: &str, instance: Option<&str>) -> String {
        match instance {
            Some(inst) => format!("{}_{}", actor_type, inst),
            None => actor_type.to_string(),
        }
    }
    
    /// Validate actor configuration
    pub fn validate_actor_config(config: &ActorSystemConfig) -> Result<(), String> {
        if config.max_queue_size == 0 {
            return Err("max_queue_size must be greater than 0".to_string());
        }
        
        if config.startup_timeout_ms == 0 {
            return Err("startup_timeout_ms must be greater than 0".to_string());
        }
        
        if config.health_check_interval_ms < 1000 {
            return Err("health_check_interval_ms should be at least 1000ms".to_string());
        }
        
        Ok(())
    }
    
    /// Format actor metrics for display
    pub fn format_metrics(metrics: &ActorMetrics) -> String {
        format!(
            "MPS: {:.2}, AvgTime: {:.2}ms, Queue: {}, Total: {}, Memory: {}KB",
            metrics.messages_per_second,
            metrics.avg_processing_time_ms,
            metrics.queue_depth,
            metrics.total_messages,
            metrics.memory_usage_bytes / 1024
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::utils::*;
    
    #[test]
    fn test_actor_id_creation() {
        assert_eq!(create_actor_id("chain", None), "chain");
        assert_eq!(create_actor_id("chain", Some("main")), "chain_main");
    }
    
    #[test]
    fn test_config_validation() {
        let mut config = ActorSystemConfig::default();
        assert!(validate_actor_config(&config).is_ok());
        
        config.max_queue_size = 0;
        assert!(validate_actor_config(&config).is_err());
        
        config.max_queue_size = 100;
        config.startup_timeout_ms = 0;
        assert!(validate_actor_config(&config).is_err());
    }
    
    #[test]
    fn test_message_priority_ordering() {
        assert!(MessagePriority::Critical < MessagePriority::High);
        assert!(MessagePriority::High < MessagePriority::Normal);
        assert!(MessagePriority::Normal < MessagePriority::Low);
    }
    
    #[test]
    fn test_metrics_formatting() {
        let metrics = ActorMetrics {
            messages_per_second: 123.45,
            avg_processing_time_ms: 5.67,
            queue_depth: 10,
            total_messages: 1000,
            memory_usage_bytes: 2048,
        };
        
        let formatted = format_metrics(&metrics);
        assert!(formatted.contains("123.45"));
        assert!(formatted.contains("5.67ms"));
        assert!(formatted.contains("Queue: 10"));
        assert!(formatted.contains("2KB"));
    }
}