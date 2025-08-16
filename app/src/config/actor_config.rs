//! Actor system configuration with comprehensive restart strategies, mailbox capacity, and timeout settings

use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Actor system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorSystemConfig {
    /// Runtime configuration
    pub runtime: RuntimeConfig,
    
    /// Supervision configuration
    pub supervision: SupervisionConfig,
    
    /// Mailbox configuration
    pub mailbox: MailboxConfig,
    
    /// Individual actor configurations
    pub actors: ActorConfigurations,
    
    /// System-wide timeouts
    pub timeouts: SystemTimeouts,
    
    /// Performance tuning
    pub performance: PerformanceConfig,
}

/// Runtime configuration for the actor system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Number of worker threads
    pub worker_threads: Option<usize>,
    
    /// Enable I/O driver
    pub enable_io: bool,
    
    /// Enable time driver
    pub enable_time: bool,
    
    /// Thread name prefix
    pub thread_name_prefix: String,
    
    /// Thread stack size in bytes
    pub thread_stack_size: Option<usize>,
    
    /// Keep alive time for idle threads
    pub thread_keep_alive: Duration,
}

/// Supervision configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisionConfig {
    /// Default restart strategy
    pub default_restart_strategy: RestartStrategyConfig,
    
    /// Maximum number of restarts per time window
    pub max_restarts: u32,
    
    /// Time window for restart counting
    pub restart_window: Duration,
    
    /// Escalation timeout
    pub escalation_timeout: Duration,
    
    /// Health check interval
    pub health_check_interval: Duration,
    
    /// Enable automatic recovery
    pub auto_recovery: bool,
    
    /// Recovery strategies per actor type
    pub recovery_strategies: HashMap<String, RestartStrategyConfig>,
}

/// Restart strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RestartStrategyConfig {
    /// Restart immediately
    OneForOne {
        max_retries: u32,
        within_time: Duration,
    },
    /// Restart all siblings
    OneForAll {
        max_retries: u32,
        within_time: Duration,
    },
    /// Restart affected siblings
    RestForOne {
        max_retries: u32,
        within_time: Duration,
    },
    /// Exponential backoff
    ExponentialBackoff {
        initial_delay: Duration,
        max_delay: Duration,
        multiplier: f64,
        max_retries: u32,
    },
    /// Circuit breaker
    CircuitBreaker {
        failure_threshold: u32,
        recovery_timeout: Duration,
        success_threshold: u32,
    },
    /// Never restart
    Never,
}

/// Mailbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxConfig {
    /// Default mailbox capacity
    pub default_capacity: usize,
    
    /// Backpressure strategy
    pub backpressure_strategy: BackpressureStrategy,
    
    /// Message timeout
    pub message_timeout: Option<Duration>,
    
    /// Priority queue configuration
    pub priority_queue: Option<PriorityQueueConfig>,
    
    /// Dead letter handling
    pub dead_letter: DeadLetterConfig,
}

/// Backpressure strategies
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackpressureStrategy {
    /// Drop oldest messages when full
    DropOldest,
    /// Drop newest messages when full
    DropNewest,
    /// Block sender until space available
    Block,
    /// Return error to sender
    Fail,
}

/// Priority queue configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityQueueConfig {
    /// Number of priority levels
    pub levels: u8,
    
    /// Default priority
    pub default_priority: u8,
    
    /// Priority scheduling algorithm
    pub algorithm: PriorityAlgorithm,
}

/// Priority scheduling algorithms
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityAlgorithm {
    /// Strict priority (higher priority always first)
    Strict,
    /// Weighted fair queuing
    WeightedFair,
    /// Round robin with priority
    RoundRobin,
}

/// Dead letter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterConfig {
    /// Enable dead letter queue
    pub enabled: bool,
    
    /// Maximum dead letters to keep
    pub max_messages: usize,
    
    /// Dead letter retention time
    pub retention_time: Duration,
    
    /// Dead letter handler
    pub handler: DeadLetterHandler,
}

