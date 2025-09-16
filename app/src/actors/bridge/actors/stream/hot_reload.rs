//! Configuration Hot-Reload System
//! 
//! Advanced configuration management with file watching, validation,
//! and change notification for the StreamActor

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, RwLock};
use notify::{Watcher, RecursiveMode, Event, EventKind, event::AccessKind};
use futures::TryFutureExt;
use tracing::*;

use crate::actors::bridge::config::StreamConfig;
use crate::types::errors::BridgeError as ConfigError;

/// Configuration change notification system
#[derive(Debug, Clone)]
pub struct ConfigChangeNotification {
    pub field_path: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub change_type: ConfigChangeType,
    pub timestamp: std::time::SystemTime,
}

#[derive(Debug, Clone)]
pub enum ConfigChangeType {
    Added,
    Modified,
    Removed,
    Validated,
    ValidationFailed,
    FileChanged,
    ReloadTriggered,
}

/// Hot-reload configuration manager
pub struct ConfigHotReloadManager {
    config: Arc<RwLock<StreamConfig>>,
    file_path: PathBuf,
    watcher: Option<notify::RecommendedWatcher>,
    change_sender: watch::Sender<ConfigChangeNotification>,
    change_receiver: watch::Receiver<ConfigChangeNotification>,
    validation_enabled: bool,
    auto_reload: bool,
    reload_debounce: Duration,
    last_reload: Option<std::time::SystemTime>,
    reload_count: u64,
    error_count: u64,
}

impl ConfigHotReloadManager {
    /// Create new hot-reload manager
    pub fn new(
        initial_config: StreamConfig,
        file_path: PathBuf,
    ) -> Result<Self, ConfigError> {
        let (change_sender, change_receiver) = watch::channel(
            ConfigChangeNotification {
                field_path: "init".to_string(),
                old_value: None,
                new_value: None,
                change_type: ConfigChangeType::Added,
                timestamp: std::time::SystemTime::now(),
            }
        );

        Ok(Self {
            config: Arc::new(RwLock::new(initial_config)),
            file_path,
            watcher: None,
            change_sender,
            change_receiver,
            validation_enabled: true,
            auto_reload: true,
            reload_debounce: Duration::from_millis(500),
            last_reload: None,
            reload_count: 0,
            error_count: 0,
        })
    }

    /// Start file watching for hot-reload
    pub async fn start_watching(&mut self) -> Result<(), ConfigError> {
        info!("Starting configuration file watching: {:?}", self.file_path);
        
        let sender = self.change_sender.clone();
        let file_path = self.file_path.clone();
        let reload_debounce = self.reload_debounce;
        let config = Arc::clone(&self.config);
        let validation_enabled = self.validation_enabled;
        
        let mut watcher = notify::recommended_watcher(
            move |result: Result<Event, notify::Error>| {
                let sender = sender.clone();
                let file_path = file_path.clone();
                let config = Arc::clone(&config);
                
                tokio::spawn(async move {
                    match result {
                        Ok(event) => {
                            debug!("File system event: {:?}", event);
                            
                            // Check if it's our config file and a relevant event
                            if Self::should_trigger_reload(&event, &file_path) {
                                info!("Configuration file changed, triggering reload");
                                
                                // Send file change notification
                                let change_notification = ConfigChangeNotification {
                                    field_path: "file_system".to_string(),
                                    old_value: None,
                                    new_value: Some(format!("{:?}", event.paths)),
                                    change_type: ConfigChangeType::FileChanged,
                                    timestamp: std::time::SystemTime::now(),
                                };
                                let _ = sender.send(change_notification);
                                
                                // Debounce file changes
                                tokio::time::sleep(reload_debounce).await;
                                
                                // Trigger reload
                                let reload_notification = ConfigChangeNotification {
                                    field_path: "reload_trigger".to_string(),
                                    old_value: None,
                                    new_value: Some(file_path.to_string_lossy().to_string()),
                                    change_type: ConfigChangeType::ReloadTriggered,
                                    timestamp: std::time::SystemTime::now(),
                                };
                                let _ = sender.send(reload_notification);
                                
                                match Self::reload_config_from_file(&file_path, config, validation_enabled).await {
                                    Ok(changes) => {
                                        info!("Configuration reloaded successfully, {} changes detected", changes.len());
                                        for change in changes {
                                            let _ = sender.send(change);
                                        }
                                    },
                                    Err(e) => {
                                        error!("Failed to reload configuration: {:?}", e);
                                        let error_notification = ConfigChangeNotification {
                                            field_path: "reload_error".to_string(),
                                            old_value: None,
                                            new_value: Some(format!("{:?}", e)),
                                            change_type: ConfigChangeType::ValidationFailed,
                                            timestamp: std::time::SystemTime::now(),
                                        };
                                        let _ = sender.send(error_notification);
                                    }
                                }
                            }
                        },
                        Err(e) => {
                            error!("Configuration file watch error: {:?}", e);
                        }
                    }
                });
            }
        ).map_err(|e| ConfigError::ValidationError(format!("Failed to create file watcher: {:?}", e)))?;

        watcher.watch(&self.file_path, RecursiveMode::NonRecursive)
            .map_err(|e| ConfigError::ValidationError(format!("Failed to start watching config file: {:?}", e)))?;

        self.watcher = Some(watcher);
        info!("Configuration file watching started successfully");
        Ok(())
    }

