#![recursion_limit = "256"]

mod app;
mod aura;
mod auxpow;
mod auxpow_miner;
mod block_hash_cache;
mod error;
mod metrics;
mod network;
pub mod rpc; // Unified RPC server
mod signatures;
mod bridge_compat; // Federation compatibility layer
mod spec;
mod store;

// V2 Actor System modules
pub mod actors;
pub mod config;
pub mod integration;
pub mod messages;
pub mod serde_utils;
pub mod types;

// for main.rs
pub use app::run;

// for miner crate
pub use auxpow::AuxPow;
pub use auxpow_miner::AuxBlock;
use lighthouse_facade as lighthouse_types;

pub trait EthSpec: lighthouse_types::EthSpec {}
impl EthSpec for lighthouse_types::MainnetEthSpec {}
