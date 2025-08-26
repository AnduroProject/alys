//! Consensus Handler Implementation
//!
//! Handles Aura PoA consensus operations, slot management, and validator coordination.
//! This module implements the hybrid PoA/PoW consensus mechanism where federation
//! members produce signed blocks optimistically and Bitcoin miners provide finalization.

use std::collections::{HashMap, VecDeque, BTreeMap};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use actix::prelude::*;
use tracing::*;
use uuid::Uuid;

use crate::types::*;
use super::super::{ChainActor, messages::*, state::*};

/// Configuration for Aura PoA consensus operations
#[derive(Debug, Clone)]
pub struct AuraConfig {
    /// Duration of each consensus slot
    pub slot_duration: Duration,
    /// Maximum allowed clock drift
    pub max_clock_drift: Duration,
    /// Minimum time before slot to start preparation
    pub preparation_time: Duration,
    /// Maximum time to wait for block production
    pub production_timeout: Duration,
    /// Number of missed slots before marking validator as down
    pub max_missed_slots: u32,
}

impl Default for AuraConfig {
    fn default() -> Self {
        Self {
            slot_duration: Duration::from_secs(2),
            max_clock_drift: Duration::from_millis(500),
            preparation_time: Duration::from_millis(100),
            production_timeout: Duration::from_secs(1),
            max_missed_slots: 5,
        }
    }
}

/// Slot assignment and scheduling information
#[derive(Debug, Clone)]
pub struct SlotSchedule {
    /// Slot number
    pub slot: u64,
    /// Expected start time of the slot
    pub start_time: SystemTime,
    /// Authority responsible for this slot
    pub authority: Address,
    /// Authority index in federation
    pub authority_index: usize,
    /// Whether this slot has been processed
    pub processed: bool,
}

/// Validator performance tracking
#[derive(Debug, Clone, Default)]
pub struct ValidatorMetrics {
    /// Total slots assigned
    pub slots_assigned: u64,
    /// Blocks successfully produced
    pub blocks_produced: u64,
    /// Slots missed
    pub slots_missed: u64,
    /// Average block production time
    pub avg_production_time_ms: f64,
    /// Recent performance window
    pub recent_performance: VecDeque<bool>,
    /// Last seen activity
    pub last_activity: Option<SystemTime>,
}

/// Aura consensus state manager
#[derive(Debug)]
pub struct AuraConsensusManager {
    /// Current consensus configuration
    config: AuraConfig,
    /// Active validator set
    validator_set: Vec<FederationMember>,
    /// Current slot information
    current_slot: u64,
    /// Next scheduled slot assignments
    slot_schedule: BTreeMap<u64, SlotSchedule>,
    /// Validator performance metrics
    validator_metrics: HashMap<Address, ValidatorMetrics>,
    /// Genesis timestamp for slot calculation
    genesis_timestamp: SystemTime,
    /// Slot preparation tasks
    preparation_tasks: HashMap<u64, Instant>,
}

impl AuraConsensusManager {
    pub fn new(config: AuraConfig, genesis_timestamp: SystemTime) -> Self {
        Self {
            config,
            validator_set: Vec::new(),
            current_slot: 0,
            slot_schedule: BTreeMap::new(),
            validator_metrics: HashMap::new(),
            genesis_timestamp,
            preparation_tasks: HashMap::new(),
        }
    }

    /// Update the validator set from federation configuration
    pub fn update_validator_set(&mut self, validators: Vec<FederationMember>) {
        info!("Updating validator set with {} members", validators.len());
        
        // Initialize metrics for new validators
        for validator in &validators {
            self.validator_metrics.entry(validator.address)
                .or_insert_with(ValidatorMetrics::default);
        }
        
        self.validator_set = validators;
        self.rebuild_slot_schedule();
    }

    /// Calculate the current slot based on system time
    pub fn calculate_current_slot(&self) -> u64 {
        let now = SystemTime::now();
        let elapsed = now.duration_since(self.genesis_timestamp)
            .unwrap_or_default();
        elapsed.as_secs() / self.config.slot_duration.as_secs()
    }

    /// Get the authority for a specific slot
    pub fn get_slot_authority(&self, slot: u64) -> Option<&FederationMember> {
        if self.validator_set.is_empty() {
            return None;
        }
        
        let authority_index = (slot % self.validator_set.len() as u64) as usize;
        self.validator_set.get(authority_index)
    }

    /// Check if we are the authority for the given slot
    pub fn is_our_slot(&self, slot: u64, our_address: &Address) -> bool {
        self.get_slot_authority(slot)
            .map(|auth| &auth.address == our_address)
            .unwrap_or(false)
    }

