//! Bridge and two-way peg related types

use crate::types::*;
use serde::{Deserialize, Serialize};

/// Peg-in operation status and tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PegInStatus {
    Detected {
        bitcoin_txid: bitcoin::Txid,
        detected_at: std::time::SystemTime,
        confirmations: u32,
    },
    Confirming {
        bitcoin_txid: bitcoin::Txid,
        current_confirmations: u32,
        required_confirmations: u32,
        estimated_completion: Option<std::time::SystemTime>,
    },
    Confirmed {
        bitcoin_txid: bitcoin::Txid,
        alys_recipient: Address,
        amount_satoshis: u64,
        confirmed_at: std::time::SystemTime,
    },
    Processing {
        bitcoin_txid: bitcoin::Txid,
        alys_recipient: Address,
        amount_satoshis: u64,
        processing_started: std::time::SystemTime,
    },
    Completed {
        bitcoin_txid: bitcoin::Txid,
        alys_tx_hash: H256,
        alys_recipient: Address,
        amount_satoshis: u64,
        completed_at: std::time::SystemTime,
    },
    Failed {
        bitcoin_txid: bitcoin::Txid,
        error_reason: String,
        failed_at: std::time::SystemTime,
        retry_count: u32,
    },
}

/// Peg-out operation status and tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PegOutStatus {
    Initiated {
        burn_tx_hash: H256,
        bitcoin_recipient: bitcoin::Address,
        amount_satoshis: u64,
        initiated_at: std::time::SystemTime,
    },
    ValidatingBurn {
        burn_tx_hash: H256,
        bitcoin_recipient: bitcoin::Address,
        amount_satoshis: u64,
        validation_started: std::time::SystemTime,
    },
    CollectingSignatures {
        burn_tx_hash: H256,
        bitcoin_tx_unsigned: bitcoin::Transaction,
        signatures_collected: usize,
        signatures_required: usize,
        collection_started: std::time::SystemTime,
        deadline: std::time::SystemTime,
    },
    SigningComplete {
        burn_tx_hash: H256,
        bitcoin_tx_signed: bitcoin::Transaction,
        signatures: Vec<FederationSignature>,
        completed_at: std::time::SystemTime,
    },
    Broadcasting {
        burn_tx_hash: H256,
        bitcoin_txid: bitcoin::Txid,
        broadcast_attempts: u32,
        last_attempt: std::time::SystemTime,
    },
    Broadcast {
        burn_tx_hash: H256,
        bitcoin_txid: bitcoin::Txid,
        broadcast_at: std::time::SystemTime,
        confirmations: u32,
    },
    Completed {
        burn_tx_hash: H256,
        bitcoin_txid: bitcoin::Txid,
        amount_satoshis: u64,
        completed_at: std::time::SystemTime,
        final_confirmations: u32,
    },
    Failed {
        burn_tx_hash: H256,
        error_reason: String,
        failed_at: std::time::SystemTime,
        recovery_possible: bool,
    },
}

/// Federation member information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMember {
    pub alys_address: Address,
    pub bitcoin_public_key: bitcoin::PublicKey,
    pub signing_weight: u32,
    pub is_active: bool,
    pub joined_at: std::time::SystemTime,
    pub last_activity: std::time::SystemTime,
    pub reputation_score: i32,
    pub successful_signatures: u64,
    pub failed_signatures: u64,
}

/// Federation signature for multi-sig operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationSignature {
    pub signer_address: Address,
    pub signature_data: Vec<u8>,
    pub public_key: bitcoin::PublicKey,
    pub signature_type: FederationSignatureType,
    pub created_at: std::time::SystemTime,
    pub message_hash: Hash256,
}

/// Types of federation signatures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FederationSignatureType {
    ECDSA,
    Schnorr,
    BLS,
    Threshold,
}

/// Federation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    pub members: Vec<FederationMember>,
    pub threshold: usize,
    pub multisig_address: bitcoin::Address,
    pub emergency_addresses: Vec<bitcoin::Address>,
    pub signing_timeout: std::time::Duration,
    pub minimum_confirmations: u32,
    pub maximum_amount: u64,
    pub fee_rate_sat_per_vbyte: u64,
}

/// Bitcoin UTXO information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoInfo {
    pub outpoint: bitcoin::OutPoint,
    pub value_satoshis: u64,
    pub script_pubkey: bitcoin::ScriptBuf,
    pub confirmations: u32,
    pub is_locked: bool,
    pub locked_until: Option<std::time::SystemTime>,
    pub reserved_for: Option<String>, // Operation ID that reserved this UTXO
}

/// Bitcoin transaction fee estimation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeEstimate {
    pub sat_per_vbyte: u64,
    pub total_fee_satoshis: u64,
    pub confidence_level: f64,
    pub estimated_confirmation_blocks: u32,
    pub estimated_confirmation_time: std::time::Duration,
}

