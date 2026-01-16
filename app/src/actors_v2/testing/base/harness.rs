use super::{ActorTestHarness, TestContext};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Generic test harness implementation
pub struct BaseTestHarness<T> {
    pub context: TestContext,
    pub actor: Arc<RwLock<T>>,
    pub metrics: TestMetrics,
}

#[derive(Debug, Default)]
pub struct TestMetrics {
    pub messages_sent: u64,
    pub errors_encountered: u64,
    pub test_duration: Option<std::time::Duration>,
    pub memory_peak: u64,
    pub operations_completed: u64,
    pub operations_failed: u64,
}

impl<T> BaseTestHarness<T> {
    pub fn new_with_actor(actor: T) -> Self {
        Self {
            context: TestContext::default(),
            actor: Arc::new(RwLock::new(actor)),
            metrics: TestMetrics::default(),
        }
    }

    pub async fn with_timeout(&mut self, timeout: std::time::Duration) {
        self.context.timeout = timeout;
    }

    pub async fn measure_memory(&mut self) {
        // Platform-specific memory measurement would go here
        self.metrics.memory_peak = self.get_current_memory_usage();
    }

    pub async fn start_operation(&mut self) {
        debug!("Starting test operation: {}", self.context.test_name);
        self.metrics.operations_completed += 1;
    }

    pub async fn record_error(&mut self, error: &str) {
        error!("Test error recorded: {}", error);
        self.metrics.errors_encountered += 1;
        self.metrics.operations_failed += 1;
    }

    pub async fn record_success(&mut self) {
        debug!("Test operation completed successfully");
        // Success is already recorded in operations_completed
    }

    fn get_current_memory_usage(&self) -> u64 {
        // In a real implementation, this would use platform-specific memory profiling
        // For now, return a mock value based on operations count
        self.metrics.operations_completed * 1024 // Mock: 1KB per operation
    }

    pub async fn get_actor_ref(&self) -> Arc<RwLock<T>> {
        self.actor.clone()
    }

    pub async fn set_test_name(&mut self, name: String) {
        self.context.test_name = name;
        info!("Test context updated: {}", self.context.test_name);
    }

    pub fn get_metrics(&self) -> &TestMetrics {
        &self.metrics
    }

    pub fn get_context(&self) -> &TestContext {
        &self.context
    }
}
