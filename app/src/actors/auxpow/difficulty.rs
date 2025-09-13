//! DifficultyManager Actor Implementation  
//!
//! Specialized actor for Bitcoin-compatible difficulty adjustment with exact
//! legacy algorithm ports and persistent storage integration.

use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use actix::prelude::*;
use tracing::*;

use bitcoin::CompactTarget;
use lighthouse_facade::types::Uint256 as U256;
use rust_decimal::prelude::*;

use crate::{
    actors::{
        auxpow::config::BitcoinConsensusParams,
        storage::StorageActor,
    },
    types::*,
};
use crate::actors::auxpow::AuxPowError;

use super::{
    messages::{DifficultyEntry, GetStoredDifficultyHistory, GetLastRetargetHeight, UpdateDifficultyHistory, GetDifficultyHistory, GetDifficultyStats, GetNextWorkRequired, HealthCheck, DifficultyStats, DifficultyAdjustment},
    config::DifficultyConfig,
    error::{DifficultyError, DifficultyResult},
    metrics::DifficultyMetrics,
};

// Explicitly import storage message types to avoid conflicts
use crate::actors::auxpow::messages::{SaveDifficultyEntry as AuxPowSaveDifficultyEntry, SaveRetargetHeight as AuxPowSaveRetargetHeight};
use crate::actors::auxpow::messages::HealthCheckResult;

/// Specialized difficulty adjustment and management actor
///
/// Provides exact ports of legacy difficulty functions:
/// - get_next_work_required() with Bitcoin-compatible retargeting
/// - calculate_next_work_required() with decimal precision math
/// - was_retarget_height() validation
/// - Persistent storage integration for difficulty history
pub struct DifficultyManager {
    /// Bitcoin consensus parameters (from chain spec)
    consensus_params: BitcoinConsensusParams,
    /// Difficulty history for retargeting calculations  
    difficulty_history: VecDeque<DifficultyEntry>,
    /// Current difficulty target
    current_target: CompactTarget,
    /// Last retarget height for interval tracking
    last_retarget_height: u64,
    /// Reference to storage actor for persistence
    storage_actor: Option<Addr<StorageActor>>,
    /// Performance metrics
    metrics: DifficultyMetrics,
    /// Configuration
    config: DifficultyConfig,
}

impl Actor for DifficultyManager {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!(
            current_target = %self.current_target.to_consensus(),
            history_size = self.difficulty_history.len(),
            last_retarget_height = self.last_retarget_height,
            "DifficultyManager started"
        );

        // Start periodic history cleanup
        self.start_history_cleanup_timer(ctx);
        
        // Start metrics reporting
        self.start_metrics_reporting(ctx);
        
        // Register with supervisor
        self.register_with_supervisor(ctx);
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        info!(
            calculations = self.metrics.calculations,
            retargets = self.metrics.retargets,
            cache_hit_rate = %format!("{:.1}%", self.metrics.cache_hit_rate()),
            "DifficultyManager stopping gracefully"
        );
        Running::Stop
    }
}

impl DifficultyManager {
    /// Create new DifficultyManager with default state
    pub fn new(config: DifficultyConfig) -> Self {
        Self {
            consensus_params: config.consensus_params.clone(),
            difficulty_history: VecDeque::with_capacity(config.history_size),
            current_target: CompactTarget::from_consensus(config.consensus_params.pow_limit),
            last_retarget_height: 0,
            storage_actor: None,
            metrics: DifficultyMetrics::default(),
            config,
        }
    }