/// Bridge operation metrics and statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeMetrics {
    // Peg-in metrics
    pub total_pegins: u64,
    pub successful_pegins: u64,
    pub failed_pegins: u64,
    pub pending_pegins: u64,
    pub total_pegin_value_satoshis: u64,
    pub average_pegin_time: std::time::Duration,
    
    // Peg-out metrics
    pub total_pegouts: u64,
    pub successful_pegouts: u64,
    pub failed_pegouts: u64,
    pub pending_pegouts: u64,
    pub total_pegout_value_satoshis: u64,
    pub average_pegout_time: std::time::Duration,
    
    // Federation metrics
    pub federation_health_score: f64,
    pub active_federation_members: usize,
    pub successful_signatures_24h: u64,
    pub failed_signatures_24h: u64,
    
    // System metrics
    pub bridge_uptime: std::time::Duration,
    pub last_bitcoin_block_seen: u64,
    pub bitcoin_node_sync_status: bool,
}

/// Bridge configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub bitcoin_network: bitcoin::Network,
    pub bitcoin_node_url: String,
    pub bitcoin_node_auth: BitcoinNodeAuth,
    pub federation_config: FederationConfig,
    pub monitoring_addresses: Vec<MonitoredAddress>,
    pub operation_limits: OperationLimits,
    pub security_params: SecurityParams,
}

/// Bitcoin node authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BitcoinNodeAuth {
    None,
    UserPass { username: String, password: String },
    Cookie { cookie_file: String },
}

/// Monitored Bitcoin address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoredAddress {
    pub address: bitcoin::Address,
    pub purpose: AddressPurpose,
    pub derivation_path: Option<String>,
    pub created_at: std::time::SystemTime,
    pub last_activity: Option<std::time::SystemTime>,
}

/// Purpose of monitored addresses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AddressPurpose {
    PegIn,
    Federation,
    Emergency,
    Change,
    Temporary { expires_at: std::time::SystemTime },
}

/// Operation limits and constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationLimits {
    pub min_pegin_amount: u64,
    pub max_pegin_amount: u64,
    pub min_pegout_amount: u64,
    pub max_pegout_amount: u64,
    pub daily_volume_limit: u64,
    pub max_pending_operations: usize,
    pub operation_timeout: std::time::Duration,
}

/// Security parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityParams {
    pub required_confirmations_pegin: u32,
    pub required_confirmations_pegout: u32,
    pub reorg_protection_depth: u32,
    pub signature_timeout: std::time::Duration,
    pub emergency_pause_threshold: f64,
    pub max_federation_offline: usize,
}

/// Bitcoin blockchain reorg handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorgInfo {
    pub old_chain_tip: BlockHash,
    pub new_chain_tip: BlockHash,
    pub reorg_depth: u32,
    pub affected_transactions: Vec<bitcoin::Txid>,
    pub detected_at: std::time::SystemTime,
    pub resolved: bool,
}

/// Bridge health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeHealth {
    Healthy,
    Warning { issues: Vec<String> },
    Critical { critical_issues: Vec<String> },
    Emergency { reason: String, paused_at: std::time::SystemTime },
}

/// Bridge operational state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeState {
    Active,
    Paused { reason: String, paused_at: std::time::SystemTime },
    Emergency { reason: String, triggered_at: std::time::SystemTime },
    Maintenance { 
        reason: String, 
        started_at: std::time::SystemTime,
        estimated_duration: std::time::Duration,
    },
}

impl PegInStatus {
    /// Check if peg-in is in a final state
    pub fn is_final(&self) -> bool {
        matches!(self, PegInStatus::Completed { .. } | PegInStatus::Failed { .. })
    }
    
    /// Get current confirmation count
    pub fn confirmations(&self) -> u32 {
        match self {
            PegInStatus::Detected { confirmations, .. } => *confirmations,
            PegInStatus::Confirming { current_confirmations, .. } => *current_confirmations,
            _ => 0,
        }
    }
    
    /// Get estimated completion time if available
    pub fn estimated_completion(&self) -> Option<std::time::SystemTime> {
        match self {
            PegInStatus::Confirming { estimated_completion, .. } => *estimated_completion,
            _ => None,
        }
    }
    
    /// Get processing duration
    pub fn processing_duration(&self) -> Option<std::time::Duration> {
        match self {
            PegInStatus::Completed { detected_at, completed_at, .. } => {
                Some(completed_at.duration_since(*detected_at).unwrap_or_default())
            }
            _ => None,
        }
    }
}

impl PegOutStatus {
    /// Check if peg-out is in a final state
    pub fn is_final(&self) -> bool {
        matches!(self, PegOutStatus::Completed { .. } | PegOutStatus::Failed { .. })
    }
    
    /// Get signature collection progress
    pub fn signature_progress(&self) -> Option<(usize, usize)> {
        match self {
            PegOutStatus::CollectingSignatures { signatures_collected, signatures_required, .. } => {
                Some((*signatures_collected, *signatures_required))
            }
            _ => None,
        }
    }
    