/// Dead letter handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeadLetterHandler {
    /// Log dead letters
    Log { level: LogLevel },
    /// Write to file
    File { path: String },
    /// Send to external system
    External { endpoint: String },
    /// Ignore dead letters
    Ignore,
}

/// Individual actor configurations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorConfigurations {
    /// Chain actor configuration
    pub chain_actor: ActorConfig,
    
    /// Engine actor configuration
    pub engine_actor: ActorConfig,
    
    /// Bridge actor configuration
    pub bridge_actor: ActorConfig,
    
    /// Network actor configuration
    pub network_actor: ActorConfig,
    
    /// Sync actor configuration
    pub sync_actor: ActorConfig,
    
    /// Stream actor configuration
    pub stream_actor: ActorConfig,
    
    /// Storage actor configuration
    pub storage_actor: ActorConfig,
    
    /// Supervisor actor configuration
    pub supervisor_actor: ActorConfig,
}

/// Individual actor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorConfig {
    /// Enable this actor
    pub enabled: bool,
    
    /// Mailbox capacity
    pub mailbox_capacity: Option<usize>,
    
    /// Restart strategy
    pub restart_strategy: Option<RestartStrategyConfig>,
    
    /// Health check configuration
    pub health_check: ActorHealthConfig,
    
    /// Performance configuration
    pub performance: ActorPerformanceConfig,
    
    /// Custom configuration
    pub custom: HashMap<String, serde_json::Value>,
}

/// Actor health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorHealthConfig {
    /// Enable health checks
    pub enabled: bool,
    
    /// Health check interval
    pub interval: Duration,
    
    /// Health check timeout
    pub timeout: Duration,
    
    /// Failure threshold
    pub failure_threshold: u32,
    
    /// Recovery threshold
    pub recovery_threshold: u32,
}

/// Actor performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorPerformanceConfig {
    /// Message processing timeout
    pub message_timeout: Option<Duration>,
    
    /// Maximum memory usage in MB
    pub max_memory_mb: Option<u64>,
    
    /// CPU limit as percentage (0-100)
    pub cpu_limit_percent: Option<f64>,
    
    /// Enable performance monitoring
    pub monitoring: bool,
    
    /// Performance metrics collection interval
    pub metrics_interval: Duration,
}

/// System-wide timeouts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemTimeouts {
    /// Actor startup timeout
    pub startup_timeout: Duration,
    
    /// Actor shutdown timeout
    pub shutdown_timeout: Duration,
    
    /// System initialization timeout
    pub initialization_timeout: Duration,
    
    /// Health check timeout
    pub health_check_timeout: Duration,
    
    /// Configuration reload timeout
    pub config_reload_timeout: Duration,
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable performance monitoring
    pub monitoring: bool,
    
    /// Metrics collection interval
    pub metrics_interval: Duration,
    
    /// Enable profiling
    pub profiling: bool,
    
    /// Memory pool settings
    pub memory_pool: MemoryPoolConfig,
    
    /// Message batching settings
    pub message_batching: MessageBatchingConfig,
}

/// Memory pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPoolConfig {
    /// Enable memory pooling
    pub enabled: bool,
    
    /// Initial pool size
    pub initial_size: usize,
    
    /// Maximum pool size
    pub max_size: usize,
    
    /// Pool growth factor
    pub growth_factor: f64,
    
    /// Pool shrink threshold
    pub shrink_threshold: f64,
}

/// Message batching configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBatchingConfig {
    /// Enable message batching
    pub enabled: bool,
    
    /// Maximum batch size
    pub max_batch_size: usize,
    
    /// Batch timeout
    pub batch_timeout: Duration,
    
    /// Batch compression
    pub compression: bool,
}

impl Default for ActorSystemConfig {
    fn default() -> Self {
        Self {
            runtime: RuntimeConfig::default(),
            supervision: SupervisionConfig::default(),
            mailbox: MailboxConfig::default(),
            actors: ActorConfigurations::default(),
            timeouts: SystemTimeouts::default(),
            performance: PerformanceConfig::default(),
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            worker_threads: None, // Use Tokio default
            enable_io: true,
            enable_time: true,
            thread_name_prefix: "alys-actor".to_string(),
            thread_stack_size: None,
            thread_keep_alive: Duration::from_secs(60),
        }
    }
}

