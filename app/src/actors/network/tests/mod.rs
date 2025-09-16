//! Network Actor Tests
//! 
//! Comprehensive test suite for all network-related actors and their interactions.
//! Organized following the bridge actor test pattern with helpers, unit tests,
//! integration tests, performance tests, and chaos engineering.

pub mod helpers;
pub mod unit;
pub mod integration;
pub mod performance;

#[cfg(test)]
mod chaos;

pub use helpers::*;