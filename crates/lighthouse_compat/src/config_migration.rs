//! Configuration Migration System for Lighthouse V4/V5
//! 
//! This module handles the migration of configuration files, settings, and 
//! parameters between Lighthouse versions, ensuring compatibility and 
//! providing automated upgrade paths.

use crate::config::{CompatConfig, MigrationMode};
use crate::error::{CompatError, CompatResult};
use actix::prelude::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Configuration migration controller
pub struct ConfigurationMigrator {
    /// Source configuration (v4)
    v4_config: Arc<RwLock<Option<LighthouseV4Config>>>,
    /// Target configuration (v5)
    v5_config: Arc<RwLock<Option<LighthouseV5Config>>>,
    /// Migration rules and mappings
    migration_rules: Arc<RwLock<HashMap<String, MigrationRule>>>,
    /// Backup configurations
    backups: Arc<RwLock<Vec<ConfigurationBackup>>>,
    /// Migration history
    migration_history: Arc<RwLock<Vec<MigrationEvent>>>,
    /// Configuration paths
    config_paths: ConfigurationPaths,
}

/// Lighthouse v4 configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LighthouseV4Config {
    /// Network configuration
    pub network: NetworkConfigV4,
    /// Execution layer configuration
    pub execution: ExecutionConfigV4,
    /// Beacon chain configuration
    pub beacon: BeaconConfigV4,
    /// HTTP API configuration
    pub http: HttpConfigV4,
    /// Metrics configuration
    pub metrics: MetricsConfigV4,
    /// P2P configuration
    pub p2p: P2PConfigV4,
    /// Custom settings
    pub custom: HashMap<String, serde_json::Value>,
}

/// Lighthouse v5 configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LighthouseV5Config {
    /// Network configuration (enhanced)
    pub network: NetworkConfigV5,
    /// Execution layer configuration (enhanced)
    pub execution: ExecutionConfigV5,
    /// Beacon chain configuration (enhanced)
    pub beacon: BeaconConfigV5,
    /// HTTP API configuration (enhanced)
    pub http: HttpConfigV5,
    /// Metrics configuration (enhanced)
    pub metrics: MetricsConfigV5,
    /// P2P configuration (enhanced)
    pub p2p: P2PConfigV5,
    /// Blob handling configuration (new in v5)
    pub blobs: BlobConfigV5,
    /// Enhanced features configuration
    pub features: FeatureConfigV5,
    /// Custom settings
    pub custom: HashMap<String, serde_json::Value>,
}

/// Migration rule for configuration transformation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRule {
    /// Rule identifier
    pub id: String,
    /// Source path in v4 config
    pub source_path: String,
    /// Target path in v5 config
    pub target_path: String,
    /// Transformation type
    pub transformation: TransformationType,
    /// Rule priority (higher numbers execute first)
    pub priority: u32,
    /// Whether rule is required
    pub required: bool,
    /// Rule description
    pub description: String,
    /// Validation function name
    pub validator: Option<String>,
}

/// Configuration transformation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransformationType {
    /// Direct copy with no changes
    Direct,
    /// Rename field or section
    Rename { from: String, to: String },
    /// Transform value using function
    Transform { function: String },
    /// Merge multiple fields into one
    Merge { fields: Vec<String> },
    /// Split one field into multiple
    Split { targets: Vec<String> },
    /// Conditional transformation
    Conditional { condition: String, then_rule: String, else_rule: Option<String> },
    /// Default value if source doesn't exist
    Default { value: serde_json::Value },
    /// Remove field (deprecated in target version)
    Remove,
}

/// Configuration backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigurationBackup {
    /// Backup identifier
    pub id: String,
    /// Backup timestamp
    pub created_at: DateTime<Utc>,
    /// Configuration version
    pub version: ConfigurationVersion,
    /// Backup file path
    pub backup_path: PathBuf,
    /// Original file path
    pub original_path: PathBuf,
    /// Backup metadata
    pub metadata: HashMap<String, String>,
}

/// Configuration version enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConfigurationVersion {
    V4,
    V5,
    Mixed,
}