impl Default for SupervisionConfig {
    fn default() -> Self {
        Self {
            default_restart_strategy: RestartStrategyConfig::OneForOne {
                max_retries: 3,
                within_time: Duration::from_secs(60),
            },
            max_restarts: 5,
            restart_window: Duration::from_secs(300),
            escalation_timeout: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(30),
            auto_recovery: true,
            recovery_strategies: HashMap::new(),
        }
    }
}

impl Default for MailboxConfig {
    fn default() -> Self {
        Self {
            default_capacity: 1000,
            backpressure_strategy: BackpressureStrategy::DropOldest,
            message_timeout: Some(Duration::from_secs(30)),
            priority_queue: None,
            dead_letter: DeadLetterConfig::default(),
        }
    }
}

impl Default for DeadLetterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_messages: 10000,
            retention_time: Duration::from_hours(1),
            handler: DeadLetterHandler::Log { level: LogLevel::Warn },
        }
    }
}

impl Default for ActorConfigurations {
    fn default() -> Self {
        Self {
            chain_actor: ActorConfig::default(),
            engine_actor: ActorConfig::default(),
            bridge_actor: ActorConfig::default(),
            network_actor: ActorConfig::default(),
            sync_actor: ActorConfig::default(),
            stream_actor: ActorConfig::default(),
            storage_actor: ActorConfig::default(),
            supervisor_actor: ActorConfig::default(),
        }
    }
}

impl Default for ActorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mailbox_capacity: None, // Use system default
            restart_strategy: None, // Use system default
            health_check: ActorHealthConfig::default(),
            performance: ActorPerformanceConfig::default(),
            custom: HashMap::new(),
        }
    }
}

impl Default for ActorHealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
            failure_threshold: 3,
            recovery_threshold: 2,
        }
    }
}

impl Default for ActorPerformanceConfig {
    fn default() -> Self {
        Self {
            message_timeout: Some(Duration::from_secs(10)),
            max_memory_mb: None,
            cpu_limit_percent: None,
            monitoring: true,
            metrics_interval: Duration::from_secs(60),
        }
    }
}

impl Default for SystemTimeouts {
    fn default() -> Self {
        Self {
            startup_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(30),
            initialization_timeout: Duration::from_secs(60),
            health_check_timeout: Duration::from_secs(5),
            config_reload_timeout: Duration::from_secs(10),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            monitoring: true,
            metrics_interval: Duration::from_secs(30),
            profiling: false,
            memory_pool: MemoryPoolConfig::default(),
            message_batching: MessageBatchingConfig::default(),
        }
    }
}

impl Default for MemoryPoolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            initial_size: 1000,
            max_size: 10000,
            growth_factor: 1.5,
            shrink_threshold: 0.25,
        }
    }
}

impl Default for MessageBatchingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_batch_size: 100,
            batch_timeout: Duration::from_millis(10),
            compression: false,
        }
    }
}

