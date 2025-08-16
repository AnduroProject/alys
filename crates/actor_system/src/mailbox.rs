//! Enhanced mailbox implementation with backpressure and priority queuing
//!
//! This module provides mailbox capabilities including priority-based message
//! queuing, backpressure handling, bounded channels, and message routing.

use crate::{
    error::{ActorError, ActorResult},
    message::{AlysMessage, MessageEnvelope, MessagePriority},
    metrics::MailboxMetrics,
};
use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BinaryHeap, VecDeque},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Mailbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxConfig {
    /// Maximum number of messages in mailbox
    pub capacity: usize,
    /// Enable priority queue for messages
    pub enable_priority: bool,
    /// Maximum processing time per message
    pub processing_timeout: Duration,
    /// Backpressure threshold (percentage of capacity)
    pub backpressure_threshold: f64,
    /// Drop old messages when full
    pub drop_on_full: bool,
    /// Metrics collection interval
    pub metrics_interval: Duration,
}

impl Default for MailboxConfig {
    fn default() -> Self {
        Self {
            capacity: 1000,
            enable_priority: true,
            processing_timeout: Duration::from_secs(30),
            backpressure_threshold: 0.8,
            drop_on_full: false,
            metrics_interval: Duration::from_secs(10),
        }
    }
}

/// Backpressure state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpressureState {
    /// Normal operation
    Normal,
    /// Warning level (approaching capacity)
    Warning,
    /// Critical level (at or near capacity)
    Critical,
    /// Blocked (at capacity)
    Blocked,
}

/// Message wrapper with metadata for queuing
#[derive(Debug)]
pub struct QueuedMessage<M>
where
    M: AlysMessage,
{
    /// Message envelope
    pub envelope: MessageEnvelope<M>,
    /// Queue entry time
    pub queued_at: SystemTime,
    /// Message ID for tracking
    pub message_id: Uuid,
    /// Response channel for request-response pattern
    pub response_tx: Option<oneshot::Sender<M::Result>>,
}

impl<M> PartialEq for QueuedMessage<M>
where
    M: AlysMessage,
{
    fn eq(&self, other: &Self) -> bool {
        self.envelope.metadata.priority == other.envelope.metadata.priority
            && self.queued_at == other.queued_at
    }
}

impl<M> Eq for QueuedMessage<M> where M: AlysMessage {}

impl<M> PartialOrd for QueuedMessage<M>
where
    M: AlysMessage,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<M> Ord for QueuedMessage<M>
where
    M: AlysMessage,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority messages come first, then older messages
        match self.envelope.metadata.priority.cmp(&other.envelope.metadata.priority) {
            std::cmp::Ordering::Equal => other.queued_at.cmp(&self.queued_at),
            other => other,
        }
    }
}

/// Priority queue implementation for messages
#[derive(Debug)]
pub struct PriorityQueue<M>
where
    M: AlysMessage,
{
    /// Priority heap for high/critical messages
    high_priority: BinaryHeap<QueuedMessage<M>>,
    /// FIFO queue for normal priority messages
    normal_priority: VecDeque<QueuedMessage<M>>,
    /// FIFO queue for low priority messages
    low_priority: VecDeque<QueuedMessage<M>>,
    /// Total message count
    total_count: usize,
}

impl<M> PriorityQueue<M>
where
    M: AlysMessage,
{
    /// Create new priority queue
    pub fn new() -> Self {
        Self {
            high_priority: BinaryHeap::new(),
            normal_priority: VecDeque::new(),
            low_priority: VecDeque::new(),
            total_count: 0,
        }
    }

    /// Push message to appropriate queue
    pub fn push(&mut self, message: QueuedMessage<M>) {
        match message.envelope.metadata.priority {
            MessagePriority::Emergency | MessagePriority::Critical | MessagePriority::High => {
                self.high_priority.push(message);
            }
            MessagePriority::Normal => {
                self.normal_priority.push_back(message);
            }
            MessagePriority::Low | MessagePriority::Background => {
                self.low_priority.push_back(message);
            }
        }
        self.total_count += 1;
    }

    /// Pop highest priority message
    pub fn pop(&mut self) -> Option<QueuedMessage<M>> {
        // Process high priority first
        if let Some(message) = self.high_priority.pop() {
            self.total_count -= 1;
            return Some(message);
        }

        // Then normal priority
        if let Some(message) = self.normal_priority.pop_front() {
            self.total_count -= 1;
            return Some(message);
        }

        // Finally low priority
        if let Some(message) = self.low_priority.pop_front() {
            self.total_count -= 1;
            return Some(message);
        }

        None
    }

    /// Get total message count
    pub fn len(&self) -> usize {
        self.total_count
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.total_count == 0
    }

    /// Get message counts by priority
    pub fn priority_counts(&self) -> (usize, usize, usize) {
        (
            self.high_priority.len(),
            self.normal_priority.len(),
            self.low_priority.len(),
        )
    }
}

