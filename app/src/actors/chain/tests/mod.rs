//! Chain Actor Test Suite
//!
//! Comprehensive test coverage for the ChainActor implementation including:
//! - Unit tests for individual components
//! - Integration tests for actor interactions
//! - Performance benchmarks for critical paths
//! - Mock helpers and utilities for testing

pub mod unit_tests;
pub mod integration_tests;
pub mod performance_tests;
pub mod mock_helpers;

// Re-export common test utilities
pub use mock_helpers::{MockChainActor, create_test_config, create_test_block};