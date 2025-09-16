//! Comprehensive testing infrastructure for the Alys V2 actor-based architecture
//!
//! This module provides testing utilities, harnesses, and frameworks for testing
//! actor systems, including integration testing, property-based testing, chaos
//! testing, and mock implementations for external systems.

pub mod actor_harness;
pub mod property_testing;
pub mod chaos_testing;
pub mod test_utilities;
pub mod mocks;
pub mod fixtures;

// Re-export commonly used testing components
pub use actor_harness::{ActorTestHarness, TestEnvironment, ActorTestResult};
pub use property_testing::{PropertyTestFramework, ActorPropertyTest, MessageOrderingTest};
pub use chaos_testing::{ChaosTestEngine, ChaosTestScenario, NetworkPartition, ActorFailure};
pub use test_utilities::{TestUtil, TestMessage, TestData, TestTimeout};
pub use mocks::{MockGovernanceClient, MockBitcoinClient, MockExecutionClient, MockTestEnvironment, MockEnvironmentBuilder};
pub use fixtures::{TestFixtures, ActorFixtures, ConfigurationFixtures};