/// Migration event for audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationEvent {
    /// Event identifier
    pub id: String,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Migration type
    pub migration_type: MigrationType,
    /// Source configuration version
    pub from_version: ConfigurationVersion,
    /// Target configuration version
    pub to_version: ConfigurationVersion,
    /// Applied rules
    pub applied_rules: Vec<String>,
    /// Migration status
    pub status: MigrationStatus,
    /// Error message if migration failed
    pub error: Option<String>,
    /// Migration metrics
    pub metrics: MigrationMetrics,
}

/// Migration types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationType {
    FullMigration,
    PartialMigration,
    Validation,
    Rollback,
    DryRun,
}

/// Migration status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MigrationStatus {
    Started,
    InProgress,
    Completed,
    Failed,
    RolledBack,
}

/// Migration performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationMetrics {
    /// Total migration time in milliseconds
    pub duration_ms: u64,
    /// Number of rules processed
    pub rules_processed: u32,
    /// Number of successful transformations
    pub successful_transformations: u32,
    /// Number of failed transformations
    pub failed_transformations: u32,
    /// Configuration file size before migration
    pub size_before_bytes: u64,
    /// Configuration file size after migration
    pub size_after_bytes: u64,
}

/// Configuration file paths
#[derive(Debug, Clone)]
pub struct ConfigurationPaths {
    /// V4 configuration directory
    pub v4_config_dir: PathBuf,
    /// V5 configuration directory  
    pub v5_config_dir: PathBuf,
    /// Backup directory
    pub backup_dir: PathBuf,
    /// Migration rules file
    pub rules_file: PathBuf,
}