    /// Get the next slot we're responsible for
    pub fn get_next_our_slot(&self, our_address: &Address) -> Option<u64> {
        let current_slot = self.calculate_current_slot();
        
        for slot in (current_slot + 1)..(current_slot + 100) {
            if self.is_our_slot(slot, our_address) {
                return Some(slot);
            }
        }
        None
    }

    /// Start preparation for an upcoming slot
    pub fn prepare_for_slot(&mut self, slot: u64) {
        let now = Instant::now();
        self.preparation_tasks.insert(slot, now);
        
        debug!(slot = slot, "Started preparation for slot");
    }

    /// Record block production for a validator
    pub fn record_block_production(&mut self, authority: &Address, slot: u64, production_time: Duration) {
        let metrics = self.validator_metrics.entry(*authority)
            .or_insert_with(ValidatorMetrics::default);
        
        metrics.slots_assigned += 1;
        metrics.blocks_produced += 1;
        metrics.last_activity = Some(SystemTime::now());
        
        // Update average production time
        let new_time_ms = production_time.as_millis() as f64;
        metrics.avg_production_time_ms = 
            (metrics.avg_production_time_ms * (metrics.blocks_produced - 1) as f64 + new_time_ms) 
            / metrics.blocks_produced as f64;
        
        // Update recent performance window
        metrics.recent_performance.push_back(true);
        if metrics.recent_performance.len() > 100 {
            metrics.recent_performance.pop_front();
        }
        
        info!(
            authority = %authority,
            slot = slot,
            production_time_ms = production_time.as_millis(),
            "Recorded successful block production"
        );
    }

    /// Record missed slot for a validator
    pub fn record_missed_slot(&mut self, authority: &Address, slot: u64) {
        let metrics = self.validator_metrics.entry(*authority)
            .or_insert_with(ValidatorMetrics::default);
        
        metrics.slots_assigned += 1;
        metrics.slots_missed += 1;
        
        // Update recent performance window
        metrics.recent_performance.push_back(false);
        if metrics.recent_performance.len() > 100 {
            metrics.recent_performance.pop_front();
        }
        
        warn!(
            authority = %authority,
            slot = slot,
            total_missed = metrics.slots_missed,
            "Recorded missed slot"
        );
    }

    /// Get performance metrics for a validator
    pub fn get_validator_performance(&self, authority: &Address) -> Option<ValidatorPerformance> {
        let metrics = self.validator_metrics.get(authority)?;
        
        let success_rate = if metrics.slots_assigned > 0 {
            (metrics.blocks_produced as f64 / metrics.slots_assigned as f64) * 100.0
        } else {
            0.0
        };

        let uptime_percent = if !metrics.recent_performance.is_empty() {
            let successful = metrics.recent_performance.iter()
                .filter(|&&success| success)
                .count();
            (successful as f64 / metrics.recent_performance.len() as f64) * 100.0
        } else {
            0.0
        };

        Some(ValidatorPerformance {
            blocks_produced: metrics.blocks_produced as u32,
            blocks_missed: metrics.slots_missed as u32,
            success_rate,
            avg_production_time_ms: metrics.avg_production_time_ms as u64,
            uptime_percent,
        })
    }

    /// Rebuild the slot schedule based on current validator set
    fn rebuild_slot_schedule(&mut self) {
        let current_slot = self.calculate_current_slot();
        
        // Clear old schedule entries
        self.slot_schedule.clear();
        
        // Generate schedule for next 100 slots
        for slot in current_slot..(current_slot + 100) {
            if let Some(authority) = self.get_slot_authority(slot) {
                let slot_start = self.genesis_timestamp + 
                    Duration::from_secs(slot * self.config.slot_duration.as_secs());
                
                let schedule = SlotSchedule {
                    slot,
                    start_time: slot_start,
                    authority: authority.address,
                    authority_index: (slot % self.validator_set.len() as u64) as usize,
                    processed: false,
                };
                
                self.slot_schedule.insert(slot, schedule);
            }
        }
        
        debug!("Rebuilt slot schedule for {} slots", self.slot_schedule.len());
    }

