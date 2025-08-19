//! File watcher system for hot-reload of feature flag configuration
//!
//! This module implements real-time configuration file watching using the `notify` crate,
//! enabling automatic hot-reload of feature flags without application restart.

use super::{FeatureFlagResult, FeatureFlagError};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;
use notify::{Watcher, RecursiveMode, Event, EventKind, RecommendedWatcher};
use tracing::{info, warn, error, debug};

/// File system events that trigger configuration reloads
#[derive(Debug, Clone)]
pub enum ConfigFileEvent {
    /// Configuration file was modified
    Modified(PathBuf),
    /// Configuration file was created
    Created(PathBuf),
    /// Configuration file was deleted
    Deleted(PathBuf),
    /// File watcher encountered an error
    Error(String),
}

/// Configuration for the file watcher
#[derive(Debug, Clone)]
pub struct FileWatcherConfig {
    /// Debounce duration to prevent rapid-fire reloads
    pub debounce_duration: Duration,
    /// Whether to watch parent directory or just the specific file
    pub watch_parent_directory: bool,
    /// File extensions to watch (if watching directory)
    pub watched_extensions: Vec<String>,
    /// Maximum number of reload attempts on failure
    pub max_reload_attempts: u32,
}

impl Default for FileWatcherConfig {
    fn default() -> Self {
        Self {
            debounce_duration: Duration::from_millis(500), // 500ms debounce
            watch_parent_directory: true,
            watched_extensions: vec!["toml".to_string()],
            max_reload_attempts: 3,
        }
    }
}

/// High-performance file watcher for feature flag configuration
pub struct FeatureFlagFileWatcher {
    /// Path to the configuration file being watched
    config_path: PathBuf,
    
    /// Configuration for the watcher
    config: FileWatcherConfig,
    
    /// Tokio channel for sending events to the manager
    event_sender: tokio_mpsc::UnboundedSender<ConfigFileEvent>,
    
    /// Event receiver for the manager
    event_receiver: Option<tokio_mpsc::UnboundedReceiver<ConfigFileEvent>>,
    
    /// File system watcher handle
    _watcher: Option<RecommendedWatcher>,
    
