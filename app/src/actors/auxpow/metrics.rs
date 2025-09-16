//! Metrics for V2 AuxPow system
//!
//! Provides comprehensive observability with exact legacy metric compatibility

use std::time::Instant;
use std::collections::VecDeque;

/// AuxPow actor metrics with legacy compatibility
#[derive(Debug)]
pub struct AuxPowMetrics {
    /// Total create_aux_block calls (legacy compatible)
    pub create_calls: u64,
    /// Total submit_aux_block calls (legacy compatible) 
    pub submit_calls: u64,
    /// Total successful submissions
    pub successful_submissions: u64,
    /// Total failed submissions
    pub failed_submissions: u64,
    /// Total blocks mined by this actor
    pub blocks_mined: u64,
    /// Total hashes processed (legacy compatible)
    pub hashes_processed: u64,
    /// Average time for create_aux_block operations
    pub avg_create_time_ms: f64,
    /// Average time for submit_aux_block operations
    pub avg_submit_time_ms: f64,
    /// Recent response times for performance monitoring
    pub recent_create_times: VecDeque<u64>,
    pub recent_submit_times: VecDeque<u64>,
    /// Actor start time
    pub started_at: Instant,
    /// Last activity timestamp
    pub last_activity: Option<Instant>,
    /// Error counters by type
    pub error_counts: ErrorCounts,
}

impl Default for AuxPowMetrics {
    fn default() -> Self {
        Self {
            create_calls: 0,
            submit_calls: 0,
            successful_submissions: 0,
            failed_submissions: 0,
            blocks_mined: 0,
            hashes_processed: 0,
            avg_create_time_ms: 0.0,
            avg_submit_time_ms: 0.0,
            recent_create_times: VecDeque::with_capacity(100),
            recent_submit_times: VecDeque::with_capacity(100),
            started_at: Instant::now(),
            last_activity: None,
            error_counts: ErrorCounts::default(),
        }
    }
}

impl AuxPowMetrics {
    /// Record create_aux_block call (legacy compatible)
    pub fn record_create_call(&mut self, duration_ms: u64) {
        self.create_calls += 1;
        self.last_activity = Some(Instant::now());
        
        // Update response time tracking
        self.recent_create_times.push_back(duration_ms);
        if self.recent_create_times.len() > 100 {
            self.recent_create_times.pop_front();
        }
        
        // Update average
        self.avg_create_time_ms = self.recent_create_times.iter()
            .sum::<u64>() as f64 / self.recent_create_times.len() as f64;
    }
    
    /// Record submit_aux_block call (legacy compatible)
    pub fn record_submit_call(&mut self, duration_ms: u64, success: bool) {
        self.submit_calls += 1;
        self.last_activity = Some(Instant::now());
        
        if success {
            self.successful_submissions += 1;
            self.blocks_mined += 1;
        } else {
            self.failed_submissions += 1;
        }
        
        // Update response time tracking
        self.recent_submit_times.push_back(duration_ms);
        if self.recent_submit_times.len() > 100 {
            self.recent_submit_times.pop_front();
        }
        
        // Update average
        self.avg_submit_time_ms = self.recent_submit_times.iter()
            .sum::<u64>() as f64 / self.recent_submit_times.len() as f64;
    }
    
    /// Record hash processing (legacy compatible)
    pub fn record_hashes_processed(&mut self, count: usize) {
        self.hashes_processed += count as u64;
    }
    
    /// Record error by type
    pub fn record_error(&mut self, error_type: &str) {
        self.error_counts.increment(error_type);
        self.last_activity = Some(Instant::now());
    }
    
    /// Get success rate percentage
    pub fn success_rate(&self) -> f64 {
        if self.submit_calls == 0 {
            0.0
        } else {
            (self.successful_submissions as f64 / self.submit_calls as f64) * 100.0
        }
    }
    
    /// Get uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
    
