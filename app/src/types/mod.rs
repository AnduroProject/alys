//! Type definitions for the Alys V2 actor system
//! 
//! This module contains all the shared data structures and types used
//! throughout the actor system, designed to be actor-friendly and support
//! efficient message passing.

pub mod blockchain;
pub mod network;
pub mod consensus;
pub mod bridge;
pub mod errors;

pub use blockchain::*;
pub use network::*;
pub use consensus::*;
pub use bridge::*;
pub use errors::*;

// Re-export commonly used external types
pub use ethereum_types::{Address, H256, U256, H160, H512};

// Type aliases for clarity
pub type BlockHash = H256;
pub type Hash256 = H256;
pub type PeerId = String;

// Bitcoin types (re-exports)
pub use bitcoin;

// Cryptographic types
pub type Signature = [u8; 64];
pub type PublicKey = [u8; 33];
pub type PrivateKey = [u8; 32];

// Actix actor framework re-exports
pub use actix::prelude::*;