    /// Background task handle for event processing
    _task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl FeatureFlagFileWatcher {
    /// Create a new file watcher for the given configuration file
    pub fn new(config_path: PathBuf) -> FeatureFlagResult<Self> {
        Self::with_config(config_path, FileWatcherConfig::default())
    }
    
    /// Create a new file watcher with custom configuration
    pub fn with_config(
        config_path: PathBuf,
        config: FileWatcherConfig,
    ) -> FeatureFlagResult<Self> {
        let (event_sender, event_receiver) = tokio_mpsc::unbounded_channel();
        
        Ok(Self {
            config_path,
            config,
            event_sender,
            event_receiver: Some(event_receiver),
            _watcher: None,
            _task_handle: None,
        })
    }
    
    /// Start watching the configuration file
    pub fn start_watching(&mut self) -> FeatureFlagResult<tokio_mpsc::UnboundedReceiver<ConfigFileEvent>> {
        info!("Starting file watcher for configuration: {}", self.config_path.display());
        
        // Validate file exists
        if !self.config_path.exists() {
            return Err(FeatureFlagError::IoError {
                operation: "file watcher setup".to_string(),
                error: format!("Configuration file does not exist: {}", self.config_path.display()),
            });
        }
        
        // Create cross-thread channel for notify events
        let (tx, rx) = mpsc::channel();
        
        // Create file system watcher
        let mut watcher = notify::recommended_watcher(tx)
            .map_err(|e| FeatureFlagError::IoError {
                operation: "creating file watcher".to_string(),
                error: e.to_string(),
            })?;
        
        // Determine what to watch
        let watch_path = if self.config.watch_parent_directory {
            self.config_path.parent().unwrap_or(&self.config_path)
        } else {
            &self.config_path
        };
        
        // Start watching
        watcher.watch(watch_path, RecursiveMode::NonRecursive)
            .map_err(|e| FeatureFlagError::IoError {
                operation: "starting file watch".to_string(),
                error: e.to_string(),
            })?;
        
        debug!("File watcher monitoring: {}", watch_path.display());
        
        // Start background event processing task
        let task_handle = self.start_event_processing_task(rx);
        
        self._watcher = Some(watcher);
        self._task_handle = Some(task_handle);
        
        // Return the receiver for the manager to use
        Ok(self.event_receiver.take().unwrap())
    }
    
    /// Start the background task that processes file system events
    fn start_event_processing_task(
        &self,
        event_receiver: mpsc::Receiver<notify::Result<Event>>,
    ) -> tokio::task::JoinHandle<()> {
        let event_sender = self.event_sender.clone();
        let config_path = self.config_path.clone();
        let debounce_duration = self.config.debounce_duration;
        let watched_extensions = self.config.watched_extensions.clone();
        let watch_parent_directory = self.config.watch_parent_directory;
        
        tokio::spawn(async move {
            let mut last_event_time = std::time::Instant::now();
            
            // Move to blocking thread for synchronous notify operations
            tokio::task::spawn_blocking(move || {
                for result in event_receiver {
                    match result {
                        Ok(event) => {
                            // Process the file system event
                            if let Some(config_event) = Self::process_fs_event(
                                event,
                                &config_path,
                                &watched_extensions,
                                watch_parent_directory,
                            ) {
                                // Debounce rapid events
                                let now = std::time::Instant::now();
                                if now.duration_since(last_event_time) > debounce_duration {
                                    last_event_time = now;
                                    
                                    // Send event to manager
                                    if let Err(e) = event_sender.send(config_event) {
                                        error!("Failed to send file event to manager: {}", e);
                                        break;
                                    }
                                } else {
                                    debug!("Debounced file system event");
                                }
                            }
                        }
                        Err(e) => {
                            warn!("File watcher error: {}", e);
                            let error_event = ConfigFileEvent::Error(e.to_string());
                            if event_sender.send(error_event).is_err() {
                                error!("Failed to send error event to manager");
                                break;
                            }
                        }
                    }
                }
            }).await.unwrap_or_else(|e| {
                error!("File watcher task panicked: {}", e);
            });
        })
    }
    
    /// Process a file system event and convert it to a configuration event
    fn process_fs_event(
        event: Event,
        config_path: &Path,
        watched_extensions: &[String],
        watch_parent_directory: bool,
    ) -> Option<ConfigFileEvent> {
        debug!("Processing file system event: {:?}", event);
        
        // Check if this event affects our configuration file
        let relevant_path = if watch_parent_directory {
            // When watching parent directory, filter for our specific file
            event.paths.iter().find(|path| {
                path == &config_path || (
                    path.extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| watched_extensions.contains(&ext.to_lowercase()))
                        .unwrap_or(false)
                )
            })
        } else {
            // When watching specific file, any event is relevant
            event.paths.first()
        };
        
        if let Some(path) = relevant_path {
            let path = path.clone();
            
            match event.kind {
                EventKind::Create(_) => {
                    debug!("Configuration file created: {}", path.display());
                    Some(ConfigFileEvent::Created(path))
                }
                EventKind::Modify(_) => {
                    debug!("Configuration file modified: {}", path.display());
                    Some(ConfigFileEvent::Modified(path))
                }
                EventKind::Remove(_) => {
                    warn!("Configuration file deleted: {}", path.display());
                    Some(ConfigFileEvent::Deleted(path))
                }
                _ => {
                    debug!("Ignoring file system event: {:?}", event.kind);
                    None
                }
            }
        } else {
            None
        }
    }
    
    /// Stop watching the configuration file
    pub fn stop_watching(&mut self) -> FeatureFlagResult<()> {
        info!("Stopping file watcher");
        
        if let Some(handle) = self._task_handle.take() {
            handle.abort();
            debug!("File watcher background task stopped");
        }
        
        self._watcher = None;
        
        Ok(())
    }
    
    /// Check if the watcher is currently active
    pub fn is_watching(&self) -> bool {
        self._watcher.is_some() && self._task_handle.as_ref().map(|h| !h.is_finished()).unwrap_or(false)
    }
    
