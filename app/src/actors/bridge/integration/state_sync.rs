//! State Synchronization
//! 
//! Manages state consistency across bridge actors

use actix::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tracing::{info, warn, error};
use serde::{Serialize, Deserialize};

use crate::actors::bridge::{
    messages::{
        bridge_messages::{BridgeCoordinationMessage, BridgeSystemStatus, ActorType},
        pegin_messages::{PegInActorStatus},
        pegout_messages::{PegOutMessage, PegOutResponse, PegOutStatus},
        stream_messages::{StreamMessage, StreamResponse, NodeConnectionStatus}
    },
    actors::{
        bridge::BridgeActor,
        pegin::{PegInActor, handlers::GetPegInStatus},
        pegout::{PegOutActor},
        stream::StreamActor
    },
};
use crate::types::bridge::*;

/// State synchronization manager
pub struct StateSyncManager {
    /// Actor addresses for state sync
    bridge_actor: Option<Addr<BridgeActor>>,
    pegin_actor: Option<Addr<PegInActor>>,
    pegout_actor: Option<Addr<PegOutActor>>,
    stream_actor: Option<Addr<StreamActor>>,
    
    /// State tracking
    actor_states: HashMap<ActorType, ActorStateSnapshot>,
    state_versions: HashMap<ActorType, u64>,
    sync_operations: HashMap<String, SyncOperation>,
    
    /// Synchronization configuration
    sync_interval: Duration,
    max_sync_attempts: u32,
    sync_timeout: Duration,
    
    /// Metrics
    sync_metrics: StateSyncMetrics,
}

/// Actor state representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorStateSnapshot {
    pub actor_type: ActorType,
    pub version: u64,
    pub timestamp: SystemTime,
    pub health_status: String,
    pub key_metrics: HashMap<String, StateValue>,
    pub checksum: String,
}

/// State value types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateValue {
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    List(Vec<StateValue>),
}

// Using ActorType from bridge_messages

/// Synchronization operation
#[derive(Debug, Clone)]
pub struct SyncOperation {
    pub sync_id: String,
    pub operation_type: SyncType,
    pub participants: Vec<ActorType>,
    pub started_at: SystemTime,
    pub status: SyncStatus,
    pub attempt_count: u32,
    pub error_history: Vec<String>,
}

/// Types of synchronization operations
#[derive(Debug, Clone)]
pub enum SyncType {
    FullSync,
    IncrementalSync,
    HealthSync,
    ConfigSync,
    RecoverySync,
}

/// Synchronization status
#[derive(Debug, Clone)]
pub enum SyncStatus {
    Initiated,
    InProgress,
    WaitingForResponses,
    Completed,
    Failed(String),
    Retrying,
}

/// State synchronization metrics
#[derive(Debug, Default)]
pub struct StateSyncMetrics {
    pub total_sync_operations: u64,
    pub successful_syncs: u64,
    pub failed_syncs: u64,
    pub average_sync_time: Duration,
    pub last_full_sync: Option<SystemTime>,
    pub state_inconsistencies_detected: u64,
    pub state_inconsistencies_resolved: u64,
}

impl StateSyncManager {
    pub fn new(
        sync_interval: Duration,
        max_sync_attempts: u32,
        sync_timeout: Duration,
    ) -> Self {
        Self {
            bridge_actor: None,
            pegin_actor: None,
            pegout_actor: None,
            stream_actor: None,
            actor_states: HashMap::new(),
            state_versions: HashMap::new(),
            sync_operations: HashMap::new(),
            sync_interval,
            max_sync_attempts,
            sync_timeout,
            sync_metrics: StateSyncMetrics::default(),
        }
    }

    /// Register actors for state synchronization
    pub fn register_actors(
        &mut self,
        bridge_actor: Option<Addr<BridgeActor>>,
        pegin_actor: Option<Addr<PegInActor>>,
        pegout_actor: Option<Addr<PegOutActor>>,
        stream_actor: Option<Addr<StreamActor>>,
    ) {
        self.bridge_actor = bridge_actor;
        self.pegin_actor = pegin_actor;
        self.pegout_actor = pegout_actor;
        self.stream_actor = stream_actor;
        
        // Initialize state versions
        self.state_versions.insert(ActorType::Bridge, 0);
        self.state_versions.insert(ActorType::PegIn, 0);
        self.state_versions.insert(ActorType::PegOut, 0);
        self.state_versions.insert(ActorType::Stream, 0);
        
        info!("Actors registered for state synchronization");
    }

    /// Start periodic state synchronization
    pub async fn start_periodic_sync(&mut self) -> Result<(), StateSyncError> {
        info!("Starting periodic state synchronization every {:?}", self.sync_interval);
        
        // Perform initial full sync
        self.perform_full_sync().await?;
        
        Ok(())
    }

