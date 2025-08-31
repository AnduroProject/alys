//! Bridge Coordinator Actor
//! 
//! Main coordination actor that orchestrates peg-in and peg-out operations

pub mod actor;
pub mod handlers;
pub mod state;
pub mod metrics;
pub mod alys_actor_impl;

pub use actor::BridgeActor;