    /// Create performance snapshot
    pub fn performance_snapshot(&self) -> PerformanceSnapshot {
        PerformanceSnapshot {
            create_calls: self.create_calls,
            submit_calls: self.submit_calls,
            success_rate: self.success_rate(),
            avg_create_time_ms: self.avg_create_time_ms,
            avg_submit_time_ms: self.avg_submit_time_ms,
            blocks_mined: self.blocks_mined,
            uptime_seconds: self.uptime_seconds(),
            last_activity: self.last_activity,
        }
    }
}

/// Error counters by type
#[derive(Debug, Default)]
pub struct ErrorCounts {
    pub chain_syncing: u64,
    pub unknown_block: u64,
    pub invalid_pow: u64,
    pub invalid_auxpow: u64,
    pub communication_error: u64,
    pub other: u64,
}

impl ErrorCounts {
    fn increment(&mut self, error_type: &str) {
        match error_type {
            "chain_syncing" => self.chain_syncing += 1,
            "unknown_block" => self.unknown_block += 1,
            "invalid_pow" => self.invalid_pow += 1,
            "invalid_auxpow" => self.invalid_auxpow += 1,
            "communication_error" => self.communication_error += 1,
            _ => self.other += 1,
        }
    }
    
    pub fn total(&self) -> u64 {
        self.chain_syncing + self.unknown_block + self.invalid_pow + 
        self.invalid_auxpow + self.communication_error + self.other
    }
}

/// Performance snapshot for reporting
#[derive(Debug, Clone)]
pub struct PerformanceSnapshot {
    pub create_calls: u64,
    pub submit_calls: u64,
    pub success_rate: f64,
    pub avg_create_time_ms: f64,
    pub avg_submit_time_ms: f64,
    pub blocks_mined: u64,
    pub uptime_seconds: u64,
    pub last_activity: Option<Instant>,
}

/// DifficultyManager metrics
#[derive(Debug)]
pub struct DifficultyMetrics {
    /// Total difficulty calculations performed
    pub calculations: u64,
    /// Total retargeting events
    pub retargets: u64,
    /// Average calculation time
    pub avg_calc_time_ms: f64,
    /// Recent calculation times
    pub recent_calc_times: VecDeque<u64>,
    /// Cache hit/miss statistics
    pub cache_hits: u64,
    pub cache_misses: u64,
    /// History entries processed
    pub history_entries: u64,
    /// Actor start time
    pub started_at: Instant,
    /// Last activity
    pub last_activity: Option<Instant>,
}

impl Default for DifficultyMetrics {
    fn default() -> Self {
        Self {
            calculations: 0,
            retargets: 0,
            avg_calc_time_ms: 0.0,
            recent_calc_times: VecDeque::with_capacity(50),
            cache_hits: 0,
            cache_misses: 0,
            history_entries: 0,
            started_at: Instant::now(),
            last_activity: None,
        }
    }
}

impl DifficultyMetrics {
    /// Record difficulty calculation
    pub fn record_calculation(&mut self, duration_ms: u64, was_retarget: bool) {
        self.calculations += 1;
        self.last_activity = Some(Instant::now());
        
        if was_retarget {
            self.retargets += 1;
        }
        
        // Update timing
        self.recent_calc_times.push_back(duration_ms);
        if self.recent_calc_times.len() > 50 {
            self.recent_calc_times.pop_front();
        }
        
        self.avg_calc_time_ms = self.recent_calc_times.iter()
            .sum::<u64>() as f64 / self.recent_calc_times.len() as f64;
    }
    
    /// Record cache hit
    pub fn record_cache_hit(&mut self) {
        self.cache_hits += 1;
    }
    
    /// Record cache miss
    pub fn record_cache_miss(&mut self) {
        self.cache_misses += 1;
    }
    
    /// Get cache hit rate
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            (self.cache_hits as f64 / total as f64) * 100.0
        }
    }
    
    /// Record history entry processed
    pub fn record_history_entry(&mut self) {
        self.history_entries += 1;
    }
}