impl<M> Default for PriorityQueue<M>
where
    M: AlysMessage,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Enhanced mailbox with backpressure and priority handling
pub struct EnhancedMailbox<M>
where
    M: AlysMessage + 'static,
{
    /// Mailbox configuration
    config: MailboxConfig,
    /// Message queue
    queue: Arc<parking_lot::Mutex<PriorityQueue<M>>>,
    /// Backpressure semaphore
    backpressure_semaphore: Arc<Semaphore>,
    /// Current mailbox metrics
    metrics: Arc<MailboxMetrics>,
    /// Backpressure state
    backpressure_state: Arc<std::sync::atomic::AtomicU8>,
    /// Message processing channel
    message_tx: mpsc::UnboundedSender<QueuedMessage<M>>,
    /// Message processing receiver
    message_rx: Arc<parking_lot::Mutex<Option<mpsc::UnboundedReceiver<QueuedMessage<M>>>>>,
}

impl<M> EnhancedMailbox<M>
where
    M: AlysMessage + 'static,
{
    /// Create new enhanced mailbox
    pub fn new(config: MailboxConfig) -> Self {
        let (message_tx, message_rx) = mpsc::unbounded_channel();

        Self {
            backpressure_semaphore: Arc::new(Semaphore::new(config.capacity)),
            queue: Arc::new(parking_lot::Mutex::new(PriorityQueue::new())),
            metrics: Arc::new(MailboxMetrics::new()),
            backpressure_state: Arc::new(std::sync::atomic::AtomicU8::new(
                BackpressureState::Normal as u8,
            )),
            config,
            message_tx,
            message_rx: Arc::new(parking_lot::Mutex::new(Some(message_rx))),
        }
    }

    /// Send message to mailbox
    pub async fn send(&self, envelope: MessageEnvelope<M>) -> ActorResult<()> {
        // Check backpressure
        self.update_backpressure_state();
        
        let current_state = BackpressureState::from(
            self.backpressure_state.load(Ordering::Relaxed)
        );

        match current_state {
            BackpressureState::Blocked => {
                if self.config.drop_on_full {
                    warn!("Mailbox full, dropping message");
                    self.metrics.messages_dropped.fetch_add(1, Ordering::Relaxed);
                    return Err(ActorError::MailboxFull {
                        actor_name: "unknown".to_string(),
                        current_size: self.len(),
                        max_size: self.config.capacity,
                    });
                }
            }
            BackpressureState::Critical => {
                warn!("Mailbox at critical capacity, applying backpressure");
            }
            BackpressureState::Warning => {
                debug!("Mailbox approaching capacity threshold");
            }
            BackpressureState::Normal => {}
        }

        // Acquire semaphore permit for backpressure control
        let _permit = self.backpressure_semaphore.acquire().await
            .map_err(|_| ActorError::MailboxFull {
                actor_name: "unknown".to_string(),
                current_size: self.len(),
                max_size: self.config.capacity,
            })?;

        let queued_message = QueuedMessage {
            envelope,
            queued_at: SystemTime::now(),
            message_id: Uuid::new_v4(),
            response_tx: None,
        };

        // Add to queue
        {
            let mut queue = self.queue.lock();
            queue.push(queued_message);
        }

        // Update metrics
        self.metrics.messages_queued.fetch_add(1, Ordering::Relaxed);
        self.metrics.current_size.store(self.len(), Ordering::Relaxed);

        Ok(())
    }

    /// Send message with response channel
    pub async fn send_and_wait(&self, envelope: MessageEnvelope<M>) -> ActorResult<M::Result> {
        let (tx, rx) = oneshot::channel();

        let queued_message = QueuedMessage {
            envelope,
            queued_at: SystemTime::now(),
            message_id: Uuid::new_v4(),
            response_tx: Some(tx),
        };

        // Send to internal channel
        self.message_tx.send(queued_message)
            .map_err(|_| ActorError::MessageDeliveryFailed {
                from: "mailbox".to_string(),
                to: "actor".to_string(),
                reason: "Channel closed".to_string(),
            })?;

        // Wait for response with timeout
        let response = tokio::time::timeout(self.config.processing_timeout, rx).await
            .map_err(|_| ActorError::Timeout {
                operation: "message_processing".to_string(),
                timeout: self.config.processing_timeout,
            })?
            .map_err(|_| ActorError::MessageHandlingFailed {
                message_type: std::any::type_name::<M>().to_string(),
                reason: "Response channel closed".to_string(),
            })?;

        Ok(response)
    }

    /// Receive next message from mailbox
    pub async fn recv(&self) -> Option<QueuedMessage<M>> {
        let mut queue = self.queue.lock();
        let message = queue.pop();
        
        if message.is_some() {
            self.metrics.messages_processed.fetch_add(1, Ordering::Relaxed);
            self.metrics.current_size.store(queue.len(), Ordering::Relaxed);
        }
        
        message
    }

    /// Get current mailbox size
    pub fn len(&self) -> usize {
        self.queue.lock().len()
    }

    /// Check if mailbox is empty
    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }

    /// Get current backpressure state
    pub fn backpressure_state(&self) -> BackpressureState {
        BackpressureState::from(self.backpressure_state.load(Ordering::Relaxed))
    }

    /// Update backpressure state based on current queue size
    fn update_backpressure_state(&self) {
        let current_size = self.len();
        let capacity = self.config.capacity;
        let threshold = (capacity as f64 * self.config.backpressure_threshold) as usize;

        let new_state = if current_size >= capacity {
            BackpressureState::Blocked
        } else if current_size >= threshold {
            BackpressureState::Critical
        } else if current_size >= capacity / 2 {
            BackpressureState::Warning
        } else {
            BackpressureState::Normal
        };

        self.backpressure_state.store(new_state as u8, Ordering::Relaxed);
    }

    /// Get mailbox metrics
    pub fn metrics(&self) -> Arc<MailboxMetrics> {
        self.metrics.clone()
    }

    /// Get priority distribution
    pub fn priority_distribution(&self) -> (usize, usize, usize) {
        self.queue.lock().priority_counts()
    }

    /// Clear all messages (for shutdown)
    pub fn clear(&self) {
        let mut queue = self.queue.lock();
        let dropped_count = queue.len();
        
        while queue.pop().is_some() {
            // Drop all messages
        }
        
        self.metrics.messages_dropped.fetch_add(dropped_count, Ordering::Relaxed);
        self.metrics.current_size.store(0, Ordering::Relaxed);
        
        info!("Cleared {} messages from mailbox", dropped_count);
    }
}

