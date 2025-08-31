//! UTXO Management for Bridge Operations
//! 
//! Advanced UTXO tracking, selection, and management for peg-out operations

use bitcoin::{OutPoint, TxOut, Address as BtcAddress, ScriptBuf, Txid};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime};
use tracing::{info, warn, error, debug};
use crate::types::*;

/// Minimum satoshis for a spendable UTXO (dust limit)
pub const DUST_LIMIT: u64 = 546;

/// Minimum confirmations required for UTXO to be spendable
pub const MIN_CONFIRMATIONS: u32 = 6;

/// How often to refresh UTXO set from Bitcoin node
pub const UTXO_REFRESH_INTERVAL: Duration = Duration::from_secs(120);

/// UTXO with metadata for bridge operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utxo {
    pub outpoint: OutPoint,
    pub output: TxOut,
    pub confirmations: u32,
    pub block_height: u32,
    pub spendable: bool,
    pub reserved: bool,
    pub reserved_for: Option<String>, // pegout_id if reserved
    pub created_at: SystemTime,
    pub last_seen: SystemTime,
}

/// Comprehensive UTXO manager for federation funds
#[derive(Debug)]
pub struct UtxoManager {
    /// Current UTXO set
    utxo_set: HashMap<OutPoint, Utxo>,
    
    /// UTXOs that have been spent (to avoid double-spending)
    spent_utxos: HashSet<OutPoint>,
    
    /// UTXOs reserved for pending operations
    reserved_utxos: HashMap<OutPoint, String>, // outpoint -> pegout_id
    
    /// Federation address and script
    federation_address: BtcAddress,
    federation_script: ScriptBuf,
    
    /// Statistics and monitoring
    last_refresh: SystemTime,
    total_value: u64,
    stats: UtxoStats,
}

/// UTXO set statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoStats {
    pub total_utxos: usize,
    pub spendable_utxos: usize,
    pub reserved_utxos: usize,
    pub total_value: u64,
    pub spendable_value: u64,
    pub reserved_value: u64,
    pub last_updated: SystemTime,
}

/// UTXO selection strategy result
#[derive(Debug, Clone)]
pub struct UtxoSelection {
    pub selected_utxos: Vec<Utxo>,
    pub total_input_value: u64,
    pub estimated_fee: u64,
    pub change_amount: u64,
    pub selection_strategy: String,
}

/// UTXO selection criteria
#[derive(Debug, Clone)]
pub struct SelectionCriteria {
    pub target_amount: u64,
    pub fee_rate: u64, // sat/vB
    pub strategy: SelectionStrategy,
    pub max_utxos: Option<usize>,
    pub exclude_dust: bool,
    pub prefer_confirmed: bool,
}

/// UTXO selection strategies
#[derive(Debug, Clone)]
pub enum SelectionStrategy {
    /// Select oldest UTXOs first (good for consolidation)
    OldestFirst,
    /// Select largest UTXOs first (minimizes transaction size)
    LargestFirst,
    /// Select to minimize fees (branch and bound)
    MinimizeFees,
    /// Random selection (privacy)
    Random,
    /// Consolidation strategy (select many small UTXOs)
    Consolidate,
}

impl UtxoManager {
    /// Create new UTXO manager
    pub fn new(federation_address: BtcAddress, federation_script: ScriptBuf) -> Self {
        Self {
            utxo_set: HashMap::new(),
            spent_utxos: HashSet::new(),
            reserved_utxos: HashMap::new(),
            federation_address,
            federation_script,
            last_refresh: SystemTime::now(),
            total_value: 0,
            stats: UtxoStats::default(),
        }
    }

    /// Get all spendable UTXOs (confirmed, not spent, not reserved)
    pub fn get_spendable_utxos(&self) -> Vec<Utxo> {
        self.utxo_set
            .values()
            .filter(|utxo| {
                utxo.spendable 
                    && !utxo.reserved 
                    && utxo.confirmations >= MIN_CONFIRMATIONS
                    && utxo.output.value >= DUST_LIMIT
                    && !self.spent_utxos.contains(&utxo.outpoint)
            })
            .cloned()
            .collect()
    }

    /// Select UTXOs for a transaction
    pub fn select_utxos(&self, criteria: SelectionCriteria) -> Result<UtxoSelection, UtxoError> {
        let available_utxos = self.get_spendable_utxos();
        
        if available_utxos.is_empty() {
            return Err(UtxoError::InsufficientFunds {
                requested: criteria.target_amount,
                available: 0,
            });
        }

        let selection = match criteria.strategy {
            SelectionStrategy::LargestFirst => self.select_largest_first(&available_utxos, &criteria)?,
            SelectionStrategy::OldestFirst => self.select_oldest_first(&available_utxos, &criteria)?,
            SelectionStrategy::MinimizeFees => self.select_minimize_fees(&available_utxos, &criteria)?,
            SelectionStrategy::Random => self.select_random(&available_utxos, &criteria)?,
            SelectionStrategy::Consolidate => self.select_consolidate(&available_utxos, &criteria)?,
        };

        Ok(selection)
    }

