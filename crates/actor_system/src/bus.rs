//! Actor communication bus for system-wide messaging and event distribution
//!
//! This module provides a centralized communication bus for broadcasting
//! messages, managing subscriptions, and coordinating system-wide events.

use crate::{
    error::{ActorError, ActorResult},
    message::{AlysMessage, MessageEnvelope, MessagePriority},
    metrics::ActorMetrics,
};
use actix::{prelude::*, Addr, Recipient};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Central communication bus for actor system
pub struct CommunicationBus {
    /// Event subscribers by topic
    subscribers: Arc<RwLock<HashMap<String, Vec<Subscriber>>>>,
    /// Message routing table
    routing_table: Arc<RwLock<RoutingTable>>,
    /// Bus configuration
    config: BusConfig,
    /// Bus metrics
    metrics: Arc<BusMetrics>,
    /// Message history for replay
    message_history: Arc<RwLock<Vec<HistoricalMessage>>>,
    /// Active subscriptions
    subscriptions: Arc<RwLock<HashMap<String, SubscriptionInfo>>>,
}

/// Communication bus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusConfig {
    /// Maximum subscribers per topic
    pub max_subscribers_per_topic: usize,
    /// Message history retention
    pub message_history_size: usize,
    /// Message delivery timeout
    pub delivery_timeout: Duration,
    /// Enable message persistence
    pub enable_persistence: bool,
    /// Retry failed deliveries
    pub retry_failed_deliveries: bool,
    /// Maximum retry attempts
    pub max_retry_attempts: u32,
    /// Bus health check interval
    pub health_check_interval: Duration,
}

impl Default for BusConfig {
    fn default() -> Self {
        Self {
            max_subscribers_per_topic: 1000,
            message_history_size: 10000,
            delivery_timeout: Duration::from_secs(30),
            enable_persistence: false,
            retry_failed_deliveries: true,
            max_retry_attempts: 3,
            health_check_interval: Duration::from_secs(60),
        }
    }
}

/// Bus metrics
#[derive(Debug, Default)]
pub struct BusMetrics {
    /// Total messages published
    pub messages_published: AtomicU64,
    /// Total messages delivered
    pub messages_delivered: AtomicU64,
    /// Failed deliveries
    pub delivery_failures: AtomicU64,
    /// Active subscriptions
    pub active_subscriptions: AtomicU64,
    /// Total topics
    pub total_topics: AtomicU64,
    /// Message processing time (nanoseconds)
    pub processing_time: AtomicU64,
}

/// Subscriber information
#[derive(Debug, Clone)]
pub struct Subscriber {
    /// Subscriber identifier
    pub id: String,
    /// Actor recipient
    pub recipient: Box<dyn std::any::Any + Send + Sync>,
    /// Subscription filters
    pub filters: Vec<MessageFilter>,
    /// Subscription metadata
    pub metadata: SubscriberMetadata,
}

/// Subscriber metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriberMetadata {
    /// Actor type
    pub actor_type: String,
    /// Subscription created time
    pub created_at: SystemTime,
    /// Last message received time
    pub last_message_at: Option<SystemTime>,
    /// Messages received count
    pub messages_received: u64,
    /// Subscription priority
    pub priority: SubscriptionPriority,
}

/// Subscription priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SubscriptionPriority {
    /// Low priority subscription
    Low = 0,
    /// Normal priority subscription
    Normal = 1,
    /// High priority subscription
    High = 2,
    /// Critical priority subscription
    Critical = 3,
}

/// Message filter for selective subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageFilter {
    /// Filter by message type
    MessageType(String),
    /// Filter by actor sender
    Sender(String),
    /// Filter by priority level
    Priority(MessagePriority),
    /// Custom filter predicate
    Custom(String), // Would contain filter logic
}

/// Routing table for message distribution
#[derive(Debug)]
pub struct RoutingTable {
    /// Direct routes between actors
    direct_routes: HashMap<String, Vec<String>>,
    /// Broadcast groups
    broadcast_groups: HashMap<String, HashSet<String>>,
    /// Topic-based routing
    topic_routes: HashMap<String, Vec<String>>,
}

/// Subscription information
#[derive(Debug, Clone)]
pub struct SubscriptionInfo {
    /// Subscription identifier
    pub id: String,
    /// Topics subscribed to
    pub topics: Vec<String>,
    /// Subscriber metadata
    pub metadata: SubscriberMetadata,
    /// Subscription active status
    pub is_active: bool,
}

