//! External system integration interfaces
//!
//! This module provides integration interfaces for external systems that Alys
//! interacts with, including Bitcoin nodes, Ethereum execution layers, and
//! governance systems.

pub mod bitcoin;
pub mod ethereum;
pub mod execution;
pub mod governance;
pub mod monitoring;

pub use bitcoin::*;
pub use ethereum::*;
pub use execution::*;
pub use governance::*;
pub use monitoring::*;