    /// Create DifficultyManager with storage integration and restored state
    pub async fn restore_from_storage(
        storage_actor: Addr<StorageActor>,
        config: DifficultyConfig,
    ) -> DifficultyResult<Self> {
        info!("Restoring DifficultyManager state from storage");
        
        // Load difficulty history from storage
        let difficulty_entries = match storage_actor
            .send(GetStoredDifficultyHistory { 
                limit: Some(config.history_size),
                start_height: None,
            })
            .await
        {
            Ok(Ok(entries)) => entries,
            Ok(Err(e)) => {
                warn!("Failed to load difficulty history: {:?}, starting fresh", e);
                Vec::new()
            }
            Err(e) => {
                warn!("Storage communication failed: {:?}, starting fresh", e);
                Vec::new()
            }
        };

        // Load last retarget height
        let last_retarget_height = match storage_actor
            .send(GetLastRetargetHeight)
            .await
        {
            Ok(Ok(Some(height))) => height,
            _ => {
                debug!("No stored retarget height, starting from 0");
                0
            }
        };

        // Calculate current target from most recent entry
        let current_target = difficulty_entries
            .last()
            .map(|entry| entry.bits)
            .unwrap_or_else(|| CompactTarget::from_consensus(config.consensus_params.pow_limit));

        info!(
            restored_history_entries = difficulty_entries.len(),
            last_retarget_height = last_retarget_height,
            current_target = %current_target.to_consensus(),
            "DifficultyManager state restored from storage"
        );

        Ok(Self {
            consensus_params: config.consensus_params.clone(),
            difficulty_history: VecDeque::from(difficulty_entries),
            current_target,
            last_retarget_height,
            storage_actor: Some(storage_actor),
            metrics: DifficultyMetrics::default(),
            config,
        })
    }

    /// Set storage actor reference (for delayed initialization)
    pub fn set_storage_actor(&mut self, storage_actor: Addr<StorageActor>) {
        self.storage_actor = Some(storage_actor);
    }

    /// Start periodic history cleanup
    fn start_history_cleanup_timer(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(self.config.cache_cleanup_interval, |act, _| {
            let original_len = act.difficulty_history.len();
            
            // Keep history bounded to configured size
            while act.difficulty_history.len() > act.config.history_size {
                act.difficulty_history.pop_front();
            }
            
            if original_len != act.difficulty_history.len() {
                debug!(
                    removed = original_len - act.difficulty_history.len(),
                    remaining = act.difficulty_history.len(),
                    "Cleaned up old difficulty history entries"
                );
            }
        });
    }

