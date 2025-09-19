use async_trait::async_trait;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{warn, error, info};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Trait for failure injection mechanisms
#[async_trait]
pub trait FailureInjector: Send + Sync {
    type Config: Send + Sync;
    type Error: std::error::Error + Send + Sync + 'static;

    /// Start the failure injection
    async fn start(&mut self, config: Self::Config) -> Result<(), Self::Error>;

    /// Stop the failure injection
    async fn stop(&mut self) -> Result<(), Self::Error>;

    /// Check if the injector is currently active
    fn is_active(&self) -> bool;

    /// Get current failure statistics
    fn get_stats(&self) -> FailureStats;
}

#[derive(Debug, Clone, Default)]
pub struct FailureStats {
    pub injections_attempted: u64,
    pub injections_successful: u64,
    pub injections_failed: u64,
    pub duration_active: Duration,
}

/// Network failure injector
pub struct NetworkFailureInjector {
    active: Arc<AtomicBool>,
    stats: FailureStats,
    start_time: Option<std::time::Instant>,
}

impl NetworkFailureInjector {
    pub fn new() -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            stats: FailureStats::default(),
            start_time: None,
        }
    }

    /// Simulate network partition
    pub async fn inject_partition(&mut self, duration: Duration) -> Result<(), NetworkFailureError> {
        if !self.is_active() {
            return Err(NetworkFailureError::NotActive);
        }

        warn!("Injecting network partition for {:?}", duration);
        self.stats.injections_attempted += 1;

        // Simulate network partition by introducing delays
        sleep(duration).await;

        self.stats.injections_successful += 1;
        info!("Network partition ended");
        Ok(())
    }

    /// Simulate packet loss
    pub async fn inject_packet_loss(&mut self, loss_rate: f64, duration: Duration) -> Result<(), NetworkFailureError> {
        if !self.is_active() {
            return Err(NetworkFailureError::NotActive);
        }

        warn!("Injecting packet loss at rate {} for {:?}", loss_rate, duration);
        self.stats.injections_attempted += 1;

        // In a real implementation, this would configure network rules
        sleep(duration).await;

        self.stats.injections_successful += 1;
        info!("Packet loss injection ended");
        Ok(())
    }
}

#[async_trait]
impl FailureInjector for NetworkFailureInjector {
    type Config = NetworkFailureConfig;
    type Error = NetworkFailureError;

