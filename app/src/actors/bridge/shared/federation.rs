//! Federation Management Utilities
//! 
//! Utilities for managing federation configuration and operations

use bitcoin::{Address as BtcAddress, PublicKey, ScriptBuf, Network};
use serde::{Deserialize, Serialize, Deserializer, Serializer};
use std::collections::HashMap;
use std::time::SystemTime;
use std::str::FromStr;
use crate::types::*;

/// Custom serde module for Bitcoin addresses
mod bitcoin_address_serde {
    use super::*;

    pub fn serialize<S>(address: &BtcAddress, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&address.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BtcAddress, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        BtcAddress::from_str(&s)
            .map(|addr| addr.assume_checked())
            .map_err(serde::de::Error::custom)
    }
}

/// Custom serde module for optional Bitcoin addresses
mod optional_bitcoin_address_serde {
    use super::*;

    pub fn serialize<S>(address: &Option<BtcAddress>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match address {
            Some(addr) => serializer.serialize_some(&addr.to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<BtcAddress>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt_s: Option<String> = Option::deserialize(deserializer)?;
        match opt_s {
            Some(s) => {
                let addr = BtcAddress::from_str(&s)
                    .map(|addr| addr.assume_checked())
                    .map_err(serde::de::Error::custom)?;
                Ok(Some(addr))
            }
            None => Ok(None),
        }
    }
}

/// Federation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    /// Federation members with their public keys
    pub members: Vec<FederationMember>,
    
    /// Threshold for signatures (minimum required)
    pub threshold: usize,
    
    /// Federation addresses for different script types
    pub addresses: FederationAddresses,
    
    /// Current federation version
    pub version: u32,
    
    /// Bitcoin network
    pub network: Network,
    
    /// Configuration effective from block height
    pub effective_height: u64,
    
    /// Configuration creation time
    pub created_at: SystemTime,
    
    /// Taproot configuration
    pub taproot_config: Option<TaprootConfig>,
}

/// Federation member information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMember {
    /// Unique member identifier
    pub id: String,
    
    /// Member's public key for signing
    pub public_key: PublicKey,
    
    /// Member's BLS public key (if using BLS signatures)
    pub bls_public_key: Option<Vec<u8>>,
    
    /// Member status
    pub status: MemberStatus,
    
    /// Member added at height
    pub added_height: u64,
    
    /// Member metadata
    pub metadata: MemberMetadata,
}

/// Member status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemberStatus {
    Active,
    Inactive,
    Pending,
    Removed,
}

/// Member metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberMetadata {
    pub name: Option<String>,
    pub contact: Option<String>,
    pub endpoint: Option<String>,
    pub last_seen: Option<SystemTime>,
    pub signature_count: u64,
    pub reliability_score: f64,
}

/// Federation addresses for different script types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationAddresses {
    /// Legacy P2SH multisig address
    #[serde(with = "optional_bitcoin_address_serde")]
    pub p2sh: Option<BtcAddress>,

    /// P2SH-wrapped P2WSH multisig address
    #[serde(with = "optional_bitcoin_address_serde")]
    pub p2sh_p2wsh: Option<BtcAddress>,

    /// Native P2WSH multisig address
    #[serde(with = "optional_bitcoin_address_serde")]
    pub p2wsh: Option<BtcAddress>,

    /// Taproot address (main federation address)
    #[serde(with = "bitcoin_address_serde")]
    pub taproot: BtcAddress,

    /// Emergency recovery address
    #[serde(with = "optional_bitcoin_address_serde")]
    pub recovery: Option<BtcAddress>,
}

/// Taproot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaprootConfig {
    /// Taproot script tree
    pub script_tree: Vec<ScriptBuf>,
    
    /// Internal key (for key-path spending)
    pub internal_key: PublicKey,
    
    /// Merkle root of script tree
    pub merkle_root: Option<[u8; 32]>,
    
    /// Script spend paths
    pub spend_paths: Vec<SpendPath>,
}