    /// Stop file watching
    pub fn stop_watching(&mut self) {
        if self.watcher.is_some() {
            info!("Stopping configuration file watching");
            self.watcher = None;
        }
    }

    /// Get current configuration (read-only)
    pub async fn get_config(&self) -> StreamConfig {
        self.config.read().await.clone()
    }

    /// Update configuration with validation
    pub async fn update_config(&mut self, new_config: StreamConfig) -> Result<Vec<ConfigChangeNotification>, ConfigError> {
        if self.validation_enabled {
            new_config.validate()
                .map_err(|e| ConfigError::ValidationError(format!("Config validation failed: {:?}", e))).await?;
        }

        let mut config_guard = self.config.write().await;
        let changes = self.detect_changes(&*config_guard, &new_config);
        
        // Log configuration update
        info!("Updating configuration, {} changes detected", changes.len());
        for change in &changes {
            debug!("Config change: {} -> {:?}", change.field_path, change.change_type);
        }
        
        *config_guard = new_config;
        self.reload_count += 1;
        self.last_reload = Some(std::time::SystemTime::now());
        
        Ok(changes)
    }

    /// Force reload configuration from file
    pub async fn force_reload(&mut self) -> Result<Vec<ConfigChangeNotification>, ConfigError> {
        info!("Forcing configuration reload from file");
        
        match Self::reload_config_from_file(&self.file_path, Arc::clone(&self.config), self.validation_enabled).await {
            Ok(changes) => {
                self.reload_count += 1;
                self.last_reload = Some(std::time::SystemTime::now());
                
                info!("Configuration force-reloaded successfully, {} changes detected", changes.len());
                Ok(changes)
            },
            Err(e) => {
                self.error_count += 1;
                error!("Failed to force-reload configuration: {:?}", e);
                Err(e)
            }
        }
    }

    /// Get change notification receiver
    pub fn change_receiver(&self) -> watch::Receiver<ConfigChangeNotification> {
        self.change_receiver.clone()
    }

    /// Get reload statistics
    pub fn get_stats(&self) -> ReloadStats {
        ReloadStats {
            reload_count: self.reload_count,
            error_count: self.error_count,
            last_reload: self.last_reload,
            validation_enabled: self.validation_enabled,
            auto_reload: self.auto_reload,
            file_path: self.file_path.clone(),
        }
    }

    /// Enable or disable validation
    pub fn set_validation_enabled(&mut self, enabled: bool) {
        info!("Configuration validation {}", if enabled { "enabled" } else { "disabled" });
        self.validation_enabled = enabled;
    }

