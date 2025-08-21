//! System-wide Constants - ALYS-006-05 Implementation
//! 
//! Constants and utility values for the Alys V2 actor system,
//! providing blockchain-specific timing, capacity limits,
//! and configuration values for the merged mining sidechain.

use std::time::Duration;

/// Alys blockchain-specific timing constants
pub mod blockchain {
    use super::*;

    /// Block production interval for Alys sidechain (2 seconds)
    pub const BLOCK_INTERVAL: Duration = Duration::from_secs(2);

    /// Slot duration for consensus (same as block interval)
    pub const SLOT_DURATION: Duration = BLOCK_INTERVAL;

    /// Maximum time to wait for block finalization
    pub const BLOCK_FINALIZATION_TIMEOUT: Duration = Duration::from_secs(20);

    /// Bitcoin confirmation requirements for peg-in operations
    pub const BITCOIN_CONFIRMATIONS: u32 = 6;

    /// Maximum blocks without AuxPoW before halt
    pub const MAX_BLOCKS_WITHOUT_POW: u32 = 10;

    /// AuxPoW mining work distribution timeout
    pub const AUXPOW_WORK_TIMEOUT: Duration = Duration::from_secs(30);

    /// Federation signature collection timeout
    pub const FEDERATION_SIGNATURE_TIMEOUT: Duration = Duration::from_secs(5);

    /// Consensus coordination timeout
    pub const CONSENSUS_TIMEOUT: Duration = Duration::from_secs(10);

    /// Peg-out processing timeout
    pub const PEGOUT_TIMEOUT: Duration = Duration::from_secs(60);

    /// Network partition detection timeout
    pub const NETWORK_PARTITION_TIMEOUT: Duration = Duration::from_secs(30);
}

/// Actor system lifecycle constants
pub mod lifecycle {
    use super::*;

    /// Default system startup timeout
    pub const SYSTEM_STARTUP_TIMEOUT: Duration = Duration::from_secs(60);

    /// Default graceful shutdown timeout
    pub const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(120);

    /// Actor startup timeout
    pub const ACTOR_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

    /// Actor shutdown timeout
    pub const ACTOR_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

    /// Health check response timeout
    pub const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

    /// Supervision tree initialization timeout
    pub const SUPERVISION_INIT_TIMEOUT: Duration = Duration::from_secs(15);
}

/// Message handling and mailbox constants
pub mod messaging {
    /// Default mailbox capacity for normal actors
    pub const DEFAULT_MAILBOX_CAPACITY: usize = 10000;

    /// High priority mailbox capacity for critical actors
    pub const HIGH_PRIORITY_MAILBOX_CAPACITY: usize = 50000;

    /// Critical system actor mailbox capacity
    pub const CRITICAL_MAILBOX_CAPACITY: usize = 100000;

    /// Background actor mailbox capacity
    pub const BACKGROUND_MAILBOX_CAPACITY: usize = 1000;

    /// Maximum message processing time before warning
    pub const MESSAGE_PROCESSING_WARNING_THRESHOLD: Duration = Duration::from_secs(1);

    /// Maximum message processing time before error
    pub const MESSAGE_PROCESSING_ERROR_THRESHOLD: Duration = Duration::from_secs(5);

    /// Message timeout for critical operations
    pub const CRITICAL_MESSAGE_TIMEOUT: Duration = Duration::from_secs(30);

    /// Message timeout for normal operations
    pub const NORMAL_MESSAGE_TIMEOUT: Duration = Duration::from_secs(10);

    /// Message timeout for background operations
    pub const BACKGROUND_MESSAGE_TIMEOUT: Duration = Duration::from_secs(60);
}

/// Restart strategy constants
pub mod restart {
    use super::*;

    /// Default initial delay for exponential backoff
    pub const DEFAULT_INITIAL_DELAY: Duration = Duration::from_millis(100);

    /// Default maximum delay for exponential backoff
    pub const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(60);

    /// Default exponential backoff multiplier
    pub const DEFAULT_BACKOFF_MULTIPLIER: f64 = 2.0;

    /// Default maximum restart attempts
    pub const DEFAULT_MAX_RESTARTS: usize = 10;

