// BridgeActor module for Alys V2 architecture
//
// This module implements the BridgeActor following the V2 architectural patterns
// with comprehensive peg-in/peg-out operations, UTXO management, and governance integration.

pub mod actor;
pub mod errors;
pub mod messages;
pub mod metrics;
pub mod tests;
pub mod utxo;

pub use actor::BridgeActor;
pub use errors::BridgeError;
pub use messages::*;
pub use metrics::BridgeMetrics;
pub use utxo::{UtxoManager, Utxo};