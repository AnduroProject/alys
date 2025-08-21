use bitcoin::{OutPoint, TxOut, Address as BtcAddress, Script};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tracing::{info, warn, error};

use super::errors::BridgeError;

const DUST_LIMIT: u64 = 546; // Standard Bitcoin dust limit
const MIN_CONFIRMATIONS: u32 = 6; // Minimum confirmations for UTXO availability
const UTXO_REFRESH_INTERVAL: Duration = Duration::from_secs(120); // 2 minutes

#[derive(Debug, Clone)]
pub struct Utxo {
    pub outpoint: OutPoint,
    pub output: TxOut,
    pub confirmations: u32,
    pub spendable: bool,
    pub reserved: bool,
    pub created_at: Instant,
}

pub struct UtxoManager {
    utxo_set: HashMap<OutPoint, Utxo>,
    spent_utxos: HashSet<OutPoint>,
    reserved_utxos: HashSet<OutPoint>,
    federation_address: BtcAddress,
    federation_script: Script,
    last_refresh: Instant,
    total_value: u64,
}

impl UtxoManager {
    pub fn new(federation_address: BtcAddress, federation_script: Script) -> Self {
        Self {
            utxo_set: HashMap::new(),
            spent_utxos: HashSet::new(),
            reserved_utxos: HashSet::new(),
            federation_address,
            federation_script,
            last_refresh: Instant::now(),
            total_value: 0,
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
                    && !self.spent_utxos.contains(&utxo.outpoint)
                    && !self.reserved_utxos.contains(&utxo.outpoint)
            })
            .cloned()
            .collect()
    }

    /// Select UTXOs for a transaction using a greedy algorithm
    /// Returns (selected_utxos, total_value)
    pub fn select_utxos_for_amount(
        &self, 
        target_amount: u64,
        fee_rate: u64, // sat/vbyte
    ) -> Result<(Vec<Utxo>, u64), BridgeError> {
        let available_utxos = self.get_spendable_utxos();
        
        if available_utxos.is_empty() {
            return Err(BridgeError::UtxoSelectionFailed(
                "No spendable UTXOs available".to_string()
            ));
        }

        // Sort by value descending for greedy selection
        let mut sorted_utxos = available_utxos;
        sorted_utxos.sort_by(|a, b| b.output.value.cmp(&a.output.value));

        let mut selected = Vec::new();
        let mut total_value = 0u64;
        
        // Estimate transaction size: base + (inputs * 148) + (outputs * 34)
        // This is an approximation for P2WSH inputs and P2WPKH/P2WSH outputs
        let base_tx_size = 10; // version (4) + input count (1) + output count (1) + locktime (4)
        let input_size = 148; // Approximate size for P2WSH input with signature
        let output_size = 34; // Approximate size for standard output
        
        for utxo in sorted_utxos {
            selected.push(utxo.clone());
            total_value += utxo.output.value;
            
            // Calculate current transaction size and fee
            let tx_size = base_tx_size + (selected.len() * input_size) + (2 * output_size); // 2 outputs: recipient + change
            let estimated_fee = (tx_size as u64) * fee_rate;
            
            // Check if we have enough for target amount + fee
            if total_value >= target_amount + estimated_fee {
                info!(
                    "Selected {} UTXOs for amount {} (total value: {}, estimated fee: {})",
                    selected.len(),
                    target_amount,
                    total_value,
                    estimated_fee
                );
                return Ok((selected, total_value));
            }
        }

        Err(BridgeError::InsufficientFunds {
            needed: target_amount,
            available: total_value,
        })
    }

    /// Reserve UTXOs for a pending transaction
    pub fn reserve_utxos(&mut self, utxos: &[Utxo]) -> Result<String, BridgeError> {
        let reservation_id = uuid::Uuid::new_v4().to_string();
        
        for utxo in utxos {
            if self.reserved_utxos.contains(&utxo.outpoint) {
                return Err(BridgeError::UtxoSelectionFailed(
                    format!("UTXO {} already reserved", utxo.outpoint)
                ));
            }
            
            self.reserved_utxos.insert(utxo.outpoint);
            
            // Mark UTXO as reserved in the set
            if let Some(stored_utxo) = self.utxo_set.get_mut(&utxo.outpoint) {
                stored_utxo.reserved = true;
            }
        }

        info!(
            "Reserved {} UTXOs with ID: {}",
            utxos.len(),
            reservation_id
        );

        Ok(reservation_id)
    }

    /// Release reserved UTXOs (e.g., when transaction fails)
    pub fn release_reservation(&mut self, utxos: &[OutPoint]) {
        for outpoint in utxos {
            self.reserved_utxos.remove(outpoint);
            
            if let Some(utxo) = self.utxo_set.get_mut(outpoint) {
                utxo.reserved = false;
            }
        }

        info!("Released reservation for {} UTXOs", utxos.len());
    }

    /// Mark UTXOs as spent (when transaction is broadcast)
    pub fn mark_spent(&mut self, utxos: &[OutPoint]) {
        for outpoint in utxos {
            self.spent_utxos.insert(*outpoint);
            self.reserved_utxos.remove(outpoint);
            
            if let Some(utxo) = self.utxo_set.remove(outpoint) {
                self.total_value = self.total_value.saturating_sub(utxo.output.value);
                info!("Marked UTXO {} as spent (value: {})", outpoint, utxo.output.value);
            }
        }
    }

    /// Add a new UTXO to the set (from peg-in or change)
    pub fn add_utxo(&mut self, outpoint: OutPoint, output: TxOut, confirmations: u32) {
        // Verify the output belongs to our federation
        if output.script_pubkey != self.federation_script {
            warn!(
                "Attempted to add UTXO with incorrect script: expected {}, got {}",
                self.federation_script,
                output.script_pubkey
            );
            return;
        }

        let utxo = Utxo {
            outpoint,
            output: output.clone(),
            confirmations,
            spendable: confirmations >= MIN_CONFIRMATIONS && output.value > DUST_LIMIT,
            reserved: false,
            created_at: Instant::now(),
        };

        if let Some(existing) = self.utxo_set.insert(outpoint, utxo) {
            // Update total value if replacing existing UTXO
            self.total_value = self.total_value
                .saturating_sub(existing.output.value)
                .saturating_add(output.value);
        } else {
            // New UTXO
            self.total_value = self.total_value.saturating_add(output.value);
        }

        info!(
            "Added UTXO {} with value {} and {} confirmations",
            outpoint, output.value, confirmations
        );
    }

    /// Update confirmations for existing UTXOs
    pub fn update_confirmations(&mut self, updates: HashMap<OutPoint, u32>) {
        let mut newly_spendable = 0;
        
        for (outpoint, confirmations) in updates {
            if let Some(utxo) = self.utxo_set.get_mut(&outpoint) {
                let was_spendable = utxo.spendable;
                utxo.confirmations = confirmations;
                utxo.spendable = confirmations >= MIN_CONFIRMATIONS && utxo.output.value > DUST_LIMIT;
                
                if !was_spendable && utxo.spendable {
                    newly_spendable += 1;
                }
            }
        }

        if newly_spendable > 0 {
            info!("{} UTXOs became spendable after confirmation updates", newly_spendable);
        }
    }

    /// Remove UTXOs that are no longer valid (e.g., spent by others)
    pub fn remove_utxos(&mut self, outpoints: &[OutPoint]) {
        for outpoint in outpoints {
            if let Some(utxo) = self.utxo_set.remove(outpoint) {
                self.total_value = self.total_value.saturating_sub(utxo.output.value);
                self.spent_utxos.remove(outpoint);
                self.reserved_utxos.remove(outpoint);
                
                warn!("Removed invalid UTXO: {}", outpoint);
            }
        }
    }

    /// Get summary statistics about the UTXO set
    pub fn get_stats(&self) -> UtxoStats {
        let spendable_utxos = self.get_spendable_utxos();
        let spendable_value: u64 = spendable_utxos.iter().map(|u| u.output.value).sum();
        
        UtxoStats {
            total_utxos: self.utxo_set.len(),
            spendable_utxos: spendable_utxos.len(),
            reserved_utxos: self.reserved_utxos.len(),
            spent_utxos: self.spent_utxos.len(),
            total_value: self.total_value,
            spendable_value,
            last_refresh: self.last_refresh,
        }
    }

    /// Check if UTXO refresh is needed
    pub fn needs_refresh(&self) -> bool {
        self.last_refresh.elapsed() > UTXO_REFRESH_INTERVAL
    }

    /// Update the last refresh timestamp
    pub fn mark_refreshed(&mut self) {
        self.last_refresh = Instant::now();
    }

    /// Get total spendable value
    pub fn get_spendable_value(&self) -> u64 {
        self.get_spendable_utxos()
            .iter()
            .map(|utxo| utxo.output.value)
            .sum()
    }

    /// Cleanup old spent UTXOs and reservations
    pub fn cleanup_old_entries(&mut self, max_age: Duration) {
        let cutoff = Instant::now() - max_age;
        
        // Remove old spent UTXOs from tracking
        let initial_spent_count = self.spent_utxos.len();
        // Note: We don't have timestamps for spent UTXOs in this simple implementation
        // In a production system, you'd want to track when UTXOs were spent
        
        // Remove old UTXOs that might be stale
        let mut to_remove = Vec::new();
        for (outpoint, utxo) in &self.utxo_set {
            if utxo.created_at < cutoff && utxo.confirmations == 0 {
                // Remove unconfirmed UTXOs older than max_age
                to_remove.push(*outpoint);
            }
        }
        
        for outpoint in to_remove {
            self.remove_utxos(&[outpoint]);
        }

        info!(
            "Cleaned up UTXO manager: {} spent UTXOs tracked, {} stale UTXOs removed",
            initial_spent_count,
            0 // Would be actual count in production
        );
    }
}