    /// Fast restart initial delay for critical actors
    pub const FAST_RESTART_INITIAL_DELAY: Duration = Duration::from_millis(50);

    /// Slow restart initial delay for non-critical actors
    pub const SLOW_RESTART_INITIAL_DELAY: Duration = Duration::from_millis(500);

    /// Blockchain-aware restart minimum delay
    pub const BLOCKCHAIN_MIN_RESTART_DELAY: Duration = Duration::from_millis(1500);

    /// Maximum restart attempts for critical actors
    pub const CRITICAL_ACTOR_MAX_RESTARTS: usize = 20;

    /// Maximum restart attempts for normal actors
    pub const NORMAL_ACTOR_MAX_RESTARTS: usize = 10;

    /// Maximum restart attempts for background actors
    pub const BACKGROUND_ACTOR_MAX_RESTARTS: usize = 5;
}

/// Health monitoring constants
pub mod health {
    use super::*;

    /// Default health check interval
    pub const DEFAULT_HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(10);

    /// Critical actor health check interval
    pub const CRITICAL_HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(5);

    /// Background actor health check interval
    pub const BACKGROUND_HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(30);

    /// Health check failure threshold before marking unhealthy
    pub const HEALTH_CHECK_FAILURE_THRESHOLD: u32 = 3;

    /// Health check recovery threshold before marking healthy
    pub const HEALTH_CHECK_RECOVERY_THRESHOLD: u32 = 2;

    /// System health check timeout
    pub const SYSTEM_HEALTH_TIMEOUT: Duration = Duration::from_secs(5);

    /// Detailed health reporting threshold (actors)
    pub const DETAILED_HEALTH_ACTOR_THRESHOLD: usize = 100;
}

/// Performance and resource constants
pub mod performance {
    use super::*;

    /// Default memory limit per actor (1GB)
    pub const DEFAULT_MEMORY_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;

    /// Critical actor memory limit (2GB)
    pub const CRITICAL_ACTOR_MEMORY_LIMIT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

    /// Background actor memory limit (512MB)
    pub const BACKGROUND_ACTOR_MEMORY_LIMIT_BYTES: u64 = 512 * 1024 * 1024;

    /// CPU usage warning threshold (percentage)
    pub const CPU_WARNING_THRESHOLD: f64 = 70.0;

    /// CPU usage error threshold (percentage)
    pub const CPU_ERROR_THRESHOLD: f64 = 90.0;

    /// Memory usage warning threshold (percentage)
    pub const MEMORY_WARNING_THRESHOLD: f64 = 80.0;

    /// Memory usage error threshold (percentage)
    pub const MEMORY_ERROR_THRESHOLD: f64 = 95.0;

    /// Performance metrics collection interval
    pub const METRICS_COLLECTION_INTERVAL: Duration = Duration::from_secs(10);

    /// Performance optimization check interval
    pub const OPTIMIZATION_CHECK_INTERVAL: Duration = Duration::from_secs(60);
}

/// Actor registry constants
pub mod registry {
    use super::*;
    
    /// Maximum number of actors in registry
    pub const MAX_ACTORS: usize = 50000;
    
    /// Maximum actor name length
    pub const MAX_ACTOR_NAME_LENGTH: usize = 128;
    
    /// Maximum tags per actor
    pub const MAX_TAGS_PER_ACTOR: usize = 20;
    
    /// Registry cleanup batch size
    pub const CLEANUP_BATCH_SIZE: usize = 100;
    
    /// Maximum metadata entries per actor
    pub const MAX_METADATA_ENTRIES: usize = 50;
    
    /// Registry statistics update interval
    pub const STATS_UPDATE_INTERVAL: Duration = Duration::from_secs(30);
    
    /// Health check timeout
    pub const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(5);
    
    /// Default registry maintenance interval
    pub const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(300);
    
    /// Default actor cleanup threshold (inactive for 1 hour)
    pub const DEFAULT_CLEANUP_THRESHOLD: Duration = Duration::from_secs(3600);
}

/// Network and connectivity constants
pub mod network {
    use super::*;

    /// Default network timeout for peer communication
    pub const DEFAULT_NETWORK_TIMEOUT: Duration = Duration::from_secs(10);

