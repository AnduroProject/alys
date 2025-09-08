mod app;
mod aura;
mod auxpow;
mod auxpow_miner;
mod block;
mod block_candidate;
mod block_hash_cache;
mod chain;
mod engine;
mod engine_v2; // Enhanced engine with Lighthouse compatibility
mod error;
mod metrics;
mod network;
mod rpc;
mod rpc_v2; // V2 Actor-based RPC server
mod signatures;
mod bridge_compat; // Federation compatibility layer
mod spec;
mod store;

// V2 Actor System modules
pub mod actors;
pub mod config;
pub mod features;
pub mod integration;
pub mod messages;
pub mod serde_utils;
pub mod types;

// for main.rs
pub use app::run;

// for miner crate
pub use auxpow::AuxPow;
pub use auxpow_miner::AuxBlock;
use lighthouse_facade as types;

pub trait EthSpec: types::EthSpec + serde::Serialize + serde::de::DeserializeOwned {}
impl EthSpec for types::MainnetEthSpec {}
