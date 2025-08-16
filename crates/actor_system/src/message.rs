//! Enhanced message types and routing

use crate::error::{ActorError, ActorResult};
use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::any::type_name;
use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Enhanced message trait with metadata and routing information
pub trait AlysMessage: Message + Send + Sync + Clone + fmt::Debug {
    /// Get message type name
    fn message_type(&self) -> &'static str {
        type_name::<Self>()
    }
    
    /// Get message priority
    fn priority(&self) -> MessagePriority {
        MessagePriority::Normal
    }
    
    /// Get message timeout
    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }
    
    /// Check if message can be retried on failure
    fn is_retryable(&self) -> bool {
        true
    }
    
    /// Get maximum retry attempts
    fn max_retries(&self) -> u32 {
        3
    }
    
    /// Serialize message for logging/debugging
    fn serialize_debug(&self) -> serde_json::Value {
        serde_json::json!({
            "type": self.message_type(),
            "priority": self.priority(),
            "timeout": self.timeout().as_secs(),
            "retryable": self.is_retryable(),
            "max_retries": self.max_retries()
        })
    }
}

/// Message priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MessagePriority {
    /// Lowest priority - background tasks
    Background = 0,
    
    /// Low priority - maintenance tasks
    Low = 1,
    
    /// Normal priority - regular operations
    Normal = 2,
    
    /// High priority - important operations
    High = 3,
    
    /// Critical priority - system-critical operations
    Critical = 4,
    
    /// Emergency priority - requires immediate attention
    Emergency = 5,
}

impl MessagePriority {
    /// Check if priority is urgent (high or above)
    pub fn is_urgent(&self) -> bool {
        *self >= MessagePriority::High
    }
    
    /// Check if priority is critical
    pub fn is_critical(&self) -> bool {
        *self >= MessagePriority::Critical
    }
}

/// Message envelope with metadata and routing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope<T> 
where 
    T: AlysMessage,
{
    /// Unique message ID
    pub id: Uuid,
    
    /// The actual message payload
    pub payload: T,
    
    /// Message metadata
    pub metadata: MessageMetadata,
    
    /// Routing information
    pub routing: MessageRouting,
}

/// Message metadata with enhanced distributed tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// When the message was created
    pub created_at: SystemTime,
    
    /// Message priority
    pub priority: MessagePriority,
    
    /// Message timeout
    pub timeout: Duration,
    
    /// Current retry attempt
    pub retry_attempt: u32,
    
    /// Maximum retry attempts
    pub max_retries: u32,
    
    /// Whether message can be retried
    pub retryable: bool,
    
    /// Correlation ID for message tracing
    pub correlation_id: Option<Uuid>,
    
    /// Distributed tracing context
    pub trace_context: TraceContext,
    
    /// Message causality information
    pub causality: CausalityInfo,
    
    /// Performance tracking
    pub performance: MessagePerformanceMetrics,
    
    /// Message lineage (parent messages)
    pub lineage: MessageLineage,
    
    /// Custom attributes
    pub attributes: HashMap<String, serde_json::Value>,
}

/// Distributed tracing context for messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    /// Trace ID for the entire operation flow
    pub trace_id: Option<String>,
    /// Span ID for this specific message
    pub span_id: Option<String>,
    /// Parent span ID
    pub parent_span_id: Option<String>,
    /// Trace flags (sampled, debug, etc.)
    pub trace_flags: TraceFlags,
    /// Baggage items for context propagation
    pub baggage: HashMap<String, String>,
    /// Sampling decision
    pub sampling: SamplingDecision,
    /// Trace state (vendor-specific)
    pub trace_state: Option<String>,
}

impl Default for TraceContext {
    fn default() -> Self {
        Self {
            trace_id: None,
            span_id: None,
            parent_span_id: None,
            trace_flags: TraceFlags::default(),
            baggage: HashMap::new(),
            sampling: SamplingDecision::NotSampled,
            trace_state: None,
        }
    }
}

/// Trace flags for distributed tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceFlags {
    /// Whether this trace is sampled
    pub sampled: bool,
    /// Debug flag
    pub debug: bool,
    /// Deferred flag
    pub deferred: bool,
    /// Custom flags
    pub custom: u8,
}

