mod app;
mod aura;
mod auxpow;
mod auxpow_miner;
mod block;
mod block_candidate;
mod block_hash_cache;
mod chain;
mod engine;
mod error;
mod metrics;
mod network;
mod rpc;
mod signatures;
mod spec;
mod store;

// for main.rs
pub use app::run;

// for miner crate
pub use auxpow::AuxPow;
pub use auxpow_miner::AuxBlock;
use lighthouse_wrapper::types;

pub trait EthSpec: types::EthSpec + serde::Serialize + serde::de::DeserializeOwned {}
impl EthSpec for types::MainnetEthSpec {}