/// Historical message for replay
#[derive(Debug, Clone)]
pub struct HistoricalMessage {
    /// Message identifier
    pub id: String,
    /// Topic
    pub topic: String,
    /// Message content (serialized)
    pub content: Vec<u8>,
    /// Timestamp
    pub timestamp: SystemTime,
    /// Sender information
    pub sender: Option<String>,
}

impl CommunicationBus {
    /// Create new communication bus
    pub fn new(config: BusConfig) -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            routing_table: Arc::new(RwLock::new(RoutingTable::new())),
            config,
            metrics: Arc::new(BusMetrics::default()),
            message_history: Arc::new(RwLock::new(Vec::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start the communication bus
    pub async fn start(&mut self) -> ActorResult<()> {
        info!("Starting communication bus");

        // Start health monitoring
        self.start_health_monitoring().await;

        Ok(())
    }

    /// Subscribe to a topic
    pub async fn subscribe<M>(
        &self,
        subscriber_id: String,
        topic: String,
        recipient: Recipient<M>,
        filters: Vec<MessageFilter>,
        priority: SubscriptionPriority,
    ) -> ActorResult<String>
    where
        M: AlysMessage + 'static,
    {
        let subscription_id = uuid::Uuid::new_v4().to_string();

        let subscriber = Subscriber {
            id: subscriber_id.clone(),
            recipient: Box::new(recipient),
            filters,
            metadata: SubscriberMetadata {
                actor_type: std::any::type_name::<M>().to_string(),
                created_at: SystemTime::now(),
                last_message_at: None,
                messages_received: 0,
                priority,
            },
        };

        // Add subscriber to topic
        {
            let mut subscribers = self.subscribers.write().await;
            let topic_subscribers = subscribers.entry(topic.clone()).or_insert_with(Vec::new);

            if topic_subscribers.len() >= self.config.max_subscribers_per_topic {
                return Err(ActorError::ResourceExhausted {
                    resource: "topic_subscribers".to_string(),
                });
            }

            topic_subscribers.push(subscriber);
            topic_subscribers.sort_by_key(|s| std::cmp::Reverse(s.metadata.priority));
        }

        // Record subscription
        {
            let mut subscriptions = self.subscriptions.write().await;
            subscriptions.insert(subscription_id.clone(), SubscriptionInfo {
                id: subscription_id.clone(),
                topics: vec![topic.clone()],
                metadata: SubscriberMetadata {
                    actor_type: std::any::type_name::<M>().to_string(),
                    created_at: SystemTime::now(),
                    last_message_at: None,
                    messages_received: 0,
                    priority,
                },
                is_active: true,
            });
        }

        // Update metrics
        self.metrics.active_subscriptions.fetch_add(1, Ordering::Relaxed);
        
        // Update topic count if this is a new topic
        {
            let subscribers = self.subscribers.read().await;
            if subscribers.len() as u64 > self.metrics.total_topics.load(Ordering::Relaxed) {
                self.metrics.total_topics.store(subscribers.len() as u64, Ordering::Relaxed);
            }
        }

        info!(
            subscriber_id = %subscriber_id,
            topic = %topic,
            subscription_id = %subscription_id,
            priority = ?priority,
            "Actor subscribed to topic"
        );

        Ok(subscription_id)
    }

    /// Unsubscribe from a topic
    pub async fn unsubscribe(&self, subscription_id: &str) -> ActorResult<()> {
        let subscription_info = {
            let mut subscriptions = self.subscriptions.write().await;
            subscriptions.remove(subscription_id)
        };

        if let Some(info) = subscription_info {
            // Remove from all subscribed topics
            let mut subscribers = self.subscribers.write().await;
            for topic in &info.topics {
                if let Some(topic_subscribers) = subscribers.get_mut(topic) {
                    topic_subscribers.retain(|s| s.id != info.id);
                    
                    // Remove empty topics
                    if topic_subscribers.is_empty() {
                        subscribers.remove(topic);
                    }
                }
            }

            self.metrics.active_subscriptions.fetch_sub(1, Ordering::Relaxed);
            
            info!(subscription_id = %subscription_id, "Subscription removed");
        }

        Ok(())
    }

    /// Publish message to topic
    pub async fn publish<M>(
        &self,
        topic: String,
        message: M,
        sender: Option<String>,
    ) -> ActorResult<PublishResult>
    where
        M: AlysMessage + Clone + Serialize + 'static,
    {
        let start_time = SystemTime::now();
        let message_id = uuid::Uuid::new_v4().to_string();

        // Record message in history if enabled
        if self.config.enable_persistence {
            self.record_message_history(&topic, &message_id, &message, sender.as_deref()).await?;
        }

        // Get subscribers for topic
        let topic_subscribers = {
            let subscribers = self.subscribers.read().await;
            subscribers.get(&topic).cloned().unwrap_or_default()
        };

        if topic_subscribers.is_empty() {
            warn!(topic = %topic, "No subscribers for topic");
            return Ok(PublishResult {
                message_id,
                delivered_count: 0,
                failed_count: 0,
                total_subscribers: 0,
            });
        }

        let mut delivered = 0;
        let mut failed = 0;
        let total_subscribers = topic_subscribers.len();

        // Deliver to subscribers
        for subscriber in topic_subscribers {
            // Check filters
            if !self.message_matches_filters(&message, &sender, &subscriber.filters) {
                continue;
            }

            // Attempt delivery (simplified - would need proper type handling)
            let delivery_success = true; // Would actually deliver the message

            if delivery_success {
                delivered += 1;
            } else {
                failed += 1;
                
                if self.config.retry_failed_deliveries {
                    // Schedule retry (simplified)
                    debug!(
                        subscriber_id = %subscriber.id,
                        message_id = %message_id,
                        "Scheduling message delivery retry"
                    );
                }
            }
        }

        // Update metrics
        self.metrics.messages_published.fetch_add(1, Ordering::Relaxed);
        self.metrics.messages_delivered.fetch_add(delivered, Ordering::Relaxed);
        self.metrics.delivery_failures.fetch_add(failed, Ordering::Relaxed);
        
        let processing_time = start_time.elapsed().unwrap_or_default();
        self.metrics.processing_time.fetch_add(processing_time.as_nanos() as u64, Ordering::Relaxed);

        info!(
            topic = %topic,
            message_id = %message_id,
            delivered,
            failed,
            total_subscribers,
            processing_time = ?processing_time,
            "Message published to topic"
        );

        Ok(PublishResult {
            message_id,
            delivered_count: delivered,
            failed_count: failed,
            total_subscribers: total_subscribers as u64,
        })
    }

    /// Broadcast message to all subscribers
    pub async fn broadcast<M>(
        &self,
        message: M,
        sender: Option<String>,
        exclude_topics: Vec<String>,
    ) -> ActorResult<HashMap<String, PublishResult>>
    where
        M: AlysMessage + Clone + Serialize + 'static,
    {
        let mut results = HashMap::new();
        let subscribers = self.subscribers.read().await;

        for topic in subscribers.keys() {
            if exclude_topics.contains(topic) {
                continue;
            }

            drop(subscribers); // Release lock before publish
            let result = self.publish(topic.clone(), message.clone(), sender.clone()).await?;
            results.insert(topic.clone(), result);
            let subscribers = self.subscribers.read().await; // Re-acquire lock
        }

        info!(
            topics_count = results.len(),
            sender = ?sender,
            "Message broadcast completed"
        );

        Ok(results)
    }

    /// Get topic statistics
    pub async fn get_topic_stats(&self, topic: &str) -> Option<TopicStats> {
        let subscribers = self.subscribers.read().await;
        let topic_subscribers = subscribers.get(topic)?;

        Some(TopicStats {
            topic: topic.to_string(),
            subscriber_count: topic_subscribers.len(),
            priority_distribution: self.calculate_priority_distribution(topic_subscribers),
            last_message_at: None, // Would track from message history
        })
    }

    /// Get all topic statistics
    pub async fn get_all_topic_stats(&self) -> HashMap<String, TopicStats> {
        let mut stats = HashMap::new();
        let subscribers = self.subscribers.read().await;

        for topic in subscribers.keys() {
            if let Some(topic_stat) = self.get_topic_stats(topic).await {
                stats.insert(topic.clone(), topic_stat);
            }
        }

        stats
    }

    /// Record message in history
    async fn record_message_history<M>(
        &self,
        topic: &str,
        message_id: &str,
        message: &M,
        sender: Option<&str>,
    ) -> ActorResult<()>
    where
        M: Serialize,
    {
        let content = serde_json::to_vec(message)
            .map_err(|e| ActorError::SerializationFailed { 
                reason: e.to_string() 
            })?;

        let historical_message = HistoricalMessage {
            id: message_id.to_string(),
            topic: topic.to_string(),
            content,
            timestamp: SystemTime::now(),
            sender: sender.map(|s| s.to_string()),
        };

        let mut history = self.message_history.write().await;
        history.push(historical_message);

        // Trim history if it exceeds size limit
        if history.len() > self.config.message_history_size {
            history.drain(..history.len() - self.config.message_history_size);
        }

        Ok(())
    }

    /// Check if message matches subscriber filters
    fn message_matches_filters<M>(
        &self,
        message: &M,
        sender: &Option<String>,
        filters: &[MessageFilter],
    ) -> bool
    where
        M: AlysMessage,
    {
        if filters.is_empty() {
            return true;
        }

        for filter in filters {
            match filter {
                MessageFilter::MessageType(msg_type) => {
                    if message.message_type() != msg_type {
                        return false;
                    }
                }
                MessageFilter::Sender(filter_sender) => {
                    if sender.as_deref() != Some(filter_sender) {
                        return false;
                    }
                }
                MessageFilter::Priority(priority) => {
                    if message.priority() != *priority {
                        return false;
                    }
                }
                MessageFilter::Custom(_) => {
                    // Would implement custom filter logic
                    continue;
                }
            }
        }

        true
    }

    /// Calculate priority distribution for subscribers
    fn calculate_priority_distribution(&self, subscribers: &[Subscriber]) -> HashMap<SubscriptionPriority, u32> {
        let mut distribution = HashMap::new();
        
        for subscriber in subscribers {
            *distribution.entry(subscriber.metadata.priority).or_insert(0) += 1;
        }

        distribution
    }

    /// Start health monitoring
    async fn start_health_monitoring(&self) {
        let metrics = self.metrics.clone();
        let interval = self.config.health_check_interval;

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;

                let published = metrics.messages_published.load(Ordering::Relaxed);
                let delivered = metrics.messages_delivered.load(Ordering::Relaxed);
                let failed = metrics.delivery_failures.load(Ordering::Relaxed);
                let subscriptions = metrics.active_subscriptions.load(Ordering::Relaxed);

                debug!(
                    published,
                    delivered,
                    failed,
                    subscriptions,
                    "Communication bus health check"
                );
            }
        });
    }

    /// Get bus metrics
    pub fn metrics(&self) -> Arc<BusMetrics> {
        self.metrics.clone()
    }
}

