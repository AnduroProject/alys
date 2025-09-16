//! Advanced Request/Response Tracking System
//! 
//! Comprehensive request correlation, timeout management, and response matching
//! for bridge governance communication with distributed tracing support.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::oneshot;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use tracing::*;

use crate::actors::bridge::{
    messages::stream_messages::*,
    shared::errors::BridgeError,
};

/// Advanced request tracker with correlation, timeouts, and response matching
#[derive(Debug)]
pub struct AdvancedRequestTracker {
    /// Active pending requests by request ID
    pending_requests: HashMap<String, PendingRequestEntry>,
    
    /// Timeout queue for efficient timeout checking
    timeout_queue: Vec<TimeoutEntry>,
    
    /// Request correlation mappings
    correlation_mappings: HashMap<String, String>, // correlation_id -> request_id
    
    /// Request statistics and metrics
    stats: RequestStatistics,
    
    /// Configuration
    config: RequestTrackerConfig,
}

/// Configuration for request tracker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestTrackerConfig {
    /// Default request timeout
    pub default_timeout: Duration,
    
    /// Maximum pending requests
    pub max_pending_requests: usize,
    
    /// Request retry limits
    pub max_retries: u32,
    
    /// Timeout check interval
    pub timeout_check_interval: Duration,
    
    /// Enable distributed tracing
    pub enable_tracing: bool,
    
    /// Request cleanup interval
    pub cleanup_interval: Duration,
}

/// Pending request entry with full context
#[derive(Debug)]
pub struct PendingRequestEntry {
    /// Unique request identifier
    pub request_id: String,
    
    /// Optional correlation ID for distributed tracing
    pub correlation_id: Option<String>,
    
    /// Request type classification
    pub request_type: BridgeRequestType,
    
    /// Original request timestamp
    pub created_at: Instant,
    
    /// Request timeout duration
    pub timeout: Duration,
    
    /// Absolute timeout timestamp
    pub timeout_at: Instant,
    
    /// Response callback channel
    pub response_callback: Option<oneshot::Sender<Result<StreamResponse, BridgeError>>>,
    
    /// Request retry count
    pub retry_count: u32,
    
    /// Request metadata and context
    pub metadata: RequestMetadata,
    
    /// Request priority
    pub priority: RequestPriority,
    
    /// Request state
    pub state: RequestState,
}

/// Bridge-specific request types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BridgeRequestType {
    /// Peg-out signature request
    PegOutSignature {
        pegout_id: String,
        amount: u64,
        destination: String,
    },
    
    /// Federation update notification
    FederationUpdate {
        update_id: String,
        update_type: String,
    },
    
    /// Peg-in notification
    PegInNotification {
        pegin_id: String,
        amount: u64,
    },
    
    /// Heartbeat request
    Heartbeat,
    
    /// Status check request
    StatusCheck,
    
    /// Node registration
    NodeRegistration {
        node_id: String,
    },
    
    /// Custom request type
    Custom {
        request_name: String,
        payload_size: usize,
    },
}

/// Request metadata and context information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMetadata {
    /// Source node or actor
    pub source: String,
    
    /// Target governance nodes
    pub targets: Vec<String>,
    
    /// Request trace ID for distributed tracing
    pub trace_id: Option<Uuid>,
    
    /// Request span ID
    pub span_id: Option<Uuid>,
    
    /// Additional context data
    pub context: HashMap<String, String>,
    
    /// Request size in bytes
    pub payload_size: usize,
}

/// Request priority levels
#[derive(Debug, Clone, PartialEq, Ord, PartialOrd, Eq, Serialize, Deserialize)]
pub enum RequestPriority {
    /// Critical priority - signature requests
    Critical,
    /// High priority - federation updates
    High,
    /// Normal priority - standard operations
    Normal,
    /// Low priority - monitoring and status
    Low,
}