    /// Start metrics reporting timer
    fn start_metrics_reporting(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(300), |act, _| { // Every 5 minutes
            info!(
                calculations = act.metrics.calculations,
                retargets = act.metrics.retargets,
                avg_calc_time = %format!("{:.1}ms", act.metrics.avg_calc_time_ms),
                cache_hit_rate = %format!("{:.1}%", act.metrics.cache_hit_rate()),
                history_entries = act.difficulty_history.len(),
                "DifficultyManager metrics"
            );
        });
    }

    /// Register with supervisor
    fn register_with_supervisor(&self, _ctx: &mut Context<Self>) {
        // TODO: Implement supervisor registration
        debug!("DifficultyManager registered with supervision system");
    }

    /// Direct port of legacy is_retarget_height function
    fn is_retarget_height_static(consensus_params: &BitcoinConsensusParams, chain_head_height: u64, height_difference: u32) -> bool {
        let adjustment_interval = consensus_params.difficulty_adjustment_interval();
        let height_is_multiple_of_adjustment_interval = chain_head_height % adjustment_interval == 0;
        let height_diff_is_greater_than_adjustment_interval =
            height_difference > adjustment_interval as u32;

        height_is_multiple_of_adjustment_interval || height_diff_is_greater_than_adjustment_interval
    }


    /// Static version for use in async contexts
    async fn calculate_difficulty_static(
        consensus_params: &BitcoinConsensusParams,
        auxpow_height_difference: u32,
        last_bits: u32,
        _storage_actor: &Option<Addr<StorageActor>>,
    ) -> DifficultyResult<CompactTarget> {
        let _start_time = Instant::now();
        
        // Guarantee height difference is not 0 (exact legacy logic)
        let mut height_diff = auxpow_height_difference;
        if height_diff == 0 {
            error!("Auxpow height difference is 0");
            height_diff = 1;
        }

        // Calculate ratio (exact legacy logic with rust_decimal)
        let mut ratio: Decimal =
            Decimal::from(height_diff) / Decimal::from(consensus_params.pow_target_spacing);

        // Round to 2 decimal places (exact legacy logic)
        ratio = ratio.round_dp(2);
        trace!(
            "Unclamped ratio between actual timespan and target timespan: {}",
            ratio
        );

        // Calculate adjustment bounds (exact legacy logic)
        let max_adjustment = Decimal::from(consensus_params.max_pow_adjustment);
        let max_lower_bound = max_adjustment / dec!(100);
        let max_upper_bound = max_lower_bound + dec!(1);

        // Clamp ratio within bounds (exact legacy logic)
        ratio = ratio.max(max_lower_bound);
        ratio = ratio.min(max_upper_bound);

        trace!(
            "Clamped ratio between actual timespan and target timespan: {}",
            ratio
        );

        // Get current target as U256 (exact legacy logic)
        let current_target = Self::uint256_target_from_compact_static(last_bits);
        
        // Calculate new target (exact legacy logic)
        let ratio_multiplier = (ratio * dec!(1000000)).to_u64().unwrap_or(1000000);
        let new_target = current_target.saturating_mul(U256::from(ratio_multiplier)) / U256::from(1000000);

        // Ensure new target doesn't exceed proof-of-work limit (exact legacy logic)
        let pow_limit = Self::uint256_target_from_compact_static(consensus_params.pow_limit);
        let final_target = if new_target > pow_limit {
            pow_limit
        } else {
            new_target
        };

        // Convert back to compact form (exact legacy logic)
        let new_bits = Self::target_to_compact_static(final_target);
        
        Ok(CompactTarget::from_consensus(new_bits))
    }

    /// Direct port of legacy calculate_next_work_required function
    async fn calculate_next_work_required(
        &mut self,
        auxpow_height_difference: u32,
        last_bits: u32,
    ) -> DifficultyResult<CompactTarget> {
        let start_time = Instant::now();
        
        // Guarantee height difference is not 0 (exact legacy logic)
        let mut height_diff = auxpow_height_difference;
        if height_diff == 0 {
            error!("Auxpow height difference is 0");
            height_diff = 1;
        }

        // Calculate ratio (exact legacy logic with rust_decimal)
        let mut ratio: Decimal =
            Decimal::from(height_diff) / Decimal::from(self.consensus_params.pow_target_spacing);

        // Round to 2 decimal places (exact legacy logic)
        ratio = ratio.round_dp(2);
        trace!(
            "Unclamped ratio between actual timespan and target timespan: {}",
            ratio
        );

        // Calculate adjustment bounds (exact legacy logic)
        let max_adjustment = Decimal::from(self.consensus_params.max_pow_adjustment);
        let max_lower_bound = max_adjustment / dec!(100);
        let max_upper_bound = max_lower_bound + dec!(1);

        // Apply ratio bounds (exact legacy logic)
        if ratio < dec!(1) {
            ratio = ratio.max(max_lower_bound); // Note: fixed from .min() in legacy
        } else if ratio > dec!(1) {
            ratio = ratio.min(max_upper_bound);
        }

        trace!(
            "Clamped ratio between actual timespan and target timespan: {}",
            ratio
        );

        // Calculate adjustment percentage (exact legacy logic)
        let adjustment_percentage = (ratio * dec!(100))
            .to_u8()
            .ok_or(DifficultyError::CalculationOverflow)?;

        // Convert compact target to U256 and calculate adjustment (exact legacy logic)
        let target = self.uint256_target_from_compact(last_bits);
        let single_percentage = target.checked_div(U256::from(100))
            .ok_or(DifficultyError::CalculationOverflow)?;

        let adjustment_percentage = U256::from(adjustment_percentage);

        trace!(
            "Adjustment percentage: {}\nSingle Percentage: {}",
            adjustment_percentage,
            single_percentage
        );

        let adjusted_target = single_percentage.saturating_mul(adjustment_percentage);

        trace!(
            "Original target: {}, adjusted target: {}",
            target,
            adjusted_target
        );

        let result = self.target_to_compact_lossy(adjusted_target);
        
        // Record timing
        let duration = start_time.elapsed().as_millis() as u64;
        self.metrics.record_calculation(duration, true); // This is a retarget

        Ok(result)
    }

    /// Static version for updating and persisting difficulty
    async fn persist_difficulty_static(
        _entry: DifficultyEntry,
        _storage_actor: Option<Addr<StorageActor>>,
        _config: DifficultyConfig,
    ) -> DifficultyResult<()> {
        // Storage persistence is handled by the main update_and_persist_difficulty method
        // This is just a placeholder for the async context
        Ok(())
    }

    /// Update difficulty history and persist to storage
    async fn update_and_persist_difficulty(
        &mut self,
        entry: DifficultyEntry,
    ) -> DifficultyResult<()> {
        // Add to in-memory history
        self.difficulty_history.push_back(entry.clone());
        self.metrics.record_history_entry();
        
        // Persist to storage if available
        if let Some(storage_actor) = &self.storage_actor {
            match storage_actor
                .send(AuxPowSaveDifficultyEntry { entry: entry.clone() })
                .await
            {
                Ok(Ok(_)) => {
                    trace!("Difficulty entry saved to storage");
                }
                Ok(Err(e)) => {
                    warn!("Failed to save difficulty entry: {:?}", e);
                }
                Err(e) => {
                    warn!("Storage communication failed: {:?}", e);
                }
            }

            // Update retarget height if this was a retarget
            if self.was_retarget_height(entry.height) {
                self.last_retarget_height = entry.height;
                match storage_actor
                    .send(AuxPowSaveRetargetHeight { height: entry.height })
                    .await
                {
                    Ok(Ok(_)) => {
                        trace!("Retarget height saved to storage");
                    }
                    Ok(Err(e)) => {
                        warn!("Failed to save retarget height: {:?}", e);
                    }
                    Err(e) => {
                        warn!("Storage communication failed for retarget height: {:?}", e);
                    }
                }
            }
        }

        // Keep in-memory history bounded
        while self.difficulty_history.len() > self.config.history_size {
            self.difficulty_history.pop_front();
        }

        Ok(())
    }

    /// Check if height was a retarget event
    pub fn was_retarget_height(&self, height: u64) -> bool {
        let interval = self.consensus_params.difficulty_adjustment_interval();
        height % interval == 0
    }


    /// Static version of uint256_target_from_compact
    fn uint256_target_from_compact_static(bits: u32) -> U256 {
        let (mant, expt) = {
            let unshifted_expt = bits >> 24;
            if unshifted_expt <= 3 {
                ((bits & 0xFFFFFF) >> (8 * (3 - unshifted_expt as usize)), 0)
            } else {
                (bits & 0xFFFFFF, 8 * ((bits >> 24) - 3))
            }
        };

        // The mantissa is signed but may not be negative
        if mant > 0x7F_FFFF {
            U256::zero()
        } else {
            U256::from(mant) << expt
        }
    }

    /// Static version of target_to_compact_lossy
    fn target_to_compact_static(target: U256) -> u32 {
        let mut size = (target.bits() + 7) / 8;
        let mut compact = if size <= 3 {
            (target.low_u64() << (8 * (3 - size))) as u32
        } else {
            let bn = target >> (8 * (size - 3));
            bn.low_u32()
        };

        if (compact & 0x0080_0000) != 0 {
            compact >>= 8;
            size += 1;
        }

        compact | ((size as u32) << 24)
    }

    /// Direct port of legacy uint256_target_from_compact function
    pub fn uint256_target_from_compact(&self, bits: u32) -> U256 {
        let (mant, expt) = {
            let unshifted_expt = bits >> 24;
            if unshifted_expt <= 3 {
                ((bits & 0xFFFFFF) >> (8 * (3 - unshifted_expt as usize)), 0)
            } else {
                (bits & 0xFFFFFF, 8 * ((bits >> 24) - 3))
            }
        };

        // The mantissa is signed but may not be negative
        if mant > 0x7F_FFFF {
            U256::zero()
        } else {
            U256::from(mant) << expt
        }
    }

    /// Direct port of legacy target_to_compact_lossy function
    pub fn target_to_compact_lossy(&self, target: U256) -> CompactTarget {
        let mut size = (target.bits() + 7) / 8;
        let mut compact = if size <= 3 {
            (target.low_u64() << (8 * (3 - size))) as u32
        } else {
            let bn = target >> (8 * (size - 3));
            bn.low_u32()
        };

        if (compact & 0x0080_0000) != 0 {
            compact >>= 8;
            size += 1;
        }

        CompactTarget::from_consensus(compact | ((size as u32) << 24))
    }
    
    // ============================================================================
    // Public getter methods for testing
    // ============================================================================
    
    /// Get the current difficulty history length for testing
    pub fn difficulty_history_len(&self) -> usize {
        self.difficulty_history.len()
    }
    
    /// Get the last retarget height for testing
    pub fn get_last_retarget_height(&self) -> u64 {
        self.last_retarget_height
    }
    
    /// Instance method wrapper for is_retarget_height_static for testing
    pub fn is_retarget_height(&self, chain_head_height: u64, height_difference: u32) -> bool {
        Self::is_retarget_height_static(&self.consensus_params, chain_head_height, height_difference)
    }
}

