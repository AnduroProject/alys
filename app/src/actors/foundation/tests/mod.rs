//! Test Module for Actor System Foundation
//! 
//! Comprehensive test coverage for Phase 1 through Phase 5 implementations
//! with integration to the Alys Testing Framework and property-based testing.

pub mod adapter_tests;
pub mod health_tests;
pub mod registry_tests;
pub mod supervision_tests;

// Re-export test utilities for external use
pub use adapter_tests::*;
pub use health_tests::*;
pub use registry_tests::*;
pub use supervision_tests::*;