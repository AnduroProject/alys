//! Configuration hot-reload system with actor notification and state preservation
//!
//! This module provides a comprehensive hot-reload system that can dynamically
//! update configuration while preserving actor state and ensuring system stability.

use super::*;
use crate::types::*;
use actor_system::{ActorError, ActorResult, AlysMessage, SerializableMessage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{broadcast, RwLock, watch};
use tokio::fs;
use notify::{Watcher, RecursiveMode, Event, EventKind};
use uuid::Uuid;

/// Configuration hot-reload manager
#[derive(Debug)]
pub struct ConfigReloadManager {
    /// Current configuration
    current_config: Arc<RwLock<AlysConfig>>,
    
    /// Configuration file paths being watched
    watched_files: Arc<RwLock<HashMap<PathBuf, FileWatchInfo>>>,
    
    /// File system watcher
    watcher: Arc<RwLock<Option<notify::RecommendedWatcher>>>,
    
    /// Reload event broadcaster
    reload_sender: broadcast::Sender<ConfigReloadEvent>,
    
    /// Reload processing queue
    reload_queue: Arc<RwLock<Vec<PendingReload>>>,
    
    /// Actor notification system
    actor_notifier: ActorNotificationSystem,
    
    /// State preservation manager
    state_preservation: StatePreservationManager,
    
    /// Reload history and metrics
    reload_history: Arc<RwLock<ReloadHistory>>,
    
    /// Validation engine
    validation_engine: ValidationEngine,
    
    /// Rollback system
    rollback_manager: RollbackManager,
}

/// File watching information
#[derive(Debug, Clone)]
pub struct FileWatchInfo {
    pub path: PathBuf,
    pub last_modified: SystemTime,
    pub checksum: String,
    pub watch_mode: WatchMode,
    pub reload_delay: Duration,
    pub last_reload_attempt: Option<SystemTime>,
}

/// File watching modes
#[derive(Debug, Clone, Copy)]
pub enum WatchMode {
    /// Immediate reload on change
    Immediate,
    /// Debounced reload (wait for changes to settle)
    Debounced { delay: Duration },
    /// Manual reload only
    Manual,
    /// Scheduled reload at intervals
    Scheduled { interval: Duration },
}

/// Configuration reload events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigReloadEvent {
    /// Reload initiated
    ReloadStarted {
        reload_id: String,
        timestamp: SystemTime,
        trigger: ReloadTrigger,
        files_changed: Vec<PathBuf>,
    },
    /// Reload completed successfully
    ReloadCompleted {
        reload_id: String,
        timestamp: SystemTime,
        duration: Duration,
        changes_applied: ConfigChanges,
        actors_notified: Vec<String>,
    },
    /// Reload failed
    ReloadFailed {
        reload_id: String,
        timestamp: SystemTime,
        error: String,
        rollback_performed: bool,
    },
    /// Configuration validation warning
    ValidationWarning {
        reload_id: String,
        warnings: Vec<String>,
    },
    /// Actor notification completed
    ActorNotificationCompleted {
        reload_id: String,
        actor_id: String,
        success: bool,
        response_time: Duration,
    },
}

/// Reload trigger sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReloadTrigger {
    /// File system change
    FileChanged { path: PathBuf },
    /// Manual trigger
    Manual { user: Option<String> },
    /// Scheduled reload
    Scheduled,
    /// Remote trigger (e.g., from governance)
    Remote { source: String },
    /// Environment variable change
    EnvironmentChanged,
}

/// Pending reload in queue
#[derive(Debug, Clone)]
pub struct PendingReload {
    pub reload_id: String,
    pub trigger: ReloadTrigger,
    pub files_to_reload: Vec<PathBuf>,
    pub scheduled_at: SystemTime,
    pub priority: ReloadPriority,
    pub retry_count: u32,
}