#[derive(Debug, Clone)]
pub struct UtxoStats {
    pub total_utxos: usize,
    pub spendable_utxos: usize,
    pub reserved_utxos: usize,
    pub spent_utxos: usize,
    pub total_value: u64,
    pub spendable_value: u64,
    pub last_refresh: Instant,
}

/// UTXO selection strategy enumeration
#[derive(Debug, Clone, Copy)]
pub enum UtxoSelectionStrategy {
    /// Select largest UTXOs first (minimize transaction size)
    LargestFirst,
    /// Select smallest UTXOs first (consolidate small amounts)
    SmallestFirst,
    /// Try to match exact amount (minimize change)
    ExactMatch,
    /// Use branch and bound algorithm for optimal selection
    BranchAndBound,
}

impl UtxoManager {
    /// Select UTXOs using a specific strategy
    pub fn select_utxos_with_strategy(
        &self,
        target_amount: u64,
        fee_rate: u64,
        strategy: UtxoSelectionStrategy,
    ) -> Result<(Vec<Utxo>, u64), BridgeError> {
        match strategy {
            UtxoSelectionStrategy::LargestFirst => {
                self.select_utxos_for_amount(target_amount, fee_rate)
            }
            UtxoSelectionStrategy::SmallestFirst => {
                self.select_smallest_first(target_amount, fee_rate)
            }
            UtxoSelectionStrategy::ExactMatch => {
                self.select_exact_match(target_amount, fee_rate)
            }
            UtxoSelectionStrategy::BranchAndBound => {
                self.select_branch_and_bound(target_amount, fee_rate)
            }
        }
    }