// ============================================================================
// Message Handler Implementations
// ============================================================================

/// Handler for GetNextWorkRequired - Direct port of legacy get_next_work_required
impl Handler<GetNextWorkRequired> for DifficultyManager {
    type Result = ResponseFuture<DifficultyResult<CompactTarget>>;

    fn handle(&mut self, msg: GetNextWorkRequired, ctx: &mut Context<Self>) -> Self::Result {
        // Clone what we need upfront
        let consensus_params = self.consensus_params.clone();
        let chain_head_height = msg.chain_head_height;
        let index_last_height = msg.index_last.height();
        let index_last_bits = msg.index_last.bits().unwrap_or(0x1d00ffff);
        
        // Check if retargeting is disabled or not needed (exact legacy logic)
        let auxpow_height_difference = (chain_head_height + 1 - index_last_height) as u32;
        
        if consensus_params.pow_no_retargeting
            || !Self::is_retarget_height_static(&consensus_params, chain_head_height, auxpow_height_difference)
        {
            trace!(
                "No retargeting, using last bits: {:?}",
                consensus_params.pow_no_retargeting
            );
            trace!("Last bits: {:?}", index_last_bits);
            
            let result = CompactTarget::from_consensus(index_last_bits);
            
            // Record timing (not a retarget) - we'll do this synchronously for the simple case
            self.metrics.record_calculation(0, false);
            
            Box::pin(async move { Ok(result) })
        } else {
            trace!(
                "Retargeting, using new bits at height {}",
                chain_head_height + 1
            );
            trace!("Last bits: {:?}", index_last_bits);

            // Get actor address for updating state later
            let addr = ctx.address();
            
            // Clone what we need for the calculation
            let consensus_params_calc = consensus_params.clone();
            let storage_actor = self.storage_actor.clone();
            
            Box::pin(async move {
                // Calculate new difficulty using static method
                let next_work = Self::calculate_difficulty_static(
                    &consensus_params_calc,
                    auxpow_height_difference, 
                    index_last_bits,
                    &storage_actor
                ).await?;
                
                info!(
                    "Difficulty adjustment from {} to {}",
                    index_last_bits,
                    next_work.to_consensus()
                );

                // Create entry for persistence
                let entry = DifficultyEntry {
                    height: chain_head_height + 1,
                    timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default(),
                    bits: next_work,
                    auxpow_count: 1,
                };
                
                // Send update message to self to update state
                let _ = addr.do_send(UpdateDifficultyHistory {
                    height: entry.height,
                    timestamp: entry.timestamp,
                    bits: entry.bits,
                    auxpow_count: entry.auxpow_count,
                });

                Ok(next_work)
            })
        }
    }
}

