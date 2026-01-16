//! Block Request Manager V2
//!
//! Manages block requests between NetworkActor and SyncActor.
//! Coordinates peer selection and request tracking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

use super::super::messages::PeerId;

/// Block request information
#[derive(Debug, Clone)]
pub struct BlockRequest {
    pub request_id: String,
    pub start_height: u64,
    pub block_count: u32,
    pub target_peer: PeerId,
    pub requested_at: SystemTime,
    pub timeout: Duration,
    pub retry_count: u32,
    pub max_retries: u32,
}

impl BlockRequest {
    pub fn new(start_height: u64, block_count: u32, target_peer: PeerId) -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
            start_height,
            block_count,
            target_peer,
            requested_at: SystemTime::now(),
            timeout: Duration::from_secs(30),
            retry_count: 0,
            max_retries: 3,
        }
    }

    /// Check if request has timed out
    pub fn is_timed_out(&self) -> bool {
        SystemTime::now()
            .duration_since(self.requested_at)
            .unwrap_or_default()
            > self.timeout
    }

    /// Check if request can be retried
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Mark as retry attempt
    pub fn retry(&mut self) {
        self.retry_count += 1;
        self.requested_at = SystemTime::now();
    }
}

/// Block request statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRequestStats {
    pub active_requests: usize,
    pub completed_requests: u64,
    pub failed_requests: u64,
    pub timed_out_requests: u64,
    pub retried_requests: u64,
    pub average_response_time_ms: f64,
    pub total_blocks_requested: u64,
    pub total_blocks_received: u64,
}

impl Default for BlockRequestStats {
    fn default() -> Self {
        Self {
            active_requests: 0,
            completed_requests: 0,
            failed_requests: 0,
            timed_out_requests: 0,
            retried_requests: 0,
            average_response_time_ms: 0.0,
            total_blocks_requested: 0,
            total_blocks_received: 0,
        }
    }
}

/// Block request manager for NetworkActor-SyncActor coordination
pub struct BlockRequestManager {
    /// Active block requests
    active_requests: HashMap<String, BlockRequest>,
    /// Request statistics
    stats: BlockRequestStats,
    /// Maximum concurrent requests
    max_concurrent_requests: usize,
    /// Response time tracking
    response_times: Vec<Duration>,
    /// Maximum response time samples to keep
    max_response_samples: usize,
}

impl BlockRequestManager {
    /// Create new block request manager
    pub fn new(max_concurrent_requests: usize) -> Self {
        Self {
            active_requests: HashMap::new(),
            stats: BlockRequestStats::default(),
            max_concurrent_requests,
            response_times: Vec::new(),
            max_response_samples: 100,
        }
    }

    /// Create a new block request
    pub fn create_request(
        &mut self,
        start_height: u64,
        block_count: u32,
        target_peer: PeerId,
    ) -> Result<String, String> {
        // Check if we're at capacity
        if self.active_requests.len() >= self.max_concurrent_requests {
            return Err("Maximum concurrent requests reached".to_string());
        }

        let request = BlockRequest::new(start_height, block_count, target_peer);
        let request_id = request.request_id.clone();

        tracing::debug!(
            "Creating block request {} for blocks {} to {} from peer {}",
            request_id,
            start_height,
            start_height + block_count as u64 - 1,
            request.target_peer
        );

        self.active_requests.insert(request_id.clone(), request);
        self.stats.active_requests = self.active_requests.len();
        self.stats.total_blocks_requested += block_count as u64;

        Ok(request_id)
    }

    /// Complete a block request successfully
    pub fn complete_request(
        &mut self,
        request_id: &str,
        blocks_received: u32,
    ) -> Result<(), String> {
        if let Some(request) = self.active_requests.remove(request_id) {
            let response_time = SystemTime::now()
                .duration_since(request.requested_at)
                .unwrap_or_default();

            // Update statistics
            self.stats.completed_requests += 1;
            self.stats.active_requests = self.active_requests.len();
            self.stats.total_blocks_received += blocks_received as u64;

            // Track response time
            self.record_response_time(response_time);

            tracing::debug!(
                "Completed block request {} in {:?}, received {} blocks",
                request_id,
                response_time,
                blocks_received
            );

            Ok(())
        } else {
            Err(format!("Request {} not found", request_id))
        }
    }

