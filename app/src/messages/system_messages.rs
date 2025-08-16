//! System-level messages for supervisor and lifecycle management

use crate::types::*;
use actix::prelude::*;

/// Message to register an actor with the supervisor
#[derive(Message)]
#[rtype(result = "Result<(), SystemError>")]
pub struct RegisterActorMessage {
    pub actor_name: String,
    pub actor_type: ActorType,
    pub restart_policy: RestartPolicy,
}

/// Message to unregister an actor from the supervisor
#[derive(Message)]
#[rtype(result = "Result<(), SystemError>")]
pub struct UnregisterActorMessage {
    pub actor_name: String,
}

/// Message to report actor health status
#[derive(Message)]
#[rtype(result = "()")]
pub struct HealthReportMessage {
    pub actor_name: String,
    pub health_status: ActorHealth,
    pub metrics: Option<ActorMetrics>,
}

/// Message to request system status
#[derive(Message)]
#[rtype(result = "SystemStatus")]
pub struct GetSystemStatusMessage;

/// Message to request actor restart
#[derive(Message)]
#[rtype(result = "Result<(), SystemError>")]
pub struct RestartActorMessage {
    pub actor_name: String,
    pub reason: String,
}

/// Message to shutdown the system
#[derive(Message)]
#[rtype(result = "Result<(), SystemError>")]
pub struct ShutdownMessage {
    pub graceful: bool,
    pub timeout: std::time::Duration,
}

/// Message to update system configuration
#[derive(Message)]
#[rtype(result = "Result<(), SystemError>")]
pub struct UpdateConfigMessage {
    pub config_update: ConfigUpdate,
}

/// Type of actor for registration
#[derive(Debug, Clone)]
pub enum ActorType {
    Chain,
    Engine,
    Sync,
    Network,
    Stream,
    Storage,
    Bridge,
}

/// Restart policy for actor failures
#[derive(Debug, Clone)]
pub enum RestartPolicy {
    Never,
    Always,
    OnFailure,
    Exponential { max_attempts: u32 },
}

/// Actor health status
#[derive(Debug, Clone)]
pub enum ActorHealth {
    Healthy,
    Warning { message: String },
    Critical { error: String },
    Failed { error: String },
}

/// Generic actor metrics
#[derive(Debug, Clone)]
pub struct ActorMetrics {
    pub messages_processed: u64,
    pub errors_count: u64,
    pub uptime: std::time::Duration,
    pub last_activity: std::time::SystemTime,
}

/// System-wide status information
#[derive(Debug, Clone)]
pub struct SystemStatus {
    pub version: String,
    pub uptime: std::time::Duration,
    pub active_actors: Vec<ActorInfo>,
    pub system_health: SystemHealth,
    pub resource_usage: ResourceUsage,
}

/// Information about an active actor
#[derive(Debug, Clone)]
pub struct ActorInfo {
    pub name: String,
    pub actor_type: ActorType,
    pub health: ActorHealth,
    pub uptime: std::time::Duration,
}

/// Overall system health
#[derive(Debug, Clone)]
pub enum SystemHealth {
    Healthy,
    Degraded { issues: Vec<String> },
    Critical { critical_issues: Vec<String> },
}

/// System resource usage
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    pub memory_mb: u64,
    pub cpu_percent: f64,
    pub disk_usage_mb: u64,
    pub network_connections: u32,
}

/// Configuration update types
#[derive(Debug, Clone)]
pub enum ConfigUpdate {
    LogLevel { level: String },
    NetworkConfig { config: NetworkConfigUpdate },
    StorageConfig { config: StorageConfigUpdate },
    ChainConfig { config: ChainConfigUpdate },
}

/// Network configuration updates
#[derive(Debug, Clone)]
pub struct NetworkConfigUpdate {
    pub max_peers: Option<usize>,
    pub listen_address: Option<String>,
    pub bootstrap_peers: Option<Vec<String>>,
}

/// Storage configuration updates
#[derive(Debug, Clone)]
pub struct StorageConfigUpdate {
    pub cache_size_mb: Option<usize>,
    pub sync_interval: Option<std::time::Duration>,
}

/// Chain configuration updates
#[derive(Debug, Clone)]
pub struct ChainConfigUpdate {
    pub slot_duration: Option<std::time::Duration>,
    pub max_blocks_without_pow: Option<u64>,
}