//! NetworkActor V2 Protocol Implementations
//!
//! Simplified protocol implementations for two-actor system:
//! - Gossip: Gossipsub message broadcasting (TCP only)
//! - Request-Response: Direct peer queries (TCP only)
//!
//! Removed from V1: Kademlia DHT, mDNS, QUIC transport

pub mod gossip;
pub mod request_response;

pub use gossip::*;
pub use request_response::*;