    /// Fail a block request
    pub fn fail_request(
        &mut self,
        request_id: &str,
        reason: &str,
    ) -> Result<Option<BlockRequest>, String> {
        if let Some(mut request) = self.active_requests.remove(request_id) {
            tracing::warn!("Block request {} failed: {}", request_id, reason);

            // Check if we can retry
            if request.can_retry() {
                request.retry();
                self.stats.retried_requests += 1;

                tracing::info!(
                    "Retrying block request {} (attempt {}/{})",
                    request_id,
                    request.retry_count + 1,
                    request.max_retries + 1
                );

                return Ok(Some(request));
            } else {
                // Request exhausted retries
                self.stats.failed_requests += 1;
                self.stats.active_requests = self.active_requests.len();

                tracing::error!(
                    "Block request {} failed permanently after {} retries",
                    request_id,
                    request.retry_count
                );
            }

            Ok(None)
        } else {
            Err(format!("Request {} not found", request_id))
        }
    }

    /// Retry a block request with potentially different peer
    pub fn retry_request(&mut self, mut request: BlockRequest, new_peer: Option<PeerId>) -> String {
        if let Some(peer) = new_peer {
            request.target_peer = peer;
        }

        let request_id = request.request_id.clone();
        self.active_requests.insert(request_id.clone(), request);
        self.stats.active_requests = self.active_requests.len();

        request_id
    }

    /// Check for timed out requests
    pub fn check_timeouts(&mut self) -> Vec<String> {
        let timed_out: Vec<String> = self
            .active_requests
            .iter()
            .filter(|(_, request)| request.is_timed_out())
            .map(|(id, _)| id.clone())
            .collect();

        // Remove timed out requests and update stats
        for request_id in &timed_out {
            if self.active_requests.remove(request_id).is_some() {
                self.stats.timed_out_requests += 1;
                tracing::warn!("Block request {} timed out", request_id);
            }
        }

        self.stats.active_requests = self.active_requests.len();
        timed_out
    }

    /// Get request by ID
    pub fn get_request(&self, request_id: &str) -> Option<&BlockRequest> {
        self.active_requests.get(request_id)
    }

    /// Get all active requests
    pub fn get_active_requests(&self) -> Vec<&BlockRequest> {
        self.active_requests.values().collect()
    }

    /// Get requests for a specific peer
    pub fn get_peer_requests(&self, peer_id: &PeerId) -> Vec<&BlockRequest> {
        self.active_requests
            .values()
            .filter(|request| request.target_peer == *peer_id)
            .collect()
    }

    /// Cancel request
    pub fn cancel_request(&mut self, request_id: &str) -> Result<(), String> {
        if self.active_requests.remove(request_id).is_some() {
            self.stats.active_requests = self.active_requests.len();
            tracing::debug!("Cancelled block request {}", request_id);
            Ok(())
        } else {
            Err(format!("Request {} not found", request_id))
        }
    }

    /// Cancel all requests from a specific peer
    pub fn cancel_peer_requests(&mut self, peer_id: &PeerId) -> usize {
        let cancelled: Vec<String> = self
            .active_requests
            .iter()
            .filter(|(_, request)| request.target_peer == *peer_id)
            .map(|(id, _)| id.clone())
            .collect();

        for request_id in &cancelled {
            self.active_requests.remove(request_id);
        }

        self.stats.active_requests = self.active_requests.len();

        if !cancelled.is_empty() {
            tracing::info!(
                "Cancelled {} requests from peer {}",
                cancelled.len(),
                peer_id
            );
        }

        cancelled.len()
    }

    /// Record response time for statistics
    fn record_response_time(&mut self, duration: Duration) {
        self.response_times.push(duration);

        // Keep only recent samples
        if self.response_times.len() > self.max_response_samples {
            self.response_times.remove(0);
        }

        // Update average
        if !self.response_times.is_empty() {
            let total_ms: f64 = self
                .response_times
                .iter()
                .map(|d| d.as_millis() as f64)
                .sum();
            self.stats.average_response_time_ms = total_ms / self.response_times.len() as f64;
        }
    }

    /// Get statistics
    pub fn get_stats(&self) -> BlockRequestStats {
        self.stats.clone()
    }

    /// Check if we can make more requests
    pub fn can_make_request(&self) -> bool {
        self.active_requests.len() < self.max_concurrent_requests
    }

    /// Get available request capacity
    pub fn get_available_capacity(&self) -> usize {
        self.max_concurrent_requests
            .saturating_sub(self.active_requests.len())
    }
}

impl Default for BlockRequestManager {
    fn default() -> Self {
        Self::new(10) // Default to 10 concurrent requests
    }
}