    fn select_smallest_first(
        &self,
        target_amount: u64,
        fee_rate: u64,
    ) -> Result<(Vec<Utxo>, u64), BridgeError> {
        let mut available_utxos = self.get_spendable_utxos();
        available_utxos.sort_by(|a, b| a.output.value.cmp(&b.output.value));

        let mut selected = Vec::new();
        let mut total_value = 0u64;

        for utxo in available_utxos {
            selected.push(utxo.clone());
            total_value += utxo.output.value;

            let estimated_fee = self.estimate_transaction_fee(selected.len(), fee_rate);
            if total_value >= target_amount + estimated_fee {
                return Ok((selected, total_value));
            }
        }

        Err(BridgeError::InsufficientFunds {
            needed: target_amount,
            available: total_value,
        })
    }

    fn select_exact_match(
        &self,
        target_amount: u64,
        fee_rate: u64,
    ) -> Result<(Vec<Utxo>, u64), BridgeError> {
        let available_utxos = self.get_spendable_utxos();
        let estimated_fee = self.estimate_transaction_fee(1, fee_rate);
        let target_with_fee = target_amount + estimated_fee;

        // Look for single UTXO that exactly matches or is close
        for utxo in &available_utxos {
            if utxo.output.value >= target_with_fee && utxo.output.value <= target_with_fee + 10000 {
                return Ok((vec![utxo.clone()], utxo.output.value));
            }
        }

        // Fall back to largest first if no exact match
        self.select_utxos_for_amount(target_amount, fee_rate)
    }