/// Script spend path in taproot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendPath {
    /// Path identifier
    pub path_id: String,
    
    /// Script for this path
    pub script: ScriptBuf,
    
    /// Required signatures for this path
    pub required_sigs: usize,
    
    /// Leaf version
    pub leaf_version: u8,
}

/// Federation manager for handling configuration and operations
#[derive(Debug)]
pub struct FederationManager {
    /// Current federation configuration
    current_config: FederationConfig,
    
    /// Historical configurations
    config_history: Vec<FederationConfig>,
    
    /// Pending configuration updates
    pending_updates: Vec<FederationUpdate>,
    
    /// Member performance tracking
    member_performance: HashMap<String, MemberPerformance>,
}

/// Member performance tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberPerformance {
    pub member_id: String,
    pub total_requests: u64,
    pub successful_signatures: u64,
    pub failed_signatures: u64,
    pub average_response_time: f64,
    pub reliability_score: f64,
    pub last_updated: SystemTime,
}

/// Federation configuration update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationUpdate {
    pub update_id: String,
    pub update_type: FederationUpdateType,
    pub new_config: FederationConfig,
    pub signatures: Vec<UpdateSignature>,
    pub effective_height: u64,
    pub created_at: SystemTime,
    pub status: UpdateStatus,
}

/// Types of federation updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FederationUpdateType {
    MemberAddition { member: FederationMember },
    MemberRemoval { member_id: String },
    ThresholdChange { new_threshold: usize },
    KeyRotation { new_keys: Vec<PublicKey> },
    AddressUpdate { new_addresses: FederationAddresses },
}

/// Update signature from federation member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSignature {
    pub member_id: String,
    pub signature: Vec<u8>,
    pub signed_at: SystemTime,
}

/// Update status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UpdateStatus {
    Proposed,
    InProgress,
    Approved,
    Rejected,
    Applied,
}

impl FederationManager {
    /// Create new federation manager
    pub fn new(initial_config: FederationConfig) -> Self {
        Self {
            current_config: initial_config,
            config_history: Vec::new(),
            pending_updates: Vec::new(),
            member_performance: HashMap::new(),
        }
    }

    /// Get current federation configuration
    pub fn get_current_config(&self) -> &FederationConfig {
        &self.current_config
    }

    /// Get active federation members
    pub fn get_active_members(&self) -> Vec<&FederationMember> {
        self.current_config.members
            .iter()
            .filter(|m| matches!(m.status, MemberStatus::Active))
            .collect()
    }

    /// Check if threshold is met for signatures
    pub fn is_threshold_met(&self, signature_count: usize) -> bool {
        signature_count >= self.current_config.threshold
    }

    /// Get federation address for specified script type
    pub fn get_federation_address(&self, script_type: FederationScriptType) -> Option<&BtcAddress> {
        match script_type {
            FederationScriptType::P2SH => self.current_config.addresses.p2sh.as_ref(),
            FederationScriptType::P2ShP2Wsh => self.current_config.addresses.p2sh_p2wsh.as_ref(),
            FederationScriptType::P2WSH => self.current_config.addresses.p2wsh.as_ref(),
            FederationScriptType::Taproot => Some(&self.current_config.addresses.taproot),
            FederationScriptType::Recovery => self.current_config.addresses.recovery.as_ref(),
        }
    }

    /// Propose federation update
    pub fn propose_update(&mut self, update: FederationUpdate) -> Result<(), FederationError> {
        // Validate update
        self.validate_update(&update)?;
        
        // Add to pending updates
        self.pending_updates.push(update);
        
        Ok(())
    }

    /// Apply approved federation update
    pub fn apply_update(&mut self, update_id: &str) -> Result<(), FederationError> {
        // Find and remove update from pending
        let update_index = self.pending_updates
            .iter()
            .position(|u| u.update_id == update_id)
            .ok_or_else(|| FederationError::UpdateNotFound(update_id.to_string()))?;
        
        let update = self.pending_updates.remove(update_index);
        
        // Verify update is approved
        if !matches!(update.status, UpdateStatus::Approved) {
            return Err(FederationError::UpdateNotApproved(update_id.to_string()));
        }
        
        // Store current config in history
        self.config_history.push(self.current_config.clone());
        
        // Apply new configuration
        self.current_config = update.new_config;
        
        Ok(())
    }