impl From<u8> for BackpressureState {
    fn from(value: u8) -> Self {
        match value {
            0 => BackpressureState::Normal,
            1 => BackpressureState::Warning,
            2 => BackpressureState::Critical,
            3 => BackpressureState::Blocked,
            _ => BackpressureState::Normal,
        }
    }
}

/// Mailbox manager for coordinating multiple mailboxes
pub struct MailboxManager {
    /// Mailbox configurations by actor type
    configs: std::collections::HashMap<String, MailboxConfig>,
    /// Default configuration
    default_config: MailboxConfig,
    /// Global metrics aggregation
    global_metrics: Arc<MailboxMetrics>,
}

impl MailboxManager {
    /// Create new mailbox manager
    pub fn new() -> Self {
        Self {
            configs: std::collections::HashMap::new(),
            default_config: MailboxConfig::default(),
            global_metrics: Arc::new(MailboxMetrics::new()),
        }
    }

    /// Add configuration for specific actor type
    pub fn add_config(&mut self, actor_type: String, config: MailboxConfig) {
        self.configs.insert(actor_type, config);
    }

    /// Create mailbox for actor type
    pub fn create_mailbox<M>(&self, actor_type: &str) -> EnhancedMailbox<M>
    where
        M: AlysMessage + 'static,
    {
        let config = self.configs.get(actor_type)
            .unwrap_or(&self.default_config)
            .clone();

        EnhancedMailbox::new(config)
    }