    /// Perform full state synchronization
    pub async fn perform_full_sync(&mut self) -> Result<String, StateSyncError> {
        let sync_id = format!("full_sync_{}", uuid::Uuid::new_v4());
        
        info!("Initiating full state synchronization: {}", sync_id);
        
        let participants = vec![
            ActorType::Bridge,
            ActorType::PegIn,
            ActorType::PegOut,
            ActorType::Stream,
        ];

        let sync_operation = SyncOperation {
            sync_id: sync_id.clone(),
            operation_type: SyncType::FullSync,
            participants: participants.clone(),
            started_at: SystemTime::now(),
            status: SyncStatus::Initiated,
            attempt_count: 1,
            error_history: Vec::new(),
        };

        self.sync_operations.insert(sync_id.clone(), sync_operation);
        self.sync_metrics.total_sync_operations += 1;

        // Collect states from all actors
        let mut collected_states = HashMap::new();
        
        for actor_type in &participants {
            match self.collect_actor_state(actor_type).await {
                Ok(state) => {
                    collected_states.insert(actor_type.clone(), state);
                    info!("Collected state from {:?} actor", actor_type);
                }
                Err(e) => {
                    warn!("Failed to collect state from {:?} actor: {:?}", actor_type, e);
                    return Err(StateSyncError::StateCollectionFailed(format!("{:?}: {}", actor_type, e)));
                }
            }
        }

        // Update local state tracking
        for (actor_type, state) in collected_states {
            let version = self.state_versions.get(&actor_type).unwrap_or(&0) + 1;
            self.state_versions.insert(actor_type.clone(), version);
            self.actor_states.insert(actor_type, state);
        }

        // Mark operation as completed
        if let Some(operation) = self.sync_operations.get_mut(&sync_id) {
            operation.status = SyncStatus::Completed;
        }

        self.sync_metrics.successful_syncs += 1;
        self.sync_metrics.last_full_sync = Some(SystemTime::now());
        
        info!("Full state synchronization completed: {}", sync_id);
        Ok(sync_id)
    }

    /// Collect state from a specific actor
    async fn collect_actor_state(&self, actor_type: &ActorType) -> Result<ActorStateSnapshot, StateSyncError> {
        match actor_type {
            ActorType::Bridge => {
                if let Some(actor) = &self.bridge_actor {
                    let status = actor.send(BridgeCoordinationMessage::GetSystemStatus).await
                        .map_err(|e| StateSyncError::ActorCommunicationFailed(format!("Bridge: {}", e)))?
                        .map_err(|e| StateSyncError::ActorCommunicationFailed(format!("Bridge: {:?}", e)))?;

                    Ok(self.bridge_status_to_state(status))
                } else {
                    Err(StateSyncError::ActorNotRegistered(actor_type.clone()))
                }
            }
            ActorType::PegIn => {
                if let Some(actor) = &self.pegin_actor {
                    let status = actor.send(GetPegInStatus).await
                        .map_err(|e| StateSyncError::ActorCommunicationFailed(format!("PegIn: {}", e)))?
                        .map_err(|e| StateSyncError::ActorCommunicationFailed(format!("PegIn: {:?}", e)))?;

                    Ok(self.pegin_status_to_state(status))
                } else {
                    Err(StateSyncError::ActorNotRegistered(actor_type.clone()))
                }
            }
            ActorType::PegOut => {
                if let Some(actor) = &self.pegout_actor {
                    let msg = PegOutMessage::GetPegOutStatus { pegout_id: "system_status".to_string() };
                    let response = actor.send(msg).await
                        .map_err(|e| StateSyncError::ActorCommunicationFailed(format!("PegOut: {}", e)))?
                        .map_err(|e| StateSyncError::ActorCommunicationFailed(format!("PegOut: {:?}", e)))?;

                    // Extract status from response
                    let status = match response {
                        PegOutResponse::PegOutStatus(s) => s,
                        _ => PegOutStatus::Failed { reason: "Unexpected response".to_string(), recoverable: false }
                    };
                    Ok(self.pegout_status_to_state(status))
                } else {
                    Err(StateSyncError::ActorNotRegistered(actor_type.clone()))
                }
            }
            ActorType::Stream => {
                if let Some(actor) = &self.stream_actor {
                    let response = actor.send(StreamMessage::GetConnectionStatus).await
                        .map_err(|e| StateSyncError::ActorCommunicationFailed(format!("Stream: {}", e)))?
                        .map_err(|e| StateSyncError::ActorCommunicationFailed(format!("Stream: {:?}", e)))?;

                    Ok(self.stream_status_to_state(response))
                } else {
                    Err(StateSyncError::ActorNotRegistered(actor_type.clone()))
                }
            }
        }
    }