impl ActorSystemConfig {
    /// Create a configuration optimized for high throughput
    pub fn high_throughput() -> Self {
        Self {
            runtime: RuntimeConfig {
                worker_threads: Some(num_cpus::get() * 2),
                enable_io: true,
                enable_time: true,
                thread_name_prefix: "alys-ht".to_string(),
                thread_stack_size: Some(2 * 1024 * 1024), // 2MB
                thread_keep_alive: Duration::from_secs(300),
            },
            supervision: SupervisionConfig {
                default_restart_strategy: RestartStrategyConfig::CircuitBreaker {
                    failure_threshold: 5,
                    recovery_timeout: Duration::from_secs(30),
                    success_threshold: 10,
                },
                max_restarts: 10,
                restart_window: Duration::from_secs(600),
                escalation_timeout: Duration::from_secs(60),
                health_check_interval: Duration::from_secs(15),
                auto_recovery: true,
                recovery_strategies: HashMap::new(),
            },
            mailbox: MailboxConfig {
                default_capacity: 10000,
                backpressure_strategy: BackpressureStrategy::DropOldest,
                message_timeout: Some(Duration::from_secs(60)),
                priority_queue: Some(PriorityQueueConfig {
                    levels: 5,
                    default_priority: 2,
                    algorithm: PriorityAlgorithm::WeightedFair,
                }),
                dead_letter: DeadLetterConfig {
                    enabled: true,
                    max_messages: 100000,
                    retention_time: Duration::from_hours(6),
                    handler: DeadLetterHandler::Log { level: LogLevel::Warn },
                },
            },
            actors: ActorConfigurations::high_throughput(),
            timeouts: SystemTimeouts {
                startup_timeout: Duration::from_secs(60),
                shutdown_timeout: Duration::from_secs(60),
                initialization_timeout: Duration::from_secs(120),
                health_check_timeout: Duration::from_secs(10),
                config_reload_timeout: Duration::from_secs(30),
            },
            performance: PerformanceConfig {
                monitoring: true,
                metrics_interval: Duration::from_secs(15),
                profiling: true,
                memory_pool: MemoryPoolConfig {
                    enabled: true,
                    initial_size: 10000,
                    max_size: 100000,
                    growth_factor: 2.0,
                    shrink_threshold: 0.2,
                },
                message_batching: MessageBatchingConfig {
                    enabled: true,
                    max_batch_size: 1000,
                    batch_timeout: Duration::from_millis(5),
                    compression: true,
                },
            },
        }
    }
    
    /// Create a configuration optimized for low latency
    pub fn low_latency() -> Self {
        Self {
            runtime: RuntimeConfig {
                worker_threads: Some(num_cpus::get()),
                enable_io: true,
                enable_time: true,
                thread_name_prefix: "alys-ll".to_string(),
                thread_stack_size: Some(1024 * 1024), // 1MB
                thread_keep_alive: Duration::from_secs(30),
            },
            supervision: SupervisionConfig {
                default_restart_strategy: RestartStrategyConfig::OneForOne {
                    max_retries: 1,
                    within_time: Duration::from_secs(10),
                },
                max_restarts: 3,
                restart_window: Duration::from_secs(60),
                escalation_timeout: Duration::from_secs(5),
                health_check_interval: Duration::from_secs(5),
                auto_recovery: true,
                recovery_strategies: HashMap::new(),
            },
            mailbox: MailboxConfig {
                default_capacity: 100,
                backpressure_strategy: BackpressureStrategy::Fail,
                message_timeout: Some(Duration::from_millis(100)),
                priority_queue: Some(PriorityQueueConfig {
                    levels: 3,
                    default_priority: 1,
                    algorithm: PriorityAlgorithm::Strict,
                }),
                dead_letter: DeadLetterConfig {
                    enabled: true,
                    max_messages: 1000,
                    retention_time: Duration::from_minutes(15),
                    handler: DeadLetterHandler::Log { level: LogLevel::Error },
                },
            },
            actors: ActorConfigurations::low_latency(),
            timeouts: SystemTimeouts {
                startup_timeout: Duration::from_secs(5),
                shutdown_timeout: Duration::from_secs(5),
                initialization_timeout: Duration::from_secs(15),
                health_check_timeout: Duration::from_secs(1),
                config_reload_timeout: Duration::from_secs(3),
            },
            performance: PerformanceConfig {
                monitoring: true,
                metrics_interval: Duration::from_secs(5),
                profiling: false,
                memory_pool: MemoryPoolConfig {
                    enabled: true,
                    initial_size: 1000,
                    max_size: 5000,
                    growth_factor: 1.2,
                    shrink_threshold: 0.1,
                },
                message_batching: MessageBatchingConfig {
                    enabled: false,
                    max_batch_size: 1,
                    batch_timeout: Duration::from_millis(1),
                    compression: false,
                },
            },
        }
    }
    
