//! Bridge Integration Patterns
//! 
//! Cross-actor integration and workflow coordination

pub mod workflows;
pub mod coordination;
pub mod state_sync;

pub use workflows::*;
pub use coordination::*;
pub use state_sync::*;