    /// Update member performance metrics
    pub fn update_member_performance(
        &mut self,
        member_id: &str,
        successful: bool,
        response_time: f64,
    ) {
        let performance = self.member_performance
            .entry(member_id.to_string())
            .or_insert_with(|| MemberPerformance {
                member_id: member_id.to_string(),
                total_requests: 0,
                successful_signatures: 0,
                failed_signatures: 0,
                average_response_time: 0.0,
                reliability_score: 1.0,
                last_updated: SystemTime::now(),
            });

        performance.total_requests += 1;
        
        if successful {
            performance.successful_signatures += 1;
        } else {
            performance.failed_signatures += 1;
        }

        // Update average response time
        let total_time = performance.average_response_time * (performance.total_requests - 1) as f64;
        performance.average_response_time = (total_time + response_time) / performance.total_requests as f64;

        // Update reliability score
        performance.reliability_score = performance.successful_signatures as f64 / performance.total_requests as f64;
        performance.last_updated = SystemTime::now();
    }

    /// Get member performance
    pub fn get_member_performance(&self, member_id: &str) -> Option<&MemberPerformance> {
        self.member_performance.get(member_id)
    }

    /// Validate federation update
    fn validate_update(&self, update: &FederationUpdate) -> Result<(), FederationError> {
        // Check signature count meets threshold
        let signature_count = update.signatures.len();
        if signature_count < self.current_config.threshold {
            return Err(FederationError::InsufficientSignatures {
                required: self.current_config.threshold,
                provided: signature_count,
            });
        }

        // Validate update type specific logic
        match &update.update_type {
            FederationUpdateType::ThresholdChange { new_threshold } => {
                let member_count = update.new_config.members.len();
                if *new_threshold > member_count {
                    return Err(FederationError::InvalidThreshold {
                        threshold: *new_threshold,
                        member_count,
                    });
                }
            }
            FederationUpdateType::MemberAddition { member: _ } => {
                // Validate new member doesn't already exist
                // Additional validation logic
            }
            FederationUpdateType::MemberRemoval { member_id: _ } => {
                // Ensure we don't go below minimum threshold
                let remaining_members = update.new_config.members.len();
                if remaining_members < update.new_config.threshold {
                    return Err(FederationError::InvalidThreshold {
                        threshold: update.new_config.threshold,
                        member_count: remaining_members,
                    });
                }
            }
            _ => {}
        }

        Ok(())
    }
}

/// Federation script types
#[derive(Debug, Clone)]
pub enum FederationScriptType {
    P2SH,
    P2ShP2Wsh,
    P2WSH,
    Taproot,
    Recovery,
}

/// Federation management errors
#[derive(Debug, thiserror::Error)]
pub enum FederationError {
    #[error("Update not found: {0}")]
    UpdateNotFound(String),
    
    #[error("Update not approved: {0}")]
    UpdateNotApproved(String),
    
    #[error("Insufficient signatures: required {required}, provided {provided}")]
    InsufficientSignatures { required: usize, provided: usize },
    
    #[error("Invalid threshold: {threshold} exceeds member count {member_count}")]
    InvalidThreshold { threshold: usize, member_count: usize },
    
    #[error("Member not found: {member_id}")]
    MemberNotFound { member_id: String },
    
    #[error("Invalid signature: {reason}")]
    InvalidSignature { reason: String },
    
    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            members: Vec::new(),
            threshold: 2,
            addresses: FederationAddresses {
                p2sh: None,
                p2sh_p2wsh: None,
                p2wsh: None,
                taproot: BtcAddress::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4").unwrap().assume_checked(),
                recovery: None,
            },
            version: 1,
            network: Network::Bitcoin,
            effective_height: 0,
            created_at: SystemTime::now(),
            taproot_config: None,
        }
    }
}