/// Reload priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReloadPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Configuration changes detected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigChanges {
    pub sections_changed: Vec<String>,
    pub fields_changed: Vec<FieldChange>,
    pub actors_affected: Vec<String>,
    pub requires_restart: Vec<String>,
    pub validation_errors: Vec<String>,
    pub validation_warnings: Vec<String>,
}

/// Individual field change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChange {
    pub path: String,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
    pub change_type: ChangeType,
}

/// Types of configuration changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Modified,
    Removed,
    Renamed { from: String },
}

/// Actor notification system
#[derive(Debug)]
pub struct ActorNotificationSystem {
    /// Notification channels per actor
    notification_channels: HashMap<String, broadcast::Sender<ActorConfigUpdate>>,
    
    /// Actor configuration preferences
    actor_preferences: HashMap<String, ActorNotificationPreference>,
    
    /// Notification timeout settings
    notification_timeouts: NotificationTimeouts,
}

/// Actor configuration update message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorConfigUpdate {
    pub reload_id: String,
    pub actor_id: String,
    pub config_changes: ConfigChanges,
    pub new_config: serde_json::Value, // Actor-specific config section
    pub requires_restart: bool,
    pub update_timestamp: SystemTime,
    pub rollback_token: Option<String>,
}

/// Actor notification preferences
#[derive(Debug, Clone)]
pub struct ActorNotificationPreference {
    pub notification_mode: NotificationMode,
    pub batch_updates: bool,
    pub max_batch_size: usize,
    pub batch_timeout: Duration,
    pub acknowledgment_required: bool,
    pub retry_policy: RetryPolicy,
}

/// Notification delivery modes
#[derive(Debug, Clone)]
pub enum NotificationMode {
    /// Synchronous notification (block until acknowledged)
    Synchronous,
    /// Asynchronous notification (fire and forget)
    Asynchronous,
    /// Batched notification (collect multiple updates)
    Batched,
    /// Selective notification (only for specific changes)
    Selective { watch_patterns: Vec<String> },
}

/// Retry policy for failed notifications
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
    pub jitter: bool,
}

/// Notification timeout settings
#[derive(Debug, Clone)]
pub struct NotificationTimeouts {
    pub actor_acknowledgment: Duration,
    pub total_notification_cycle: Duration,
    pub critical_section_timeout: Duration,
}

/// State preservation manager
#[derive(Debug)]
pub struct StatePreservationManager {
    /// Preserved state snapshots
    state_snapshots: HashMap<String, StateSnapshot>,
    
    /// Actor state serializers
    state_serializers: HashMap<String, Box<dyn StateSerializer>>,
    
    /// Preservation strategies
    preservation_strategies: HashMap<String, PreservationStrategy>,
}

/// State snapshot for rollback
#[derive(Debug, Clone)]
pub struct StateSnapshot {
    pub snapshot_id: String,
    pub actor_id: String,
    pub state_data: Vec<u8>,
    pub metadata: SnapshotMetadata,
    pub created_at: SystemTime,
    pub expires_at: SystemTime,
}

/// Snapshot metadata
#[derive(Debug, Clone)]
pub struct SnapshotMetadata {
    pub config_version: String,
    pub state_version: u64,
    pub dependencies: Vec<String>,
    pub preservation_strategy: PreservationStrategy,
}

/// State preservation strategies
#[derive(Debug, Clone)]
pub enum PreservationStrategy {
    /// Full state serialization
    FullSerialization,
    /// Incremental state preservation
    Incremental { checkpoint_interval: Duration },
    /// Memory-based preservation
    InMemory { max_size_mb: u64 },
    /// File-based preservation
    FileBased { storage_path: PathBuf },
    /// No preservation (restart required)
    None,
}

/// State serialization trait
pub trait StateSerializer: Send + Sync + std::fmt::Debug {
    /// Serialize actor state
    fn serialize_state(&self, actor_state: &dyn std::any::Any) -> Result<Vec<u8>, ConfigError>;
    
