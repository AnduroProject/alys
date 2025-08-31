//! PegIn Confirmation Tracking
//! 
//! Advanced confirmation tracking for Bitcoin deposits

use bitcoin::Txid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tracing::{info, warn, debug};

/// Confirmation tracker for deposits
#[derive(Debug)]
pub struct ConfirmationTracker {
    /// Required confirmations threshold
    confirmation_threshold: u32,
    
    /// Tracking entries for each transaction
    tracking_entries: HashMap<Txid, ConfirmationEntry>,
    
    /// Confirmation history for analytics
    confirmation_history: Vec<ConfirmationEvent>,
    
    /// Statistics
    stats: ConfirmationStats,
}

/// Confirmation tracking entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationEntry {
    pub txid: Txid,
    pub start_block_height: u32,
    pub current_confirmations: u32,
    pub required_confirmations: u32,
    pub first_seen: SystemTime,
    pub last_updated: SystemTime,
    pub confirmation_rate: f64,
    pub estimated_confirmation_time: Option<Duration>,
    pub status: ConfirmationStatus,
    pub updates: Vec<ConfirmationUpdate>,
}

/// Confirmation status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfirmationStatus {
    /// Just started tracking
    Tracking,
    /// Progressing normally
    Progressing,
    /// Stalled (no new confirmations for a while)
    Stalled,
    /// Confirmed (reached threshold)
    Confirmed,
    /// Lost (transaction not found anymore)
    Lost,
}

/// Confirmation update event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationUpdate {
    pub confirmations: u32,
    pub block_height: u32,
    pub timestamp: SystemTime,
    pub time_since_last: Option<Duration>,
}

/// Confirmation event for history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationEvent {
    pub txid: Txid,
    pub event_type: ConfirmationEventType,
    pub confirmations: u32,
    pub timestamp: SystemTime,
    pub duration_since_start: Duration,
}

/// Types of confirmation events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfirmationEventType {
    TrackingStarted,
    ConfirmationReceived,
    ThresholdReached,
    TrackingStalled,
    TransactionLost,
}

/// Confirmation statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfirmationStats {
    pub total_tracked: u64,
    pub currently_tracking: u64,
    pub confirmed_transactions: u64,
    pub lost_transactions: u64,
    pub average_confirmation_time: Duration,
    pub fastest_confirmation: Option<Duration>,
    pub slowest_confirmation: Option<Duration>,
    pub stalled_count: u64,
}

impl ConfirmationTracker {
    /// Create new confirmation tracker
    pub fn new(confirmation_threshold: u32) -> Self {
        Self {
            confirmation_threshold,
            tracking_entries: HashMap::new(),
            confirmation_history: Vec::new(),
            stats: ConfirmationStats::default(),
        }
    }

    /// Start tracking a transaction
    pub fn start_tracking(&mut self, txid: Txid, start_block_height: u32) {
        info!("Starting confirmation tracking for {} at block {}", txid, start_block_height);

        let entry = ConfirmationEntry {
            txid,
            start_block_height,
            current_confirmations: 0,
            required_confirmations: self.confirmation_threshold,
            first_seen: SystemTime::now(),
            last_updated: SystemTime::now(),
            confirmation_rate: 0.0,
            estimated_confirmation_time: None,
            status: ConfirmationStatus::Tracking,
            updates: Vec::new(),
        };

        self.tracking_entries.insert(txid, entry);
        self.stats.total_tracked += 1;
        self.stats.currently_tracking += 1;

        // Record event
        let event = ConfirmationEvent {
            txid,
            event_type: ConfirmationEventType::TrackingStarted,
            confirmations: 0,
            timestamp: SystemTime::now(),
            duration_since_start: Duration::from_secs(0),
        };
        self.confirmation_history.push(event);
    }

