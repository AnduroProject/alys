//! Connection Management and Reconnection
//! 
//! Handles connection failures and automatic reconnection

use std::time::{Duration, SystemTime};
use std::collections::HashMap;
use tracing::{info, warn};

/// Reconnection manager for governance connections
#[derive(Debug)]
pub struct ReconnectionManager {
    max_attempts: u32,
    base_delay: Duration,
    reconnection_state: HashMap<String, ReconnectionState>,
}

/// Reconnection state for individual nodes
#[derive(Debug, Clone)]
pub struct ReconnectionState {
    pub attempts: u32,
    pub next_attempt: SystemTime,
    pub backoff_delay: Duration,
    pub last_failure: Option<SystemTime>,
}

impl ReconnectionManager {
    pub fn new(max_attempts: u32, base_delay: Duration) -> Self {
        Self {
            max_attempts,
            base_delay,
            reconnection_state: HashMap::new(),
        }
    }

    /// Record connection failure
    pub fn record_failure(&mut self, node_id: String) {
        let state = self.reconnection_state.entry(node_id.clone())
            .or_insert_with(|| ReconnectionState {
                attempts: 0,
                next_attempt: SystemTime::now(),
                backoff_delay: self.base_delay,
                last_failure: None,
            });

        state.attempts += 1;
        state.last_failure = Some(SystemTime::now());
        
        // Exponential backoff
        state.backoff_delay = self.base_delay * 2_u32.pow(state.attempts.min(10));
        state.next_attempt = SystemTime::now() + state.backoff_delay;

        warn!("Connection failure for {}: attempt {}, next retry in {:?}", 
              node_id, state.attempts, state.backoff_delay);
    }

    /// Check if reconnection should be attempted
    pub fn should_reconnect(&self, node_id: &str) -> bool {
        if let Some(state) = self.reconnection_state.get(node_id) {
            state.attempts < self.max_attempts && SystemTime::now() >= state.next_attempt
        } else {
            true
        }
    }

    /// Record successful connection
    pub fn record_success(&mut self, node_id: String) {
        if self.reconnection_state.remove(&node_id).is_some() {
            info!("Successful reconnection to {}", node_id);
        }
    }
}