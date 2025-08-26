//! Storage Actor Tests
//!
//! This module contains comprehensive tests for the Storage Actor including
//! unit tests, integration tests, and performance tests.

#[cfg(test)]
mod integration_test;

// Re-export test utilities
pub use integration_test::*;