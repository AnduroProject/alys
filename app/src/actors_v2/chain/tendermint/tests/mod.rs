//! Tendermint Consensus Integration Tests
//!
//! Self-contained integration tests for Tendermint consensus.
//! These tests run with `cargo test` only - no external infrastructure required.
//!
//! # Design Principles
//!
//! - **No Docker**: Tests use mock actors and in-memory state
//! - **No External APIs**: All dependencies stubbed
//! - **Deterministic**: Timeouts injected, no race conditions
//! - **Fast**: Complete test suite runs in < 60 seconds
//!
//! # Test Categories
//!
//! - `single_validator_tests`: Happy path consensus cycles
//! - `timeout_tests`: Timeout handling and round advancement
//! - `recovery_tests`: WAL recovery and crash safety
//! - `locking_tests`: Locking behavior and re-proposal
//! - `validation_tests`: Block execution validation

mod harness;
mod mock_actors;

#[cfg(test)]
mod single_validator_tests;

#[cfg(test)]
mod timeout_tests;

#[cfg(test)]
mod recovery_tests;

#[cfg(test)]
mod locking_tests;

#[cfg(test)]
mod validation_tests;

pub use harness::*;
pub use mock_actors::*;