    /// Check if signature collection deadline has passed
    pub fn is_signature_deadline_passed(&self) -> bool {
        match self {
            PegOutStatus::CollectingSignatures { deadline, .. } => {
                std::time::SystemTime::now() > *deadline
            }
            _ => false,
        }
    }
}

impl FederationMember {
    /// Create new federation member
    pub fn new(
        alys_address: Address,
        bitcoin_public_key: bitcoin::PublicKey,
        signing_weight: u32,
    ) -> Self {
        Self {
            alys_address,
            bitcoin_public_key,
            signing_weight,
            is_active: true,
            joined_at: std::time::SystemTime::now(),
            last_activity: std::time::SystemTime::now(),
            reputation_score: 0,
            successful_signatures: 0,
            failed_signatures: 0,
        }
    }
    
    /// Update member activity
    pub fn update_activity(&mut self) {
        self.last_activity = std::time::SystemTime::now();
    }
    
    /// Record successful signature
    pub fn record_successful_signature(&mut self) {
        self.successful_signatures += 1;
        self.reputation_score += 1;
        self.update_activity();
    }
    
    /// Record failed signature
    pub fn record_failed_signature(&mut self) {
        self.failed_signatures += 1;
        self.reputation_score -= 2;
        self.update_activity();
    }
    
    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.successful_signatures + self.failed_signatures;
        if total == 0 {
            1.0
        } else {
            self.successful_signatures as f64 / total as f64
        }
    }
    
    /// Check if member is considered reliable
    pub fn is_reliable(&self) -> bool {
        self.reputation_score > -10 && self.success_rate() > 0.8
    }
    
    /// Check if member has been active recently
    pub fn is_recently_active(&self, threshold: std::time::Duration) -> bool {
        std::time::SystemTime::now()
            .duration_since(self.last_activity)
            .unwrap_or_default() < threshold
    }
}

impl FederationConfig {
    /// Check if threshold is met with active members
    pub fn has_sufficient_active_members(&self) -> bool {
        let active_count = self.members.iter().filter(|m| m.is_active).count();
        active_count >= self.threshold
    }
    
    /// Get active members
    pub fn active_members(&self) -> Vec<&FederationMember> {
        self.members.iter().filter(|m| m.is_active).collect()
    }
    
    /// Get total voting weight of active members
    pub fn total_active_weight(&self) -> u32 {
        self.active_members()
            .iter()
            .map(|m| m.signing_weight)
            .sum()
    }
    
    /// Check if enough signatures are collected
    pub fn is_threshold_met(&self, signatures: &[FederationSignature]) -> bool {
        let collected_weight: u32 = signatures
            .iter()
            .filter_map(|sig| {
                self.members
                    .iter()
                    .find(|m| m.alys_address == sig.signer_address)
                    .map(|m| m.signing_weight)
            })
            .sum();
            
        let required_weight: u32 = self.total_active_weight() * self.threshold as u32 / self.members.len() as u32;
        collected_weight >= required_weight
    }
}

impl BridgeMetrics {
    /// Create new bridge metrics
    pub fn new() -> Self {
        Self {
            total_pegins: 0,
            successful_pegins: 0,
            failed_pegins: 0,
            pending_pegins: 0,
            total_pegin_value_satoshis: 0,
            average_pegin_time: std::time::Duration::from_secs(0),
            total_pegouts: 0,
            successful_pegouts: 0,
            failed_pegouts: 0,
            pending_pegouts: 0,
            total_pegout_value_satoshis: 0,
            average_pegout_time: std::time::Duration::from_secs(0),
            federation_health_score: 1.0,
            active_federation_members: 0,
            successful_signatures_24h: 0,
            failed_signatures_24h: 0,
            bridge_uptime: std::time::Duration::from_secs(0),
            last_bitcoin_block_seen: 0,
            bitcoin_node_sync_status: false,
        }
    }
    
    /// Get peg-in success rate
    pub fn pegin_success_rate(&self) -> f64 {
        if self.total_pegins == 0 {
            0.0
        } else {
            self.successful_pegins as f64 / self.total_pegins as f64
        }
    }
    
    /// Get peg-out success rate
    pub fn pegout_success_rate(&self) -> f64 {
        if self.total_pegouts == 0 {
            0.0
        } else {
            self.successful_pegouts as f64 / self.total_pegouts as f64
        }
    }
    
    /// Get federation signature success rate
    pub fn federation_signature_success_rate(&self) -> f64 {
        let total_signatures = self.successful_signatures_24h + self.failed_signatures_24h;
        if total_signatures == 0 {
            1.0
        } else {
            self.successful_signatures_24h as f64 / total_signatures as f64
        }
    }
    
    /// Check if bridge is performing well
    pub fn is_healthy(&self) -> bool {
        self.pegin_success_rate() > 0.95
            && self.pegout_success_rate() > 0.95
            && self.federation_health_score > 0.8
            && self.bitcoin_node_sync_status
    }
}

impl Default for BridgeMetrics {
    fn default() -> Self {
        Self::new()
    }
}