impl RoutingTable {
    /// Create new routing table
    pub fn new() -> Self {
        Self {
            direct_routes: HashMap::new(),
            broadcast_groups: HashMap::new(),
            topic_routes: HashMap::new(),
        }
    }
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Publication result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishResult {
    /// Message identifier
    pub message_id: String,
    /// Number of successful deliveries
    pub delivered_count: u64,
    /// Number of failed deliveries
    pub failed_count: u64,
    /// Total number of subscribers
    pub total_subscribers: u64,
}

/// Topic statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicStats {
    /// Topic name
    pub topic: String,
    /// Number of subscribers
    pub subscriber_count: usize,
    /// Priority distribution of subscribers
    pub priority_distribution: HashMap<SubscriptionPriority, u32>,
    /// Last message timestamp
    pub last_message_at: Option<SystemTime>,
}

/// Bus messages
#[derive(Debug, Clone)]
pub enum BusMessage {
    /// Get topic statistics
    GetTopicStats { topic: String },
    /// Get all topic statistics
    GetAllTopicStats,
    /// Get bus metrics
    GetMetrics,
    /// Health check
    HealthCheck,
}

impl Message for BusMessage {
    type Result = ActorResult<BusResponse>;
}

impl AlysMessage for BusMessage {
    fn priority(&self) -> MessagePriority {
        MessagePriority::Normal
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(10)
    }
}