    /// Enable or disable auto-reload
    pub fn set_auto_reload_enabled(&mut self, enabled: bool) {
        info!("Configuration auto-reload {}", if enabled { "enabled" } else { "disabled" });
        self.auto_reload = enabled;
        
        if !enabled && self.watcher.is_some() {
            self.stop_watching();
        }
    }

    /// Set reload debounce duration
    pub fn set_reload_debounce(&mut self, duration: Duration) {
        info!("Configuration reload debounce set to {:?}", duration);
        self.reload_debounce = duration;
    }

    /// Check if file system event should trigger reload
    fn should_trigger_reload(event: &Event, file_path: &Path) -> bool {
        match &event.kind {
            // File was written to or closed after writing
            EventKind::Access(AccessKind::Close(_)) |
            EventKind::Modify(_) => {
                event.paths.iter().any(|p| p == file_path)
            },
            _ => false,
        }
    }

    /// Reload configuration from file
    async fn reload_config_from_file(
        file_path: &Path,
        config: Arc<RwLock<StreamConfig>>,
        validation_enabled: bool,
    ) -> Result<Vec<ConfigChangeNotification>, ConfigError> {
        debug!("Loading configuration from file: {:?}", file_path);

        let config_content = tokio::fs::read_to_string(file_path).await
            .map_err(|e| ConfigError::ConfigurationError(format!("Failed to read config file: {}", e)))?;
        let new_config: StreamConfig = serde_json::from_str(&config_content)
            .map_err(|e| ConfigError::ConfigurationError(format!("Failed to parse config: {}", e)))?;
        
        if validation_enabled {
            new_config.validate()
                .map_err(|e| ConfigError::ValidationError(format!("Config validation failed: {:?}", e)))
                .await?;

            debug!("Configuration validation passed");
        }

        let mut config_guard = config.write().await;
        let changes = Self::detect_changes_static(&*config_guard, &new_config);
        *config_guard = new_config;
        
        Ok(changes)
    }

    /// Detect configuration changes
    fn detect_changes(&self, old_config: &StreamConfig, new_config: &StreamConfig) -> Vec<ConfigChangeNotification> {
        Self::detect_changes_static(old_config, new_config)
    }

    /// Static method for detecting changes
    fn detect_changes_static(old_config: &StreamConfig, new_config: &StreamConfig) -> Vec<ConfigChangeNotification> {
        let mut changes = Vec::new();
        let timestamp = std::time::SystemTime::now();

        // Governance endpoints changes
        if old_config.governance_endpoints != new_config.governance_endpoints {
            changes.push(ConfigChangeNotification {
                field_path: "governance_endpoints".to_string(),
                old_value: Some(format!("{:?}", old_config.governance_endpoints)),
                new_value: Some(format!("{:?}", new_config.governance_endpoints)),
                change_type: ConfigChangeType::Modified,
                timestamp,
            });
        }

        // Connection timeout changes
        if old_config.connection_timeout != new_config.connection_timeout {
            changes.push(ConfigChangeNotification {
                field_path: "connection_timeout".to_string(),
                old_value: Some(format!("{:?}", old_config.connection_timeout)),
                new_value: Some(format!("{:?}", new_config.connection_timeout)),
                change_type: ConfigChangeType::Modified,
                timestamp,
            });
        }

        // Heartbeat interval changes
        if old_config.heartbeat_interval != new_config.heartbeat_interval {
            changes.push(ConfigChangeNotification {
                field_path: "heartbeat_interval".to_string(),
                old_value: Some(format!("{:?}", old_config.heartbeat_interval)),
                new_value: Some(format!("{:?}", new_config.heartbeat_interval)),
                change_type: ConfigChangeType::Modified,
                timestamp,
            });
        }

        // Max connections changes
        if old_config.max_connections != new_config.max_connections {
            changes.push(ConfigChangeNotification {
                field_path: "max_connections".to_string(),
                old_value: Some(old_config.max_connections.to_string()),
                new_value: Some(new_config.max_connections.to_string()),
                change_type: ConfigChangeType::Modified,
                timestamp,
            });
        }

        // Authentication token changes (don't log sensitive data)
        if old_config.auth_token != new_config.auth_token {
            changes.push(ConfigChangeNotification {
                field_path: "auth_token".to_string(),
                old_value: None, // Don't log sensitive auth data
                new_value: None,
                change_type: ConfigChangeType::Modified,
                timestamp,
            });
        }

        changes
    }
}