/// Request state tracking
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RequestState {
    /// Request created but not sent
    Created,
    /// Request sent, waiting for response
    Pending,
    /// Partial response received (for multi-node requests)
    PartialResponse { received: usize, expected: usize },
    /// Request completed successfully
    Completed,
    /// Request failed with error
    Failed { error: String },
    /// Request timed out
    TimedOut,
    /// Request cancelled
    Cancelled,
}

/// Timeout queue entry for efficient timeout management
#[derive(Debug, Clone)]
struct TimeoutEntry {
    /// Request ID that will timeout
    request_id: String,
    /// Absolute timeout timestamp
    timeout_at: Instant,
}

/// Request statistics and metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestStatistics {
    /// Total requests tracked
    pub total_requests: u64,
    
    /// Successful requests
    pub successful_requests: u64,
    
    /// Failed requests
    pub failed_requests: u64,
    
    /// Timed out requests
    pub timeout_requests: u64,
    
    /// Average response time
    pub avg_response_time: Duration,
    
    /// Response time percentiles
    pub response_percentiles: ResponsePercentiles,
    
    /// Requests by type
    pub requests_by_type: HashMap<String, u64>,
    
    /// Currently pending requests
    pub pending_count: u64,
    
    /// Maximum concurrent requests seen
    pub max_concurrent_requests: u64,
    
    /// Last statistics reset
    pub last_reset: SystemTime,
}

/// Response time percentiles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsePercentiles {
    pub p50: Duration,
    pub p90: Duration,
    pub p95: Duration,
    pub p99: Duration,
}

/// Request timeout result
#[derive(Debug)]
pub enum TimeoutResult {
    /// Request timed out and was removed
    TimedOut {
        request_id: String,
        request_type: BridgeRequestType,
        elapsed: Duration,
    },
    /// No requests timed out
    None,
}

/// Response matching result
#[derive(Debug)]
pub enum ResponseMatchResult {
    /// Response successfully matched to request
    Matched {
        request_id: String,
        request_type: BridgeRequestType,
        response_time: Duration,
    },
    /// Response could not be matched
    Unmatched {
        response_id: String,
        correlation_id: Option<String>,
    },
    /// Request was already completed or cancelled
    AlreadyCompleted {
        request_id: String,
        state: RequestState,
    },
}