    /// RPC call timeout for blockchain operations
    pub const RPC_TIMEOUT: Duration = Duration::from_secs(30);

    /// P2P message propagation timeout
    pub const P2P_PROPAGATION_TIMEOUT: Duration = Duration::from_secs(5);

    /// Maximum network retry attempts
    pub const MAX_NETWORK_RETRIES: usize = 3;

    /// Network retry backoff initial delay
    pub const NETWORK_RETRY_INITIAL_DELAY: Duration = Duration::from_millis(100);

    /// Network retry backoff multiplier
    pub const NETWORK_RETRY_MULTIPLIER: f64 = 1.5;

    /// Connection pool size for external services
    pub const CONNECTION_POOL_SIZE: usize = 20;

    /// Connection timeout for external services
    pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);
}

/// Error handling and logging constants
pub mod errors {
    use super::*;

    /// Maximum error history entries per actor
    pub const MAX_ERROR_HISTORY_ENTRIES: usize = 100;

    /// Error rate calculation window
    pub const ERROR_RATE_WINDOW: Duration = Duration::from_secs(300); // 5 minutes

    /// High error rate threshold (errors per minute)
    pub const HIGH_ERROR_RATE_THRESHOLD: f64 = 10.0;

    /// Critical error rate threshold (errors per minute)
    pub const CRITICAL_ERROR_RATE_THRESHOLD: f64 = 50.0;

    /// Error burst detection window
    pub const ERROR_BURST_WINDOW: Duration = Duration::from_secs(10);

    /// Error burst threshold (errors in burst window)
    pub const ERROR_BURST_THRESHOLD: usize = 5;
}

/// Testing and development constants
pub mod testing {
    use super::*;

    /// Test timeout for unit tests
    pub const UNIT_TEST_TIMEOUT: Duration = Duration::from_secs(5);

    /// Test timeout for integration tests
    pub const INTEGRATION_TEST_TIMEOUT: Duration = Duration::from_secs(30);

    /// Test timeout for end-to-end tests
    pub const E2E_TEST_TIMEOUT: Duration = Duration::from_secs(120);

    /// Chaos testing failure injection rate
    pub const CHAOS_FAILURE_RATE: f64 = 0.1; // 10%

    /// Performance test measurement duration
    pub const PERFORMANCE_TEST_DURATION: Duration = Duration::from_secs(60);

    /// Load test concurrent actors
    pub const LOAD_TEST_CONCURRENT_ACTORS: usize = 100;

    /// Mock actor response delay
    pub const MOCK_ACTOR_DELAY: Duration = Duration::from_millis(10);
}

/// Configuration validation constants
pub mod validation {
    /// Minimum mailbox capacity
    pub const MIN_MAILBOX_CAPACITY: usize = 100;

    /// Maximum mailbox capacity (prevent memory exhaustion)
    pub const MAX_MAILBOX_CAPACITY: usize = 1_000_000;

    /// Minimum restart delay
    pub const MIN_RESTART_DELAY: Duration = Duration::from_millis(10);

    /// Maximum restart delay
    pub const MAX_RESTART_DELAY: Duration = Duration::from_secs(3600); // 1 hour

    /// Minimum backoff multiplier
    pub const MIN_BACKOFF_MULTIPLIER: f64 = 1.1;

    /// Maximum backoff multiplier
    pub const MAX_BACKOFF_MULTIPLIER: f64 = 10.0;

    /// Maximum restart attempts
    pub const MAX_RESTART_ATTEMPTS: usize = 1000;

    /// Minimum health check interval
    pub const MIN_HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(1);

    /// Maximum health check interval
    pub const MAX_HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(300); // 5 minutes
}

/// System resource limits
pub mod limits {
    /// Maximum number of actors in the system
    pub const MAX_ACTORS: usize = 10000;

    /// Maximum supervision tree depth
    pub const MAX_SUPERVISION_DEPTH: usize = 10;

    /// Maximum actor dependencies
    pub const MAX_ACTOR_DEPENDENCIES: usize = 50;

    /// Maximum feature flags
    pub const MAX_FEATURE_FLAGS: usize = 1000;

    /// Maximum configuration entries
    pub const MAX_CONFIG_ENTRIES: usize = 10000;