    /// Deserialize actor state
    fn deserialize_state(&self, data: &[u8]) -> Result<Box<dyn std::any::Any>, ConfigError>;
    
    /// Get serialization format
    fn format(&self) -> &str;
    
    /// Validate state integrity
    fn validate_state(&self, data: &[u8]) -> Result<(), ConfigError>;
}

/// Reload history and metrics
#[derive(Debug, Default)]
pub struct ReloadHistory {
    /// All reload attempts
    pub reloads: Vec<ReloadAttempt>,
    
    /// Success/failure statistics
    pub stats: ReloadStats,
    
    /// Performance metrics
    pub performance: ReloadPerformanceMetrics,
}

/// Individual reload attempt
#[derive(Debug, Clone)]
pub struct ReloadAttempt {
    pub reload_id: String,
    pub timestamp: SystemTime,
    pub trigger: ReloadTrigger,
    pub duration: Duration,
    pub result: ReloadResult,
    pub changes: ConfigChanges,
    pub actors_affected: Vec<String>,
    pub error_message: Option<String>,
}

/// Reload attempt result
#[derive(Debug, Clone)]
pub enum ReloadResult {
    Success,
    PartialSuccess { failed_actors: Vec<String> },
    Failed { reason: String },
    RolledBack { reason: String },
}

/// Reload statistics
#[derive(Debug, Default)]
pub struct ReloadStats {
    pub total_reloads: u64,
    pub successful_reloads: u64,
    pub failed_reloads: u64,
    pub rolled_back_reloads: u64,
    pub average_duration: Duration,
    pub fastest_reload: Option<Duration>,
    pub slowest_reload: Option<Duration>,
}

/// Reload performance metrics
#[derive(Debug, Default)]
pub struct ReloadPerformanceMetrics {
    pub file_parse_time: Duration,
    pub validation_time: Duration,
    pub actor_notification_time: Duration,
    pub state_preservation_time: Duration,
    pub total_processing_time: Duration,
}

/// Configuration validation engine
#[derive(Debug)]
pub struct ValidationEngine {
    /// Validation rules
    validation_rules: Vec<ValidationRule>,
    
    /// Custom validators
    custom_validators: HashMap<String, Box<dyn ConfigValidator>>,
    
    /// Validation cache
    validation_cache: HashMap<String, ValidationResult>,
}

/// Validation rule
#[derive(Debug, Clone)]
pub struct ValidationRule {
    pub name: String,
    pub description: String,
    pub severity: ValidationSeverity,
    pub condition: ValidationCondition,
    pub message_template: String,
}

/// Validation severity levels
#[derive(Debug, Clone, Copy)]
pub enum ValidationSeverity {
    Error,
    Warning,
    Info,
}

/// Validation conditions
#[derive(Debug, Clone)]
pub enum ValidationCondition {
    /// Field must exist
    FieldExists { path: String },
    /// Field must be within range
    FieldRange { path: String, min: f64, max: f64 },
    /// Field must match pattern
    FieldPattern { path: String, pattern: String },
    /// Custom validation function
    Custom { validator_name: String },
    /// Cross-field dependency
    Dependency { field: String, depends_on: String },
}

/// Configuration validator trait
pub trait ConfigValidator: Send + Sync + std::fmt::Debug {
    /// Validate configuration
    fn validate(&self, config: &AlysConfig) -> ValidationResult;
    
    /// Get validator name
    fn name(&self) -> &str;
    
    /// Get validator description
    fn description(&self) -> &str;
}

/// Validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub infos: Vec<ValidationInfo>,
}

/// Validation error
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub rule_name: String,
    pub field_path: String,
    pub message: String,
    pub severity: ValidationSeverity,
}

/// Validation warning
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    pub rule_name: String,
    pub field_path: String,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Validation info
#[derive(Debug, Clone)]
pub struct ValidationInfo {
    pub rule_name: String,
    pub message: String,
}

