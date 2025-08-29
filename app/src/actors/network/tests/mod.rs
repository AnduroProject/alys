//! Network Actor System Tests
//! 
//! Comprehensive test suite for the network actor system including unit tests,
//! integration tests, performance tests, and chaos engineering.

pub mod integration_tests;
pub mod performance_tests;
pub mod sync_tests;
pub mod network_tests;
pub mod peer_tests;
pub mod chaos_tests;

#[cfg(test)]
mod test_helpers;

// Re-export common test utilities
#[cfg(test)]
pub use test_helpers::*;