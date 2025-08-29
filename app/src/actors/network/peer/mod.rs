//! PeerActor Module
//! 
//! Connection management and peer scoring for 1000+ concurrent peers with
//! federation-aware prioritization and performance tracking.

pub mod actor;
pub mod config;
pub mod store;
pub mod scoring;
pub mod connection;
pub mod handlers;

#[cfg(test)]
pub mod tests;

pub use actor::PeerActor;
pub use config::{PeerConfig, ScoringConfig, PeerDiscoveryConfig};