/// Rollback manager
#[derive(Debug)]
pub struct RollbackManager {
    /// Configuration snapshots for rollback
    config_snapshots: HashMap<String, ConfigSnapshot>,
    
    /// Rollback strategies per component
    rollback_strategies: HashMap<String, RollbackStrategy>,
    
    /// Maximum rollback history
    max_snapshots: usize,
}

/// Configuration snapshot
#[derive(Debug, Clone)]
pub struct ConfigSnapshot {
    pub snapshot_id: String,
    pub config: AlysConfig,
    pub timestamp: SystemTime,
    pub metadata: SnapshotMetadata,
    pub validation_result: ValidationResult,
}

/// Rollback strategies
#[derive(Debug, Clone)]
pub enum RollbackStrategy {
    /// Immediate rollback on any error
    Immediate,
    /// Rollback after timeout
    Timeout { duration: Duration },
    /// Manual rollback only
    Manual,
    /// Partial rollback (only failed components)
    Partial,
    /// No rollback support
    None,
}

impl ConfigReloadManager {
    /// Create new configuration reload manager
    pub async fn new(initial_config: AlysConfig) -> Result<Self, ConfigError> {
        let (reload_sender, _) = broadcast::channel(1000);
        
        let manager = Self {
            current_config: Arc::new(RwLock::new(initial_config)),
            watched_files: Arc::new(RwLock::new(HashMap::new())),
            watcher: Arc::new(RwLock::new(None)),
            reload_sender,
            reload_queue: Arc::new(RwLock::new(Vec::new())),
            actor_notifier: ActorNotificationSystem {
                notification_channels: HashMap::new(),
                actor_preferences: HashMap::new(),
                notification_timeouts: NotificationTimeouts {
                    actor_acknowledgment: Duration::from_secs(30),
                    total_notification_cycle: Duration::from_secs(300),
                    critical_section_timeout: Duration::from_secs(60),
                },
            },
            state_preservation: StatePreservationManager {
                state_snapshots: HashMap::new(),
                state_serializers: HashMap::new(),
                preservation_strategies: HashMap::new(),
            },
            reload_history: Arc::new(RwLock::new(ReloadHistory::default())),
            validation_engine: ValidationEngine {
                validation_rules: Self::default_validation_rules(),
                custom_validators: HashMap::new(),
                validation_cache: HashMap::new(),
            },
            rollback_manager: RollbackManager {
                config_snapshots: HashMap::new(),
                rollback_strategies: HashMap::new(),
                max_snapshots: 10,
            },
        };
        
        Ok(manager)
    }
    
    /// Watch configuration file for changes
    pub async fn watch_file<P: AsRef<Path>>(&mut self, path: P, mode: WatchMode) -> Result<(), ConfigError> {
        let path = path.as_ref().to_path_buf();
        let metadata = fs::metadata(&path).await
            .map_err(|e| ConfigError::FileNotFound {
                path: path.display().to_string(),
            })?;
        
        let checksum = self.calculate_file_checksum(&path).await?;
        
        let watch_info = FileWatchInfo {
            path: path.clone(),
            last_modified: metadata.modified().unwrap_or(SystemTime::now()),
            checksum,
            watch_mode: mode,
            reload_delay: match mode {
                WatchMode::Debounced { delay } => delay,
                _ => Duration::from_millis(500),
            },
            last_reload_attempt: None,
        };
        
        self.watched_files.write().await.insert(path.clone(), watch_info);
        
        // Initialize file system watcher if not already done
        if self.watcher.read().await.is_none() {
            self.init_file_watcher().await?;
        }
        
        Ok(())
    }
    