    /// Create a configuration optimized for resource conservation
    pub fn resource_conservative() -> Self {
        Self {
            runtime: RuntimeConfig {
                worker_threads: Some(2),
                enable_io: true,
                enable_time: true,
                thread_name_prefix: "alys-rc".to_string(),
                thread_stack_size: Some(512 * 1024), // 512KB
                thread_keep_alive: Duration::from_secs(10),
            },
            supervision: SupervisionConfig {
                default_restart_strategy: RestartStrategyConfig::ExponentialBackoff {
                    initial_delay: Duration::from_secs(1),
                    max_delay: Duration::from_secs(300),
                    multiplier: 2.0,
                    max_retries: 5,
                },
                max_restarts: 3,
                restart_window: Duration::from_secs(900),
                escalation_timeout: Duration::from_secs(120),
                health_check_interval: Duration::from_secs(60),
                auto_recovery: true,
                recovery_strategies: HashMap::new(),
            },
            mailbox: MailboxConfig {
                default_capacity: 100,
                backpressure_strategy: BackpressureStrategy::Block,
                message_timeout: Some(Duration::from_secs(300)),
                priority_queue: None,
                dead_letter: DeadLetterConfig {
                    enabled: true,
                    max_messages: 1000,
                    retention_time: Duration::from_hours(1),
                    handler: DeadLetterHandler::Log { level: LogLevel::Info },
                },
            },
            actors: ActorConfigurations::resource_conservative(),
            timeouts: SystemTimeouts {
                startup_timeout: Duration::from_secs(15),
                shutdown_timeout: Duration::from_secs(15),
                initialization_timeout: Duration::from_secs(30),
                health_check_timeout: Duration::from_secs(3),
                config_reload_timeout: Duration::from_secs(5),
            },
            performance: PerformanceConfig {
                monitoring: false,
                metrics_interval: Duration::from_secs(300),
                profiling: false,
                memory_pool: MemoryPoolConfig {
                    enabled: true,
                    initial_size: 100,
                    max_size: 1000,
                    growth_factor: 1.1,
                    shrink_threshold: 0.5,
                },
                message_batching: MessageBatchingConfig {
                    enabled: true,
                    max_batch_size: 50,
                    batch_timeout: Duration::from_millis(100),
                    compression: true,
                },
            },
        }
    }
}

impl ActorConfigurations {
    /// High throughput actor configurations
    pub fn high_throughput() -> Self {
        let base_config = ActorConfig {
            enabled: true,
            mailbox_capacity: Some(10000),
            restart_strategy: Some(RestartStrategyConfig::CircuitBreaker {
                failure_threshold: 10,
                recovery_timeout: Duration::from_secs(30),
                success_threshold: 20,
            }),
            health_check: ActorHealthConfig {
                enabled: true,
                interval: Duration::from_secs(15),
                timeout: Duration::from_secs(3),
                failure_threshold: 5,
                recovery_threshold: 3,
            },
            performance: ActorPerformanceConfig {
                message_timeout: Some(Duration::from_secs(30)),
                max_memory_mb: Some(1024),
                cpu_limit_percent: Some(80.0),
                monitoring: true,
                metrics_interval: Duration::from_secs(30),
            },
            custom: HashMap::new(),
        };
        
        Self {
            chain_actor: base_config.clone(),
            engine_actor: base_config.clone(),
            bridge_actor: base_config.clone(),
            network_actor: base_config.clone(),
            sync_actor: base_config.clone(),
            stream_actor: base_config.clone(),
            storage_actor: base_config.clone(),
            supervisor_actor: base_config,
        }
    }
    
