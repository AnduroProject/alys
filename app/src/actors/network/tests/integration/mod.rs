//! Network Actor Integration Tests
//! 
//! Integration tests for network actors working together and with external systems.
//! These tests verify complete workflows and actor coordination.

pub mod network_workflows;
pub mod sync_integration;
pub mod federation_integration;

// Re-export common test types and utilities
pub use crate::actors::network::tests::helpers::*;