    /// Reserve UTXOs for a specific operation
    pub fn reserve_utxos(&mut self, utxos: Vec<OutPoint>, operation_id: String) -> Result<(), UtxoError> {
        for outpoint in &utxos {
            if let Some(utxo) = self.utxo_set.get_mut(outpoint) {
                if utxo.reserved {
                    return Err(UtxoError::UtxoAlreadyReserved {
                        outpoint: *outpoint,
                        reserved_for: self.reserved_utxos.get(outpoint).cloned(),
                    });
                }
                utxo.reserved = true;
                utxo.reserved_for = Some(operation_id.clone());
                self.reserved_utxos.insert(*outpoint, operation_id.clone());
            } else {
                return Err(UtxoError::UtxoNotFound { outpoint: *outpoint });
            }
        }
        
        info!("Reserved {} UTXOs for operation {}", utxos.len(), operation_id);
        self.update_stats();
        Ok(())
    }

    /// Release reserved UTXOs
    pub fn release_utxos(&mut self, operation_id: &str) -> Result<Vec<OutPoint>, UtxoError> {
        let mut released = Vec::new();
        
        // Find all UTXOs reserved for this operation
        let reserved_outpoints: Vec<OutPoint> = self.reserved_utxos
            .iter()
            .filter(|(_, id)| *id == operation_id)
            .map(|(outpoint, _)| *outpoint)
            .collect();

        for outpoint in reserved_outpoints {
            if let Some(utxo) = self.utxo_set.get_mut(&outpoint) {
                utxo.reserved = false;
                utxo.reserved_for = None;
                self.reserved_utxos.remove(&outpoint);
                released.push(outpoint);
            }
        }

        info!("Released {} UTXOs for operation {}", released.len(), operation_id);
        self.update_stats();
        Ok(released)
    }

    /// Mark UTXOs as spent
    pub fn mark_spent(&mut self, utxos: Vec<OutPoint>, spending_txid: Txid) -> Result<(), UtxoError> {
        for outpoint in &utxos {
            if let Some(utxo) = self.utxo_set.remove(outpoint) {
                self.spent_utxos.insert(*outpoint);
                self.reserved_utxos.remove(outpoint);
                info!("Marked UTXO {} as spent in transaction {}", outpoint, spending_txid);
            } else {
                warn!("Attempted to mark non-existent UTXO {} as spent", outpoint);
            }
        }
        
        self.update_stats();
        Ok(())
    }

    /// Add new UTXO to the set
    pub fn add_utxo(&mut self, outpoint: OutPoint, output: TxOut, confirmations: u32, block_height: u32) {
        let utxo = Utxo {
            outpoint,
            output,
            confirmations,
            block_height,
            spendable: confirmations >= MIN_CONFIRMATIONS && output.value >= DUST_LIMIT,
            reserved: false,
            reserved_for: None,
            created_at: SystemTime::now(),
            last_seen: SystemTime::now(),
        };

        self.utxo_set.insert(outpoint, utxo);
        self.update_stats();
        debug!("Added UTXO {} with value {} sats", outpoint, output.value);
    }

    /// Update UTXO confirmations
    pub fn update_confirmations(&mut self, outpoint: OutPoint, confirmations: u32) {
        if let Some(utxo) = self.utxo_set.get_mut(&outpoint) {
            utxo.confirmations = confirmations;
            utxo.spendable = confirmations >= MIN_CONFIRMATIONS && utxo.output.value >= DUST_LIMIT;
            utxo.last_seen = SystemTime::now();
        }
    }

    /// Get current UTXO statistics
    pub fn get_stats(&self) -> UtxoStats {
        self.stats.clone()
    }

    /// Private helper methods for UTXO selection
    fn select_largest_first(&self, utxos: &[Utxo], criteria: &SelectionCriteria) -> Result<UtxoSelection, UtxoError> {
        let mut sorted_utxos = utxos.to_vec();
        sorted_utxos.sort_by(|a, b| b.output.value.cmp(&a.output.value));
        
        self.select_greedy(&sorted_utxos, criteria, "LargestFirst")
    }

