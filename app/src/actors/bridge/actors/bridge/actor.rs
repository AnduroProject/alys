//! Bridge Coordinator Actor Implementation
//! 
//! Orchestrates peg-in and peg-out operations across specialized actors

use actix::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tracing::{info, warn, error, debug};

use crate::actors::bridge::{
    config::BridgeConfig,
    messages::*,
    shared::errors::BridgeError,
};
use crate::types::*;
use super::metrics::*;
use super::state::{*, BridgeState};
use actor_system::metrics::ActorMetrics;

/// Bridge coordinator actor that manages the bridge system
pub struct BridgeActor {
    /// Configuration
    pub config: BridgeConfig,

    /// System state
    pub state: BridgeState,

    /// Actor registry for named actors
    pub actor_registry: ActorRegistry,

    /// Child actor addresses (backward compatibility)
    pub child_actors: ChildActors,

    /// Operation registry
    pub active_operations: HashMap<String, OperationContext>,
    
    /// System metrics
    pub metrics: BridgeCoordinationMetrics,
    
    /// Actor system metrics (for AlysActor compatibility)
    pub actor_system_metrics: ActorMetrics,
    
    /// Health monitor
    pub health_monitor: ActorHealthMonitor,
    
    /// System startup time
    pub started_at: SystemTime,
}

/// Actor registry for named actor management
#[derive(Debug, Default)]
pub struct ActorRegistry {
    pegin_actors: std::collections::HashMap<String, Addr<super::super::pegin::PegInActor>>,
    pegout_actors: std::collections::HashMap<String, Addr<super::super::pegout::PegOutActor>>,
    stream_actors: std::collections::HashMap<String, Addr<super::super::stream::StreamActor>>,
}

impl ActorRegistry {
    /// Register a PegIn actor with an identifier
    pub fn register_pegin(&mut self, id: String, addr: Addr<super::super::pegin::PegInActor>) {
        info!("Registering PegIn actor with ID: {}", id);
        self.pegin_actors.insert(id, addr);
    }

    /// Register a PegOut actor with an identifier
    pub fn register_pegout(&mut self, id: String, addr: Addr<super::super::pegout::PegOutActor>) {
        info!("Registering PegOut actor with ID: {}", id);
        self.pegout_actors.insert(id, addr);
    }

    /// Register a Stream actor with an identifier
    pub fn register_stream(&mut self, id: String, addr: Addr<super::super::stream::StreamActor>) {
        info!("Registering Stream actor with ID: {}", id);
        self.stream_actors.insert(id, addr);
    }

    /// Get a PegIn actor by ID
    pub fn get_pegin(&self, id: &str) -> Option<&Addr<super::super::pegin::PegInActor>> {
        self.pegin_actors.get(id)
    }

    /// Get a PegOut actor by ID
    pub fn get_pegout(&self, id: &str) -> Option<&Addr<super::super::pegout::PegOutActor>> {
        self.pegout_actors.get(id)
    }

    /// Get a Stream actor by ID
    pub fn get_stream(&self, id: &str) -> Option<&Addr<super::super::stream::StreamActor>> {
        self.stream_actors.get(id)
    }

    /// Get primary actors for backward compatibility
    pub fn get_primary_pegin(&self) -> Option<&Addr<super::super::pegin::PegInActor>> {
        self.pegin_actors.get("primary")
    }

    pub fn get_primary_pegout(&self) -> Option<&Addr<super::super::pegout::PegOutActor>> {
        self.pegout_actors.get("primary")
    }

    pub fn get_primary_stream(&self) -> Option<&Addr<super::super::stream::StreamActor>> {
        self.stream_actors.get("primary")
    }

    /// Get count of registered actors
    pub fn get_registered_count(&self) -> u32 {
        (self.pegin_actors.len() + self.pegout_actors.len() + self.stream_actors.len()) as u32
    }
}

