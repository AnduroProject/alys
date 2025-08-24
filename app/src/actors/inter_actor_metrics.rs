//! Inter-Actor Communication Metrics for V2 System
//!
//! This module provides comprehensive metrics for message flows and performance
//! between all V2 actors. Implements Plan B from ALYS-003 Next Steps: 
//! Inter-Actor Communication Metrics.

use crate::metrics::{ALYS_REGISTRY, MetricLabels};
use std::time::{Duration, Instant, SystemTime};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use prometheus::{
    register_histogram_vec_with_registry, register_counter_vec_with_registry,
    register_gauge_vec_with_registry, register_int_gauge_vec_with_registry,
    HistogramVec, CounterVec, GaugeVec, IntGaugeVec,
    HistogramOpts,
};
use lazy_static::lazy_static;
use tracing::*;
use serde_json;
use parking_lot::RwLock;
use uuid::Uuid;

lazy_static! {
    // === Inter-Actor Message Flow Metrics ===
    pub static ref INTER_ACTOR_MESSAGE_LATENCY: HistogramVec = register_histogram_vec_with_registry!(
        HistogramOpts::new(
            "alys_inter_actor_message_latency_seconds",
            "Message latency between actors"
        ).buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]),
        &["from_actor", "to_actor", "message_type"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref INTER_ACTOR_MESSAGE_COUNT: CounterVec = register_counter_vec_with_registry!(
        "alys_inter_actor_messages_total",
        "Total messages sent between actors",
        &["from_actor", "to_actor", "message_type", "status"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref INTER_ACTOR_MESSAGE_ERRORS: CounterVec = register_counter_vec_with_registry!(
        "alys_inter_actor_message_errors_total",
        "Total inter-actor message errors",
        &["from_actor", "to_actor", "message_type", "error_type"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref INTER_ACTOR_MESSAGE_QUEUE_SIZE: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "alys_inter_actor_message_queue_size",
        "Current message queue size between actors",
        &["from_actor", "to_actor"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    // === Actor Dependency Health Metrics ===
    pub static ref ACTOR_DEPENDENCY_HEALTH: GaugeVec = register_gauge_vec_with_registry!(
        "alys_actor_dependency_health_status",
        "Health status of actor dependencies (0=unhealthy, 1=healthy)",
        &["actor", "dependency", "dependency_type"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref ACTOR_DEPENDENCY_RESPONSE_TIME: HistogramVec = register_histogram_vec_with_registry!(
        HistogramOpts::new(
            "alys_actor_dependency_response_time_seconds",
            "Response time from actor dependencies"
        ).buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
        &["actor", "dependency", "operation"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref ACTOR_CIRCUIT_BREAKER_STATE: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "alys_actor_circuit_breaker_state",
        "Circuit breaker state for actor dependencies (0=closed, 1=open, 2=half-open)",
        &["actor", "dependency"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    // === Supervision Tree Metrics ===
    pub static ref SUPERVISION_TREE_RESTARTS: CounterVec = register_counter_vec_with_registry!(
        "alys_supervision_tree_restarts_total",
        "Supervision tree restart events",
        &["supervisor", "child_actor", "restart_reason"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref SUPERVISION_TREE_DEPTH: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "alys_supervision_tree_depth",
        "Current depth of supervision trees",
        &["root_supervisor"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref SUPERVISION_ESCALATION_EVENTS: CounterVec = register_counter_vec_with_registry!(
        "alys_supervision_escalation_events_total",
        "Supervision escalation events to parent supervisors",
        &["supervisor", "escalation_type", "child_actor"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    // === Actor Lifecycle Metrics ===
    pub static ref ACTOR_LIFECYCLE_TRANSITIONS: CounterVec = register_counter_vec_with_registry!(
        "alys_actor_lifecycle_transitions_total",
        "Actor lifecycle state transitions",
        &["actor", "from_state", "to_state", "transition_reason"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref ACTOR_STARTUP_TIME: HistogramVec = register_histogram_vec_with_registry!(
        HistogramOpts::new(
            "alys_actor_startup_time_seconds",
            "Time taken for actors to start up"
        ).buckets(vec![0.001, 0.01, 0.1, 0.5, 1.0, 5.0, 10.0]),
        &["actor_type", "startup_phase"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref ACTOR_SHUTDOWN_TIME: HistogramVec = register_histogram_vec_with_registry!(
        HistogramOpts::new(
            "alys_actor_shutdown_time_seconds",
            "Time taken for actors to shutdown gracefully"
        ).buckets(vec![0.001, 0.01, 0.1, 0.5, 1.0, 5.0, 10.0]),
        &["actor_type", "shutdown_reason"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    // === Deadlock Detection Metrics ===
    pub static ref ACTOR_DEADLOCK_DETECTIONS: CounterVec = register_counter_vec_with_registry!(
        "alys_actor_deadlock_detections_total",
        "Potential deadlock situations detected",
        &["detection_type", "actors_involved"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref ACTOR_MESSAGE_TIMEOUT_EVENTS: CounterVec = register_counter_vec_with_registry!(
        "alys_actor_message_timeout_events_total",
        "Message timeout events that could indicate deadlocks",
        &["from_actor", "to_actor", "message_type"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    // === Actor Communication Patterns ===
    pub static ref ACTOR_COMMUNICATION_PATTERNS: CounterVec = register_counter_vec_with_registry!(
        "alys_actor_communication_patterns_total",
        "Communication pattern classifications",
        &["pattern_type", "actors_involved"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref ACTOR_BROADCAST_EFFICIENCY: GaugeVec = register_gauge_vec_with_registry!(
        "alys_actor_broadcast_efficiency_ratio",
        "Efficiency ratio for broadcast messages (recipients/total_actors)",
        &["source_actor", "broadcast_type"],
        ALYS_REGISTRY
    )
    .unwrap();
}

/// Actor types in the V2 system
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActorType {
    StreamActor,
    ChainActor,
    BridgeActor,
    EngineActor,
    SyncActor,
    NetworkActor,
    StorageActor,
    SupervisorActor,
}

impl ActorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorType::StreamActor => "stream_actor",
            ActorType::ChainActor => "chain_actor",
            ActorType::BridgeActor => "bridge_actor",
            ActorType::EngineActor => "engine_actor",
            ActorType::SyncActor => "sync_actor",
            ActorType::NetworkActor => "network_actor",
            ActorType::StorageActor => "storage_actor",
            ActorType::SupervisorActor => "supervisor_actor",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "stream_actor" => Some(ActorType::StreamActor),
            "chain_actor" => Some(ActorType::ChainActor),
            "bridge_actor" => Some(ActorType::BridgeActor),
            "engine_actor" => Some(ActorType::EngineActor),
            "sync_actor" => Some(ActorType::SyncActor),
            "network_actor" => Some(ActorType::NetworkActor),
            "storage_actor" => Some(ActorType::StorageActor),
            "supervisor_actor" => Some(ActorType::SupervisorActor),
            _ => None,
        }
    }
}

/// Message flow tracking entry
#[derive(Debug, Clone)]
pub struct MessageFlowEntry {
    pub correlation_id: String,
    pub from_actor: ActorType,
    pub to_actor: ActorType,
    pub message_type: String,
    pub sent_at: Instant,
    pub timeout: Duration,
    pub hops: Vec<ActorType>, // For tracking message routing
}

/// Actor dependency relationship
#[derive(Debug, Clone)]
pub struct ActorDependency {
    pub dependent: ActorType,
    pub dependency: ActorType,
    pub dependency_type: DependencyType,
    pub health_score: f64,
    pub last_health_check: Instant,
    pub response_times: VecDeque<Duration>,
    pub circuit_breaker_state: CircuitBreakerState,
}

/// Types of dependencies between actors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyType {
    DirectMessage,
    SharedResource,
    DataFlow,
    LifecycleCoordination,
    EventSubscription,
}

impl DependencyType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DependencyType::DirectMessage => "direct_message",
            DependencyType::SharedResource => "shared_resource",
            DependencyType::DataFlow => "data_flow",
            DependencyType::LifecycleCoordination => "lifecycle_coordination",
            DependencyType::EventSubscription => "event_subscription",
        }
    }
}

/// Circuit breaker states for actor dependencies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerState {
    Closed = 0,
    Open = 1,
    HalfOpen = 2,
}

impl CircuitBreakerState {
    pub fn as_str(&self) -> &'static str {
        match self {
            CircuitBreakerState::Closed => "closed",
            CircuitBreakerState::Open => "open",
            CircuitBreakerState::HalfOpen => "half_open",
        }
    }
    
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }
}

/// Actor lifecycle states
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActorLifecycleState {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
    Restarting,
}

impl ActorLifecycleState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorLifecycleState::Starting => "starting",
            ActorLifecycleState::Running => "running",
            ActorLifecycleState::Stopping => "stopping",
            ActorLifecycleState::Stopped => "stopped",
            ActorLifecycleState::Failed => "failed",
            ActorLifecycleState::Restarting => "restarting",
        }
    }
}

/// Communication patterns between actors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunicationPattern {
    RequestResponse,
    PublishSubscribe,
    Pipeline,
    Broadcast,
    Aggregation,
    Scatter,
    CircularDependency,
}

impl CommunicationPattern {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommunicationPattern::RequestResponse => "request_response",
            CommunicationPattern::PublishSubscribe => "publish_subscribe",
            CommunicationPattern::Pipeline => "pipeline",
            CommunicationPattern::Broadcast => "broadcast",
            CommunicationPattern::Aggregation => "aggregation",
            CommunicationPattern::Scatter => "scatter",
            CommunicationPattern::CircularDependency => "circular_dependency",
        }
    }
}

/// Inter-Actor Communication Metrics Collector
pub struct InterActorMetricsCollector {
    /// Active message flow tracking
    message_flows: Arc<RwLock<HashMap<String, MessageFlowEntry>>>,
    /// Actor dependency relationships
    dependencies: Arc<RwLock<HashMap<String, ActorDependency>>>,
    /// Message queue size tracking
    queue_sizes: Arc<RwLock<HashMap<String, usize>>>,
    /// Communication pattern detection
    pattern_history: Arc<RwLock<VecDeque<(ActorType, ActorType, String, Instant)>>>,
    /// Deadlock detection data
    pending_requests: Arc<RwLock<HashMap<String, (ActorType, ActorType, Instant)>>>,
    /// Cleanup interval
    cleanup_interval: Duration,
}

impl InterActorMetricsCollector {
    /// Create a new inter-actor metrics collector
    pub fn new() -> Self {
        Self {
            message_flows: Arc::new(RwLock::new(HashMap::new())),
            dependencies: Arc::new(RwLock::new(HashMap::new())),
            queue_sizes: Arc::new(RwLock::new(HashMap::new())),
            pattern_history: Arc::new(RwLock::new(VecDeque::with_capacity(10000))),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            cleanup_interval: Duration::from_secs(300), // 5 minutes
        }
    }
    
    /// Record message sent between actors
    pub fn record_message_sent(
        &self,
        from_actor: ActorType,
        to_actor: ActorType,
        message_type: &str,
        correlation_id: Option<String>,
        timeout: Option<Duration>
    ) -> String {
        let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let timeout = timeout.unwrap_or(Duration::from_secs(30));
        
        let flow_entry = MessageFlowEntry {
            correlation_id: correlation_id.clone(),
            from_actor: from_actor.clone(),
            to_actor: to_actor.clone(),
            message_type: message_type.to_string(),
            sent_at: Instant::now(),
            timeout,
            hops: vec![from_actor.clone()],
        };
        
        self.message_flows.write().insert(correlation_id.clone(), flow_entry);
        
        // Record metrics
        let from_str = from_actor.as_str();
        let to_str = to_actor.as_str();
        let sanitized_message_type = MetricLabels::sanitize_label_value(message_type);
        
        INTER_ACTOR_MESSAGE_COUNT
            .with_label_values(&[from_str, to_str, &sanitized_message_type, "sent"])
            .inc();
        
        // Update communication pattern history
        {
            let mut pattern_history = self.pattern_history.write();
            pattern_history.push_back((from_actor.clone(), to_actor.clone(), message_type.to_string(), Instant::now()));
            
            // Keep only recent entries (last 10000)
            if pattern_history.len() > 10000 {
                pattern_history.pop_front();
            }
        }
        
        // Track pending request for deadlock detection
        self.pending_requests.write().insert(
            correlation_id.clone(),
            (from_actor, to_actor, Instant::now())
        );
        
        trace!(
            correlation_id = %correlation_id,
            from_actor = from_str,
            to_actor = to_str,
            message_type = message_type,
            "Inter-actor message sent"
        );
        
        correlation_id
    }
    
    /// Record message received and calculate latency
    pub fn record_message_received(
        &self,
        correlation_id: &str,
        success: bool,
        error_type: Option<&str>
    ) -> Option<Duration> {
        let flow_entry = self.message_flows.write().remove(correlation_id)?;
        let latency = flow_entry.sent_at.elapsed();
        
        let from_str = flow_entry.from_actor.as_str();
        let to_str = flow_entry.to_actor.as_str();
        let sanitized_message_type = MetricLabels::sanitize_label_value(&flow_entry.message_type);
        
        if success {
            // Record successful message
            INTER_ACTOR_MESSAGE_COUNT
                .with_label_values(&[from_str, to_str, &sanitized_message_type, "received"])
                .inc();
            
            INTER_ACTOR_MESSAGE_LATENCY
                .with_label_values(&[from_str, to_str, &sanitized_message_type])
                .observe(latency.as_secs_f64());
        } else {
            // Record error
            let sanitized_error_type = error_type
                .map(|e| MetricLabels::sanitize_label_value(e))
                .unwrap_or_else(|| "unknown".to_string());
            
            INTER_ACTOR_MESSAGE_ERRORS
                .with_label_values(&[from_str, to_str, &sanitized_message_type, &sanitized_error_type])
                .inc();
        }
        
        // Remove from pending requests
        self.pending_requests.write().remove(correlation_id);
        
        // Update dependency health
        self.update_dependency_health(&flow_entry.from_actor, &flow_entry.to_actor, latency, success);
        
        debug!(
            correlation_id = correlation_id,
            from_actor = from_str,
            to_actor = to_str,
            message_type = flow_entry.message_type,
            latency_ms = latency.as_millis(),
            success = success,
            "Inter-actor message received"
        );
        
        Some(latency)
    }
    
    /// Update message queue size between actors
    pub fn update_message_queue_size(&self, from_actor: ActorType, to_actor: ActorType, size: usize) {
        let queue_key = format!("{}_{}", from_actor.as_str(), to_actor.as_str());
        self.queue_sizes.write().insert(queue_key.clone(), size);
        
        let from_str = from_actor.as_str();
        let to_str = to_actor.as_str();
        
        INTER_ACTOR_MESSAGE_QUEUE_SIZE
            .with_label_values(&[from_str, to_str])
            .set(size as i64);
        
        if size > 1000 {
            warn!(
                from_actor = from_str,
                to_actor = to_str,
                queue_size = size,
                "High inter-actor message queue size detected"
            );
        }
    }
    
    /// Register actor dependency
    pub fn register_dependency(
        &self,
        dependent: ActorType,
        dependency: ActorType,
        dependency_type: DependencyType
    ) {
        let dependency_key = format!("{}_{}", dependent.as_str(), dependency.as_str());
        
        let dependency_entry = ActorDependency {
            dependent: dependent.clone(),
            dependency: dependency.clone(),
            dependency_type: dependency_type.clone(),
            health_score: 1.0, // Start healthy
            last_health_check: Instant::now(),
            response_times: VecDeque::with_capacity(100),
            circuit_breaker_state: CircuitBreakerState::Closed,
        };
        
        self.dependencies.write().insert(dependency_key, dependency_entry);
        
        // Record initial health
        ACTOR_DEPENDENCY_HEALTH
            .with_label_values(&[dependent.as_str(), dependency.as_str(), dependency_type.as_str()])
            .set(1.0);
        
        ACTOR_CIRCUIT_BREAKER_STATE
            .with_label_values(&[dependent.as_str(), dependency.as_str()])
            .set(CircuitBreakerState::Closed.as_i64());
        
        info!(
            dependent = dependent.as_str(),
            dependency = dependency.as_str(),
            dependency_type = dependency_type.as_str(),
            "Actor dependency registered"
        );
    }
    
    /// Update dependency health based on interaction results
    fn update_dependency_health(
        &self,
        dependent: &ActorType,
        dependency: &ActorType,
        response_time: Duration,
        success: bool
    ) {
        let dependency_key = format!("{}_{}", dependent.as_str(), dependency.as_str());
        let mut dependencies = self.dependencies.write();
        
        if let Some(dep_entry) = dependencies.get_mut(&dependency_key) {
            // Update response times
            dep_entry.response_times.push_back(response_time);
            if dep_entry.response_times.len() > 100 {
                dep_entry.response_times.pop_front();
            }
            
            // Calculate health score
            let mut health_score = if success { 1.0 } else { 0.0 };
            
            // Factor in response time (penalty for slow responses)
            if response_time > Duration::from_millis(100) {
                health_score *= 0.8;
            }
            if response_time > Duration::from_secs(1) {
                health_score *= 0.5;
            }
            
            // Exponential moving average for health score
            dep_entry.health_score = 0.9 * dep_entry.health_score + 0.1 * health_score;
            dep_entry.last_health_check = Instant::now();
            
            // Update circuit breaker state
            let new_circuit_state = if dep_entry.health_score < 0.3 {
                CircuitBreakerState::Open
            } else if dep_entry.health_score < 0.7 && dep_entry.circuit_breaker_state == CircuitBreakerState::Open {
                CircuitBreakerState::HalfOpen
            } else if dep_entry.health_score > 0.8 {
                CircuitBreakerState::Closed
            } else {
                dep_entry.circuit_breaker_state
            };
            
            if new_circuit_state != dep_entry.circuit_breaker_state {
                info!(
                    dependent = dependent.as_str(),
                    dependency = dependency.as_str(),
                    old_state = dep_entry.circuit_breaker_state.as_str(),
                    new_state = new_circuit_state.as_str(),
                    health_score = %format!("{:.3}", dep_entry.health_score),
                    "Circuit breaker state changed"
                );
                dep_entry.circuit_breaker_state = new_circuit_state;
            }
            
            // Update metrics
            ACTOR_DEPENDENCY_HEALTH
                .with_label_values(&[dependent.as_str(), dependency.as_str(), dep_entry.dependency_type.as_str()])
                .set(dep_entry.health_score);
            
            ACTOR_CIRCUIT_BREAKER_STATE
                .with_label_values(&[dependent.as_str(), dependency.as_str()])
                .set(new_circuit_state.as_i64());
            
            ACTOR_DEPENDENCY_RESPONSE_TIME
                .with_label_values(&[dependent.as_str(), dependency.as_str(), "interaction"])
                .observe(response_time.as_secs_f64());
        }
    }
    
    /// Record actor lifecycle transition
    pub fn record_lifecycle_transition(
        &self,
        actor: ActorType,
        from_state: ActorLifecycleState,
        to_state: ActorLifecycleState,
        reason: &str
    ) {
        let sanitized_reason = MetricLabels::sanitize_label_value(reason);
        
        ACTOR_LIFECYCLE_TRANSITIONS
            .with_label_values(&[
                actor.as_str(),
                from_state.as_str(),
                to_state.as_str(),
                &sanitized_reason
            ])
            .inc();
        
        info!(
            actor = actor.as_str(),
            from_state = from_state.as_str(),
            to_state = to_state.as_str(),
            reason = reason,
            "Actor lifecycle transition recorded"
        );
    }
    
    /// Record actor startup time
    pub fn record_actor_startup(&self, actor_type: ActorType, startup_phase: &str, duration: Duration) {
        let sanitized_startup_phase = MetricLabels::sanitize_label_value(startup_phase);
        
        ACTOR_STARTUP_TIME
            .with_label_values(&[actor_type.as_str(), &sanitized_startup_phase])
            .observe(duration.as_secs_f64());
        
        info!(
            actor_type = actor_type.as_str(),
            startup_phase = startup_phase,
            duration_ms = duration.as_millis(),
            "Actor startup time recorded"
        );
    }
    
    /// Record actor shutdown time
    pub fn record_actor_shutdown(&self, actor_type: ActorType, reason: &str, duration: Duration) {
        let sanitized_reason = MetricLabels::sanitize_label_value(reason);
        
        ACTOR_SHUTDOWN_TIME
            .with_label_values(&[actor_type.as_str(), &sanitized_reason])
            .observe(duration.as_secs_f64());
        
        info!(
            actor_type = actor_type.as_str(),
            shutdown_reason = reason,
            duration_ms = duration.as_millis(),
            "Actor shutdown time recorded"
        );
    }
    
    /// Record supervision tree restart
    pub fn record_supervision_restart(
        &self,
        supervisor: &str,
        child_actor: ActorType,
        restart_reason: &str
    ) {
        let sanitized_supervisor = MetricLabels::sanitize_label_value(supervisor);
        let sanitized_restart_reason = MetricLabels::sanitize_label_value(restart_reason);
        
        SUPERVISION_TREE_RESTARTS
            .with_label_values(&[&sanitized_supervisor, child_actor.as_str(), &sanitized_restart_reason])
            .inc();
        
        warn!(
            supervisor = supervisor,
            child_actor = child_actor.as_str(),
            restart_reason = restart_reason,
            "Supervision tree restart recorded"
        );
    }
    
    /// Record supervision escalation
    pub fn record_supervision_escalation(
        &self,
        supervisor: &str,
        escalation_type: &str,
        child_actor: ActorType
    ) {
        let sanitized_supervisor = MetricLabels::sanitize_label_value(supervisor);
        let sanitized_escalation_type = MetricLabels::sanitize_label_value(escalation_type);
        
        SUPERVISION_ESCALATION_EVENTS
            .with_label_values(&[&sanitized_supervisor, &sanitized_escalation_type, child_actor.as_str()])
            .inc();
        
        error!(
            supervisor = supervisor,
            escalation_type = escalation_type,
            child_actor = child_actor.as_str(),
            "Supervision escalation recorded"
        );
    }
    
    /// Detect potential deadlocks based on timeout patterns
    pub async fn detect_deadlocks(&self) {
        let now = Instant::now();
        let mut potential_deadlocks = Vec::new();
        
        {
            let pending_requests = self.pending_requests.read();
            
            // Look for requests that have been pending too long
            for (correlation_id, (from_actor, to_actor, sent_at)) in pending_requests.iter() {
                let age = now.duration_since(*sent_at);
                if age > Duration::from_secs(60) { // 1 minute timeout threshold
                    potential_deadlocks.push((
                        correlation_id.clone(),
                        from_actor.clone(),
                        to_actor.clone(),
                        age
                    ));
                }
            }
        }
        
        for (correlation_id, from_actor, to_actor, age) in potential_deadlocks {
            // Record timeout event
            ACTOR_MESSAGE_TIMEOUT_EVENTS
                .with_label_values(&[from_actor.as_str(), to_actor.as_str(), "unknown"])
                .inc();
            
            // Check for circular dependencies
            if self.detect_circular_dependency(&from_actor, &to_actor) {
                let actors_involved = format!("{}_{}", from_actor.as_str(), to_actor.as_str());
                
                ACTOR_DEADLOCK_DETECTIONS
                    .with_label_values(&["circular_dependency", &actors_involved])
                    .inc();
                
                error!(
                    correlation_id = %correlation_id,
                    from_actor = from_actor.as_str(),
                    to_actor = to_actor.as_str(),
                    age_secs = age.as_secs(),
                    "Potential circular dependency deadlock detected"
                );
            } else {
                warn!(
                    correlation_id = %correlation_id,
                    from_actor = from_actor.as_str(),
                    to_actor = to_actor.as_str(),
                    age_secs = age.as_secs(),
                    "Message timeout detected (potential deadlock)"
                );
            }
        }
    }
    
    /// Detect circular dependency between actors
    fn detect_circular_dependency(&self, actor_a: &ActorType, actor_b: &ActorType) -> bool {
        let dependencies = self.dependencies.read();
        
        // Simple circular dependency detection: A depends on B and B depends on A
        let a_to_b_key = format!("{}_{}", actor_a.as_str(), actor_b.as_str());
        let b_to_a_key = format!("{}_{}", actor_b.as_str(), actor_a.as_str());
        
        dependencies.contains_key(&a_to_b_key) && dependencies.contains_key(&b_to_a_key)
    }
    
    /// Analyze communication patterns
    pub fn analyze_communication_patterns(&self) {
        let pattern_history = self.pattern_history.read();
        let mut pattern_counts: HashMap<String, u32> = HashMap::new();
        
        // Analyze patterns in the last 5 minutes
        let threshold = Instant::now() - Duration::from_secs(300);
        
        for (from_actor, to_actor, message_type, timestamp) in pattern_history.iter() {
            if *timestamp > threshold {
                let pattern_key = format!("{}->{}:{}", from_actor.as_str(), to_actor.as_str(), message_type);
                *pattern_counts.entry(pattern_key).or_insert(0) += 1;
            }
        }
        
        // Classify patterns
        for (pattern_key, count) in pattern_counts.iter() {
            if *count > 100 {
                // High frequency communication
                ACTOR_COMMUNICATION_PATTERNS
                    .with_label_values(&["high_frequency", pattern_key])
                    .inc_by(*count as u64);
            } else if *count > 10 {
                // Normal frequency
                ACTOR_COMMUNICATION_PATTERNS
                    .with_label_values(&["normal_frequency", pattern_key])
                    .inc_by(*count as u64);
            }
        }
    }
    
    /// Start periodic analysis and cleanup
    pub async fn start_periodic_tasks(&self) -> tokio::task::JoinHandle<()> {
        let collector = Arc::new(self.clone());
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            
            info!("Starting inter-actor metrics periodic tasks");
            
            loop {
                interval.tick().await;
                
                let task_start = Instant::now();
                
                // Detect deadlocks
                collector.detect_deadlocks().await;
                
                // Analyze communication patterns
                collector.analyze_communication_patterns();
                
                // Cleanup expired data
                collector.cleanup_expired_data().await;
                
                let task_duration = task_start.elapsed();
                
                trace!(
                    task_duration_ms = task_duration.as_millis(),
                    "Inter-actor metrics periodic tasks completed"
                );
            }
        })
    }
    
    /// Cleanup expired tracking data
    async fn cleanup_expired_data(&self) {
        let now = Instant::now();
        let mut expired_flows = Vec::new();
        let mut expired_requests = Vec::new();
        
        // Clean up expired message flows
        {
            let message_flows = self.message_flows.read();
            for (correlation_id, flow) in message_flows.iter() {
                if now.duration_since(flow.sent_at) > flow.timeout {
                    expired_flows.push(correlation_id.clone());
                }
            }
        }
        
        // Clean up expired pending requests
        {
            let pending_requests = self.pending_requests.read();
            for (correlation_id, (_, _, sent_at)) in pending_requests.iter() {
                if now.duration_since(*sent_at) > Duration::from_secs(300) { // 5 minutes
                    expired_requests.push(correlation_id.clone());
                }
            }
        }
        
        // Remove expired entries
        if !expired_flows.is_empty() {
            let mut message_flows = self.message_flows.write();
            for correlation_id in &expired_flows {
                message_flows.remove(correlation_id);
            }
        }
        
        if !expired_requests.is_empty() {
            let mut pending_requests = self.pending_requests.write();
            for correlation_id in &expired_requests {
                pending_requests.remove(correlation_id);
            }
        }
        
        if !expired_flows.is_empty() || !expired_requests.is_empty() {
            debug!(
                expired_flows = expired_flows.len(),
                expired_requests = expired_requests.len(),
                "Cleaned up expired inter-actor tracking data"
            );
        }
    }
    
    /// Get comprehensive metrics summary
    pub fn get_metrics_summary(&self) -> serde_json::Value {
        let message_flows = self.message_flows.read();
        let dependencies = self.dependencies.read();
        let queue_sizes = self.queue_sizes.read();
        let pending_requests = self.pending_requests.read();
        
        let mut dependency_health = serde_json::Map::new();
        for (key, dep) in dependencies.iter() {
            dependency_health.insert(key.clone(), serde_json::json!({
                "health_score": dep.health_score,
                "circuit_breaker_state": dep.circuit_breaker_state.as_str(),
                "avg_response_time_ms": if !dep.response_times.is_empty() {
                    dep.response_times.iter().map(|d| d.as_millis()).sum::<u128>() as f64 / dep.response_times.len() as f64
                } else {
                    0.0
                },
                "dependency_type": dep.dependency_type.as_str()
            }));
        }
        
        serde_json::json!({
            "active_message_flows": message_flows.len(),
            "registered_dependencies": dependencies.len(),
            "tracked_queue_sizes": queue_sizes.len(),
            "pending_requests": pending_requests.len(),
            "cleanup_interval_secs": self.cleanup_interval.as_secs(),
            "dependency_health": dependency_health
        })
    }
}

impl Clone for InterActorMetricsCollector {
    fn clone(&self) -> Self {
        Self {
            message_flows: self.message_flows.clone(),
            dependencies: self.dependencies.clone(),
            queue_sizes: self.queue_sizes.clone(),
            pattern_history: self.pattern_history.clone(),
            pending_requests: self.pending_requests.clone(),
            cleanup_interval: self.cleanup_interval,
        }
    }
}

impl Default for InterActorMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize inter-actor communication metrics
pub fn initialize_inter_actor_metrics() -> Result<(), Box<dyn std::error::Error>> {
    info!("Initializing inter-actor communication metrics");
    
    // Test metric registration
    let _test_access = [
        INTER_ACTOR_MESSAGE_LATENCY.clone(),
        ACTOR_DEPENDENCY_HEALTH.clone(),
        SUPERVISION_TREE_RESTARTS.clone(),
        ACTOR_LIFECYCLE_TRANSITIONS.clone(),
    ];
    
    info!("Inter-actor communication metrics initialization completed");
    info!("Available metrics: Message Latency, Dependency Health, Supervision, Lifecycle, Deadlock Detection");
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    
    #[test]
    fn test_actor_type_conversions() {
        assert_eq!(ActorType::StreamActor.as_str(), "stream_actor");
        assert_eq!(ActorType::from_str("chain_actor"), Some(ActorType::ChainActor));
        assert_eq!(ActorType::from_str("unknown"), None);
    }
    
    #[test]
    fn test_circuit_breaker_state() {
        assert_eq!(CircuitBreakerState::Closed.as_i64(), 0);
        assert_eq!(CircuitBreakerState::Open.as_i64(), 1);
        assert_eq!(CircuitBreakerState::HalfOpen.as_i64(), 2);
    }
    
    #[tokio::test]
    async fn test_message_flow_tracking() {
        let collector = InterActorMetricsCollector::new();
        
        let correlation_id = collector.record_message_sent(
            ActorType::StreamActor,
            ActorType::ChainActor,
            "block_proposal",
            None,
            None
        );
        
        // Verify message is being tracked
        assert_eq!(collector.message_flows.read().len(), 1);
        assert_eq!(collector.pending_requests.read().len(), 1);
        
        // Simulate processing time
        sleep(Duration::from_millis(10)).await;
        
        // Record message received
        let latency = collector.record_message_received(&correlation_id, true, None);
        assert!(latency.is_some());
        assert!(latency.unwrap() >= Duration::from_millis(10));
        
        // Verify cleanup
        assert_eq!(collector.message_flows.read().len(), 0);
        assert_eq!(collector.pending_requests.read().len(), 0);
    }
    
    #[test]
    fn test_dependency_registration() {
        let collector = InterActorMetricsCollector::new();
        
        collector.register_dependency(
            ActorType::StreamActor,
            ActorType::ChainActor,
            DependencyType::DirectMessage
        );
        
        assert_eq!(collector.dependencies.read().len(), 1);
        
        let dependency_key = format!("{}_{}", ActorType::StreamActor.as_str(), ActorType::ChainActor.as_str());
        let dependency = collector.dependencies.read();
        let dep_entry = dependency.get(&dependency_key).unwrap();
        
        assert_eq!(dep_entry.health_score, 1.0);
        assert_eq!(dep_entry.circuit_breaker_state, CircuitBreakerState::Closed);
    }
    
    #[tokio::test]
    async fn test_deadlock_detection() {
        let collector = InterActorMetricsCollector::new();
        
        // Register circular dependency
        collector.register_dependency(
            ActorType::StreamActor,
            ActorType::ChainActor,
            DependencyType::DirectMessage
        );
        collector.register_dependency(
            ActorType::ChainActor,
            ActorType::StreamActor,
            DependencyType::DirectMessage
        );
        
        // Verify circular dependency detection
        assert!(collector.detect_circular_dependency(&ActorType::StreamActor, &ActorType::ChainActor));
    }
}