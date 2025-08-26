//! Message definitions for actor communication
//! 
//! This module contains all typed messages used for communication between actors
//! in the Alys V2 architecture. Messages are organized by functional area.

pub mod system_messages;
pub mod chain_messages;
pub mod sync_messages;
pub mod network_messages;
pub mod stream_messages;
pub mod bridge_messages;

pub use system_messages::*;
pub use chain_messages::*;
pub use sync_messages::*;
pub use network_messages::*;
pub use stream_messages::*;
pub use bridge_messages::*;

// NOTE: storage_messages has been moved to crate::actors::storage::messages
// Import from there instead of the global messages module