    /// Register actor for configuration notifications
    pub async fn register_actor(&mut self, actor_id: String, preferences: ActorNotificationPreference) -> Result<broadcast::Receiver<ActorConfigUpdate>, ConfigError> {
        let (sender, receiver) = broadcast::channel(1000);
        
        self.actor_notifier.notification_channels.insert(actor_id.clone(), sender);
        self.actor_notifier.actor_preferences.insert(actor_id, preferences);
        
        Ok(receiver)
    }
    
    /// Trigger manual configuration reload
    pub async fn trigger_reload(&self, files: Vec<PathBuf>, user: Option<String>) -> Result<String, ConfigError> {
        let reload_id = Uuid::new_v4().to_string();
        
        let pending_reload = PendingReload {
            reload_id: reload_id.clone(),
            trigger: ReloadTrigger::Manual { user },
            files_to_reload: files,
            scheduled_at: SystemTime::now(),
            priority: ReloadPriority::High,
            retry_count: 0,
        };
        
        self.reload_queue.write().await.push(pending_reload);
        self.process_reload_queue().await?;
        
        Ok(reload_id)
    }
    
    /// Process pending reloads
    async fn process_reload_queue(&self) -> Result<(), ConfigError> {
        let mut queue = self.reload_queue.write().await;
        if queue.is_empty() {
            return Ok(());
        }
        
        // Sort by priority and timestamp
        queue.sort_by(|a, b| {
            b.priority.cmp(&a.priority)
                .then(a.scheduled_at.cmp(&b.scheduled_at))
        });
        
        let reload = queue.remove(0);
        drop(queue);
        
        self.execute_reload(reload).await
    }
    
    /// Execute configuration reload
    async fn execute_reload(&self, reload: PendingReload) -> Result<(), ConfigError> {
        let start_time = SystemTime::now();
        
        // Emit reload started event
        let _ = self.reload_sender.send(ConfigReloadEvent::ReloadStarted {
            reload_id: reload.reload_id.clone(),
            timestamp: start_time,
            trigger: reload.trigger.clone(),
            files_changed: reload.files_to_reload.clone(),
        });
        
        // Create configuration snapshot for rollback
        let current_config = self.current_config.read().await.clone();
        let snapshot_id = format!("{}_snapshot", reload.reload_id);
        self.rollback_manager.config_snapshots.insert(
            snapshot_id.clone(),
            ConfigSnapshot {
                snapshot_id,
                config: current_config,
                timestamp: start_time,
                metadata: SnapshotMetadata {
                    config_version: "1.0".to_string(),
                    state_version: 1,
                    dependencies: Vec::new(),
                    preservation_strategy: PreservationStrategy::InMemory { max_size_mb: 100 },
                },
                validation_result: ValidationResult {
                    is_valid: true,
                    errors: Vec::new(),
                    warnings: Vec::new(),
                    infos: Vec::new(),
                },
            }
        );
        
        // Load new configuration
        let new_config = match self.load_configuration_from_files(&reload.files_to_reload).await {
            Ok(config) => config,
            Err(e) => {
                let _ = self.reload_sender.send(ConfigReloadEvent::ReloadFailed {
                    reload_id: reload.reload_id,
                    timestamp: SystemTime::now(),
                    error: e.to_string(),
                    rollback_performed: false,
                });
                return Err(e);
            }
        };
        
        // Validate new configuration
        let validation_result = self.validation_engine.validate(&new_config);
        if !validation_result.is_valid {
            let error_msg = format!("Configuration validation failed: {:?}", validation_result.errors);
            let _ = self.reload_sender.send(ConfigReloadEvent::ReloadFailed {
                reload_id: reload.reload_id,
                timestamp: SystemTime::now(),
                error: error_msg.clone(),
                rollback_performed: false,
            });
            return Err(ConfigError::ValidationError {
                field: "global".to_string(),
                reason: error_msg,
            });
        }
        
        // Detect configuration changes
        let changes = self.detect_changes(&current_config, &new_config).await;
        
        // Preserve actor states
        self.preserve_actor_states(&changes.actors_affected).await?;
        
        // Update configuration
        *self.current_config.write().await = new_config;
        
        // Notify actors
        let notified_actors = self.notify_actors(&reload.reload_id, &changes).await?;
        
        let duration = start_time.elapsed().unwrap_or(Duration::from_secs(0));
        
        // Record successful reload
        let reload_attempt = ReloadAttempt {
            reload_id: reload.reload_id.clone(),
            timestamp: start_time,
            trigger: reload.trigger,
            duration,
            result: ReloadResult::Success,
            changes: changes.clone(),
            actors_affected: changes.actors_affected.clone(),
            error_message: None,
        };
        
        self.reload_history.write().await.reloads.push(reload_attempt);
        
        // Emit completion event
        let _ = self.reload_sender.send(ConfigReloadEvent::ReloadCompleted {
            reload_id: reload.reload_id,
            timestamp: SystemTime::now(),
            duration,
            changes_applied: changes,
            actors_notified: notified_actors,
        });
        
        Ok(())
    }
    
