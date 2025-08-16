//! Storage and database configuration

use super::*;
use std::time::Duration;

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub data_dir: String,
    pub database_type: DatabaseType,
    pub connection_pool: ConnectionPoolConfig,
    pub backup: BackupConfig,
    pub performance: StoragePerformanceConfig,
}

/// Database types
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseType {
    Rocksdb,
    Sqlite,
    Postgresql,
}

/// Connection pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPoolConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub connection_timeout: Duration,
    pub idle_timeout: Duration,
}

/// Backup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub enabled: bool,
    pub interval: Duration,
    pub retention_count: u32,
    pub backup_dir: String,
}

/// Storage performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePerformanceConfig {
    pub cache_size_mb: u64,
    pub write_buffer_size_mb: u64,
    pub max_open_files: u32,
    pub compression: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: "./data/storage".to_string(),
            database_type: DatabaseType::Rocksdb,
            connection_pool: ConnectionPoolConfig::default(),
            backup: BackupConfig::default(),
            performance: StoragePerformanceConfig::default(),
        }
    }
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 1,
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300),
        }
    }
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_hours(6),
            retention_count: 7,
            backup_dir: "./backups".to_string(),
        }
    }
}

impl Default for StoragePerformanceConfig {
    fn default() -> Self {
        Self {
            cache_size_mb: 512,
            write_buffer_size_mb: 64,
            max_open_files: 1000,
            compression: true,
        }
    }
}

impl Validate for StorageConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.connection_pool.max_connections == 0 {
            return Err(ConfigError::ValidationError {
                field: "storage.connection_pool.max_connections".to_string(),
                reason: "Max connections must be greater than 0".to_string(),
            });
        }
        Ok(())
    }
}