    fn select_oldest_first(&self, utxos: &[Utxo], criteria: &SelectionCriteria) -> Result<UtxoSelection, UtxoError> {
        let mut sorted_utxos = utxos.to_vec();
        sorted_utxos.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        
        self.select_greedy(&sorted_utxos, criteria, "OldestFirst")
    }

    fn select_minimize_fees(&self, utxos: &[Utxo], criteria: &SelectionCriteria) -> Result<UtxoSelection, UtxoError> {
        // Branch and bound algorithm for optimal selection
        // For simplicity, fall back to largest first
        self.select_largest_first(utxos, criteria)
    }

    fn select_random(&self, utxos: &[Utxo], criteria: &SelectionCriteria) -> Result<UtxoSelection, UtxoError> {
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        let mut shuffled_utxos = utxos.to_vec();
        shuffled_utxos.shuffle(&mut rng);
        
        self.select_greedy(&shuffled_utxos, criteria, "Random")
    }

    fn select_consolidate(&self, utxos: &[Utxo], criteria: &SelectionCriteria) -> Result<UtxoSelection, UtxoError> {
        let mut sorted_utxos = utxos.to_vec();
        sorted_utxos.sort_by(|a, b| a.output.value.cmp(&b.output.value));
        
        self.select_greedy(&sorted_utxos, criteria, "Consolidate")
    }

    fn select_greedy(&self, utxos: &[Utxo], criteria: &SelectionCriteria, strategy: &str) -> Result<UtxoSelection, UtxoError> {
        let mut selected = Vec::new();
        let mut total_input = 0u64;
        
        for utxo in utxos {
            if let Some(max_utxos) = criteria.max_utxos {
                if selected.len() >= max_utxos {
                    break;
                }
            }

            selected.push(utxo.clone());
            total_input += utxo.output.value;
            
            // Estimate fee for current selection
            let estimated_fee = self.estimate_fee(selected.len(), criteria.fee_rate);
            
            if total_input >= criteria.target_amount + estimated_fee {
                let change_amount = total_input - criteria.target_amount - estimated_fee;
                
                return Ok(UtxoSelection {
                    selected_utxos: selected,
                    total_input_value: total_input,
                    estimated_fee,
                    change_amount,
                    selection_strategy: strategy.to_string(),
                });
            }
        }

        // Insufficient funds
        let available_total: u64 = utxos.iter().map(|u| u.output.value).sum();
        Err(UtxoError::InsufficientFunds {
            requested: criteria.target_amount,
            available: available_total,
        })
    }

    /// Estimate transaction fee based on inputs and outputs
    fn estimate_fee(&self, num_inputs: usize, fee_rate: u64) -> u64 {
        // Rough estimation: base size + inputs + outputs
        let base_size = 10; // version, locktime, etc.
        let input_size = num_inputs * 148; // P2WPKH input
        let output_size = 2 * 34; // Two outputs (destination + change)
        let total_vbytes = base_size + input_size + output_size;
        
        (total_vbytes as u64) * fee_rate
    }

    /// Update internal statistics
    fn update_stats(&mut self) {
        let total_utxos = self.utxo_set.len();
        let spendable_utxos = self.get_spendable_utxos().len();
        let reserved_utxos = self.reserved_utxos.len();
        
        let total_value = self.utxo_set.values().map(|u| u.output.value).sum();
        let spendable_value = self.get_spendable_utxos().iter().map(|u| u.output.value).sum();
        let reserved_value = self.utxo_set.values()
            .filter(|u| u.reserved)
            .map(|u| u.output.value)
            .sum();

        self.total_value = total_value;
        self.stats = UtxoStats {
            total_utxos,
            spendable_utxos,
            reserved_utxos,
            total_value,
            spendable_value,
            reserved_value,
            last_updated: SystemTime::now(),
        };
    }
}

impl Default for UtxoStats {
    fn default() -> Self {
        Self {
            total_utxos: 0,
            spendable_utxos: 0,
            reserved_utxos: 0,
            total_value: 0,
            spendable_value: 0,
            reserved_value: 0,
            last_updated: SystemTime::now(),
        }
    }
}

/// UTXO management errors
#[derive(Debug, thiserror::Error)]
pub enum UtxoError {
    #[error("Insufficient funds: requested {requested}, available {available}")]
    InsufficientFunds { requested: u64, available: u64 },
    
    #[error("UTXO {outpoint} not found")]
    UtxoNotFound { outpoint: OutPoint },
    
    #[error("UTXO {outpoint} already reserved for operation {reserved_for:?}")]
    UtxoAlreadyReserved { 
        outpoint: OutPoint, 
        reserved_for: Option<String> 
    },
    
    #[error("No spendable UTXOs available")]
    NoSpendableUtxos,
    
    #[error("Internal error: {message}")]
    Internal { message: String },
}