    /// Initialize file system watcher
    async fn init_file_watcher(&self) -> Result<(), ConfigError> {
        use notify::{Watcher, RecursiveMode};
        
        let reload_sender = self.reload_sender.clone();
        let watched_files = self.watched_files.clone();
        
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    if let EventKind::Modify(_) = event.kind {
                        for path in event.paths {
                            // TODO: Handle file change events
                            // This would trigger reload processing
                        }
                    }
                },
                Err(e) => {
                    eprintln!("File watcher error: {:?}", e);
                }
            }
        }).map_err(|e| ConfigError::ValidationError {
            field: "file_watcher".to_string(),
            reason: format!("Failed to create file watcher: {}", e),
        })?;
        
        // Watch all registered files
        let files = self.watched_files.read().await;
        for (path, _) in files.iter() {
            if let Some(parent) = path.parent() {
                watcher.watch(parent, RecursiveMode::NonRecursive)
                    .map_err(|e| ConfigError::ValidationError {
                        field: "file_watcher".to_string(),
                        reason: format!("Failed to watch path {:?}: {}", parent, e),
                    })?;
            }
        }
        drop(files);
        
        *self.watcher.write().await = Some(watcher);
        Ok(())
    }
    
    /// Calculate file checksum
    async fn calculate_file_checksum(&self, path: &Path) -> Result<String, ConfigError> {
        let content = fs::read(path).await
            .map_err(|e| ConfigError::IoError {
                operation: format!("read file {:?}", path),
                error: e.to_string(),
            })?;
        
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        Ok(format!("{:x}", hasher.finish()))
    }
    
    /// Load configuration from multiple files
    async fn load_configuration_from_files(&self, files: &[PathBuf]) -> Result<AlysConfig, ConfigError> {
        if files.is_empty() {
            return Err(ConfigError::ValidationError {
                field: "files".to_string(),
                reason: "No files specified for reload".to_string(),
            });
        }
        
        // For now, load from the first file
        // In a real implementation, you'd merge multiple files
        AlysConfig::load_from_file(&files[0])
    }
    
    /// Detect changes between configurations
    async fn detect_changes(&self, old_config: &AlysConfig, new_config: &AlysConfig) -> ConfigChanges {
        let mut changes = ConfigChanges {
            sections_changed: Vec::new(),
            fields_changed: Vec::new(),
            actors_affected: Vec::new(),
            requires_restart: Vec::new(),
            validation_errors: Vec::new(),
            validation_warnings: Vec::new(),
        };
        
        // Compare actor configurations
        if old_config.actors != new_config.actors {
            changes.sections_changed.push("actors".to_string());
            changes.actors_affected.extend([
                "chain_actor".to_string(),
                "engine_actor".to_string(),
                "bridge_actor".to_string(),
                "network_actor".to_string(),
                "sync_actor".to_string(),
                "stream_actor".to_string(),
                "storage_actor".to_string(),
                "supervisor_actor".to_string(),
            ]);
        }
        
        // Compare network configuration
        if old_config.network.listen_address != new_config.network.listen_address {
            changes.fields_changed.push(FieldChange {
                path: "network.listen_address".to_string(),
                old_value: Some(serde_json::to_value(&old_config.network.listen_address).unwrap()),
                new_value: Some(serde_json::to_value(&new_config.network.listen_address).unwrap()),
                change_type: ChangeType::Modified,
            });
            changes.actors_affected.push("network_actor".to_string());
            changes.requires_restart.push("network_actor".to_string());
        }
        
        // Compare storage configuration
        if old_config.storage.database_url != new_config.storage.database_url {
            changes.fields_changed.push(FieldChange {
                path: "storage.database_url".to_string(),
                old_value: Some(serde_json::to_value(&old_config.storage.database_url).unwrap()),
                new_value: Some(serde_json::to_value(&new_config.storage.database_url).unwrap()),
                change_type: ChangeType::Modified,
            });
            changes.actors_affected.push("storage_actor".to_string());
            changes.requires_restart.push("storage_actor".to_string());
        }
        
        changes
    }
    
    /// Preserve actor states before configuration change
    async fn preserve_actor_states(&self, actor_ids: &[String]) -> Result<(), ConfigError> {
        for actor_id in actor_ids {
            if let Some(strategy) = self.state_preservation.preservation_strategies.get(actor_id) {
                match strategy {
                    PreservationStrategy::InMemory { .. } => {
                        // TODO: Capture actor state in memory
                    },
                    PreservationStrategy::FileBased { storage_path } => {
                        // TODO: Serialize actor state to file
                    },
                    PreservationStrategy::None => {
                        // No preservation needed
                    },
                    _ => {
                        // Other strategies
                    }
                }
            }
        }
        Ok(())
    }
    
    /// Notify actors of configuration changes
    async fn notify_actors(&self, reload_id: &str, changes: &ConfigChanges) -> Result<Vec<String>, ConfigError> {
        let mut notified_actors = Vec::new();
        
        for actor_id in &changes.actors_affected {
            if let Some(sender) = self.actor_notifier.notification_channels.get(actor_id) {
                let update = ActorConfigUpdate {
                    reload_id: reload_id.to_string(),
                    actor_id: actor_id.clone(),
                    config_changes: changes.clone(),
                    new_config: serde_json::json!({}), // TODO: Extract actor-specific config
                    requires_restart: changes.requires_restart.contains(actor_id),
                    update_timestamp: SystemTime::now(),
                    rollback_token: None,
                };
                
                if sender.send(update).is_ok() {
                    notified_actors.push(actor_id.clone());
                }
            }
        }
        
        Ok(notified_actors)
    }
    
    /// Get current configuration
    pub async fn current_config(&self) -> AlysConfig {
        self.current_config.read().await.clone()
    }
    
    /// Get reload history
    pub async fn reload_history(&self) -> ReloadHistory {
        self.reload_history.read().await.clone()
    }
    
    /// Get reload event stream
    pub fn reload_events(&self) -> broadcast::Receiver<ConfigReloadEvent> {
        self.reload_sender.subscribe()
    }
    
    /// Default validation rules
    fn default_validation_rules() -> Vec<ValidationRule> {
        vec![
            ValidationRule {
                name: "system_name_required".to_string(),
                description: "System name must be provided".to_string(),
                severity: ValidationSeverity::Error,
                condition: ValidationCondition::FieldExists {
                    path: "system.name".to_string(),
                },
                message_template: "System name is required".to_string(),
            },
            ValidationRule {
                name: "listen_address_valid".to_string(),
                description: "Network listen address must be valid".to_string(),
                severity: ValidationSeverity::Error,
                condition: ValidationCondition::Custom {
                    validator_name: "socket_address_validator".to_string(),
                },
                message_template: "Invalid listen address format".to_string(),
            },
            ValidationRule {
                name: "database_url_format".to_string(),
                description: "Database URL must be properly formatted".to_string(),
                severity: ValidationSeverity::Warning,
                condition: ValidationCondition::FieldPattern {
                    path: "storage.database_url".to_string(),
                    pattern: r"^[a-zA-Z][a-zA-Z0-9+.-]*://".to_string(),
                },
                message_template: "Database URL should start with a valid scheme".to_string(),
            },
        ]
    }
}