/// Child actor addresses - kept for backward compatibility
#[derive(Debug, Default)]
pub struct ChildActors {
    pub pegin_actor: Option<Addr<super::super::pegin::PegInActor>>,
    pub pegout_actor: Option<Addr<super::super::pegout::PegOutActor>>,
    pub stream_actor: Option<Addr<super::super::stream::StreamActor>>,
}

impl ChildActors {
    /// Get count of registered actors
    pub fn get_registered_count(&self) -> u32 {
        let mut count = 0;
        if self.pegin_actor.is_some() { count += 1; }
        if self.pegout_actor.is_some() { count += 1; }
        if self.stream_actor.is_some() { count += 1; }
        count
    }

    /// Update from registry for backward compatibility
    pub fn sync_with_registry(&mut self, registry: &ActorRegistry) {
        self.pegin_actor = registry.get_primary_pegin().cloned();
        self.pegout_actor = registry.get_primary_pegout().cloned();
        self.stream_actor = registry.get_primary_stream().cloned();
    }
}

/// Operation context for tracking
#[derive(Debug, Clone)]
pub struct OperationContext {
    pub operation_id: String,
    pub operation_type: OperationType,
    pub status: OperationState,
    pub created_at: SystemTime,
    pub last_updated: SystemTime,
    pub assigned_actor: Option<String>,
    pub retry_count: u32,
    pub metadata: OperationMetadata,
}

/// Operation metadata
#[derive(Debug, Clone, Default)]
pub struct OperationMetadata {
    pub bitcoin_txid: Option<bitcoin::Txid>,
    pub alys_tx_hash: Option<H256>,
    pub amount: Option<u64>,
    pub requester: Option<H160>,
    pub destination: Option<String>,
}

impl BridgeActor {
    /// Create new bridge coordinator actor
    pub fn new(config: BridgeConfig) -> Result<Self, BridgeError> {
        let metrics = BridgeCoordinationMetrics::new()
            .map_err(|e| BridgeError::InternalError(format!("Failed to initialize metrics: {}", e)))?;
        let health_monitor = ActorHealthMonitor::new(config.health_check_interval);
        let actor_system_metrics = ActorMetrics::new();
        
        Ok(Self {
            config,
            state: BridgeState::Initializing,
            actor_registry: ActorRegistry::default(),
            child_actors: ChildActors::default(),
            active_operations: HashMap::new(),
            metrics,
            actor_system_metrics,
            health_monitor,
            started_at: SystemTime::now(),
        })
    }

    /// Initialize bridge system
    async fn initialize_system(&mut self, ctx: &mut Context<Self>) -> Result<(), BridgeError> {
        info!("Initializing bridge coordination system");
        
        // Start health monitoring
        self.start_health_monitoring(ctx);
        
        // Start metrics collection
        self.start_metrics_collection(ctx);
        
        // Update state
        self.state = BridgeState::Running;
        self.metrics.record_system_start();
        
        info!("Bridge coordination system initialized successfully");
        Ok(())
    }

    /// Register child actors
    fn register_child_actors(&mut self) {
        info!("Registering child actors with coordinator");
        
        // Child actors will register themselves via messages
        // This method sets up the registration handlers
    }

