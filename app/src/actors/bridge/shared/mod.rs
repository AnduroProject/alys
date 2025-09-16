//! Shared Bridge Utilities
//!
//! Common utilities and components used across bridge actors

pub mod utxo;
pub mod federation;
pub mod bitcoin_client;
pub mod validation;
pub mod constants;
pub mod errors;
pub mod types;

// Specific re-exports to avoid ambiguous glob issues
pub use utxo::{Utxo, UtxoManager, UtxoStats, UtxoSelection, SelectionCriteria, SelectionStrategy, UtxoError, UTXO_REFRESH_INTERVAL};
pub use federation::*;
pub use bitcoin_client::*;
pub use validation::*;
pub use constants::{DUST_LIMIT}; // Only re-export DUST_LIMIT from constants
pub use errors::*;
pub use types::*;