impl ValidationEngine {
    /// Validate configuration against all rules
    fn validate(&self, config: &AlysConfig) -> ValidationResult {
        let mut result = ValidationResult {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            infos: Vec::new(),
        };
        
        // Run built-in validation rules
        for rule in &self.validation_rules {
            match rule.severity {
                ValidationSeverity::Error => {
                    if !self.check_rule(rule, config) {
                        result.is_valid = false;
                        result.errors.push(ValidationError {
                            rule_name: rule.name.clone(),
                            field_path: self.extract_field_path(&rule.condition),
                            message: rule.message_template.clone(),
                            severity: rule.severity,
                        });
                    }
                },
                ValidationSeverity::Warning => {
                    if !self.check_rule(rule, config) {
                        result.warnings.push(ValidationWarning {
                            rule_name: rule.name.clone(),
                            field_path: self.extract_field_path(&rule.condition),
                            message: rule.message_template.clone(),
                            suggestion: None,
                        });
                    }
                },
                ValidationSeverity::Info => {
                    if !self.check_rule(rule, config) {
                        result.infos.push(ValidationInfo {
                            rule_name: rule.name.clone(),
                            message: rule.message_template.clone(),
                        });
                    }
                },
            }
        }
        
        // Run custom validators
        for (name, validator) in &self.custom_validators {
            let custom_result = validator.validate(config);
            result.errors.extend(custom_result.errors);
            result.warnings.extend(custom_result.warnings);
            result.infos.extend(custom_result.infos);
            
            if !custom_result.is_valid {
                result.is_valid = false;
            }
        }
        
        result
    }
    