    /// Start a new peg-in operation
    pub async fn start_pegin_operation(
        &mut self,
        pegin_id: String,
        bitcoin_txid: bitcoin::Txid,
    ) -> Result<(), BridgeError> {
        info!("Starting peg-in operation {} for txid {}", pegin_id, bitcoin_txid);
        
        // Create operation context
        let operation = OperationContext {
            operation_id: pegin_id.clone(),
            operation_type: OperationType::PegIn,
            status: OperationState::Initiated,
            created_at: SystemTime::now(),
            last_updated: SystemTime::now(),
            assigned_actor: Some("pegin_actor".to_string()),
            retry_count: 0,
            metadata: OperationMetadata {
                bitcoin_txid: Some(bitcoin_txid),
                ..Default::default()
            },
        };

        // Store operation
        self.active_operations.insert(pegin_id.clone(), operation);

        // Forward to PegInActor
        if let Some(pegin_actor) = &self.child_actors.pegin_actor {
            let msg = PegInMessage::ProcessDeposit {
                txid: bitcoin_txid,
                bitcoin_tx: bitcoin::Transaction {
                    version: 1,
                    lock_time: bitcoin::absolute::LockTime::ZERO,
                    input: vec![],
                    output: vec![],
                }, // Will be fetched by PegInActor
                block_height: 0, // Will be determined by PegInActor
            };
            
            match pegin_actor.send(msg).await {
                Ok(Ok(_)) => {
                    self.metrics.record_operation_started(OperationType::PegIn);
                    info!("Peg-in operation {} forwarded to PegInActor", pegin_id);
                }
                Ok(Err(e)) => {
                    error!("PegInActor returned error for operation {}: {:?}", pegin_id, e);
                    self.update_operation_status(pegin_id, OperationState::Failed { 
                        reason: format!("PegInActor error: {:?}", e) 
                    });
                }
                Err(e) => {
                    error!("Failed to send message to PegInActor for operation {}: {:?}", pegin_id, e);
                    self.update_operation_status(pegin_id, OperationState::Failed { 
                        reason: format!("Message send error: {:?}", e) 
                    });
                }
            }
        } else {
            error!("PegInActor not registered for operation {}", pegin_id);
            return Err(BridgeError::ActorSystemError("PegInActor not available".to_string()));
        }

        Ok(())
    }

    /// Start a new peg-out operation
    pub async fn start_pegout_operation(
        &mut self,
        pegout_id: String,
        burn_tx_hash: H256,
    ) -> Result<(), BridgeError> {
        info!("Starting peg-out operation {} for burn tx {}", pegout_id, burn_tx_hash);
        
        // Create operation context
        let operation = OperationContext {
            operation_id: pegout_id.clone(),
            operation_type: OperationType::PegOut,
            status: OperationState::Initiated,
            created_at: SystemTime::now(),
            last_updated: SystemTime::now(),
            assigned_actor: Some("pegout_actor".to_string()),
            retry_count: 0,
            metadata: OperationMetadata {
                alys_tx_hash: Some(burn_tx_hash),
                ..Default::default()
            },
        };

        // Store operation
        self.active_operations.insert(pegout_id.clone(), operation);

        // Forward to PegOutActor
        if let Some(pegout_actor) = &self.child_actors.pegout_actor {
            let msg = PegOutMessage::ProcessBurnEvent {
                burn_tx: burn_tx_hash,
                destination: "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4".to_string(), // Placeholder
                amount: 100_000_000, // Placeholder
                requester: H160::zero(), // Placeholder
            };
            
            match pegout_actor.send(msg).await {
                Ok(Ok(_)) => {
                    self.metrics.record_operation_started(OperationType::PegOut);
                    info!("Peg-out operation {} forwarded to PegOutActor", pegout_id);
                }
                Ok(Err(e)) => {
                    error!("PegOutActor returned error for operation {}: {:?}", pegout_id, e);
                    self.update_operation_status(pegout_id, OperationState::Failed { 
                        reason: format!("PegOutActor error: {:?}", e) 
                    });
                }
                Err(e) => {
                    error!("Failed to send message to PegOutActor for operation {}: {:?}", pegout_id, e);
                    self.update_operation_status(pegout_id, OperationState::Failed { 
                        reason: format!("Message send error: {:?}", e) 
                    });
                }
            }
        } else {
            error!("PegOutActor not registered for operation {}", pegout_id);
            return Err(BridgeError::ActorSystemError("PegOutActor not available".to_string()));
        }

        Ok(())
    }

