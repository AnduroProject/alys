//! NetworkActor V2 Manager Components
//!
//! Simplified manager components for two-actor system:
//! - PeerManager: Connection and reputation management
//! - GossipHandler: Gossip message processing
//! - BlockRequestManager: Block sync coordination

pub mod peer_manager;
pub mod gossip_handler;
pub mod block_request_manager;

pub use peer_manager::{PeerManager, Violation};
pub use gossip_handler::GossipHandler;
pub use block_request_manager::BlockRequestManager;