    /// Update confirmations for a transaction
    pub fn update_confirmations(&mut self, txid: Txid, confirmations: u32, block_height: u32) -> bool {
        if let Some(entry) = self.tracking_entries.get_mut(&txid) {
            let now = SystemTime::now();
            let time_since_last = entry.last_updated.elapsed().ok();
            
            // Only update if confirmations increased
            if confirmations > entry.current_confirmations {
                debug!("Updating confirmations for {}: {} -> {}", txid, entry.current_confirmations, confirmations);
                
                entry.current_confirmations = confirmations;
                entry.last_updated = now;
                
                // Update confirmation rate (confirmations per minute)
                if let Ok(duration_since_start) = now.duration_since(entry.first_seen) {
                    if duration_since_start.as_secs() > 0 {
                        entry.confirmation_rate = (confirmations as f64) / (duration_since_start.as_secs_f64() / 60.0);
                    }
                }

                // Estimate time to completion
                if entry.confirmation_rate > 0.0 {
                    let remaining_confirmations = entry.required_confirmations.saturating_sub(confirmations);
                    let estimated_minutes = (remaining_confirmations as f64) / entry.confirmation_rate;
                    entry.estimated_confirmation_time = Some(Duration::from_secs((estimated_minutes * 60.0) as u64));
                }

                // Add update record
                let update = ConfirmationUpdate {
                    confirmations,
                    block_height,
                    timestamp: now,
                    time_since_last,
                };
                entry.updates.push(update);

                // Update status
                entry.status = if confirmations >= entry.required_confirmations {
                    ConfirmationStatus::Confirmed
                } else {
                    ConfirmationStatus::Progressing
                };

                // Record event
                let event_type = if confirmations >= entry.required_confirmations {
                    ConfirmationEventType::ThresholdReached
                } else {
                    ConfirmationEventType::ConfirmationReceived
                };

                let event = ConfirmationEvent {
                    txid,
                    event_type,
                    confirmations,
                    timestamp: now,
                    duration_since_start: now.duration_since(entry.first_seen).unwrap_or_default(),
                };
                self.confirmation_history.push(event);

                // Handle threshold reached
                if confirmations >= entry.required_confirmations {
                    self.handle_confirmation_complete(txid);
                }

                true
            } else {
                false // No update needed
            }
        } else {
            warn!("Attempted to update confirmations for untracked transaction: {}", txid);
            false
        }
    }

    /// Check for stalled transactions
    pub fn check_for_stalled_transactions(&mut self, stall_threshold: Duration) {
        let now = SystemTime::now();
        let mut stalled_txids = Vec::new();

        for (txid, entry) in &mut self.tracking_entries {
            if matches!(entry.status, ConfirmationStatus::Tracking | ConfirmationStatus::Progressing) {
                if let Ok(time_since_update) = now.duration_since(entry.last_updated) {
                    if time_since_update > stall_threshold {
                        warn!("Transaction {} appears stalled: {} seconds since last confirmation", 
                              txid, time_since_update.as_secs());
                        
                        entry.status = ConfirmationStatus::Stalled;
                        stalled_txids.push(*txid);
                        self.stats.stalled_count += 1;

                        // Record event
                        let event = ConfirmationEvent {
                            txid: *txid,
                            event_type: ConfirmationEventType::TrackingStalled,
                            confirmations: entry.current_confirmations,
                            timestamp: now,
                            duration_since_start: now.duration_since(entry.first_seen).unwrap_or_default(),
                        };
                        self.confirmation_history.push(event);
                    }
                }
            }
        }
    }

    /// Handle confirmation complete
    fn handle_confirmation_complete(&mut self, txid: Txid) {
        if let Some(entry) = self.tracking_entries.get(&txid) {
            let confirmation_duration = SystemTime::now()
                .duration_since(entry.first_seen)
                .unwrap_or_default();

            info!("Transaction {} confirmed in {:.1} minutes", 
                  txid, confirmation_duration.as_secs_f64() / 60.0);

            // Update statistics
            self.stats.confirmed_transactions += 1;
            self.stats.currently_tracking = self.stats.currently_tracking.saturating_sub(1);
            
            // Update timing statistics
            let total_time = self.stats.average_confirmation_time.as_secs_f64() * (self.stats.confirmed_transactions - 1) as f64;
            self.stats.average_confirmation_time = Duration::from_secs_f64(
                (total_time + confirmation_duration.as_secs_f64()) / self.stats.confirmed_transactions as f64
            );

            if self.stats.fastest_confirmation.map_or(true, |fastest| confirmation_duration < fastest) {
                self.stats.fastest_confirmation = Some(confirmation_duration);
            }

            if self.stats.slowest_confirmation.map_or(true, |slowest| confirmation_duration > slowest) {
                self.stats.slowest_confirmation = Some(confirmation_duration);
            }
        }
    }

