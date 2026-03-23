//! Protocol buffer definitions for Alys governance service.
//!
//! This crate provides the shared proto definitions used by both:
//! - The mock governance server (for testing)
//! - Validator nodes (gRPC client)
//!
//! The proto definitions are designed to be compatible with the real
//! anduro-governance service for easy migration.

// Include the generated protobuf code
tonic::include_proto!("governance");

// Re-export commonly used types for convenience
pub use governance_service_client::GovernanceServiceClient;
pub use governance_service_server::{GovernanceService, GovernanceServiceServer};