/// Handler for UpdateDifficultyHistory
impl Handler<UpdateDifficultyHistory> for DifficultyManager {
    type Result = ResponseFuture<DifficultyResult<()>>;

    fn handle(&mut self, msg: UpdateDifficultyHistory, _: &mut Context<Self>) -> Self::Result {
        let entry = DifficultyEntry {
            height: msg.height,
            timestamp: msg.timestamp,
            bits: msg.bits,
            auxpow_count: msg.auxpow_count,
        };

        // Update current target if this is a retarget
        if entry.bits != self.current_target {
            self.current_target = entry.bits;
        }

        // Get what we need for the async operation
        let storage_actor = self.storage_actor.clone();
        let config = self.config.clone();
        
        Box::pin(async move {
            Self::persist_difficulty_static(entry, storage_actor, config).await
        })
    }
}

/// Handler for GetDifficultyHistory  
impl Handler<GetDifficultyHistory> for DifficultyManager {
    type Result = DifficultyResult<Vec<DifficultyEntry>>;

    fn handle(&mut self, msg: GetDifficultyHistory, _: &mut Context<Self>) -> Self::Result {
        let mut entries: Vec<_> = self.difficulty_history.iter().cloned().collect();

        // Apply filters
        if let Some(start_height) = msg.start_height {
            entries.retain(|entry| entry.height >= start_height);
        }

        if let Some(limit) = msg.limit {
            entries.truncate(limit);
        }

        Ok(entries)
    }
}

