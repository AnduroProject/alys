//! Storage Actor Tests - Phase 5: Testing & Validation
//!
//! This module contains comprehensive tests for the Storage Actor including:
//! - Unit tests for individual components
//! - Integration tests for full system behavior  
//! - Performance tests for throughput and latency
//! - Chaos engineering tests for resilience
//! - Mock helpers and test utilities

// Core test modules
#[cfg(test)]
mod integration_test;

#[cfg(test)]
mod integration_test_enhanced;

// Phase 5: Testing & Validation - Comprehensive test suite
#[cfg(test)]
mod unit_tests;

#[cfg(test)]
mod performance_tests;

#[cfg(test)]
pub mod mock_helpers;

#[cfg(test)]
mod chaos_tests;

// Re-export commonly used test utilities
pub use mock_helpers::{TestDataGenerator, StorageTestFixture, StorageAssertions, MockDatabase};
pub use mock_helpers::test_utils;