    /// Check if any validators should be marked as down
    pub fn check_validator_health(&self) -> Vec<Address> {
        let mut down_validators = Vec::new();
        let now = SystemTime::now();
        
        for (address, metrics) in &self.validator_metrics {
            // Check if validator has missed too many recent slots
            let recent_failures = metrics.recent_performance.iter()
                .rev()
                .take(self.config.max_missed_slots as usize)
                .filter(|&&success| !success)
                .count();
            
            if recent_failures >= self.config.max_missed_slots as usize {
                down_validators.push(*address);
            }
            
            // Check last activity time
            if let Some(last_activity) = metrics.last_activity {
                let inactive_duration = now.duration_since(last_activity)
                    .unwrap_or_default();
                
                if inactive_duration > self.config.slot_duration * 10 {
                    down_validators.push(*address);
                }
            }
        }
        
        down_validators
    }
}

// Handler implementations for ChainActor
impl ChainActor {
    /// Handle federation update for consensus
    pub async fn handle_update_federation(&mut self, msg: UpdateFederation) -> Result<(), ChainError> {
        info!(
            version = msg.version,
            members = msg.members.len(),
            threshold = msg.threshold,
            "Updating federation configuration"
        );

        // Validate federation configuration
        if msg.members.is_empty() {
            return Err(ChainError::InvalidFederation("Empty member list".to_string()));
        }

        if msg.threshold == 0 || msg.threshold > msg.members.len() {
            return Err(ChainError::InvalidFederation("Invalid threshold".to_string()));
        }

        // Update federation state
        self.federation.version = msg.version;
        self.federation.members = msg.members.clone();
        self.federation.threshold = msg.threshold;
        
        // Update the Aura consensus manager
        if let Some(aura_manager) = &mut self.consensus_state.aura_manager {
            aura_manager.update_validator_set(msg.members.clone());
        }

        // Notify other actors of federation change
        self.notify_federation_update(&msg).await?;

        info!(
            version = msg.version,
            active_members = msg.members.iter().filter(|m| m.active).count(),
            "Federation update completed successfully"
        );

        Ok(())
    }

    /// Handle chain status request with consensus information
    pub async fn handle_get_chain_status(&mut self, msg: GetChainStatus) -> Result<ChainStatus, ChainError> {
        let mut status = ChainStatus::default();

        // Basic chain information
        status.head = self.chain_state.head.clone();
        status.best_block_number = self.chain_state.height;
        status.best_block_hash = self.chain_state.head
            .as_ref()
            .map(|h| h.hash)
            .unwrap_or_default();
        status.finalized = self.chain_state.finalized.clone();

        // Validator status
        status.validator_status = if self.config.is_validator {
            let authority_address = self.config.authority_key
                .as_ref()
                .map(|k| k.address())
                .unwrap_or_default();

            if let Some(aura_manager) = &self.consensus_state.aura_manager {
                let next_slot = aura_manager.get_next_our_slot(&authority_address);
                let next_slot_time = next_slot.map(|slot| {
                    let slot_start = aura_manager.genesis_timestamp + 
                        Duration::from_secs(slot * aura_manager.config.slot_duration.as_secs());
                    slot_start.duration_since(SystemTime::now())
                        .unwrap_or_default()
                        .as_millis() as u64
                });

                let performance = aura_manager.get_validator_performance(&authority_address)
                    .unwrap_or_default();

                ValidatorStatus::Validator {
                    address: authority_address,
                    is_active: true,
                    next_slot,
                    next_slot_in_ms: next_slot_time,
                    recent_performance: performance,
                    weight: 1, // Simplified weight system
                }
            } else {
                ValidatorStatus::NotValidator
            }
        } else {
            ValidatorStatus::NotValidator
        };

        // Federation status
        status.federation_status = FederationStatus {
            version: self.federation.version,
            active_members: self.federation.members.iter()
                .filter(|m| m.active)
                .count(),
            threshold: self.federation.threshold,
            ready: !self.federation.members.is_empty() && 
                   self.federation.threshold <= self.federation.members.len(),
            pending_changes: vec![], // Would track pending configuration changes
        };

        // Include additional metrics if requested
        if msg.include_metrics {
            status.performance = self.get_performance_status().await?;
        }

        if msg.include_sync_info {
            status.sync_status = self.get_sync_status().await?;
            status.network_status = self.get_network_status().await?;
        }

        Ok(status)
    }

    /// Handle pause block production request
    pub async fn handle_pause_block_production(&mut self, msg: PauseBlockProduction) -> Result<(), ChainError> {
        info!(
            reason = msg.reason,
            duration = ?msg.duration,
            finish_current = msg.finish_current,
            "Pausing block production"
        );

        // Verify authority if specified
        if let Some(authority) = &msg.authority {
            if !self.is_authorized_for_governance(authority) {
                return Err(ChainError::Unauthorized);
            }
        }

        // Pause production
        self.production_state.paused = true;
        self.production_state.pause_reason = Some(msg.reason);
        self.production_state.paused_at = Some(SystemTime::now());

        // Set resume time if duration specified
        if let Some(duration) = msg.duration {
            self.production_state.resume_at = Some(SystemTime::now() + duration);
        }

        // Notify other actors
        self.notify_production_pause().await?;

        Ok(())
    }