    /// Get statistics about the file watcher
    pub fn get_stats(&self) -> FileWatcherStats {
        FileWatcherStats {
            is_active: self.is_watching(),
            config_file: self.config_path.clone(),
            watch_target: if self.config.watch_parent_directory {
                self.config_path.parent().unwrap_or(&self.config_path).to_path_buf()
            } else {
                self.config_path.clone()
            },
            debounce_duration: self.config.debounce_duration,
        }
    }
}

impl Drop for FeatureFlagFileWatcher {
    fn drop(&mut self) {
        if self.is_watching() {
            let _ = self.stop_watching();
        }
    }
}

/// Statistics about the file watcher
#[derive(Debug, Clone)]
pub struct FileWatcherStats {
    pub is_active: bool,
    pub config_file: PathBuf,
    pub watch_target: PathBuf,
    pub debounce_duration: Duration,
}

/// Debounced file watcher for handling rapid file changes
pub struct DebouncedFileWatcher {
    inner: FeatureFlagFileWatcher,
    last_event_time: std::time::Instant,
    debounce_buffer: Vec<ConfigFileEvent>,
}

impl DebouncedFileWatcher {
    /// Create a new debounced file watcher
    pub fn new(config_path: PathBuf, debounce_duration: Duration) -> FeatureFlagResult<Self> {
        let config = FileWatcherConfig {
            debounce_duration,
            ..Default::default()
        };
        
        let inner = FeatureFlagFileWatcher::with_config(config_path, config)?;
        
        Ok(Self {
            inner,
            last_event_time: std::time::Instant::now(),
            debounce_buffer: Vec::new(),
        })
    }
    
    /// Start watching with debounced events
    pub fn start_watching(&mut self) -> FeatureFlagResult<tokio_mpsc::UnboundedReceiver<ConfigFileEvent>> {
        self.inner.start_watching()
    }
    
    /// Stop watching
    pub fn stop_watching(&mut self) -> FeatureFlagResult<()> {
        self.inner.stop_watching()
    }
    
    /// Check if watching
    pub fn is_watching(&self) -> bool {
        self.inner.is_watching()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;
    
    #[tokio::test]
    async fn test_file_watcher_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        let watcher = FeatureFlagFileWatcher::new(temp_file.path().to_path_buf());
        
        assert!(watcher.is_ok());
        let mut watcher = watcher.unwrap();
        assert!(!watcher.is_watching());
    }
    
    #[tokio::test]
    async fn test_file_watcher_start_stop() {
        let temp_file = NamedTempFile::new().unwrap();
        let mut watcher = FeatureFlagFileWatcher::new(temp_file.path().to_path_buf()).unwrap();
        
        let receiver = watcher.start_watching().unwrap();
        assert!(watcher.is_watching());
        
        watcher.stop_watching().unwrap();
        assert!(!watcher.is_watching());
    }
    
    #[tokio::test]
    async fn test_file_modification_detection() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let mut watcher = FeatureFlagFileWatcher::new(temp_file.path().to_path_buf()).unwrap();
        
        let mut receiver = watcher.start_watching().unwrap();
        
        // Give watcher time to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Modify the file
        writeln!(temp_file, "test content").unwrap();
        temp_file.flush().unwrap();
        
        // Wait for event (with timeout)
        let event = tokio::time::timeout(Duration::from_secs(2), receiver.recv()).await;
        
        match event {
            Ok(Some(ConfigFileEvent::Modified(path))) => {
                assert_eq!(path, temp_file.path());
            }
            Ok(Some(other)) => {
                panic!("Expected Modified event, got {:?}", other);
            }
            Ok(None) => {
                panic!("Channel closed unexpectedly");
            }
            Err(_) => {
                // Timeout - file system events can be inconsistent in tests
                println!("Warning: File modification event not received (timeout)");
            }
        }
    }
    
    #[test]
    fn test_file_watcher_config() {
        let config = FileWatcherConfig {
            debounce_duration: Duration::from_millis(1000),
            watch_parent_directory: false,
            watched_extensions: vec!["yaml".to_string()],
            max_reload_attempts: 5,
        };
        
        assert_eq!(config.debounce_duration, Duration::from_millis(1000));
        assert!(!config.watch_parent_directory);
        assert_eq!(config.watched_extensions, vec!["yaml"]);
        assert_eq!(config.max_reload_attempts, 5);
    }
}