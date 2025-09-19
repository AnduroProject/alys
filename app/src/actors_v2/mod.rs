//! V2 Actor System
//!
//! This module contains the V2 actor implementations that use pure Actix
//! without the custom actor_system crate dependency.

pub mod storage;

#[cfg(test)]
pub mod testing;