impl AdvancedRequestTracker {
    /// Create new advanced request tracker
    pub fn new(config: RequestTrackerConfig) -> Self {
        Self {
            pending_requests: HashMap::new(),
            timeout_queue: Vec::new(),
            correlation_mappings: HashMap::new(),
            stats: RequestStatistics::default(),
            config,
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(RequestTrackerConfig::default())
    }

    /// Track a new request with correlation support
    pub fn track_request(
        &mut self,
        request: StreamMessage,
        response_callback: oneshot::Sender<Result<StreamResponse, BridgeError>>,
    ) -> Result<String, BridgeError> {
        // Check capacity
        if self.pending_requests.len() >= self.config.max_pending_requests {
            return Err(BridgeError::InternalError(
                "Maximum pending requests exceeded".to_string()
            ));
        }

        let request_id = Uuid::new_v4().to_string();
        let correlation_id = if self.config.enable_tracing {
            Some(Uuid::new_v4().to_string())
        } else {
            None
        };

        let now = Instant::now();
        let timeout = self.get_timeout_for_request(&request);
        let timeout_at = now + timeout;

        // Create request metadata
        let metadata = RequestMetadata {
            source: "stream_actor".to_string(),
            targets: vec![], // Would be populated with actual target nodes
            trace_id: if self.config.enable_tracing { Some(Uuid::new_v4()) } else { None },
            span_id: if self.config.enable_tracing { Some(Uuid::new_v4()) } else { None },
            context: HashMap::new(),
            payload_size: self.estimate_request_size(&request),
        };

        let request_type = self.classify_request(&request);
        let priority = self.get_request_priority(&request_type);

        let entry = PendingRequestEntry {
            request_id: request_id.clone(),
            correlation_id: correlation_id.clone(),
            request_type: request_type.clone(),
            created_at: now,
            timeout,
            timeout_at,
            response_callback: Some(response_callback),
            retry_count: 0,
            metadata,
            priority,
            state: RequestState::Created,
        };

        // Add to pending requests
        self.pending_requests.insert(request_id.clone(), entry);

        // Add to timeout queue
        self.timeout_queue.push(TimeoutEntry {
            request_id: request_id.clone(),
            timeout_at,
        });

        // Sort timeout queue by timeout time
        self.timeout_queue.sort_by_key(|entry| entry.timeout_at);

        // Add correlation mapping if enabled
        if let Some(correlation_id) = &correlation_id {
            self.correlation_mappings.insert(correlation_id.clone(), request_id.clone());
        }

        // Update statistics
        self.stats.total_requests += 1;
        self.stats.pending_count += 1;
        self.stats.max_concurrent_requests = self.stats.max_concurrent_requests
            .max(self.stats.pending_count);

        // Update request type statistics
        let request_type_str = format!("{:?}", request_type);
        *self.stats.requests_by_type.entry(request_type_str).or_insert(0) += 1;

        info!(
            request_id = %request_id,
            correlation_id = ?correlation_id,
            request_type = ?request_type,
            timeout = ?timeout,
            "Tracking new request"
        );

        Ok(request_id)
    }

    /// Match incoming response to pending request
    pub fn match_response(
        &mut self,
        response: StreamResponse,
        response_id: Option<String>,
        correlation_id: Option<String>,
    ) -> ResponseMatchResult {
        let request_id = if let Some(response_id) = &response_id {
            // Try direct request ID match first
            if self.pending_requests.contains_key(response_id) {
                response_id.clone()
            } else if let Some(correlation_id) = &correlation_id {
                // Try correlation ID match
                self.correlation_mappings.get(correlation_id).cloned().unwrap_or_default()
            } else {
                return ResponseMatchResult::Unmatched {
                    response_id: response_id.clone(),
                    correlation_id,
                };
            }
        } else {
            return ResponseMatchResult::Unmatched {
                response_id: "unknown".to_string(),
                correlation_id,
            };
        };

        if let Some(mut request_entry) = self.pending_requests.remove(&request_id) {
            let response_time = request_entry.created_at.elapsed();
            
            // Check if request was already completed
            if matches!(request_entry.state, RequestState::Completed | RequestState::Failed { .. } | RequestState::Cancelled) {
                return ResponseMatchResult::AlreadyCompleted {
                    request_id,
                    state: request_entry.state,
                };
            }

            // Update request state
            request_entry.state = RequestState::Completed;

            // Send response via callback
            if let Some(callback) = request_entry.response_callback {
                if let Err(_) = callback.send(Ok(response)) {
                    warn!("Failed to deliver response to callback for request {}", request_id);
                }
            }

            // Remove from correlation mappings
            if let Some(correlation_id) = &request_entry.correlation_id {
                self.correlation_mappings.remove(correlation_id);
            }

            // Update statistics
            self.stats.successful_requests += 1;
            self.stats.pending_count = self.stats.pending_count.saturating_sub(1);
            self.update_response_time_stats(response_time);

            info!(
                request_id = %request_id,
                request_type = ?request_entry.request_type,
                response_time = ?response_time,
                "Successfully matched and completed request"
            );

            ResponseMatchResult::Matched {
                request_id,
                request_type: request_entry.request_type,
                response_time,
            }
        } else {
            ResponseMatchResult::Unmatched {
                response_id: request_id,
                correlation_id,
            }
        }
    }

    /// Check for timed out requests
    pub fn check_timeouts(&mut self) -> Vec<TimeoutResult> {
        let now = Instant::now();
        let mut timeout_results = Vec::new();
        let mut timed_out_indices = Vec::new();

        // Check timeout queue
        for (index, timeout_entry) in self.timeout_queue.iter().enumerate() {
            if now >= timeout_entry.timeout_at {
                timed_out_indices.push(index);
            } else {
                // Since queue is sorted, no more timeouts
                break;
            }
        }

        // Process timed out requests
        for index in timed_out_indices.into_iter().rev() {
            let timeout_entry = self.timeout_queue.remove(index);
            
            if let Some(mut request_entry) = self.pending_requests.remove(&timeout_entry.request_id) {
                let elapsed = request_entry.created_at.elapsed();
                
                // Update request state
                request_entry.state = RequestState::TimedOut;
                
                // Send timeout error via callback
                if let Some(callback) = request_entry.response_callback {
                    let error = BridgeError::RequestTimeout {
                        request_id: request_entry.request_id.clone(),
                        timeout: request_entry.timeout,
                    };
                    let _ = callback.send(Err(error));
                }

                // Remove from correlation mappings
                if let Some(correlation_id) = &request_entry.correlation_id {
                    self.correlation_mappings.remove(correlation_id);
                }

                // Update statistics
                self.stats.timeout_requests += 1;
                self.stats.pending_count = self.stats.pending_count.saturating_sub(1);

                warn!(
                    request_id = %request_entry.request_id,
                    request_type = ?request_entry.request_type,
                    elapsed = ?elapsed,
                    timeout = ?request_entry.timeout,
                    "Request timed out"
                );

                timeout_results.push(TimeoutResult::TimedOut {
                    request_id: request_entry.request_id,
                    request_type: request_entry.request_type,
                    elapsed,
                });
            }
        }

        if timeout_results.is_empty() {
            vec![TimeoutResult::None]
        } else {
            timeout_results
        }
    }

    /// Cancel a pending request
    pub fn cancel_request(&mut self, request_id: &str) -> Result<(), BridgeError> {
        if let Some(mut request_entry) = self.pending_requests.remove(request_id) {
            request_entry.state = RequestState::Cancelled;
            
            // Send cancellation via callback
            if let Some(callback) = request_entry.response_callback {
                let error = BridgeError::RequestCancelled {
                    request_id: request_id.to_string(),
                };
                let _ = callback.send(Err(error));
            }

            // Remove from correlation mappings
            if let Some(correlation_id) = &request_entry.correlation_id {
                self.correlation_mappings.remove(correlation_id);
            }

            // Remove from timeout queue
            self.timeout_queue.retain(|entry| entry.request_id != request_id);

            // Update statistics
            self.stats.pending_count = self.stats.pending_count.saturating_sub(1);

            info!("Cancelled request {}", request_id);
            Ok(())
        } else {
            Err(BridgeError::RequestNotFound {
                request_id: request_id.to_string(),
            })
        }
    }

    /// Get pending request count
    pub fn pending_count(&self) -> usize {
        self.pending_requests.len()
    }

    /// Get request statistics
    pub fn get_statistics(&self) -> &RequestStatistics {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_statistics(&mut self) {
        self.stats = RequestStatistics::default();
        self.stats.last_reset = SystemTime::now();
    }

    /// Get all pending request IDs
    pub fn get_pending_request_ids(&self) -> Vec<String> {
        self.pending_requests.keys().cloned().collect()
    }

    /// Get request state
    pub fn get_request_state(&self, request_id: &str) -> Option<&RequestState> {
        self.pending_requests.get(request_id).map(|entry| &entry.state)
    }

    /// Classify request type
    fn classify_request(&self, request: &StreamMessage) -> BridgeRequestType {
        match request {
            StreamMessage::RequestPegOutSignatures { request } => {
                BridgeRequestType::PegOutSignature {
                    pegout_id: request.pegout_id.clone(),
                    amount: request.amount,
                    destination: request.destination_address.to_string(),
                }
            }
            StreamMessage::HandleFederationUpdate { update } => {
                BridgeRequestType::FederationUpdate {
                    update_id: update.update_id.clone(),
                    update_type: format!("{:?}", update.update_type),
                }
            }
            StreamMessage::NotifyPegIn { notification } => {
                BridgeRequestType::PegInNotification {
                    pegin_id: notification.pegin_id.clone(),
                    amount: notification.amount,
                }
            }
            StreamMessage::SendHeartbeat => BridgeRequestType::Heartbeat,
            StreamMessage::GetConnectionStatus => BridgeRequestType::StatusCheck,
            _ => BridgeRequestType::Custom {
                request_name: request.message_type().to_string(),
                payload_size: 0, // Would calculate actual size
            },
        }
    }

    /// Get timeout for specific request type
    fn get_timeout_for_request(&self, request: &StreamMessage) -> Duration {
        match request {
            StreamMessage::RequestPegOutSignatures { request } => request.timeout,
            StreamMessage::HandleFederationUpdate { .. } => Duration::from_secs(120),
            StreamMessage::NotifyPegIn { .. } => Duration::from_secs(30),
            StreamMessage::SendHeartbeat => Duration::from_secs(10),
            StreamMessage::GetConnectionStatus => Duration::from_secs(5),
            _ => self.config.default_timeout,
        }
    }

    /// Get request priority
    fn get_request_priority(&self, request_type: &BridgeRequestType) -> RequestPriority {
        match request_type {
            BridgeRequestType::PegOutSignature { .. } => RequestPriority::Critical,
            BridgeRequestType::FederationUpdate { .. } => RequestPriority::High,
            BridgeRequestType::PegInNotification { .. } => RequestPriority::High,
            BridgeRequestType::NodeRegistration { .. } => RequestPriority::Normal,
            BridgeRequestType::StatusCheck => RequestPriority::Low,
            BridgeRequestType::Heartbeat => RequestPriority::Low,
            BridgeRequestType::Custom { .. } => RequestPriority::Normal,
        }
    }

    /// Estimate request payload size
    fn estimate_request_size(&self, request: &StreamMessage) -> usize {
        // Simplified size estimation - in real implementation would serialize
        match request {
            StreamMessage::RequestPegOutSignatures { .. } => 1024,
            StreamMessage::HandleFederationUpdate { .. } => 512,
            StreamMessage::NotifyPegIn { .. } => 256,
            _ => 128,
        }
    }

    /// Update response time statistics
    fn update_response_time_stats(&mut self, response_time: Duration) {
        let count = self.stats.successful_requests;
        if count <= 1 {
            self.stats.avg_response_time = response_time;
        } else {
            // Calculate running average
            let current_total = self.stats.avg_response_time.as_nanos() * (count - 1) as u128;
            let new_total = current_total + response_time.as_nanos();
            self.stats.avg_response_time = Duration::from_nanos((new_total / count as u128) as u64);
        }

        // Update percentiles (simplified - would use proper percentile calculation)
        self.stats.response_percentiles.p50 = response_time;
        self.stats.response_percentiles.p90 = response_time;
        self.stats.response_percentiles.p95 = response_time;
        self.stats.response_percentiles.p99 = response_time;
    }

    /// Check if there's a pending request with the given ID
    pub fn has_pending_request(&self, request_id: &str) -> bool {
        self.pending_requests.contains_key(request_id)
    }

    /// Complete a request and return its details
    pub fn complete_request(&mut self, request_id: &str) -> Option<PendingRequestEntry> {
        if let Some(mut entry) = self.pending_requests.remove(request_id) {
            entry.state = RequestState::Completed;

            // Update statistics
            self.stats.total_requests += 1;
            self.stats.successful_requests += 1;

            Some(entry)
        } else {
            None
        }
    }
}

impl Default for RequestTrackerConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(60),
            max_pending_requests: 1000,
            max_retries: 3,
            timeout_check_interval: Duration::from_secs(1),
            enable_tracing: true,
            cleanup_interval: Duration::from_secs(300),
        }
    }
}

impl Default for RequestStatistics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            timeout_requests: 0,
            avg_response_time: Duration::from_secs(0),
            response_percentiles: ResponsePercentiles {
                p50: Duration::from_secs(0),
                p90: Duration::from_secs(0),
                p95: Duration::from_secs(0),
                p99: Duration::from_secs(0),
            },
            requests_by_type: HashMap::new(),
            pending_count: 0,
            max_concurrent_requests: 0,
            last_reset: SystemTime::now(),
        }
    }
}