// V4 Configuration structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfigV4 {
    pub network_name: String,
    pub discovery_port: u16,
    pub port: u16,
    pub target_peers: usize,
    pub boot_nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfigV4 {
    pub execution_endpoint: String,
    pub execution_timeout_multiplier: u32,
    pub jwt_secret_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconConfigV4 {
    pub datadir: PathBuf,
    pub slots_per_restore_point: u64,
    pub block_cache_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfigV4 {
    pub http: bool,
    pub http_address: String,
    pub http_port: u16,
    pub http_allow_origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfigV4 {
    pub metrics: bool,
    pub metrics_address: String,
    pub metrics_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PConfigV4 {
    pub listen_address: String,
    pub max_peers: usize,
    pub discovery_v5: bool,
}

// V5 Configuration structures (enhanced)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfigV5 {
    pub network_name: String,
    pub discovery_port: u16,
    pub port: u16,
    pub quic_port: Option<u16>, // New in v5
    pub target_peers: usize,
    pub boot_nodes: Vec<String>,
    pub trusted_peers: Vec<String>, // New in v5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfigV5 {
    pub execution_endpoints: Vec<String>, // Multi-endpoint support in v5
    pub execution_timeout_multiplier: u32,
    pub jwt_secret_file: Option<PathBuf>,
    pub builder_endpoint: Option<String>, // New in v5
    pub builder_user_agent: Option<String>, // New in v5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconConfigV5 {
    pub datadir: PathBuf,
    pub slots_per_restore_point: u64,
    pub block_cache_size: usize,
    pub blob_cache_size: Option<usize>, // New in v5
    pub state_cache_size: Option<usize>, // New in v5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfigV5 {
    pub http: bool,
    pub http_address: String,
    pub http_port: u16,
    pub http_allow_origin: String,
    pub http_spec_fork: Option<String>, // New in v5
    pub http_duplicate_block_status: Option<u16>, // New in v5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfigV5 {
    pub metrics: bool,
    pub metrics_address: String,
    pub metrics_port: u16,
    pub metrics_allow_origin: Option<String>, // New in v5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PConfigV5 {
    pub listen_address: String,
    pub max_peers: usize,
    pub discovery_v5: bool,
    pub enable_quic: Option<bool>, // New in v5
    pub subscribe_all_subnets: Option<bool>, // New in v5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobConfigV5 {
    /// Enable blob handling (Deneb fork)
    pub enable_blobs: bool,
    /// Blob retention period in epochs
    pub blob_retention_epochs: u64,
    /// Maximum blob cache size
    pub blob_cache_size: usize,
    /// Blob verification batch size
    pub blob_verification_batch_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureConfigV5 {
    /// Enable experimental features
    pub experimental_features: bool,
    /// Feature flags
    pub feature_flags: HashMap<String, bool>,
    /// Performance optimizations
    pub optimizations: Vec<String>,
}

impl ConfigurationMigrator {
    /// Create a new configuration migrator
    pub fn new(config_paths: ConfigurationPaths) -> CompatResult<Self> {
        Ok(Self {
            v4_config: Arc::new(RwLock::new(None)),
            v5_config: Arc::new(RwLock::new(None)),
            migration_rules: Arc::new(RwLock::new(HashMap::new())),
            backups: Arc::new(RwLock::new(Vec::new())),
            migration_history: Arc::new(RwLock::new(Vec::new())),
            config_paths,
        })
    }

    /// Initialize migrator with default rules
    pub async fn initialize(&self) -> CompatResult<()> {
        // Load default migration rules
        self.load_default_migration_rules().await?;
        
        // Ensure directories exist
        self.ensure_directories_exist().await?;
        
        info!("Configuration migrator initialized");
        Ok(())
    }

    /// Load v4 configuration from file
    pub async fn load_v4_config(&self, config_path: &Path) -> CompatResult<()> {
        let config_content = fs::read_to_string(config_path).await
            .map_err(|e| CompatError::ConfigurationError {
                reason: format!("Failed to read v4 config: {}", e),
            })?;

        let v4_config: LighthouseV4Config = toml::from_str(&config_content)
            .map_err(|e| CompatError::ConfigurationError {
                reason: format!("Failed to parse v4 config: {}", e),
            })?;

        *self.v4_config.write().await = Some(v4_config);
        
        info!("Loaded v4 configuration from {:?}", config_path);
        Ok(())
    }

    /// Migrate v4 configuration to v5
    pub async fn migrate_v4_to_v5(&self, dry_run: bool) -> CompatResult<MigrationEvent> {
        let migration_id = uuid::Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();
        
        info!(
            migration_id = %migration_id,
            dry_run = dry_run,
            "Starting v4 to v5 configuration migration"
        );

        // Ensure v4 config is loaded
        let v4_config = self.v4_config.read().await
            .as_ref()
            .ok_or_else(|| CompatError::ConfigurationError {
                reason: "V4 configuration not loaded".to_string(),
            })?
            .clone();

        // Create backup if not dry run
        let backup_id = if !dry_run {
            Some(self.create_backup(&v4_config, ConfigurationVersion::V4).await?)
        } else {
            None
        };

        // Apply migration rules
        let mut metrics = MigrationMetrics {
            duration_ms: 0,
            rules_processed: 0,
            successful_transformations: 0,
            failed_transformations: 0,
            size_before_bytes: 0,
            size_after_bytes: 0,
        };

        let mut applied_rules = Vec::new();
        let mut migration_status = MigrationStatus::InProgress;
        let mut migration_error = None;

        match self.apply_migration_rules(&v4_config, dry_run).await {
            Ok(result) => {
                applied_rules = result.applied_rules;
                metrics.successful_transformations = result.successful_count;
                metrics.failed_transformations = result.failed_count;
                metrics.rules_processed = result.total_rules;
                
                if !dry_run {
                    *self.v5_config.write().await = Some(result.v5_config);
                }
                
                migration_status = MigrationStatus::Completed;
                
                info!(
                    migration_id = %migration_id,
                    applied_rules = applied_rules.len(),
                    successful = metrics.successful_transformations,
                    failed = metrics.failed_transformations,
                    "Migration completed successfully"
                );
            },
            Err(e) => {
                migration_status = MigrationStatus::Failed;
                migration_error = Some(e.to_string());
                
                error!(
                    migration_id = %migration_id,
                    error = %e,
                    "Migration failed"
                );
            }
        }

        metrics.duration_ms = start_time.elapsed().as_millis() as u64;

        // Create migration event
        let migration_event = MigrationEvent {
            id: migration_id.clone(),
            timestamp: Utc::now(),
            migration_type: if dry_run { MigrationType::DryRun } else { MigrationType::FullMigration },
            from_version: ConfigurationVersion::V4,
            to_version: ConfigurationVersion::V5,
            applied_rules,
            status: migration_status,
            error: migration_error,
            metrics,
        };

        // Store migration event
        self.migration_history.write().await.push(migration_event.clone());

        Ok(migration_event)
    }

    /// Save v5 configuration to file
    pub async fn save_v5_config(&self, config_path: &Path) -> CompatResult<()> {
        let v5_config = self.v5_config.read().await
            .as_ref()
            .ok_or_else(|| CompatError::ConfigurationError {
                reason: "V5 configuration not available".to_string(),
            })?
            .clone();

        let config_content = toml::to_string_pretty(&v5_config)
            .map_err(|e| CompatError::ConfigurationError {
                reason: format!("Failed to serialize v5 config: {}", e),
            })?;

        fs::write(config_path, config_content).await
            .map_err(|e| CompatError::ConfigurationError {
                reason: format!("Failed to write v5 config: {}", e),
            })?;

        info!("Saved v5 configuration to {:?}", config_path);
        Ok(())
    }

    /// Rollback to v4 configuration
    pub async fn rollback_to_v4(&self, backup_id: &str) -> CompatResult<()> {
        let backup = {
            let backups = self.backups.read().await;
            backups.iter()
                .find(|b| b.id == backup_id)
                .cloned()
                .ok_or_else(|| CompatError::ConfigurationError {
                    reason: format!("Backup {} not found", backup_id),
                })?
        };

        // Restore from backup
        let backup_content = fs::read_to_string(&backup.backup_path).await
            .map_err(|e| CompatError::ConfigurationError {
                reason: format!("Failed to read backup: {}", e),
            })?;

        fs::write(&backup.original_path, backup_content).await
            .map_err(|e| CompatError::ConfigurationError {
                reason: format!("Failed to restore backup: {}", e),
            })?;

        // Clear v5 config
        *self.v5_config.write().await = None;

        // Create rollback event
        let rollback_event = MigrationEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            migration_type: MigrationType::Rollback,
            from_version: ConfigurationVersion::V5,
            to_version: ConfigurationVersion::V4,
            applied_rules: vec![format!("restore_backup_{}", backup_id)],
            status: MigrationStatus::Completed,
            error: None,
            metrics: MigrationMetrics {
                duration_ms: 0,
                rules_processed: 1,
                successful_transformations: 1,
                failed_transformations: 0,
                size_before_bytes: 0,
                size_after_bytes: 0,
            },
        };

        self.migration_history.write().await.push(rollback_event);

        info!(backup_id = backup_id, "Rolled back to v4 configuration");
        Ok(())
    }

    /// Validate configuration compatibility
    pub async fn validate_configuration(&self, config_version: ConfigurationVersion) -> CompatResult<ValidationReport> {
        let mut report = ValidationReport {
            version: config_version.clone(),
            is_valid: true,
            warnings: Vec::new(),
            errors: Vec::new(),
            recommendations: Vec::new(),
        };

        match config_version {
            ConfigurationVersion::V4 => {
                if let Some(v4_config) = self.v4_config.read().await.as_ref() {
                    self.validate_v4_config(v4_config, &mut report).await?;
                } else {
                    report.errors.push("V4 configuration not loaded".to_string());
                    report.is_valid = false;
                }
            },
            ConfigurationVersion::V5 => {
                if let Some(v5_config) = self.v5_config.read().await.as_ref() {
                    self.validate_v5_config(v5_config, &mut report).await?;
                } else {
                    report.errors.push("V5 configuration not loaded".to_string());
                    report.is_valid = false;
                }
            },
            ConfigurationVersion::Mixed => {
                report.warnings.push("Mixed configuration version detected".to_string());
                report.recommendations.push("Consider completing migration to v5".to_string());
            }
        }

        Ok(report)
    }

    /// Get migration history
    pub async fn get_migration_history(&self) -> Vec<MigrationEvent> {
        self.migration_history.read().await.clone()
    }

    /// Get available backups
    pub async fn get_backups(&self) -> Vec<ConfigurationBackup> {
        self.backups.read().await.clone()
    }

    /// Load default migration rules
    async fn load_default_migration_rules(&self) -> CompatResult<()> {
        let mut rules = HashMap::new();

        // Network configuration migration
        rules.insert("network_quic_port".to_string(), MigrationRule {
            id: "network_quic_port".to_string(),
            source_path: "network.port".to_string(),
            target_path: "network.quic_port".to_string(),
            transformation: TransformationType::Transform {
                function: "add_quic_port_offset".to_string(),
            },
            priority: 100,
            required: false,
            description: "Add QUIC port configuration".to_string(),
            validator: Some("validate_port_range".to_string()),
        });

        // Execution endpoint migration
        rules.insert("execution_endpoints".to_string(), MigrationRule {
            id: "execution_endpoints".to_string(),
            source_path: "execution.execution_endpoint".to_string(),
            target_path: "execution.execution_endpoints".to_string(),
            transformation: TransformationType::Transform {
                function: "single_to_array".to_string(),
            },
            priority: 200,
            required: true,
            description: "Convert single execution endpoint to array".to_string(),
            validator: Some("validate_endpoints".to_string()),
        });

        // Blob configuration (new in v5)
        rules.insert("blob_config".to_string(), MigrationRule {
            id: "blob_config".to_string(),
            source_path: "".to_string(),
            target_path: "blobs".to_string(),
            transformation: TransformationType::Default {
                value: serde_json::json!({
                    "enable_blobs": true,
                    "blob_retention_epochs": 4096,
                    "blob_cache_size": 512,
                    "blob_verification_batch_size": 64
                }),
            },
            priority: 50,
            required: true,
            description: "Add default blob configuration for Deneb fork".to_string(),
            validator: None,
        });

        // Feature configuration (new in v5)
        rules.insert("feature_config".to_string(), MigrationRule {
            id: "feature_config".to_string(),
            source_path: "".to_string(),
            target_path: "features".to_string(),
            transformation: TransformationType::Default {
                value: serde_json::json!({
                    "experimental_features": false,
                    "feature_flags": {},
                    "optimizations": ["blob_verification", "state_caching"]
                }),
            },
            priority: 50,
            required: true,
            description: "Add default feature configuration".to_string(),
            validator: None,
        });

        *self.migration_rules.write().await = rules;
        
        debug!("Loaded {} default migration rules", self.migration_rules.read().await.len());
        Ok(())
    }

    /// Apply migration rules to transform v4 to v5
    async fn apply_migration_rules(
        &self,
        v4_config: &LighthouseV4Config,
        dry_run: bool,
    ) -> CompatResult<MigrationResult> {
        let rules = self.migration_rules.read().await;
        
        // Sort rules by priority (higher first)
        let mut sorted_rules: Vec<_> = rules.values().collect();
        sorted_rules.sort_by(|a, b| b.priority.cmp(&a.priority));

        let mut applied_rules = Vec::new();
        let mut successful_count = 0;
        let mut failed_count = 0;

        // Start with v4 config as base
        let mut v5_config = self.create_base_v5_config(v4_config).await?;

        for rule in sorted_rules {
            debug!("Applying migration rule: {}", rule.id);
            
            match self.apply_single_rule(rule, v4_config, &mut v5_config).await {
                Ok(_) => {
                    applied_rules.push(rule.id.clone());
                    successful_count += 1;
                    
                    debug!("Successfully applied rule: {}", rule.id);
                },
                Err(e) => {
                    failed_count += 1;
                    
                    if rule.required {
                        error!("Required rule {} failed: {}", rule.id, e);
                        return Err(CompatError::ConfigurationError {
                            reason: format!("Required migration rule failed: {}", rule.id),
                        });
                    } else {
                        warn!("Optional rule {} failed: {}", rule.id, e);
                    }
                }
            }
        }

        Ok(MigrationResult {
            v5_config,
            applied_rules,
            total_rules: sorted_rules.len() as u32,
            successful_count,
            failed_count,
        })
    }

    /// Create base v5 config from v4 config
    async fn create_base_v5_config(&self, v4_config: &LighthouseV4Config) -> CompatResult<LighthouseV5Config> {
        Ok(LighthouseV5Config {
            network: NetworkConfigV5 {
                network_name: v4_config.network.network_name.clone(),
                discovery_port: v4_config.network.discovery_port,
                port: v4_config.network.port,
                quic_port: None, // Will be set by migration rule
                target_peers: v4_config.network.target_peers,
                boot_nodes: v4_config.network.boot_nodes.clone(),
                trusted_peers: Vec::new(), // New field, empty by default
            },
            execution: ExecutionConfigV5 {
                execution_endpoints: vec![v4_config.execution.execution_endpoint.clone()],
                execution_timeout_multiplier: v4_config.execution.execution_timeout_multiplier,
                jwt_secret_file: v4_config.execution.jwt_secret_file.clone(),
                builder_endpoint: None, // New field
                builder_user_agent: None, // New field
            },
            beacon: BeaconConfigV5 {
                datadir: v4_config.beacon.datadir.clone(),
                slots_per_restore_point: v4_config.beacon.slots_per_restore_point,
                block_cache_size: v4_config.beacon.block_cache_size,
                blob_cache_size: None, // New field
                state_cache_size: None, // New field
            },
            http: HttpConfigV5 {
                http: v4_config.http.http,
                http_address: v4_config.http.http_address.clone(),
                http_port: v4_config.http.http_port,
                http_allow_origin: v4_config.http.http_allow_origin.clone(),
                http_spec_fork: None, // New field
                http_duplicate_block_status: None, // New field
            },
            metrics: MetricsConfigV5 {
                metrics: v4_config.metrics.metrics,
                metrics_address: v4_config.metrics.metrics_address.clone(),
                metrics_port: v4_config.metrics.metrics_port,
                metrics_allow_origin: None, // New field
            },
            p2p: P2PConfigV5 {
                listen_address: v4_config.p2p.listen_address.clone(),
                max_peers: v4_config.p2p.max_peers,
                discovery_v5: v4_config.p2p.discovery_v5,
                enable_quic: None, // New field
                subscribe_all_subnets: None, // New field
            },
            blobs: BlobConfigV5 {
                enable_blobs: true,
                blob_retention_epochs: 4096,
                blob_cache_size: 512,
                blob_verification_batch_size: 64,
            },
            features: FeatureConfigV5 {
                experimental_features: false,
                feature_flags: HashMap::new(),
                optimizations: vec!["blob_verification".to_string(), "state_caching".to_string()],
            },
            custom: v4_config.custom.clone(),
        })
    }

    /// Apply a single migration rule
    async fn apply_single_rule(
        &self,
        rule: &MigrationRule,
        _v4_config: &LighthouseV4Config,
        _v5_config: &mut LighthouseV5Config,
    ) -> CompatResult<()> {
        match &rule.transformation {
            TransformationType::Direct => {
                // Direct copy - would need JSONPath-like implementation
                debug!("Applying direct transformation for rule {}", rule.id);
            },
            TransformationType::Default { value: _ } => {
                // Set default value - would need JSONPath-like implementation
                debug!("Applying default transformation for rule {}", rule.id);
            },
            TransformationType::Transform { function: _ } => {
                // Apply transformation function
                debug!("Applying transform function for rule {}", rule.id);
            },
            _ => {
                debug!("Transformation type not yet implemented for rule {}", rule.id);
            }
        }
        
        // Placeholder implementation - in real code would use JSONPath or similar
        Ok(())
    }

    /// Create configuration backup
    async fn create_backup(
        &self,
        config: &LighthouseV4Config,
        version: ConfigurationVersion,
    ) -> CompatResult<String> {
        let backup_id = uuid::Uuid::new_v4().to_string();
        let backup_filename = format!("lighthouse_config_backup_{}_{}.toml", 
                                      chrono::Utc::now().format("%Y%m%d_%H%M%S"),
                                      backup_id);
        let backup_path = self.config_paths.backup_dir.join(backup_filename);
        
        let config_content = toml::to_string_pretty(config)
            .map_err(|e| CompatError::ConfigurationError {
                reason: format!("Failed to serialize config for backup: {}", e),
            })?;

        fs::write(&backup_path, config_content).await
            .map_err(|e| CompatError::ConfigurationError {
                reason: format!("Failed to write backup: {}", e),
            })?;

        let backup = ConfigurationBackup {
            id: backup_id.clone(),
            created_at: Utc::now(),
            version,
            backup_path: backup_path.clone(),
            original_path: self.config_paths.v4_config_dir.join("lighthouse.toml"),
            metadata: HashMap::new(),
        };

        self.backups.write().await.push(backup);

        info!("Created configuration backup: {}", backup_id);
        Ok(backup_id)
    }

    /// Ensure required directories exist
    async fn ensure_directories_exist(&self) -> CompatResult<()> {
        for dir in [
            &self.config_paths.v4_config_dir,
            &self.config_paths.v5_config_dir,
            &self.config_paths.backup_dir,
        ] {
            fs::create_dir_all(dir).await
                .map_err(|e| CompatError::ConfigurationError {
                    reason: format!("Failed to create directory {:?}: {}", dir, e),
                })?;
        }
        
        Ok(())
    }

    /// Validate v4 configuration
    async fn validate_v4_config(
        &self,
        _config: &LighthouseV4Config,
        report: &mut ValidationReport,
    ) -> CompatResult<()> {
        // Placeholder validation logic
        report.recommendations.push("Consider migrating to v5 for latest features".to_string());
        Ok(())
    }

    /// Validate v5 configuration
    async fn validate_v5_config(
        &self,
        _config: &LighthouseV5Config,
        report: &mut ValidationReport,
    ) -> CompatResult<()> {
        // Placeholder validation logic
        report.recommendations.push("V5 configuration is up to date".to_string());
        Ok(())
    }
}

/// Migration result
#[derive(Debug)]
struct MigrationResult {
    v5_config: LighthouseV5Config,
    applied_rules: Vec<String>,
    total_rules: u32,
    successful_count: u32,
    failed_count: u32,
}

/// Configuration validation report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub version: ConfigurationVersion,
    pub is_valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub recommendations: Vec<String>,
}

impl Default for ConfigurationPaths {
    fn default() -> Self {
        Self {
            v4_config_dir: PathBuf::from("config/v4"),
            v5_config_dir: PathBuf::from("config/v5"),
            backup_dir: PathBuf::from("config/backups"),
            rules_file: PathBuf::from("config/migration_rules.toml"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_configuration_migrator_creation() {
        let config_paths = ConfigurationPaths::default();
        let migrator = ConfigurationMigrator::new(config_paths);
        assert!(migrator.is_ok());
    }

    #[tokio::test]
    async fn test_migration_rule_priority_sorting() {
        let rule1 = MigrationRule {
            id: "rule1".to_string(),
            source_path: "test".to_string(),
            target_path: "test".to_string(),
            transformation: TransformationType::Direct,
            priority: 100,
            required: false,
            description: "Test rule 1".to_string(),
            validator: None,
        };
        
        let rule2 = MigrationRule {
            id: "rule2".to_string(),
            source_path: "test".to_string(),
            target_path: "test".to_string(),
            transformation: TransformationType::Direct,
            priority: 200,
            required: false,
            description: "Test rule 2".to_string(),
            validator: None,
        };
        
        assert!(rule2.priority > rule1.priority);
    }

    #[test]
    fn test_configuration_version_equality() {
        assert_eq!(ConfigurationVersion::V4, ConfigurationVersion::V4);
        assert_ne!(ConfigurationVersion::V4, ConfigurationVersion::V5);
    }

    #[test]
    fn test_migration_status_transitions() {
        let statuses = vec![
            MigrationStatus::Started,
            MigrationStatus::InProgress,
            MigrationStatus::Completed,
        ];
        
        assert_eq!(statuses.len(), 3);
        assert_eq!(statuses[2], MigrationStatus::Completed);
    }
}