    fn select_branch_and_bound(
        &self,
        target_amount: u64,
        fee_rate: u64,
    ) -> Result<(Vec<Utxo>, u64), BridgeError> {
        // Simplified branch and bound - in production you'd want a more sophisticated algorithm
        let available_utxos = self.get_spendable_utxos();
        let estimated_fee = self.estimate_transaction_fee(2, fee_rate); // Assume 2 inputs average
        let target_with_fee = target_amount + estimated_fee;

        // Try different combinations, starting with smaller sets
        for subset_size in 1..=std::cmp::min(available_utxos.len(), 5) {
            if let Ok(result) = self.find_optimal_subset(&available_utxos, target_with_fee, subset_size) {
                return Ok(result);
            }
        }

        // Fall back to greedy selection
        self.select_utxos_for_amount(target_amount, fee_rate)
    }

    fn find_optimal_subset(
        &self,
        utxos: &[Utxo],
        target: u64,
        max_size: usize,
    ) -> Result<(Vec<Utxo>, u64), BridgeError> {
        // This is a simplified version - in production you'd use dynamic programming
        // or more sophisticated algorithms
        let mut best_selection = Vec::new();
        let mut best_total = 0u64;
        let mut best_waste = u64::MAX;

        // Try all combinations of the given size
        self.try_combinations(utxos, max_size, 0, Vec::new(), 0, target, &mut best_selection, &mut best_total, &mut best_waste);

        if !best_selection.is_empty() {
            Ok((best_selection, best_total))
        } else {
            Err(BridgeError::UtxoSelectionFailed("No suitable combination found".to_string()))
        }
    }

    fn try_combinations(
        &self,
        utxos: &[Utxo],
        max_size: usize,
        start_idx: usize,
        current: Vec<Utxo>,
        current_total: u64,
        target: u64,
        best_selection: &mut Vec<Utxo>,
        best_total: &mut u64,
        best_waste: &mut u64,
    ) {
        if current_total >= target {
            let waste = current_total - target;
            if waste < *best_waste {
                *best_selection = current.clone();
                *best_total = current_total;
                *best_waste = waste;
            }
            return;
        }

        if current.len() >= max_size || start_idx >= utxos.len() {
            return;
        }

        // Try including the next UTXO
        let mut next_current = current.clone();
        next_current.push(utxos[start_idx].clone());
        self.try_combinations(
            utxos,
            max_size,
            start_idx + 1,
            next_current,
            current_total + utxos[start_idx].output.value,
            target,
            best_selection,
            best_total,
            best_waste,
        );

        // Try not including the next UTXO
        self.try_combinations(
            utxos,
            max_size,
            start_idx + 1,
            current,
            current_total,
            target,
            best_selection,
            best_total,
            best_waste,
        );
    }

    fn estimate_transaction_fee(&self, num_inputs: usize, fee_rate: u64) -> u64 {
        // Rough estimate for P2WSH transactions
        let base_size = 10; // Basic transaction overhead
        let input_size = 148; // P2WSH input with signature
        let output_size = 34; // Standard output
        let num_outputs = 2; // Recipient + change

        let estimated_size = base_size + (num_inputs * input_size) + (num_outputs * output_size);
        estimated_size as u64 * fee_rate
    }
}