    /// Convert bridge status to actor state
    fn bridge_status_to_state(&self, status: BridgeSystemStatus) -> ActorStateSnapshot {
        let mut key_metrics = HashMap::new();
        key_metrics.insert("bridge_status".to_string(), StateValue::String("active".to_string()));

        let checksum = self.calculate_state_checksum(&key_metrics);
        ActorStateSnapshot {
            actor_type: ActorType::Bridge,
            version: self.state_versions.get(&ActorType::Bridge).unwrap_or(&0) + 1,
            timestamp: SystemTime::now(),
            health_status: "healthy".to_string(),
            key_metrics,
            checksum,
        }
    }

    /// Create default state snapshot for error cases
    fn create_default_state_snapshot(&self, actor_type: ActorType) -> ActorStateSnapshot {
        let mut key_metrics = HashMap::new();
        key_metrics.insert("status".to_string(), StateValue::String("unknown".to_string()));

        let checksum = self.calculate_state_checksum(&key_metrics);
        ActorStateSnapshot {
            actor_type: actor_type.clone(),
            version: self.state_versions.get(&actor_type).unwrap_or(&0) + 1,
            timestamp: SystemTime::now(),
            health_status: "unknown".to_string(),
            key_metrics,
            checksum,
        }
    }

    /// Convert pegin status to actor state
    fn pegin_status_to_state(&self, status: PegInActorStatus) -> ActorStateSnapshot {
        let mut key_metrics = HashMap::new();
        key_metrics.insert("pending_deposits".to_string(), StateValue::Integer(status.pending_deposits as i64));
        key_metrics.insert("total_deposits_processed".to_string(), StateValue::Integer(status.total_deposits_processed as i64));
        key_metrics.insert("last_block_checked".to_string(), StateValue::Integer(status.last_block_checked as i64));
        key_metrics.insert("recent_errors".to_string(), StateValue::Integer(status.recent_errors as i64));

        let checksum = self.calculate_state_checksum(&key_metrics);
        ActorStateSnapshot {
            actor_type: ActorType::PegIn,
            version: self.state_versions.get(&ActorType::PegIn).unwrap_or(&0) + 1,
            timestamp: SystemTime::now(),
            health_status: format!("{:?}", status.state),
            key_metrics,
            checksum,
        }
    }

    /// Convert pegout status to actor state
    fn pegout_status_to_state(&self, status: PegOutStatus) -> ActorStateSnapshot {
        let mut key_metrics = HashMap::new();

        // Extract meaningful metrics from the PegOutStatus enum
        let (status_str, error_count) = match &status {
            PegOutStatus::BurnDetected => ("burn_detected".to_string(), 0),
            PegOutStatus::ValidatingBurn => ("validating_burn".to_string(), 0),
            PegOutStatus::ValidationFailed { reason: _ } => ("validation_failed".to_string(), 1),
            PegOutStatus::BuildingTransaction => ("building_transaction".to_string(), 0),
            PegOutStatus::TransactionBuilt { fee } => {
                key_metrics.insert("transaction_fee".to_string(), StateValue::Integer(*fee as i64));
                ("transaction_built".to_string(), 0)
            },
            PegOutStatus::RequestingSignatures => ("requesting_signatures".to_string(), 0),
            PegOutStatus::CollectingSignatures { collected, required } => {
                key_metrics.insert("signatures_collected".to_string(), StateValue::Integer(*collected as i64));
                key_metrics.insert("signatures_required".to_string(), StateValue::Integer(*required as i64));
                ("collecting_signatures".to_string(), 0)
            },
            PegOutStatus::SignaturesComplete => ("signatures_complete".to_string(), 0),
            PegOutStatus::Broadcasting => ("broadcasting".to_string(), 0),
            PegOutStatus::Broadcast { txid: _, confirmations } => {
                key_metrics.insert("confirmations".to_string(), StateValue::Integer(*confirmations as i64));
                ("broadcast".to_string(), 0)
            },
            PegOutStatus::Confirmed { txid: _, confirmations } => {
                key_metrics.insert("confirmations".to_string(), StateValue::Integer(*confirmations as i64));
                ("confirmed".to_string(), 0)
            },
            PegOutStatus::Completed { txid: _, final_confirmations } => {
                key_metrics.insert("final_confirmations".to_string(), StateValue::Integer(*final_confirmations as i64));
                ("completed".to_string(), 0)
            },
            PegOutStatus::Failed { reason: _, recoverable } => {
                key_metrics.insert("recoverable".to_string(), StateValue::Boolean(*recoverable));
                ("failed".to_string(), 1)
            },
            PegOutStatus::Cancelled { reason: _ } => ("cancelled".to_string(), 0),
        };

        key_metrics.insert("status".to_string(), StateValue::String(status_str.clone()));
        key_metrics.insert("error_count".to_string(), StateValue::Integer(error_count));

        let checksum = self.calculate_state_checksum(&key_metrics);
        ActorStateSnapshot {
            actor_type: ActorType::PegOut,
            version: self.state_versions.get(&ActorType::PegOut).unwrap_or(&0) + 1,
            timestamp: SystemTime::now(),
            health_status: status_str,
            key_metrics,
            checksum,
        }
    }

