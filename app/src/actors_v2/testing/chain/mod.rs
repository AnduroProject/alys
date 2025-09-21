//! ChainActor V2 Testing Framework
//!
//! Simplified testing infrastructure for ChainActor

pub mod fixtures;
pub mod unit;
pub mod integration;

pub use fixtures::*;

use tempfile::TempDir;

use crate::actors_v2::chain::{ChainConfig, ChainError};

/// Simplified ChainActor test harness
pub struct ChainTestHarness {
    pub temp_dir: TempDir,
    pub config: ChainConfig,
}

impl ChainTestHarness {
    /// Create new test harness
    pub async fn new() -> Result<Self, ChainTestError> {
        let temp_dir = TempDir::new().map_err(|e| ChainTestError::Setup(e.to_string()))?;
        let config = ChainConfig::default();

        Ok(Self {
            temp_dir,
            config,
        })
    }

    /// Setup with custom configuration
    pub async fn with_config(config: ChainConfig) -> Result<Self, ChainTestError> {
        let mut harness = Self::new().await?;
        harness.config = config;
        Ok(harness)
    }

    /// Setup validator configuration
    pub async fn validator() -> Result<Self, ChainTestError> {
        let mut config = ChainConfig::default();
        config.is_validator = true;
        config.enable_auxpow = true;
        config.enable_peg_operations = true;
        Self::with_config(config).await
    }

    /// Setup non-validator configuration
    pub async fn non_validator() -> Result<Self, ChainTestError> {
        let mut config = ChainConfig::default();
        config.is_validator = false;
        Self::with_config(config).await
    }

    /// Verify configuration is valid
    pub async fn verify_config(&self) -> Result<(), ChainTestError> {
        self.config.validate()
            .map_err(|e| ChainTestError::Configuration(e.to_string()))?;
        Ok(())
    }
}

/// ChainActor test errors
#[derive(Debug, thiserror::Error)]
pub enum ChainTestError {
    #[error("Setup error: {0}")]
    Setup(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("State inconsistency: {0}")]
    StateInconsistency(String),

    #[error("Block operation error: {0}")]
    BlockOperation(String),

    #[error("AuxPoW operation error: {0}")]
    AuxPowOperation(String),

    #[error("Peg operation error: {0}")]
    PegOperation(String),

    #[error("Message not implemented: {0}")]
    MessageNotImplemented(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Chain error: {0}")]
    Chain(#[from] ChainError),
}