/// Reload statistics
#[derive(Debug, Clone)]
pub struct ReloadStats {
    pub reload_count: u64,
    pub error_count: u64,
    pub last_reload: Option<std::time::SystemTime>,
    pub validation_enabled: bool,
    pub auto_reload: bool,
    pub file_path: PathBuf,
}

/// Configuration validation trait
#[async_trait::async_trait]
pub trait ConfigValidator {
    type Error;
    
    async fn validate(&self) -> Result<(), Self::Error>;
    fn validate_field(&self, field_name: &str) -> Result<(), Self::Error>;
}

#[async_trait::async_trait]
impl ConfigValidator for StreamConfig {
    type Error = ConfigError;

    async fn validate(&self) -> Result<(), Self::Error> {
        debug!("Starting configuration validation");

        // Basic validation
        self.validate_connection_limits()?;
        self.validate_timeout_relationships()?;

        info!("Configuration validation completed successfully");
        Ok(())
    }

    fn validate_field(&self, field_name: &str) -> Result<(), Self::Error> {
        debug!("Validating field: {}", field_name);

        match field_name {
            "governance_endpoints" => {
                if self.governance_endpoints.is_empty() {
                    Err(ConfigError::ConfigurationError("governance_endpoints cannot be empty".to_string()))
                } else {
                    Ok(())
                }
            },
            "max_connections" => {
                if self.max_connections == 0 {
                    Err(ConfigError::ConfigurationError("max_connections must be greater than 0".to_string()))
                } else {
                    Ok(())
                }
            },
            "message_buffer_size" => {
                if self.message_buffer_size == 0 {
                    Err(ConfigError::ConfigurationError("message_buffer_size must be greater than 0".to_string()))
                } else {
                    Ok(())
                }
            },
            _ => Err(ConfigError::ConfigurationError(format!("Unknown field: {}", field_name))),
        }
    }
}

impl StreamConfig {
    /// Validate connection limits
    fn validate_connection_limits(&self) -> Result<(), ConfigError> {
        if self.max_connections == 0 {
            return Err(ConfigError::ConfigurationError("max_connections must be greater than 0".to_string()));
        }

        if self.message_buffer_size == 0 {
            return Err(ConfigError::ConfigurationError("message_buffer_size must be greater than 0".to_string()));
        }

        Ok(())
    }

    /// Validate timeout relationships
    fn validate_timeout_relationships(&self) -> Result<(), ConfigError> {
        if self.heartbeat_interval >= self.connection_timeout {
            return Err(ConfigError::ConfigurationError("heartbeat_interval should be less than connection_timeout".to_string()));
        }

        if self.reconnect_delay >= self.connection_timeout {
            warn!("Reconnect delay is greater than or equal to connection timeout, this may cause long delays");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_config_hot_reload_manager_creation() {
        let config = StreamConfig::default();
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_path_buf();

        let manager = ConfigHotReloadManager::new(config.clone(), file_path).unwrap();
        let loaded_config = manager.get_config().await;

        // Basic sanity check
        assert_eq!(loaded_config.max_connections, config.max_connections);
    }

    #[tokio::test]
    async fn test_config_validation() {
        let config = StreamConfig::default();
        let result = config.validate().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_config_update_with_validation() {
        let config = StreamConfig::default();
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_path_buf();

        let mut manager = ConfigHotReloadManager::new(config.clone(), file_path).unwrap();

        let mut new_config = config.clone();
        new_config.max_connections = 20;

        let changes = manager.update_config(new_config).await.unwrap();
        assert!(!changes.is_empty());
    }
}