impl Default for TraceFlags {
    fn default() -> Self {
        Self {
            sampled: false,
            debug: false,
            deferred: false,
            custom: 0,
        }
    }
}

/// Sampling decision for traces
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SamplingDecision {
    /// Not sampled
    NotSampled,
    /// Sampled for collection
    Sampled,
    /// Sampled for debug purposes
    SampledDebug,
    /// Sampled based on rate limit
    SampledRateLimit { rate: f64 },
}

/// Message causality information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalityInfo {
    /// Causal relationship type
    pub relationship: CausalRelationship,
    /// Vector clock for ordering
    pub vector_clock: VectorClock,
    /// Logical timestamp
    pub logical_timestamp: u64,
    /// Causal dependencies
    pub dependencies: Vec<MessageCausalityReference>,
}

impl Default for CausalityInfo {
    fn default() -> Self {
        Self {
            relationship: CausalRelationship::Root,
            vector_clock: VectorClock::default(),
            logical_timestamp: 0,
            dependencies: Vec::new(),
        }
    }
}

/// Types of causal relationships
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CausalRelationship {
    /// Root message (no parent)
    Root,
    /// Direct response to another message
    Response { to_message_id: Uuid },
    /// Triggered by another message
    Triggered { by_message_id: Uuid },
    /// Part of a saga/workflow
    WorkflowStep { workflow_id: Uuid, step: u32 },
    /// Broadcast/fan-out message
    Broadcast { from_message_id: Uuid },
    /// Aggregation/fan-in message
    Aggregation { from_message_ids: Vec<Uuid> },
}

/// Vector clock for message ordering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorClock {
    /// Clock values per actor
    pub clocks: HashMap<String, u64>,
    /// Last updated timestamp
    pub last_updated: SystemTime,
}

impl Default for VectorClock {
    fn default() -> Self {
        Self {
            clocks: HashMap::new(),
            last_updated: SystemTime::now(),
        }
    }
}

/// Reference to causally related message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageCausalityReference {
    /// Referenced message ID
    pub message_id: Uuid,
    /// Actor that sent the referenced message
    pub actor: String,
    /// Relationship type
    pub relationship: String,
    /// When the dependency was established
    pub established_at: SystemTime,
}

/// Message performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePerformanceMetrics {
    /// Message size in bytes
    pub size_bytes: Option<usize>,
    /// Serialization time
    pub serialization_time: Option<Duration>,
    /// Queue time before processing
    pub queue_time: Option<Duration>,
    /// Processing time
    pub processing_time: Option<Duration>,
    /// Network transit time
    pub transit_time: Option<Duration>,
    /// Round-trip time (for request-response)
    pub round_trip_time: Option<Duration>,
    /// Memory usage during processing
    pub memory_usage: Option<u64>,
}

impl Default for MessagePerformanceMetrics {
    fn default() -> Self {
        Self {
            size_bytes: None,
            serialization_time: None,
            queue_time: None,
            processing_time: None,
            transit_time: None,
            round_trip_time: None,
            memory_usage: None,
        }
    }
}

/// Message lineage tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageLineage {
    /// Root message ID in the chain
    pub root_message_id: Option<Uuid>,
    /// Immediate parent message ID
    pub parent_message_id: Option<Uuid>,
    /// Child message IDs spawned from this message
    pub child_message_ids: Vec<Uuid>,
    /// Generation number (depth from root)
    pub generation: u32,
    /// Branch ID for parallel processing
    pub branch_id: Option<String>,
}

impl Default for MessageLineage {
    fn default() -> Self {
        Self {
            root_message_id: None,
            parent_message_id: None,
            child_message_ids: Vec::new(),
            generation: 0,
            branch_id: None,
        }
    }
}

/// Message routing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRouting {
    /// Source actor name
    pub from: Option<String>,
    
    /// Destination actor name
    pub to: Option<String>,
    
    /// Reply-to address for responses
    pub reply_to: Option<String>,
    
    /// Message path (for tracing)
    pub path: Vec<String>,
    
    /// Routing hints
    pub hints: HashMap<String, String>,
}