    /// Maximum log entries in memory
    pub const MAX_LOG_ENTRIES: usize = 100000;

    /// Maximum metrics entries
    pub const MAX_METRICS_ENTRIES: usize = 1000000;
}

/// Adapter system constants for migration patterns
pub mod adapter {
    use super::*;

    /// Maximum metrics history per adapter
    pub const MAX_METRICS_HISTORY: usize = 10000;
    
    /// Metrics cleanup batch size
    pub const METRICS_CLEANUP_BATCH_SIZE: usize = 1000;
    
    /// Default performance monitoring interval
    pub const PERFORMANCE_MONITORING_INTERVAL: Duration = Duration::from_secs(60);
    
    /// Default consistency check interval
    pub const CONSISTENCY_CHECK_INTERVAL: Duration = Duration::from_secs(30);
    
    /// Migration operation timeout
    pub const MIGRATION_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);
    
    /// Dual-path execution timeout
    pub const DUAL_PATH_TIMEOUT: Duration = Duration::from_secs(20);
    
    /// Legacy system timeout
    pub const LEGACY_TIMEOUT: Duration = Duration::from_secs(15);
    
    /// Actor system timeout
    pub const ACTOR_TIMEOUT: Duration = Duration::from_secs(15);
    
    /// Rollback decision timeout
    pub const ROLLBACK_TIMEOUT: Duration = Duration::from_secs(10);
    
    /// Maximum inconsistencies before rollback
    pub const MAX_INCONSISTENCIES_BEFORE_ROLLBACK: usize = 10;
    
    /// Performance degradation threshold (ratio)
    pub const PERFORMANCE_DEGRADATION_THRESHOLD: f64 = 2.0;
    
    /// Minimum operations for performance comparison
    pub const MIN_OPERATIONS_FOR_COMPARISON: usize = 100;
}

/// Migration system constants
pub mod migration {
    use super::*;

    /// Migration phase transition timeout
    pub const PHASE_TRANSITION_TIMEOUT: Duration = Duration::from_secs(120);
    
    /// Migration validation timeout
    pub const VALIDATION_TIMEOUT: Duration = Duration::from_secs(60);
    
    /// Migration rollback timeout
    pub const ROLLBACK_TIMEOUT: Duration = Duration::from_secs(180);
    
    /// Feature flag evaluation timeout
    pub const FEATURE_FLAG_TIMEOUT: Duration = Duration::from_secs(5);
    
    /// Migration state persistence interval
    pub const STATE_PERSISTENCE_INTERVAL: Duration = Duration::from_secs(30);
    
    /// Migration progress reporting interval
    pub const PROGRESS_REPORTING_INTERVAL: Duration = Duration::from_secs(60);
    
    /// Maximum migration phases
    pub const MAX_MIGRATION_PHASES: usize = 6;
    
    /// Migration success rate threshold
    pub const SUCCESS_RATE_THRESHOLD: f64 = 0.99;
    
    /// Migration performance threshold
    pub const PERFORMANCE_THRESHOLD: f64 = 1.5;
    
    /// Migration consistency threshold
    pub const CONSISTENCY_THRESHOLD: f64 = 0.99;
    