    /// Convert stream status to actor state
    fn stream_status_to_state(&self, response: StreamResponse) -> ActorStateSnapshot {
        let mut key_metrics = HashMap::new();

        // Extract connection status from StreamResponse
        let (is_connected, status_str) = match response {
            StreamResponse::ConnectionStatus(status) => {
                match status {
                    NodeConnectionStatus::Connected => (true, "connected".to_string()),
                    NodeConnectionStatus::Connecting => (false, "connecting".to_string()),
                    NodeConnectionStatus::Disconnected => (false, "disconnected".to_string()),
                    NodeConnectionStatus::Failed { error } => {
                        key_metrics.insert("error".to_string(), StateValue::String(error));
                        (false, "failed".to_string())
                    },
                    NodeConnectionStatus::Timeout => (false, "timeout".to_string()),
                }
            },
            _ => (false, "unknown".to_string()),
        };

        key_metrics.insert("is_connected".to_string(), StateValue::Boolean(is_connected));
        key_metrics.insert("status".to_string(), StateValue::String(status_str.clone()));

        let checksum = self.calculate_state_checksum(&key_metrics);
        ActorStateSnapshot {
            actor_type: ActorType::Stream,
            version: self.state_versions.get(&ActorType::Stream).unwrap_or(&0) + 1,
            timestamp: SystemTime::now(),
            health_status: status_str,
            key_metrics,
            checksum,
        }
    }

    /// Calculate state checksum for integrity verification
    fn calculate_state_checksum(&self, metrics: &HashMap<String, StateValue>) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let serialized = serde_json::to_string(metrics).unwrap_or_default();
        let mut hasher = DefaultHasher::new();
        serialized.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Detect state inconsistencies
    pub fn detect_inconsistencies(&mut self) -> Vec<StateInconsistency> {
        let mut inconsistencies = Vec::new();
        
        // Check for version mismatches
        for (actor_type, expected_version) in &self.state_versions {
            if let Some(state) = self.actor_states.get(actor_type) {
                if state.version != *expected_version {
                    inconsistencies.push(StateInconsistency {
                        actor_type: actor_type.clone(),
                        inconsistency_type: InconsistencyType::VersionMismatch,
                        description: format!("Expected version {}, found {}", expected_version, state.version),
                        severity: InconsistencySeverity::Medium,
                    });
                }
            }
        }

        // Check for stale states
        let now = SystemTime::now();
        let stale_threshold = Duration::from_secs(300); // 5 minutes

        for (actor_type, state) in &self.actor_states {
            if now.duration_since(state.timestamp).unwrap_or_default() > stale_threshold {
                inconsistencies.push(StateInconsistency {
                    actor_type: actor_type.clone(),
                    inconsistency_type: InconsistencyType::StaleState,
                    description: format!("State hasn't been updated in {:?}", now.duration_since(state.timestamp).unwrap_or_default()),
                    severity: InconsistencySeverity::High,
                });
            }
        }

        if !inconsistencies.is_empty() {
            self.sync_metrics.state_inconsistencies_detected += inconsistencies.len() as u64;
            warn!("Detected {} state inconsistencies", inconsistencies.len());
        }

        inconsistencies
    }

    /// Get synchronization metrics
    pub fn get_metrics(&self) -> &StateSyncMetrics {
        &self.sync_metrics
    }

    /// Get current actor states
    pub fn get_actor_states(&self) -> &HashMap<ActorType, ActorStateSnapshot> {
        &self.actor_states
    }
}

/// State inconsistency detection
#[derive(Debug, Clone)]
pub struct StateInconsistency {
    pub actor_type: ActorType,
    pub inconsistency_type: InconsistencyType,
    pub description: String,
    pub severity: InconsistencySeverity,
}

/// Types of inconsistencies
#[derive(Debug, Clone)]
pub enum InconsistencyType {
    VersionMismatch,
    StaleState,
    ChecksumMismatch,
    MissingState,
    InvalidState,
}

/// Inconsistency severity levels
#[derive(Debug, Clone)]
pub enum InconsistencySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// State synchronization errors
#[derive(Debug, thiserror::Error)]
pub enum StateSyncError {
    #[error("Actor not registered: {0:?}")]
    ActorNotRegistered(ActorType),
    
    #[error("Actor communication failed: {0}")]
    ActorCommunicationFailed(String),
    
    #[error("State collection failed: {0}")]
    StateCollectionFailed(String),
    
    #[error("Synchronization timeout: {0}")]
    SyncTimeout(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}