    /// Handle resume block production request
    pub async fn handle_resume_block_production(&mut self, msg: ResumeBlockProduction) -> Result<(), ChainError> {
        info!(
            force = msg.force,
            "Resuming block production"
        );

        // Verify authority if specified
        if let Some(authority) = &msg.authority {
            if !self.is_authorized_for_governance(authority) {
                return Err(ChainError::Unauthorized);
            }
        }

        // Check conditions for resume unless forced
        if !msg.force {
            if let Some(reason) = &self.production_state.pause_reason {
                if reason.contains("emergency") || reason.contains("critical") {
                    return Err(ChainError::ProductionPaused {
                        reason: "Emergency pause requires manual intervention".to_string(),
                    });
                }
            }
        }

        // Resume production
        self.production_state.paused = false;
        self.production_state.pause_reason = None;
        self.production_state.paused_at = None;
        self.production_state.resume_at = None;

        // Notify other actors
        self.notify_production_resume().await?;

        info!("Block production resumed successfully");
        Ok(())
    }

    /// Check if an address is authorized for governance operations
    fn is_authorized_for_governance(&self, address: &Address) -> bool {
        // Check if address is a federation member
        self.federation.members.iter()
            .any(|member| &member.address == address && member.active)
    }

    /// Notify other actors of federation update
    async fn notify_federation_update(&self, _msg: &UpdateFederation) -> Result<(), ChainError> {
        // Implementation would notify engine, bridge, and other relevant actors
        debug!("Notifying actors of federation update");
        Ok(())
    }

    /// Notify other actors of production pause
    async fn notify_production_pause(&self) -> Result<(), ChainError> {
        debug!("Notifying actors of production pause");
        Ok(())
    }

    /// Notify other actors of production resume
    async fn notify_production_resume(&self) -> Result<(), ChainError> {
        debug!("Notifying actors of production resume");
        Ok(())
    }

    /// Get current performance status
    async fn get_performance_status(&self) -> Result<ChainPerformanceStatus, ChainError> {
        let metrics_snapshot = self.metrics.snapshot();
        
        Ok(ChainPerformanceStatus {
            avg_block_time_ms: self.config.slot_duration.as_millis() as u64,
            blocks_per_second: 1.0 / self.config.slot_duration.as_secs_f64(),
            transactions_per_second: 0.0, // Would calculate from recent blocks
            memory_usage_mb: 0, // Would get from system metrics
            cpu_usage_percent: 0.0, // Would get from system metrics
        })
    }

    /// Get current sync status
    async fn get_sync_status(&self) -> Result<SyncStatus, ChainError> {
        // Implementation would check sync state with network
        Ok(SyncStatus::Synced)
    }

    /// Get network status
    async fn get_network_status(&self) -> Result<NetworkStatus, ChainError> {
        // Implementation would get status from network actor
        Ok(NetworkStatus {
            connected_peers: 0,
            inbound_connections: 0,
            outbound_connections: 0,
            avg_peer_height: None,
            health_score: 100,
        })
    }
}

/// Handler implementations for Actix messages
impl Handler<UpdateFederation> for ChainActor {
    type Result = ResponseActFuture<Self, Result<(), ChainError>>;

    fn handle(&mut self, msg: UpdateFederation, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            self.handle_update_federation(msg).await
        }.into_actor(self))
    }
}

impl Handler<GetChainStatus> for ChainActor {
    type Result = ResponseActFuture<Self, Result<ChainStatus, ChainError>>;

    fn handle(&mut self, msg: GetChainStatus, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            self.handle_get_chain_status(msg).await
        }.into_actor(self))
    }
}

impl Handler<PauseBlockProduction> for ChainActor {
    type Result = ResponseActFuture<Self, Result<(), ChainError>>;

    fn handle(&mut self, msg: PauseBlockProduction, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            self.handle_pause_block_production(msg).await
        }.into_actor(self))
    }
}

impl Handler<ResumeBlockProduction> for ChainActor {
    type Result = ResponseActFuture<Self, Result<(), ChainError>>;

    fn handle(&mut self, msg: ResumeBlockProduction, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            self.handle_resume_block_production(msg).await
        }.into_actor(self))
    }
}