    /// Stop tracking a transaction
    pub fn stop_tracking(&mut self, txid: Txid) -> Option<ConfirmationEntry> {
        info!("Stopping confirmation tracking for {}", txid);
        
        if let Some(entry) = self.tracking_entries.remove(&txid) {
            self.stats.currently_tracking = self.stats.currently_tracking.saturating_sub(1);
            Some(entry)
        } else {
            None
        }
    }

    /// Get tracking entry for transaction
    pub fn get_tracking_entry(&self, txid: &Txid) -> Option<&ConfirmationEntry> {
        self.tracking_entries.get(txid)
    }

    /// Get all tracking entries
    pub fn get_all_tracking_entries(&self) -> Vec<&ConfirmationEntry> {
        self.tracking_entries.values().collect()
    }

    /// Get transactions that need confirmation updates
    pub fn get_transactions_needing_updates(&self) -> Vec<Txid> {
        self.tracking_entries
            .iter()
            .filter(|(_, entry)| {
                matches!(entry.status, ConfirmationStatus::Tracking | ConfirmationStatus::Progressing)
                    && entry.current_confirmations < entry.required_confirmations
            })
            .map(|(txid, _)| *txid)
            .collect()
    }

    /// Update confirmation threshold
    pub fn update_threshold(&mut self, new_threshold: u32) {
        info!("Updating confirmation threshold: {} -> {}", self.confirmation_threshold, new_threshold);
        
        let old_threshold = self.confirmation_threshold;
        self.confirmation_threshold = new_threshold;

        // Update all tracking entries
        for entry in self.tracking_entries.values_mut() {
            entry.required_confirmations = new_threshold;
            
            // Re-evaluate status based on new threshold
            if entry.current_confirmations >= new_threshold && !matches!(entry.status, ConfirmationStatus::Confirmed) {
                entry.status = ConfirmationStatus::Confirmed;
                self.handle_confirmation_complete(entry.txid);
            } else if entry.current_confirmations < new_threshold && matches!(entry.status, ConfirmationStatus::Confirmed) {
                entry.status = ConfirmationStatus::Progressing;
            }
        }
    }

    /// Get confirmation statistics
    pub fn get_stats(&self) -> ConfirmationStats {
        self.stats.clone()
    }

    /// Clean up old history entries
    pub fn cleanup_old_entries(&mut self, max_history_entries: usize, max_age: Duration) {
        let now = SystemTime::now();
        
        // Remove old history entries
        self.confirmation_history.retain(|event| {
            now.duration_since(event.timestamp).unwrap_or_default() <= max_age
        });

        // Keep only recent entries if still over limit
        if self.confirmation_history.len() > max_history_entries {
            let excess = self.confirmation_history.len() - max_history_entries;
            self.confirmation_history.drain(0..excess);
        }

        // Remove completed/lost tracking entries older than max_age
        let txids_to_remove: Vec<Txid> = self.tracking_entries
            .iter()
            .filter(|(_, entry)| {
                matches!(entry.status, ConfirmationStatus::Confirmed | ConfirmationStatus::Lost)
                    && now.duration_since(entry.last_updated).unwrap_or_default() > max_age
            })
            .map(|(txid, _)| *txid)
            .collect();

        for txid in txids_to_remove {
            self.tracking_entries.remove(&txid);
        }

        debug!("Cleaned up confirmation tracker: {} history entries, {} tracking entries", 
               self.confirmation_history.len(), self.tracking_entries.len());
    }

    /// Get estimated time to confirmation for a transaction
    pub fn get_estimated_confirmation_time(&self, txid: &Txid) -> Option<Duration> {
        self.tracking_entries.get(txid)?.estimated_confirmation_time
    }

    /// Check if transaction is confirmed
    pub fn is_confirmed(&self, txid: &Txid) -> bool {
        self.tracking_entries.get(txid)
            .map_or(false, |entry| matches!(entry.status, ConfirmationStatus::Confirmed))
    }

    /// Get current confirmations for transaction
    pub fn get_current_confirmations(&self, txid: &Txid) -> Option<u32> {
        self.tracking_entries.get(txid).map(|entry| entry.current_confirmations)
    }
}