    /// Low latency actor configurations
    pub fn low_latency() -> Self {
        let base_config = ActorConfig {
            enabled: true,
            mailbox_capacity: Some(100),
            restart_strategy: Some(RestartStrategyConfig::OneForOne {
                max_retries: 1,
                within_time: Duration::from_secs(5),
            }),
            health_check: ActorHealthConfig {
                enabled: true,
                interval: Duration::from_secs(5),
                timeout: Duration::from_millis(500),
                failure_threshold: 2,
                recovery_threshold: 1,
            },
            performance: ActorPerformanceConfig {
                message_timeout: Some(Duration::from_millis(50)),
                max_memory_mb: Some(256),
                cpu_limit_percent: Some(50.0),
                monitoring: true,
                metrics_interval: Duration::from_secs(10),
            },
            custom: HashMap::new(),
        };
        
        Self {
            chain_actor: base_config.clone(),
            engine_actor: base_config.clone(),
            bridge_actor: base_config.clone(),
            network_actor: base_config.clone(),
            sync_actor: base_config.clone(),
            stream_actor: base_config.clone(),
            storage_actor: base_config.clone(),
            supervisor_actor: base_config,
        }
    }
    
    /// Resource conservative actor configurations
    pub fn resource_conservative() -> Self {
        let base_config = ActorConfig {
            enabled: true,
            mailbox_capacity: Some(50),
            restart_strategy: Some(RestartStrategyConfig::ExponentialBackoff {
                initial_delay: Duration::from_secs(2),
                max_delay: Duration::from_secs(120),
                multiplier: 1.5,
                max_retries: 3,
            }),
            health_check: ActorHealthConfig {
                enabled: true,
                interval: Duration::from_secs(60),
                timeout: Duration::from_secs(5),
                failure_threshold: 3,
                recovery_threshold: 2,
            },
            performance: ActorPerformanceConfig {
                message_timeout: Some(Duration::from_secs(120)),
                max_memory_mb: Some(128),
                cpu_limit_percent: Some(25.0),
                monitoring: false,
                metrics_interval: Duration::from_secs(300),
            },
            custom: HashMap::new(),
        };
        
        Self {
            chain_actor: base_config.clone(),
            engine_actor: base_config.clone(),
            bridge_actor: base_config.clone(),
            network_actor: base_config.clone(),
            sync_actor: base_config.clone(),
            stream_actor: base_config.clone(),
            storage_actor: base_config.clone(),
            supervisor_actor: base_config,
        }
    }
}

impl Validate for ActorSystemConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        // Validate runtime configuration
        if let Some(threads) = self.runtime.worker_threads {
            if threads == 0 {
                return Err(ConfigError::ValidationError {
                    field: "actors.runtime.worker_threads".to_string(),
                    reason: "Worker threads must be greater than 0".to_string(),
                });
            }
            
            if threads > 1000 {
                return Err(ConfigError::ValidationError {
                    field: "actors.runtime.worker_threads".to_string(),
                    reason: "Worker threads should not exceed 1000".to_string(),
                });
            }
        }
        
        // Validate mailbox configuration
        if self.mailbox.default_capacity == 0 {
            return Err(ConfigError::ValidationError {
                field: "actors.mailbox.default_capacity".to_string(),
                reason: "Mailbox capacity must be greater than 0".to_string(),
            });
        }
        
        if self.mailbox.default_capacity > 1_000_000 {
            return Err(ConfigError::ValidationError {
                field: "actors.mailbox.default_capacity".to_string(),
                reason: "Mailbox capacity should not exceed 1,000,000 messages".to_string(),
            });
        }
        
        // Validate supervision configuration
        if self.supervision.max_restarts == 0 {
            return Err(ConfigError::ValidationError {
                field: "actors.supervision.max_restarts".to_string(),
                reason: "Max restarts must be greater than 0".to_string(),
            });
        }
        
        if self.supervision.restart_window.as_secs() == 0 {
            return Err(ConfigError::ValidationError {
                field: "actors.supervision.restart_window".to_string(),
                reason: "Restart window must be greater than 0".to_string(),
            });
        }
        
        // Validate individual actor configurations
        self.actors.validate()?;
        
        // Validate performance configuration
        if let Some(max_batch) = self.performance.message_batching.max_batch_size.into() {
            if max_batch > 10000 {
                return Err(ConfigError::ValidationError {
                    field: "actors.performance.message_batching.max_batch_size".to_string(),
                    reason: "Batch size should not exceed 10,000 messages".to_string(),
                });
            }
        }
        
        // Validate memory pool configuration
        if self.performance.memory_pool.initial_size > self.performance.memory_pool.max_size {
            return Err(ConfigError::ValidationError {
                field: "actors.performance.memory_pool".to_string(),
                reason: "Initial pool size cannot be larger than max pool size".to_string(),
            });
        }
        
        if self.performance.memory_pool.growth_factor <= 1.0 {
            return Err(ConfigError::ValidationError {
                field: "actors.performance.memory_pool.growth_factor".to_string(),
                reason: "Growth factor must be greater than 1.0".to_string(),
            });
        }
        
        if self.performance.memory_pool.shrink_threshold <= 0.0 || self.performance.memory_pool.shrink_threshold >= 1.0 {
            return Err(ConfigError::ValidationError {
                field: "actors.performance.memory_pool.shrink_threshold".to_string(),
                reason: "Shrink threshold must be between 0.0 and 1.0".to_string(),
            });
        }
        
        Ok(())
    }
}