impl<T> MessageEnvelope<T> 
where 
    T: AlysMessage,
{
    /// Create new message envelope
    pub fn new(payload: T) -> Self {
        Self {
            id: Uuid::new_v4(),
            metadata: MessageMetadata {
                created_at: SystemTime::now(),
                priority: payload.priority(),
                timeout: payload.timeout(),
                retry_attempt: 0,
                max_retries: payload.max_retries(),
                retryable: payload.is_retryable(),
                correlation_id: None,
                trace_context: TraceContext::default(),
                causality: CausalityInfo::default(),
                performance: MessagePerformanceMetrics::default(),
                lineage: MessageLineage::default(),
                attributes: HashMap::new(),
            },
            routing: MessageRouting {
                from: None,
                to: None,
                reply_to: None,
                path: Vec::new(),
                hints: HashMap::new(),
            },
            payload,
        }
    }
    
    /// Start a new distributed trace
    pub fn start_trace(&mut self) -> &mut Self {
        self.metadata.trace_context.trace_id = Some(Uuid::new_v4().to_string());
        self.metadata.trace_context.span_id = Some(Uuid::new_v4().to_string());
        self.metadata.trace_context.trace_flags.sampled = true;
        self
    }
    
    /// Create child span for this message
    pub fn create_child_span(&mut self, operation_name: &str) -> &mut Self {
        let parent_span_id = self.metadata.trace_context.span_id.clone();
        self.metadata.trace_context.parent_span_id = parent_span_id;
        self.metadata.trace_context.span_id = Some(Uuid::new_v4().to_string());
        
        // Add operation name to baggage
        self.metadata.trace_context.baggage.insert(
            "operation".to_string(),
            operation_name.to_string()
        );
        
        self
    }
    
    /// Add baggage item for trace context propagation
    pub fn add_baggage(&mut self, key: &str, value: &str) -> &mut Self {
        self.metadata.trace_context.baggage.insert(key.to_string(), value.to_string());
        self
    }
    
    /// Set causality relationship
    pub fn set_causality(&mut self, relationship: CausalRelationship) -> &mut Self {
        self.metadata.causality.relationship = relationship;
        self
    }
    
    /// Add causal dependency
    pub fn add_causal_dependency(&mut self, dependency: MessageCausalityReference) -> &mut Self {
        self.metadata.causality.dependencies.push(dependency);
        self
    }
    
    /// Update vector clock with actor timestamp
    pub fn update_vector_clock(&mut self, actor_name: &str) -> &mut Self {
        let current_time = self.metadata.causality.vector_clock
            .clocks
            .get(actor_name)
            .unwrap_or(&0) + 1;
        
        self.metadata.causality.vector_clock.clocks.insert(
            actor_name.to_string(),
            current_time
        );
        self.metadata.causality.vector_clock.last_updated = SystemTime::now();
        self.metadata.causality.logical_timestamp = current_time;
        self
    }
    
    /// Start performance timing
    pub fn start_timing(&mut self, metric: &str) -> &mut Self {
        match metric {
            "queue" => {
                // Queue time is from creation to now
                if let Ok(elapsed) = self.metadata.created_at.elapsed() {
                    self.metadata.performance.queue_time = Some(elapsed);
                }
            }
            "processing" => {
                // Start processing timer (will be calculated on finish)
                self.metadata.performance.processing_time = Some(Duration::from_nanos(0));
            }
            _ => {}
        }
        self
    }
    
    /// Record performance metric
    pub fn record_metric(&mut self, metric: &str, duration: Duration) -> &mut Self {
        match metric {
            "serialization" => self.metadata.performance.serialization_time = Some(duration),
            "processing" => self.metadata.performance.processing_time = Some(duration),
            "transit" => self.metadata.performance.transit_time = Some(duration),
            "round_trip" => self.metadata.performance.round_trip_time = Some(duration),
            _ => {}
        }
        self
    }
    
    /// Set memory usage
    pub fn set_memory_usage(&mut self, bytes: u64) -> &mut Self {
        self.metadata.performance.memory_usage = Some(bytes);
        self
    }
    
    /// Add child message to lineage
    pub fn add_child_message(&mut self, child_id: Uuid) -> &mut Self {
        self.metadata.lineage.child_message_ids.push(child_id);
        self
    }
    
    /// Create child envelope with proper lineage
    pub fn create_child<U>(&self, payload: U) -> MessageEnvelope<U> 
    where 
        U: AlysMessage,
    {
        let mut child = MessageEnvelope::new(payload);
        
        // Set up lineage
        child.metadata.lineage.root_message_id = self.metadata.lineage.root_message_id
            .or(Some(self.id));
        child.metadata.lineage.parent_message_id = Some(self.id);
        child.metadata.lineage.generation = self.metadata.lineage.generation + 1;
        child.metadata.lineage.branch_id = self.metadata.lineage.branch_id.clone();
        
        // Inherit trace context
        child.metadata.trace_context.trace_id = self.metadata.trace_context.trace_id.clone();
        child.metadata.trace_context.parent_span_id = self.metadata.trace_context.span_id.clone();
        child.metadata.trace_context.span_id = Some(Uuid::new_v4().to_string());
        child.metadata.trace_context.baggage = self.metadata.trace_context.baggage.clone();
        
        // Set correlation ID
        child.metadata.correlation_id = self.metadata.correlation_id;
        
        child
    }
    
    /// Set correlation ID
    pub fn with_correlation_id(mut self, correlation_id: Uuid) -> Self {
        self.metadata.correlation_id = Some(correlation_id);
        self
    }
    
    /// Set source actor
    pub fn from(mut self, actor_name: String) -> Self {
        self.routing.from = Some(actor_name);
        self
    }
    
    /// Set destination actor
    pub fn to(mut self, actor_name: String) -> Self {
        self.routing.to = Some(actor_name);
        self
    }
    
    /// Set reply-to address
    pub fn reply_to(mut self, actor_name: String) -> Self {
        self.routing.reply_to = Some(actor_name);
        self
    }
    
    /// Add routing hint
    pub fn with_hint(mut self, key: String, value: String) -> Self {
        self.routing.hints.insert(key, value);
        self
    }
    
    /// Add custom attribute
    pub fn with_attribute(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.attributes.insert(key, value);
        self
    }
    
    /// Check if message has expired
    pub fn is_expired(&self) -> bool {
        self.metadata.created_at.elapsed()
            .map(|elapsed| elapsed > self.metadata.timeout)
            .unwrap_or(false)
    }
    
    /// Check if message can be retried
    pub fn can_retry(&self) -> bool {
        self.metadata.retryable && self.metadata.retry_attempt < self.metadata.max_retries
    }
    
    /// Create retry envelope
    pub fn create_retry(&self) -> Option<Self> {
        if !self.can_retry() {
            return None;
        }
        
        let mut retry = self.clone();
        retry.id = Uuid::new_v4();
        retry.metadata.retry_attempt += 1;
        retry.metadata.created_at = SystemTime::now();
        
        Some(retry)
    }
    
    /// Add to routing path
    pub fn add_to_path(&mut self, actor_name: String) {
        self.routing.path.push(actor_name);
    }
    
    /// Get message age
    pub fn age(&self) -> Duration {
        self.metadata.created_at.elapsed().unwrap_or_default()
    }
    
    /// Check if message is part of a trace
    pub fn is_traced(&self) -> bool {
        self.metadata.trace_context.trace_id.is_some()
    }
    
    /// Get trace ID if available
    pub fn trace_id(&self) -> Option<&str> {
        self.metadata.trace_context.trace_id.as_deref()
    }
    
    /// Get span ID if available
    pub fn span_id(&self) -> Option<&str> {
        self.metadata.trace_context.span_id.as_deref()
    }
    
    /// Check if message is sampled for tracing
    pub fn is_sampled(&self) -> bool {
        self.metadata.trace_context.trace_flags.sampled
    }
}