    /// Check individual validation rule
    fn check_rule(&self, rule: &ValidationRule, config: &AlysConfig) -> bool {
        match &rule.condition {
            ValidationCondition::FieldExists { path } => {
                // Simplified field existence check
                match path.as_str() {
                    "system.name" => !config.system.name.is_empty(),
                    _ => true, // Default to true for unknown paths
                }
            },
            ValidationCondition::Custom { validator_name } => {
                // Use custom validator
                self.custom_validators.get(validator_name)
                    .map(|validator| validator.validate(config).is_valid)
                    .unwrap_or(true)
            },
            _ => true, // Other conditions not implemented
        }
    }
    
    /// Extract field path from validation condition
    fn extract_field_path(&self, condition: &ValidationCondition) -> String {
        match condition {
            ValidationCondition::FieldExists { path } => path.clone(),
            ValidationCondition::FieldRange { path, .. } => path.clone(),
            ValidationCondition::FieldPattern { path, .. } => path.clone(),
            ValidationCondition::Dependency { field, .. } => field.clone(),
            ValidationCondition::Custom { .. } => "unknown".to_string(),
        }
    }
}

impl Default for WatchMode {
    fn default() -> Self {
        Self::Debounced { delay: Duration::from_millis(500) }
    }
}

impl Default for ReloadPriority {
    fn default() -> Self {
        Self::Normal
    }
}

impl Default for NotificationMode {
    fn default() -> Self {
        Self::Asynchronous
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl Default for ActorNotificationPreference {
    fn default() -> Self {
        Self {
            notification_mode: NotificationMode::default(),
            batch_updates: false,
            max_batch_size: 10,
            batch_timeout: Duration::from_secs(5),
            acknowledgment_required: false,
            retry_policy: RetryPolicy::default(),
        }
    }
}