use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Test timing utilities
pub struct TestTimer {
    start_time: Instant,
    checkpoints: HashMap<String, Instant>,
}

impl TestTimer {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            checkpoints: HashMap::new(),
        }
    }

    pub fn checkpoint(&mut self, name: &str) {
        self.checkpoints.insert(name.to_string(), Instant::now());
        debug!("Test checkpoint '{}' at {:?}", name, self.elapsed());
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn checkpoint_elapsed(&self, name: &str) -> Option<Duration> {
        self.checkpoints.get(name).map(|time| time.elapsed())
    }

    pub fn since_checkpoint(&self, name: &str) -> Option<Duration> {
        self.checkpoints.get(name).map(|time| time.elapsed())
    }
}

impl Default for TestTimer {
    fn default() -> Self {
        Self::new()
    }
}

/// Retry utilities for flaky operations
pub async fn retry_async<T, E, F, Fut>(
    mut operation: F,
    max_attempts: u32,
    base_delay: Duration,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Debug,
{
    let mut attempts = 0;

    loop {
        attempts += 1;

        match operation().await {
            Ok(result) => {
                if attempts > 1 {
                    info!("Operation succeeded after {} attempts", attempts);
                }
                return Ok(result);
            }
            Err(error) => {
                if attempts >= max_attempts {
                    error!("Operation failed after {} attempts: {:?}", attempts, error);
                    return Err(error);
                }

                let delay = base_delay * attempts;
                warn!(
                    "Operation failed (attempt {}/{}), retrying in {:?}: {:?}",
                    attempts, max_attempts, delay, error
                );
                sleep(delay).await;
            }
        }
    }
}

/// Wait for a condition to become true with timeout
pub async fn wait_for_condition<F>(
    mut condition: F,
    timeout: Duration,
    check_interval: Duration,
) -> Result<(), WaitError>
where
    F: FnMut() -> bool,
{
    let start = Instant::now();

    while start.elapsed() < timeout {
        if condition() {
            return Ok(());
        }
        sleep(check_interval).await;
    }

    Err(WaitError::Timeout(timeout))
}

/// Wait for an async condition to become true with timeout
pub async fn wait_for_async_condition<F, Fut>(
    mut condition: F,
    timeout: Duration,
    check_interval: Duration,
) -> Result<(), WaitError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = Instant::now();

    while start.elapsed() < timeout {
        if condition().await {
            return Ok(());
        }
        sleep(check_interval).await;
    }

    Err(WaitError::Timeout(timeout))
}

#[derive(Debug, thiserror::Error)]
pub enum WaitError {
    #[error("Condition did not become true within timeout: {0:?}")]
    Timeout(Duration),
}

/// Generate test identifiers
pub fn generate_test_id() -> String {
    Uuid::new_v4().to_string()
}

/// Generate test names with timestamp
pub fn generate_test_name(prefix: &str) -> String {
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S_%3f");
    format!("{}_{}", prefix, timestamp)
}

/// Memory usage utilities
pub fn get_memory_usage() -> Result<u64, std::io::Error> {
    // This would be platform-specific in a real implementation
    // For now, return a mock value
    Ok(1024 * 1024) // 1MB mock value
}

/// Test data generation utilities
pub fn generate_random_bytes(size: usize) -> Vec<u8> {
    use rand::RngCore;
    let mut rng = rand::thread_rng();
    let mut bytes = vec![0u8; size];
    rng.fill_bytes(&mut bytes);
    bytes
}

pub fn generate_random_string(length: usize) -> String {
    use rand::{distributions::Alphanumeric, Rng};
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

/// Test assertion helpers
#[macro_export]
macro_rules! assert_within_timeout {
    ($condition:expr, $timeout:expr) => {
        assert_within_timeout!($condition, $timeout, std::time::Duration::from_millis(100))
    };
    ($condition:expr, $timeout:expr, $interval:expr) => {
        $crate::actors_v2::testing::base::utils::wait_for_condition(
            || $condition,
            $timeout,
            $interval,
        )
        .await
        .expect("Condition did not become true within timeout")
    };
}

#[macro_export]
macro_rules! assert_async_within_timeout {
    ($condition:expr, $timeout:expr) => {
        assert_async_within_timeout!($condition, $timeout, std::time::Duration::from_millis(100))
    };
    ($condition:expr, $timeout:expr, $interval:expr) => {
        $crate::actors_v2::testing::base::utils::wait_for_async_condition(
            || async { $condition.await },
            $timeout,
            $interval,
        )
        .await
        .expect("Async condition did not become true within timeout")
    };
}

pub use {assert_async_within_timeout, assert_within_timeout};