    /// Minimum migration duration before advancement
    pub const MIN_PHASE_DURATION: Duration = Duration::from_secs(300); // 5 minutes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blockchain_constants() {
        assert_eq!(blockchain::BLOCK_INTERVAL, Duration::from_secs(2));
        assert_eq!(blockchain::BITCOIN_CONFIRMATIONS, 6);
        assert_eq!(blockchain::MAX_BLOCKS_WITHOUT_POW, 10);
        assert!(blockchain::BLOCK_FINALIZATION_TIMEOUT > blockchain::BLOCK_INTERVAL);
    }

    #[test]
    fn test_messaging_constants() {
        assert!(messaging::HIGH_PRIORITY_MAILBOX_CAPACITY > messaging::DEFAULT_MAILBOX_CAPACITY);
        assert!(messaging::CRITICAL_MAILBOX_CAPACITY > messaging::HIGH_PRIORITY_MAILBOX_CAPACITY);
        assert!(messaging::BACKGROUND_MAILBOX_CAPACITY < messaging::DEFAULT_MAILBOX_CAPACITY);
    }

    #[test]
    fn test_restart_constants() {
        assert!(restart::FAST_RESTART_INITIAL_DELAY < restart::DEFAULT_INITIAL_DELAY);
        assert!(restart::SLOW_RESTART_INITIAL_DELAY > restart::DEFAULT_INITIAL_DELAY);
        assert!(restart::CRITICAL_ACTOR_MAX_RESTARTS > restart::NORMAL_ACTOR_MAX_RESTARTS);
    }

    #[test]
    fn test_validation_constants() {
        assert!(validation::MIN_MAILBOX_CAPACITY < validation::MAX_MAILBOX_CAPACITY);
        assert!(validation::MIN_RESTART_DELAY < validation::MAX_RESTART_DELAY);
        assert!(validation::MIN_BACKOFF_MULTIPLIER < validation::MAX_BACKOFF_MULTIPLIER);
    }

    #[test]
    fn test_performance_constants() {
        assert!(performance::CRITICAL_ACTOR_MEMORY_LIMIT_BYTES > performance::DEFAULT_MEMORY_LIMIT_BYTES);
        assert!(performance::BACKGROUND_ACTOR_MEMORY_LIMIT_BYTES < performance::DEFAULT_MEMORY_LIMIT_BYTES);
        assert!(performance::CPU_ERROR_THRESHOLD > performance::CPU_WARNING_THRESHOLD);
    }

    #[test]
    fn test_health_constants() {
        assert!(health::CRITICAL_HEALTH_CHECK_INTERVAL < health::DEFAULT_HEALTH_CHECK_INTERVAL);
        assert!(health::BACKGROUND_HEALTH_CHECK_INTERVAL > health::DEFAULT_HEALTH_CHECK_INTERVAL);
        assert!(health::HEALTH_CHECK_RECOVERY_THRESHOLD < health::HEALTH_CHECK_FAILURE_THRESHOLD);
    }

    #[test]
    fn test_registry_constants() {
        assert!(registry::MAX_ACTORS > 1000);
        assert!(registry::MAX_ACTOR_NAME_LENGTH > 0);
        assert!(registry::MAX_TAGS_PER_ACTOR > 0);
        assert!(registry::CLEANUP_BATCH_SIZE > 0);
        assert!(registry::MAX_METADATA_ENTRIES > 0);
        assert!(registry::HEALTH_CHECK_TIMEOUT.as_secs() > 0);
        assert!(registry::MAINTENANCE_INTERVAL > registry::STATS_UPDATE_INTERVAL);
        assert!(registry::DEFAULT_CLEANUP_THRESHOLD > registry::MAINTENANCE_INTERVAL);
    }

    #[test]
    fn test_adapter_constants() {
        assert!(adapter::MAX_METRICS_HISTORY > 0);
        assert!(adapter::METRICS_CLEANUP_BATCH_SIZE > 0);
        assert!(adapter::DUAL_PATH_TIMEOUT > Duration::from_secs(0));
        assert!(adapter::LEGACY_TIMEOUT > Duration::from_secs(0));
        assert!(adapter::ACTOR_TIMEOUT > Duration::from_secs(0));
        assert!(adapter::MAX_INCONSISTENCIES_BEFORE_ROLLBACK > 0);
        assert!(adapter::PERFORMANCE_DEGRADATION_THRESHOLD > 1.0);
    }

    #[test]
    fn test_migration_constants() {
        assert!(migration::PHASE_TRANSITION_TIMEOUT > Duration::from_secs(0));
        assert!(migration::VALIDATION_TIMEOUT > Duration::from_secs(0));
        assert!(migration::ROLLBACK_TIMEOUT > Duration::from_secs(0));
        assert!(migration::SUCCESS_RATE_THRESHOLD > 0.0);
        assert!(migration::SUCCESS_RATE_THRESHOLD <= 1.0);
        assert!(migration::PERFORMANCE_THRESHOLD > 1.0);
        assert!(migration::CONSISTENCY_THRESHOLD > 0.0);
        assert!(migration::CONSISTENCY_THRESHOLD <= 1.0);
        assert!(migration::MIN_PHASE_DURATION > Duration::from_secs(0));
    }
}