    /// Update operation status
    pub fn update_operation_status(&mut self, operation_id: String, status: OperationState) {
        if let Some(operation) = self.active_operations.get_mut(&operation_id) {
            let old_status = operation.status.clone();
            operation.status = status.clone();
            operation.last_updated = SystemTime::now();
            
            // Record metrics
            self.metrics.record_operation_status_change(&operation.operation_type, &old_status, &status);
            
            // Log status change
            debug!("Operation {} status changed: {:?} -> {:?}", operation_id, old_status, status);
            
            // Handle completion
            if matches!(status, OperationState::Completed | OperationState::Failed { .. }) {
                self.metrics.record_operation_completed(&operation.operation_type, matches!(status, OperationState::Completed));
            }
        } else {
            warn!("Attempted to update status for unknown operation: {}", operation_id);
        }
    }

    /// Get system status
    pub fn get_system_status(&self) -> BridgeSystemStatus {
        let registered_actors = ActorStatusRegistry {
            pegin_actor: self.child_actors.pegin_actor.as_ref().map(|_| ActorInfo {
                actor_type: ActorType::PegIn,
                status: ActorStatus::Running,
                registered_at: self.started_at,
                last_heartbeat: SystemTime::now(),
                message_count: 0, // Would be tracked in practice
            }),
            pegout_actor: self.child_actors.pegout_actor.as_ref().map(|_| ActorInfo {
                actor_type: ActorType::PegOut,
                status: ActorStatus::Running,
                registered_at: self.started_at,
                last_heartbeat: SystemTime::now(),
                message_count: 0,
            }),
            stream_actor: self.child_actors.stream_actor.as_ref().map(|_| ActorInfo {
                actor_type: ActorType::Stream,
                status: ActorStatus::Running,
                registered_at: self.started_at,
                last_heartbeat: SystemTime::now(),
                message_count: 0,
            }),
        };

        let system_health = if self.child_actors.pegin_actor.is_some() 
            && self.child_actors.pegout_actor.is_some() 
            && self.child_actors.stream_actor.is_some() {
            SystemHealthStatus::Healthy
        } else {
            SystemHealthStatus::Degraded { 
                issues: vec!["Some child actors not registered".to_string()] 
            }
        };

        BridgeSystemStatus {
            status: system_health,
            active_operations: self.active_operations.len() as u32,
            registered_actors,
            last_activity: SystemTime::now(),
            uptime: SystemTime::now().duration_since(self.started_at).unwrap_or_default(),
        }
    }

    /// Start health monitoring
    fn start_health_monitoring(&mut self, ctx: &mut Context<Self>) {
        let interval = self.config.health_check_interval;
        ctx.run_interval(interval, move |actor, _ctx| {
            actor.health_monitor.check_system_health();
            // Additional health checks would be implemented here
        });
    }

    /// Start metrics collection
    fn start_metrics_collection(&mut self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(10), move |actor, _ctx| {
            actor.metrics.update_active_operations(actor.active_operations.len());
            // Additional metrics collection
        });
    }

    /// Handle actor failure
    pub async fn handle_actor_failure(&mut self, actor_type: ActorType, error: BridgeError) {
        error!("Actor failure detected: {:?} - {:?}", actor_type, error);
        
        // Record failure
        self.metrics.record_actor_failure(&actor_type);
        
        // Implement recovery strategy based on actor type
        match actor_type {
            ActorType::PegIn => {
                warn!("PegInActor failed, operations may be affected");
                // In practice, we would attempt to restart the actor
            }
            ActorType::PegOut => {
                warn!("PegOutActor failed, operations may be affected");
                // In practice, we would attempt to restart the actor
            }
            ActorType::Stream => {
                warn!("StreamActor failed, governance communication affected");
                // In practice, we would attempt to restart the actor
            }
            ActorType::Bridge => {
                error!("Bridge coordinator failure - this should not happen");
            }
        }
        
        // Update system health
        self.health_monitor.record_actor_failure(actor_type);
    }
}

impl Actor for BridgeActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("Bridge coordinator actor starting");
        
        // TODO: Implement proper health monitoring initialization
        // Health monitoring should be started via messages after actor is fully initialized
        
        // Update state to running
        self.state = BridgeState::Running;
        
        info!("Bridge coordinator actor started successfully");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Bridge coordinator actor stopped");
        self.metrics.record_system_stop();
    }
}