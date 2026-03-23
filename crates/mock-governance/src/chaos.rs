//! Chaos testing support for mock governance server.

use rand::Rng;
use std::sync::atomic::{AtomicU64, Ordering};

/// Chaos injection state and logic.
#[derive(Debug)]
pub struct ChaosController {
    /// Rate of peg-in rejections (0.0 - 1.0)
    pub pegin_reject_rate: f64,

    /// Number of requests to disconnect after (0 = never)
    pub disconnect_after: u64,

    /// Current request counter
    request_count: AtomicU64,

    /// Whether chaos mode is enabled
    pub enabled: bool,
}

impl ChaosController {
    /// Create a new chaos controller.
    pub fn new(pegin_reject_rate: f64, disconnect_after: u64, enabled: bool) -> Self {
        Self {
            pegin_reject_rate: pegin_reject_rate.clamp(0.0, 1.0),
            disconnect_after,
            request_count: AtomicU64::new(0),
            enabled,
        }
    }

    /// Increment request counter and return the new count.
    pub fn increment_requests(&self) -> u64 {
        self.request_count.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Check if we should reject a peg-in (based on reject rate).
    pub fn should_reject_pegin(&self) -> bool {
        if !self.enabled || self.pegin_reject_rate == 0.0 {
            return false;
        }

        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < self.pegin_reject_rate
    }

    /// Check if we should disconnect (based on request count).
    pub fn should_disconnect(&self) -> bool {
        if !self.enabled || self.disconnect_after == 0 {
            return false;
        }

        self.request_count.load(Ordering::SeqCst) >= self.disconnect_after
    }

    /// Get current request count.
    #[allow(dead_code)]
    pub fn get_request_count(&self) -> u64 {
        self.request_count.load(Ordering::SeqCst)
    }

    /// Reset state (e.g., for new connection).
    pub fn reset(&self) {
        self.request_count.store(0, Ordering::SeqCst);
    }
}

impl Default for ChaosController {
    fn default() -> Self {
        Self::new(0.0, 0, false)
    }
}
