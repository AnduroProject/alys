//! Network Actor Message Protocol
//! 
//! This module defines the complete message protocol for the network actor system,
//! including message envelopes, correlation tracking, and priority management.

use actix::{Message, Result as ActorResult};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use uuid::Uuid;

pub mod sync_messages;
pub mod network_messages;
pub mod peer_messages;

pub use sync_messages::*;
pub use network_messages::*;
pub use peer_messages::*;

/// Core network message trait for type safety and runtime identification
pub trait NetworkMessage: Message + Send + Sync + 'static {}

/// Message priority levels for network operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MessagePriority {
    Critical = 0,    // Federation consensus operations
    High = 1,        // Block production and validation
    Normal = 2,      // Regular sync operations
    Low = 3,         // Background tasks (discovery, maintenance)
}

impl Default for MessagePriority {
    fn default() -> Self {
        MessagePriority::Normal
    }
}

/// Message envelope with correlation tracking and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope<T> {
    pub message: T,
    pub correlation_id: Uuid,
    pub timestamp: Instant,
    pub priority: MessagePriority,
    pub retry_count: u32,
    pub max_retries: u32,
}

impl<T> MessageEnvelope<T> {
    pub fn new(message: T) -> Self {
        Self {
            message,
            correlation_id: Uuid::new_v4(),
            timestamp: Instant::now(),
            priority: MessagePriority::default(),
            retry_count: 0,
            max_retries: 3,
        }
    }

    pub fn with_priority(mut self, priority: MessagePriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }

    pub fn age(&self) -> std::time::Duration {
        self.timestamp.elapsed()
    }
}

/// Standard response wrapper for all network operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkResponse<T> {
    Success(T),
    Error(NetworkError),
}

/// Network operation error types
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum NetworkError {
    #[error("Peer not found: {peer_id}")]
    PeerNotFound { peer_id: String },
    
    #[error("Sync operation failed: {reason}")]
    SyncError { reason: String },
    
    #[error("Network operation timeout after {duration_ms}ms")]
    Timeout { duration_ms: u64 },
    
    #[error("Protocol error: {message}")]
    ProtocolError { message: String },
    
    #[error("Connection failed: {reason}")]
    ConnectionError { reason: String },
    
    #[error("Message validation failed: {reason}")]
    ValidationError { reason: String },
    
    #[error("Actor communication error: {reason}")]
    ActorError { reason: String },
    
    #[error("Resource exhausted: {resource}")]
    ResourceExhausted { resource: String },
}

impl<T> From<NetworkError> for NetworkResponse<T> {
    fn from(error: NetworkError) -> Self {
        NetworkResponse::Error(error)
    }
}

/// Result type alias for network operations
pub type NetworkResult<T> = Result<T, NetworkError>;
pub type NetworkActorResult<T> = ActorResult<NetworkResult<T>>;

// Auto-implement NetworkMessage for our core message types
impl<T> NetworkMessage for MessageEnvelope<T> where T: Message + Send + Sync + 'static {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_envelope_creation() {
        let msg = MessageEnvelope::new("test message");
        assert_eq!(msg.message, "test message");
        assert_eq!(msg.priority, MessagePriority::Normal);
        assert_eq!(msg.retry_count, 0);
        assert_eq!(msg.max_retries, 3);
        assert!(msg.can_retry());
    }

    #[test]
    fn message_priority_ordering() {
        assert!(MessagePriority::Critical < MessagePriority::High);
        assert!(MessagePriority::High < MessagePriority::Normal);
        assert!(MessagePriority::Normal < MessagePriority::Low);
    }

    #[test]
    fn retry_logic() {
        let mut msg = MessageEnvelope::new("test");
        assert!(msg.can_retry());
        
        for _ in 0..3 {
            msg.increment_retry();
        }
        
        assert!(!msg.can_retry());
    }
}