    /// Get global metrics
    pub fn global_metrics(&self) -> Arc<MailboxMetrics> {
        self.global_metrics.clone()
    }
}

impl Default for MailboxManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Mailbox metrics implementation
impl MailboxMetrics {
    /// Create new mailbox metrics
    pub fn new() -> Self {
        Self {
            messages_queued: AtomicU64::new(0),
            messages_processed: AtomicU64::new(0),
            messages_dropped: AtomicU64::new(0),
            current_size: AtomicUsize::new(0),
            max_size_reached: AtomicUsize::new(0),
            total_wait_time: AtomicU64::new(0),
            processing_times: parking_lot::RwLock::new(Vec::new()),
        }
    }

    /// Record message wait time
    pub fn record_wait_time(&self, wait_time: Duration) {
        self.total_wait_time.fetch_add(wait_time.as_nanos() as u64, Ordering::Relaxed);
    }

    /// Record message processing time
    pub fn record_processing_time(&self, processing_time: Duration) {
        let mut times = self.processing_times.write();
        times.push(processing_time);
        
        // Keep only recent processing times (sliding window)
        if times.len() > 1000 {
            times.drain(..500);
        }
    }

    /// Get average wait time
    pub fn average_wait_time(&self) -> Duration {
        let total_wait = self.total_wait_time.load(Ordering::Relaxed);
        let processed = self.messages_processed.load(Ordering::Relaxed);
        
        if processed > 0 {
            Duration::from_nanos(total_wait / processed)
        } else {
            Duration::ZERO
        }
    }

    /// Get current queue utilization
    pub fn queue_utilization(&self, max_capacity: usize) -> f64 {
        let current = self.current_size.load(Ordering::Relaxed) as f64;
        let max = max_capacity as f64;
        if max > 0.0 { current / max } else { 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::HealthCheckMessage;

    #[test]
    fn test_priority_queue_ordering() {
        let mut queue = PriorityQueue::new();
        
        // Create messages with different priorities
        let low_msg = QueuedMessage {
            envelope: MessageEnvelope::new(HealthCheckMessage)
                .with_priority(MessagePriority::Low),
            queued_at: SystemTime::now(),
            message_id: Uuid::new_v4(),
            response_tx: None,
        };
        
        let high_msg = QueuedMessage {
            envelope: MessageEnvelope::new(HealthCheckMessage)
                .with_priority(MessagePriority::Critical),
            queued_at: SystemTime::now(),
            message_id: Uuid::new_v4(),
            response_tx: None,
        };
        
        queue.push(low_msg);
        queue.push(high_msg);
        
        // High priority should come out first
        let first = queue.pop().unwrap();
        assert_eq!(first.envelope.metadata.priority, MessagePriority::Critical);
        
        let second = queue.pop().unwrap();
        assert_eq!(second.envelope.metadata.priority, MessagePriority::Low);
    }

    #[test]
    fn test_backpressure_state_conversion() {
        assert_eq!(BackpressureState::from(0), BackpressureState::Normal);
        assert_eq!(BackpressureState::from(1), BackpressureState::Warning);
        assert_eq!(BackpressureState::from(2), BackpressureState::Critical);
        assert_eq!(BackpressureState::from(3), BackpressureState::Blocked);
        assert_eq!(BackpressureState::from(255), BackpressureState::Normal);
    }

    #[tokio::test]
    async fn test_mailbox_basic_operations() {
        let config = MailboxConfig::default();
        let mailbox = EnhancedMailbox::new(config);
        
        let envelope = MessageEnvelope::new(HealthCheckMessage);
        
        // Send message
        assert!(mailbox.send(envelope).await.is_ok());
        assert_eq!(mailbox.len(), 1);
        
        // Receive message
        let received = mailbox.recv().await;
        assert!(received.is_some());
        assert_eq!(mailbox.len(), 0);
    }

    #[test]
    fn test_mailbox_manager() {
        let mut manager = MailboxManager::new();
        
        let custom_config = MailboxConfig {
            capacity: 500,
            ..Default::default()
        };
        
        manager.add_config("test_actor".to_string(), custom_config);
        
        let mailbox: EnhancedMailbox<HealthCheckMessage> = manager.create_mailbox("test_actor");
        // Mailbox should use custom config
        assert_eq!(mailbox.config.capacity, 500);
    }
}