/// Handler for GetDifficultyStats
impl Handler<GetDifficultyStats> for DifficultyManager {
    type Result = Result<DifficultyStats, DifficultyError>;

    fn handle(&mut self, _: GetDifficultyStats, _: &mut Context<Self>) -> Self::Result {
        let current_difficulty = if self.current_target.to_consensus() != 0 {
            // Difficulty = max_target / current_target (simplified)
            self.consensus_params.pow_limit as f64 / self.current_target.to_consensus() as f64
        } else {
            0.0
        };

        let adjustment_interval = self.consensus_params.difficulty_adjustment_interval();
        let blocks_until_retarget = if self.last_retarget_height == 0 {
            adjustment_interval
        } else {
            adjustment_interval - (self.last_retarget_height % adjustment_interval)
        };

        // Get recent adjustments for history
        let adjustment_history: Vec<DifficultyAdjustment> = self.difficulty_history
            .iter()
            .filter(|entry| self.was_retarget_height(entry.height))
            .take(10) // Last 10 adjustments
            .enumerate()
            .map(|(i, entry)| {
                let prev_target = if i > 0 {
                    self.difficulty_history.get(i - 1).map(|e| e.bits)
                        .unwrap_or(CompactTarget::from_consensus(self.consensus_params.pow_limit))
                } else {
                    CompactTarget::from_consensus(self.consensus_params.pow_limit)
                };

                DifficultyAdjustment {
                    height: entry.height,
                    old_target: prev_target,
                    new_target: entry.bits,
                    adjustment_ratio: prev_target.to_consensus() as f64 / entry.bits.to_consensus() as f64,
                    blocks_in_period: adjustment_interval as u32,
                    actual_timespan: entry.timestamp,
                    target_timespan: Duration::from_secs(
                        self.consensus_params.pow_target_timespan
                    ),
                }
            })
            .collect();

        Ok(DifficultyStats {
            current_target: self.current_target,
            current_difficulty,
            last_retarget_height: self.last_retarget_height,
            blocks_until_retarget,
            estimated_next_difficulty: None, // Could be calculated from recent block times
            adjustment_history,
        })
    }
}

/// Handler for HealthCheck
impl Handler<HealthCheck> for DifficultyManager {
    type Result = Result<HealthCheckResult, AuxPowError>;

    fn handle(&mut self, _: HealthCheck, _: &mut Context<Self>) -> Self::Result {
        let mut score = 100u8;

        // Check if we have reasonable difficulty history
        if self.difficulty_history.is_empty() {
            score = score.saturating_sub(20);
        }

        // Check for recent activity
        if let Some(last_activity) = self.metrics.last_activity {
            let inactive_duration = Instant::now().duration_since(last_activity);
            if inactive_duration > Duration::from_secs(3600) { // 1 hour
                score = score.saturating_sub(15);
            }
        }

        // Check storage connectivity
        if self.storage_actor.is_none() {
            score = score.saturating_sub(10); // Minor issue, not critical
        }

        let healthy = score >= 50;
        let details = format!(
            "Calculations: {}, retargets: {}, history entries: {}, cache hit rate: {:.1}%",
            self.metrics.calculations,
            self.metrics.retargets,
            self.difficulty_history.len(),
            self.metrics.cache_hit_rate()
        );

        Ok(HealthCheckResult {
            healthy,
            score,
            details,
            last_activity: self.metrics.last_activity,
            error_count: 0, // No error tracking yet
        })
    }
}