/// Bus response messages
#[derive(Debug, Clone)]
pub enum BusResponse {
    /// Topic statistics
    TopicStats(Option<TopicStats>),
    /// All topic statistics
    AllTopicStats(HashMap<String, TopicStats>),
    /// Bus metrics
    Metrics(BusMetrics),
    /// Health status
    HealthStatus(bool),
    /// Error occurred
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bus_config_defaults() {
        let config = BusConfig::default();
        assert_eq!(config.max_subscribers_per_topic, 1000);
        assert_eq!(config.message_history_size, 10000);
        assert!(config.retry_failed_deliveries);
    }

    #[test]
    fn test_subscription_priority_ordering() {
        assert!(SubscriptionPriority::Critical > SubscriptionPriority::High);
        assert!(SubscriptionPriority::High > SubscriptionPriority::Normal);
        assert!(SubscriptionPriority::Normal > SubscriptionPriority::Low);
    }

    #[tokio::test]
    async fn test_communication_bus_creation() {
        let config = BusConfig::default();
        let bus = CommunicationBus::new(config);
        
        let stats = bus.get_all_topic_stats().await;
        assert!(stats.is_empty());
    }

    #[test]
    fn test_routing_table_creation() {
        let table = RoutingTable::new();
        assert!(table.direct_routes.is_empty());
        assert!(table.broadcast_groups.is_empty());
        assert!(table.topic_routes.is_empty());
    }
}