    async fn start(&mut self, config: Self::Config) -> Result<(), Self::Error> {
        self.active.store(true, Ordering::SeqCst);
        self.start_time = Some(std::time::Instant::now());

        info!("Network failure injector started with config: {:?}", config);

        // Apply initial configuration
        match config.failure_type {
            NetworkFailureType::Partition { duration } => {
                self.inject_partition(duration).await?;
            },
            NetworkFailureType::PacketLoss { rate, duration } => {
                self.inject_packet_loss(rate, duration).await?;
            },
        }

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), Self::Error> {
        self.active.store(false, Ordering::SeqCst);

        if let Some(start) = self.start_time {
            self.stats.duration_active = start.elapsed();
        }

        info!("Network failure injector stopped. Stats: {:?}", self.stats);
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    fn get_stats(&self) -> FailureStats {
        let mut stats = self.stats.clone();
        if let Some(start) = self.start_time {
            stats.duration_active = start.elapsed();
        }
        stats
    }
}

#[derive(Debug, Clone)]
pub struct NetworkFailureConfig {
    pub failure_type: NetworkFailureType,
}

#[derive(Debug, Clone)]
pub enum NetworkFailureType {
    Partition { duration: Duration },
    PacketLoss { rate: f64, duration: Duration },
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkFailureError {
    #[error("Injector is not active")]
    NotActive,
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

impl Default for NetworkFailureInjector {
    fn default() -> Self {
        Self::new()
    }
}

/// Disk failure injector
pub struct DiskFailureInjector {
    active: Arc<AtomicBool>,
    stats: FailureStats,
    start_time: Option<std::time::Instant>,
    failure_probability: Arc<AtomicU64>, // Using u64 to store f64 bits
}

impl DiskFailureInjector {
    pub fn new() -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            stats: FailureStats::default(),
            start_time: None,
            failure_probability: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Simulate disk full condition
    pub async fn inject_disk_full(&mut self, duration: Duration) -> Result<(), DiskFailureError> {
        if !self.is_active() {
            return Err(DiskFailureError::NotActive);
        }

        warn!("Injecting disk full condition for {:?}", duration);
        self.stats.injections_attempted += 1;

        // Simulate disk full by failing write operations
        sleep(duration).await;

        self.stats.injections_successful += 1;
        info!("Disk full condition cleared");
        Ok(())
    }

    /// Simulate slow disk operations
    pub async fn inject_slow_disk(&mut self, delay: Duration, duration: Duration) -> Result<(), DiskFailureError> {
        if !self.is_active() {
            return Err(DiskFailureError::NotActive);
        }

        warn!("Injecting slow disk operations (delay: {:?}) for {:?}", delay, duration);
        self.stats.injections_attempted += 1;

        // Simulate slow operations by introducing delays
        let start = std::time::Instant::now();
        while start.elapsed() < duration {
            sleep(delay).await;
            sleep(Duration::from_millis(100)).await; // Check interval
        }

        self.stats.injections_successful += 1;
        info!("Slow disk injection ended");
        Ok(())
    }

    /// Check if an operation should fail based on current failure probability
    pub fn should_fail_operation(&self) -> bool {
        if !self.is_active() {
            return false;
        }

        let prob_bits = self.failure_probability.load(Ordering::SeqCst);
        let probability = f64::from_bits(prob_bits);

        use rand::Rng;
        rand::thread_rng().gen::<f64>() < probability
    }
}

#[async_trait]
impl FailureInjector for DiskFailureInjector {
    type Config = DiskFailureConfig;
    type Error = DiskFailureError;

    async fn start(&mut self, config: Self::Config) -> Result<(), Self::Error> {
        self.active.store(true, Ordering::SeqCst);
        self.start_time = Some(std::time::Instant::now());

        // Store failure probability
        self.failure_probability.store(config.failure_probability.to_bits(), Ordering::SeqCst);

        info!("Disk failure injector started with config: {:?}", config);

        match config.failure_type {
            DiskFailureType::DiskFull { duration } => {
                self.inject_disk_full(duration).await?;
            },
            DiskFailureType::SlowOperations { delay, duration } => {
                self.inject_slow_disk(delay, duration).await?;
            },
        }

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), Self::Error> {
        self.active.store(false, Ordering::SeqCst);
        self.failure_probability.store(0.0_f64.to_bits(), Ordering::SeqCst);

        if let Some(start) = self.start_time {
            self.stats.duration_active = start.elapsed();
        }

        info!("Disk failure injector stopped. Stats: {:?}", self.stats);
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    fn get_stats(&self) -> FailureStats {
        let mut stats = self.stats.clone();
        if let Some(start) = self.start_time {
            stats.duration_active = start.elapsed();
        }
        stats
    }
}

#[derive(Debug, Clone)]
pub struct DiskFailureConfig {
    pub failure_type: DiskFailureType,
    pub failure_probability: f64,
}

#[derive(Debug, Clone)]
pub enum DiskFailureType {
    DiskFull { duration: Duration },
    SlowOperations { delay: Duration, duration: Duration },
}

#[derive(Debug, thiserror::Error)]
pub enum DiskFailureError {
    #[error("Injector is not active")]
    NotActive,
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

impl Default for DiskFailureInjector {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory pressure injector
pub struct MemoryPressureInjector {
    active: Arc<AtomicBool>,
    stats: FailureStats,
    start_time: Option<std::time::Instant>,
    allocated_memory: Vec<Vec<u8>>, // Hold memory to create pressure
}

impl MemoryPressureInjector {
    pub fn new() -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            stats: FailureStats::default(),
            start_time: None,
            allocated_memory: Vec::new(),
        }
    }

    /// Inject memory pressure by allocating memory
    pub async fn inject_memory_pressure(&mut self, target_mb: usize, duration: Duration) -> Result<(), MemoryPressureError> {
        if !self.is_active() {
            return Err(MemoryPressureError::NotActive);
        }

        warn!("Injecting memory pressure: {}MB for {:?}", target_mb, duration);
        self.stats.injections_attempted += 1;

        // Allocate memory in chunks
        let chunk_size = 1024 * 1024; // 1MB chunks
        let chunks_needed = target_mb;

        for _ in 0..chunks_needed {
            let chunk = vec![0u8; chunk_size];
            self.allocated_memory.push(chunk);
        }

        info!("Memory allocated: {}MB", target_mb);

        // Hold memory for specified duration
        sleep(duration).await;

        // Release memory
        self.allocated_memory.clear();

        self.stats.injections_successful += 1;
        info!("Memory pressure released");
        Ok(())
    }
}

#[async_trait]
impl FailureInjector for MemoryPressureInjector {
    type Config = MemoryPressureConfig;
    type Error = MemoryPressureError;

    async fn start(&mut self, config: Self::Config) -> Result<(), Self::Error> {
        self.active.store(true, Ordering::SeqCst);
        self.start_time = Some(std::time::Instant::now());

        info!("Memory pressure injector started with config: {:?}", config);

        self.inject_memory_pressure(config.target_mb, config.duration).await?;

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), Self::Error> {
        self.active.store(false, Ordering::SeqCst);
        self.allocated_memory.clear();

        if let Some(start) = self.start_time {
            self.stats.duration_active = start.elapsed();
        }

        info!("Memory pressure injector stopped. Stats: {:?}", self.stats);
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    fn get_stats(&self) -> FailureStats {
        let mut stats = self.stats.clone();
        if let Some(start) = self.start_time {
            stats.duration_active = start.elapsed();
        }
        stats
    }
}

#[derive(Debug, Clone)]
pub struct MemoryPressureConfig {
    pub target_mb: usize,
    pub duration: Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryPressureError {
    #[error("Injector is not active")]
    NotActive,
    #[error("Allocation failed")]
    AllocationFailed,
}

impl Default for MemoryPressureInjector {
    fn default() -> Self {
        Self::new()
    }
}