impl<T> Message for MessageEnvelope<T> 
where 
    T: AlysMessage,
{
    type Result = T::Result;
}

/// Enhanced handler trait with error handling and metrics
pub trait AlysHandler<M>: Actor + Handler<M> 
where 
    M: AlysMessage,
{
    /// Handle message with enhanced error reporting
    fn handle_enhanced(&mut self, msg: MessageEnvelope<M>, ctx: &mut Self::Context) -> <MessageEnvelope<M> as Message>::Result;
    
    /// Pre-process message before handling
    fn pre_handle(&mut self, _envelope: &MessageEnvelope<M>) -> ActorResult<()> {
        Ok(())
    }
    
    /// Post-process message after handling
    fn post_handle(&mut self, _envelope: &MessageEnvelope<M>, _result: &M::Result) -> ActorResult<()> {
        Ok(())
    }
    
    /// Handle message error
    fn handle_error(&mut self, _envelope: &MessageEnvelope<M>, _error: &ActorError) -> ActorResult<()> {
        Ok(())
    }
}

/// Standard message types for common operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckMessage;

impl Message for HealthCheckMessage {
    type Result = ActorResult<bool>;
}

impl AlysMessage for HealthCheckMessage {
    fn message_type(&self) -> &'static str {
        "HealthCheck"
    }
    
    fn priority(&self) -> MessagePriority {
        MessagePriority::Low
    }
    
    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
    
    fn is_retryable(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownMessage {
    pub graceful: bool,
    pub timeout: Duration,
}

impl Message for ShutdownMessage {
    type Result = ActorResult<()>;
}

impl AlysMessage for ShutdownMessage {
    fn message_type(&self) -> &'static str {
        "Shutdown"
    }
    
    fn priority(&self) -> MessagePriority {
        MessagePriority::Critical
    }
    
    fn timeout(&self) -> Duration {
        self.timeout + Duration::from_secs(5)
    }
    
    fn is_retryable(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PauseMessage;

impl Message for PauseMessage {
    type Result = ActorResult<()>;
}

impl AlysMessage for PauseMessage {
    fn message_type(&self) -> &'static str {
        "Pause"
    }
    
    fn priority(&self) -> MessagePriority {
        MessagePriority::High
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeMessage;

impl Message for ResumeMessage {
    type Result = ActorResult<()>;
}

impl AlysMessage for ResumeMessage {
    fn message_type(&self) -> &'static str {
        "Resume"
    }
    
    fn priority(&self) -> MessagePriority {
        MessagePriority::High
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartMessage {
    pub reason: String,
}

impl Message for RestartMessage {
    type Result = ActorResult<()>;
}

impl AlysMessage for RestartMessage {
    fn message_type(&self) -> &'static str {
        "Restart"
    }
    
    fn priority(&self) -> MessagePriority {
        MessagePriority::Critical
    }
    
    fn is_retryable(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetMetricsMessage;

impl Message for GetMetricsMessage {
    type Result = ActorResult<crate::metrics::ActorMetrics>;
}

impl AlysMessage for GetMetricsMessage {
    fn message_type(&self) -> &'static str {
        "GetMetrics"
    }
    
    fn priority(&self) -> MessagePriority {
        MessagePriority::Low
    }
}

/// Message builder for convenient message construction
pub struct MessageBuilder<T> 
where 
    T: AlysMessage,
{
    envelope: MessageEnvelope<T>,
}

impl<T> MessageBuilder<T> 
where 
    T: AlysMessage,
{
    /// Create new message builder
    pub fn new(payload: T) -> Self {
        Self {
            envelope: MessageEnvelope::new(payload),
        }
    }
    
    /// Set priority
    pub fn priority(mut self, priority: MessagePriority) -> Self {
        self.envelope.metadata.priority = priority;
        self
    }
    
    /// Set timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.envelope.metadata.timeout = timeout;
        self
    }
    
    /// Set correlation ID
    pub fn correlation_id(mut self, id: Uuid) -> Self {
        self.envelope.metadata.correlation_id = Some(id);
        self
    }
    
    /// Set source
    pub fn from(mut self, actor_name: String) -> Self {
        self.envelope.routing.from = Some(actor_name);
        self
    }
    
    /// Set destination
    pub fn to(mut self, actor_name: String) -> Self {
        self.envelope.routing.to = Some(actor_name);
        self
    }
    
    /// Add attribute
    pub fn attribute<V: Into<serde_json::Value>>(mut self, key: String, value: V) -> Self {
        self.envelope.metadata.attributes.insert(key, value.into());
        self
    }
    
    /// Add routing hint
    pub fn hint(mut self, key: String, value: String) -> Self {
        self.envelope.routing.hints.insert(key, value);
        self
    }
    
    /// Build the message envelope
    pub fn build(self) -> MessageEnvelope<T> {
        self.envelope
    }
}

/// Convenience functions for creating common messages
pub mod messages {
    use super::*;
    
    /// Create health check message
    pub fn health_check() -> MessageEnvelope<HealthCheckMessage> {
        MessageBuilder::new(HealthCheckMessage).build()
    }
    
    /// Create shutdown message
    pub fn shutdown(graceful: bool, timeout: Duration) -> MessageEnvelope<ShutdownMessage> {
        MessageBuilder::new(ShutdownMessage { graceful, timeout })
            .priority(MessagePriority::Critical)
            .build()
    }
    
    /// Create pause message
    pub fn pause() -> MessageEnvelope<PauseMessage> {
        MessageBuilder::new(PauseMessage)
            .priority(MessagePriority::High)
            .build()
    }
    
    /// Create resume message
    pub fn resume() -> MessageEnvelope<ResumeMessage> {
        MessageBuilder::new(ResumeMessage)
            .priority(MessagePriority::High)
            .build()
    }
    
    /// Create restart message
    pub fn restart(reason: String) -> MessageEnvelope<RestartMessage> {
        MessageBuilder::new(RestartMessage { reason })
            .priority(MessagePriority::Critical)
            .build()
    }
    
    /// Create get metrics message
    pub fn get_metrics() -> MessageEnvelope<GetMetricsMessage> {
        MessageBuilder::new(GetMetricsMessage)
            .priority(MessagePriority::Low)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestMessage {
        content: String,
    }
    
    impl Message for TestMessage {
        type Result = String;
    }
    
    impl AlysMessage for TestMessage {
        fn priority(&self) -> MessagePriority {
            MessagePriority::High
        }
    }
    
    #[test]
    fn test_message_envelope_creation() {
        let msg = TestMessage { content: "test".to_string() };
        let envelope = MessageEnvelope::new(msg);
        
        assert_eq!(envelope.metadata.priority, MessagePriority::High);
        assert_eq!(envelope.metadata.retry_attempt, 0);
        assert!(envelope.metadata.retryable);
        assert!(!envelope.is_expired());
        assert!(envelope.can_retry());
    }
    
    #[test]
    fn test_message_builder() {
        let msg = TestMessage { content: "test".to_string() };
        let envelope = MessageBuilder::new(msg)
            .priority(MessagePriority::Critical)
            .timeout(Duration::from_secs(10))
            .from("actor1".to_string())
            .to("actor2".to_string())
            .attribute("key".to_string(), "value")
            .build();
        
        assert_eq!(envelope.metadata.priority, MessagePriority::Critical);
        assert_eq!(envelope.metadata.timeout, Duration::from_secs(10));
        assert_eq!(envelope.routing.from, Some("actor1".to_string()));
        assert_eq!(envelope.routing.to, Some("actor2".to_string()));
        assert!(envelope.metadata.attributes.contains_key("key"));
    }
    
    #[test]
    fn test_message_retry() {
        let msg = TestMessage { content: "test".to_string() };
        let envelope = MessageEnvelope::new(msg);
        
        assert!(envelope.can_retry());
        
        let retry = envelope.create_retry().unwrap();
        assert_eq!(retry.metadata.retry_attempt, 1);
        assert_ne!(retry.id, envelope.id);
        
        // Test max retries
        let mut retry = envelope;
        retry.metadata.retry_attempt = retry.metadata.max_retries;
        assert!(!retry.can_retry());
        assert!(retry.create_retry().is_none());
    }
    
    #[test]
    fn test_message_priority_ordering() {
        assert!(MessagePriority::Emergency > MessagePriority::Critical);
        assert!(MessagePriority::Critical > MessagePriority::High);
        assert!(MessagePriority::High.is_urgent());
        assert!(MessagePriority::Critical.is_critical());
        assert!(!MessagePriority::Normal.is_urgent());
    }
}