impl Validate for ActorConfigurations {
    fn validate(&self) -> Result<(), ConfigError> {
        self.chain_actor.validate()?;
        self.engine_actor.validate()?;
        self.bridge_actor.validate()?;
        self.network_actor.validate()?;
        self.sync_actor.validate()?;
        self.stream_actor.validate()?;
        self.storage_actor.validate()?;
        self.supervisor_actor.validate()?;
        Ok(())
    }
}

impl Validate for ActorConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        // Validate mailbox capacity
        if let Some(capacity) = self.mailbox_capacity {
            if capacity == 0 {
                return Err(ConfigError::ValidationError {
                    field: "actor.mailbox_capacity".to_string(),
                    reason: "Actor mailbox capacity must be greater than 0".to_string(),
                });
            }
            
            if capacity > 10_000_000 {
                return Err(ConfigError::ValidationError {
                    field: "actor.mailbox_capacity".to_string(),
                    reason: "Actor mailbox capacity should not exceed 10,000,000 messages".to_string(),
                });
            }
        }
        
        // Validate health check configuration
        if self.health_check.enabled {
            if self.health_check.interval.as_millis() == 0 {
                return Err(ConfigError::ValidationError {
                    field: "actor.health_check.interval".to_string(),
                    reason: "Health check interval must be greater than 0".to_string(),
                });
            }
            
            if self.health_check.timeout >= self.health_check.interval {
                return Err(ConfigError::ValidationError {
                    field: "actor.health_check.timeout".to_string(),
                    reason: "Health check timeout must be less than interval".to_string(),
                });
            }
            
            if self.health_check.failure_threshold == 0 {
                return Err(ConfigError::ValidationError {
                    field: "actor.health_check.failure_threshold".to_string(),
                    reason: "Health check failure threshold must be greater than 0".to_string(),
                });
            }
            
            if self.health_check.recovery_threshold == 0 {
                return Err(ConfigError::ValidationError {
                    field: "actor.health_check.recovery_threshold".to_string(),
                    reason: "Health check recovery threshold must be greater than 0".to_string(),
                });
            }
        }
        
        // Validate performance configuration
        if let Some(cpu_limit) = self.performance.cpu_limit_percent {
            if cpu_limit <= 0.0 || cpu_limit > 100.0 {
                return Err(ConfigError::ValidationError {
                    field: "actor.performance.cpu_limit_percent".to_string(),
                    reason: "CPU limit must be between 0.0 and 100.0".to_string(),
                });
            }
        }
        
        if let Some(memory_mb) = self.performance.max_memory_mb {
            if memory_mb == 0 {
                return Err(ConfigError::ValidationError {
                    field: "actor.performance.max_memory_mb".to_string(),
                    reason: "Memory limit must be greater than 0".to_string(),
                });
            }
        }
        
        Ok(())
    }
}