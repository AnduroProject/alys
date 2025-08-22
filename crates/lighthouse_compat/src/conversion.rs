//! Type conversion utilities for Lighthouse v4 ↔ v5 compatibility
//!
//! This module provides comprehensive bidirectional type conversion between Lighthouse v4
//! and v5 types, enabling seamless migration and parallel operation of both versions.

use crate::{
    error::{CompatError, CompatResult},
    types::*,
};
use ethereum_types::{Address, H256, U256};
use std::collections::HashMap;

/// Convert types from v4 to v5
pub mod v4_to_v5 {
    use super::*;
    
    /// Convert v4 execution payload to unified format
    pub fn convert_execution_payload_capella(
        payload: lighthouse_wrapper::types::ExecutionPayloadCapella<lighthouse_wrapper::types::MainnetEthSpec>,
    ) -> CompatResult<ExecutionPayload> {
        Ok(ExecutionPayload {
            parent_hash: payload.parent_hash,
            fee_recipient: payload.fee_recipient,
            state_root: payload.state_root,
            receipts_root: payload.receipts_root,
            logs_bloom: payload.logs_bloom.into_iter().collect(),
            prev_randao: payload.prev_randao,
            block_number: payload.block_number,
            gas_limit: payload.gas_limit,
            gas_used: payload.gas_used,
            timestamp: payload.timestamp,
            extra_data: payload.extra_data.into_iter().collect(),
            base_fee_per_gas: payload.base_fee_per_gas,
            block_hash: payload.block_hash,
            transactions: payload.transactions.iter()
                .map(|tx| tx.clone().into_iter().collect())
                .collect(),
            withdrawals: payload.withdrawals.map(|w| 
                w.iter().map(|withdrawal| convert_withdrawal(withdrawal)).collect()
            ),
            // v5-specific fields default to None for v4 payloads
            blob_gas_used: None,
            excess_blob_gas: None,
            parent_beacon_block_root: None,
        })
    }
    
    /// Convert v4 withdrawal to unified format
    pub fn convert_withdrawal(
        withdrawal: &lighthouse_wrapper::types::Withdrawal,
    ) -> Withdrawal {
        Withdrawal {
            index: withdrawal.index,
            validator_index: withdrawal.validator_index,
            address: withdrawal.address,
            amount: withdrawal.amount,
        }
    }
    
    /// Convert v4 forkchoice state to unified format
    pub fn convert_forkchoice_state(
        state: lighthouse_wrapper::execution_layer::ForkchoiceState,
    ) -> ForkchoiceState {
        ForkchoiceState {
            head_block_hash: state.head_block_hash,
            safe_block_hash: state.safe_block_hash,
            finalized_block_hash: state.finalized_block_hash,
            // v5-specific field defaults to None for v4
            justified_block_hash: None,
        }
    }
    
    /// Convert v4 payload attributes to unified format  
    pub fn convert_payload_attributes(
        attrs: lighthouse_wrapper::execution_layer::PayloadAttributes,
    ) -> CompatResult<PayloadAttributes> {
        Ok(PayloadAttributes {
            timestamp: attrs.timestamp(),
            prev_randao: attrs.prev_randao(),
            suggested_fee_recipient: attrs.suggested_fee_recipient(),
            withdrawals: attrs.withdrawals().map(|w| 
                w.iter().map(|withdrawal| convert_withdrawal(withdrawal)).collect()
            ),
            // v5-specific field defaults to None for v4
            parent_beacon_block_root: None,
        })
    }
    
    /// Enhance payload with v5 features (for testing v5 compatibility)
    pub fn enhance_payload_for_v5(
        mut payload: ExecutionPayload,
        enable_deneb: bool,
    ) -> CompatResult<ExecutionPayload> {
        if enable_deneb {
            // Add default Deneb fields for testing
            payload.blob_gas_used = Some(0);
            payload.excess_blob_gas = Some(0);
            payload.parent_beacon_block_root = Some(H256::zero());
        }
        
        Ok(payload)
    }
    
    /// Enhance forkchoice state with v5 features
    pub fn enhance_forkchoice_for_v5(
        mut state: ForkchoiceState,
        justified_hash: Option<H256>,
    ) -> ForkchoiceState {
        state.justified_block_hash = justified_hash.or(Some(state.finalized_block_hash));
        state
    }
    
    /// Enhance payload attributes with v5 features
    pub fn enhance_attributes_for_v5(
        mut attrs: PayloadAttributes,
        parent_beacon_block_root: Option<H256>,
    ) -> PayloadAttributes {
        attrs.parent_beacon_block_root = parent_beacon_block_root;
        attrs
    }
}

/// Convert types from v5 to v4 (for rollback scenarios)
pub mod v5_to_v4 {
    use super::*;
    
    /// Convert unified execution payload to v4 format
    pub fn convert_execution_payload(
        payload: ExecutionPayload,
    ) -> CompatResult<lighthouse_wrapper::types::ExecutionPayloadCapella<lighthouse_wrapper::types::MainnetEthSpec>> {
        // Check for v5-only features that can't be converted
        if payload.uses_v5_features() {
            return Err(CompatError::TypeConversion {
                from_type: "ExecutionPayload (v5)".to_string(),
                to_type: "ExecutionPayloadCapella (v4)".to_string(),
                reason: format!(
                    "Payload contains v5-only features: blob_gas_used={:?}, excess_blob_gas={:?}, parent_beacon_block_root={:?}",
                    payload.blob_gas_used,
                    payload.excess_blob_gas, 
                    payload.parent_beacon_block_root
                ),
            });
        }
        
        use lighthouse_wrapper::types::*;
        use ssz_types::VariableList;
        
        let transactions = VariableList::new(
            payload.transactions.into_iter()
                .map(|tx| VariableList::new(tx))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| CompatError::TypeConversion {
                    from_type: "transactions".to_string(),
                    to_type: "VariableList".to_string(),
                    reason: format!("SSZ conversion error: {}", e),
                })?
        ).map_err(|e| CompatError::TypeConversion {
            from_type: "transaction_list".to_string(),
            to_type: "VariableList".to_string(),
            reason: format!("SSZ conversion error: {}", e),
        })?;
        
        let withdrawals = payload.withdrawals.map(|w| {
            VariableList::new(
                w.into_iter()
                    .map(|withdrawal| convert_withdrawal_to_v4(withdrawal))
                    .collect()
            )
        }).transpose().map_err(|e| CompatError::TypeConversion {
            from_type: "withdrawals".to_string(),
            to_type: "VariableList".to_string(),
            reason: format!("SSZ conversion error: {}", e),
        })?;
        
        Ok(ExecutionPayloadCapella {
            parent_hash: payload.parent_hash,
            fee_recipient: payload.fee_recipient,
            state_root: payload.state_root,
            receipts_root: payload.receipts_root,
            logs_bloom: FixedVector::new(payload.logs_bloom)
                .map_err(|e| CompatError::TypeConversion {
                    from_type: "logs_bloom".to_string(),
                    to_type: "FixedVector".to_string(),
                    reason: format!("SSZ conversion error: {}", e),
                })?,
            prev_randao: payload.prev_randao,
            block_number: payload.block_number,
            gas_limit: payload.gas_limit,
            gas_used: payload.gas_used,
            timestamp: payload.timestamp,
            extra_data: VariableList::new(payload.extra_data)
                .map_err(|e| CompatError::TypeConversion {
                    from_type: "extra_data".to_string(),
                    to_type: "VariableList".to_string(),
                    reason: format!("SSZ conversion error: {}", e),
                })?,
            base_fee_per_gas: payload.base_fee_per_gas,
            block_hash: payload.block_hash,
            transactions,
            withdrawals: withdrawals.unwrap_or_else(|| VariableList::empty()),
        })
    }
    
    /// Convert unified withdrawal to v4 format
    pub fn convert_withdrawal_to_v4(
        withdrawal: Withdrawal,
    ) -> lighthouse_wrapper::types::Withdrawal {
        lighthouse_wrapper::types::Withdrawal {
            index: withdrawal.index,
            validator_index: withdrawal.validator_index,
            address: withdrawal.address,
            amount: withdrawal.amount,
        }
    }
    
    /// Convert unified forkchoice state to v4 format
    pub fn convert_forkchoice_state(
        state: ForkchoiceState,
    ) -> lighthouse_wrapper::execution_layer::ForkchoiceState {
        // v4 doesn't support justified_block_hash, so we ignore it
        lighthouse_wrapper::execution_layer::ForkchoiceState {
            head_block_hash: state.head_block_hash,
            safe_block_hash: state.safe_block_hash,
            finalized_block_hash: state.finalized_block_hash,
        }
    }
    
    /// Convert unified payload attributes to v4 format
    pub fn convert_payload_attributes(
        attrs: PayloadAttributes,
    ) -> CompatResult<lighthouse_wrapper::execution_layer::PayloadAttributes> {
        if attrs.uses_v5_features() {
            return Err(CompatError::TypeConversion {
                from_type: "PayloadAttributes (v5)".to_string(),
                to_type: "PayloadAttributes (v4)".to_string(),
                reason: format!(
                    "Attributes contain v5-only features: parent_beacon_block_root={:?}",
                    attrs.parent_beacon_block_root
                ),
            });
        }
        
        let withdrawals = attrs.withdrawals.map(|w| {
            w.into_iter()
                .map(|withdrawal| convert_withdrawal_to_v4(withdrawal))
                .collect()
        });
        
        Ok(lighthouse_wrapper::execution_layer::PayloadAttributes::new(
            attrs.timestamp,
            attrs.prev_randao,
            attrs.suggested_fee_recipient,
            withdrawals,
        ))
    }
}

/// Conversion utilities for response types
pub mod responses {
    use super::*;
    
    /// Convert v4 payload status to unified format
    pub fn convert_payload_status_from_v4(
        status: lighthouse_wrapper::execution_layer::PayloadStatusV1,
    ) -> PayloadStatus {
        let status_type = match status.status {
            lighthouse_wrapper::execution_layer::ExecutePayloadResponseStatus::Valid => PayloadStatusType::Valid,
            lighthouse_wrapper::execution_layer::ExecutePayloadResponseStatus::Invalid => PayloadStatusType::Invalid,
            lighthouse_wrapper::execution_layer::ExecutePayloadResponseStatus::Syncing => PayloadStatusType::Syncing,
            lighthouse_wrapper::execution_layer::ExecutePayloadResponseStatus::Accepted => PayloadStatusType::Accepted,
            lighthouse_wrapper::execution_layer::ExecutePayloadResponseStatus::InvalidBlockHash => PayloadStatusType::InvalidBlockHash,
            lighthouse_wrapper::execution_layer::ExecutePayloadResponseStatus::InvalidTerminalBlock => PayloadStatusType::InvalidTerminalBlock,
        };
        
        PayloadStatus {
            status: status_type,
            latest_valid_hash: status.latest_valid_hash,
            validation_error: status.validation_error,
        }
    }
    
    /// Convert unified payload status to v4 format
    pub fn convert_payload_status_to_v4(
        status: PayloadStatus,
    ) -> lighthouse_wrapper::execution_layer::PayloadStatusV1 {
        let v4_status = match status.status {
            PayloadStatusType::Valid => lighthouse_wrapper::execution_layer::ExecutePayloadResponseStatus::Valid,
            PayloadStatusType::Invalid => lighthouse_wrapper::execution_layer::ExecutePayloadResponseStatus::Invalid,
            PayloadStatusType::Syncing => lighthouse_wrapper::execution_layer::ExecutePayloadResponseStatus::Syncing,
            PayloadStatusType::Accepted => lighthouse_wrapper::execution_layer::ExecutePayloadResponseStatus::Accepted,
            PayloadStatusType::InvalidBlockHash => lighthouse_wrapper::execution_layer::ExecutePayloadResponseStatus::InvalidBlockHash,
            PayloadStatusType::InvalidTerminalBlock => lighthouse_wrapper::execution_layer::ExecutePayloadResponseStatus::InvalidTerminalBlock,
        };
        
        lighthouse_wrapper::execution_layer::PayloadStatusV1 {
            status: v4_status,
            latest_valid_hash: status.latest_valid_hash,
            validation_error: status.validation_error,
        }
    }
}

/// Validation utilities for type conversion
pub mod validation {
    use super::*;
    
    /// Validate that a payload can be safely converted to v4
    pub fn validate_v4_compatibility(payload: &ExecutionPayload) -> CompatResult<()> {
        if payload.uses_v5_features() {
            let mut incompatible_features = Vec::new();
            
            if payload.blob_gas_used.is_some() {
                incompatible_features.push("blob_gas_used");
            }
            if payload.excess_blob_gas.is_some() {
                incompatible_features.push("excess_blob_gas");
            }
            if payload.parent_beacon_block_root.is_some() {
                incompatible_features.push("parent_beacon_block_root");
            }
            
            return Err(CompatError::IncompatibleFeature {
                feature: incompatible_features.join(", "),
                version: "v4".to_string(),
            });
        }
        
        // Validate field ranges and constraints
        if payload.gas_used > payload.gas_limit {
            return Err(CompatError::TypeConversion {
                from_type: "ExecutionPayload".to_string(),
                to_type: "validation".to_string(),
                reason: format!("gas_used ({}) exceeds gas_limit ({})", payload.gas_used, payload.gas_limit),
            });
        }
        
        if payload.block_number == 0 && payload.parent_hash != H256::zero() {
            return Err(CompatError::TypeConversion {
                from_type: "ExecutionPayload".to_string(),
                to_type: "validation".to_string(),
                reason: "Genesis block must have zero parent hash".to_string(),
            });
        }
        
        Ok(())
    }
    
    /// Validate that forkchoice state is consistent
    pub fn validate_forkchoice_consistency(state: &ForkchoiceState) -> CompatResult<()> {
        // Basic consistency checks
        if state.head_block_hash == H256::zero() {
            return Err(CompatError::TypeConversion {
                from_type: "ForkchoiceState".to_string(),
                to_type: "validation".to_string(),
                reason: "Head block hash cannot be zero".to_string(),
            });
        }
        
        // In practice, we'd validate the chain relationships here
        // For now, just ensure basic field validity
        
        Ok(())
    }
    
    /// Validate payload attributes
    pub fn validate_payload_attributes(attrs: &PayloadAttributes) -> CompatResult<()> {
        if attrs.timestamp == 0 {
            return Err(CompatError::TypeConversion {
                from_type: "PayloadAttributes".to_string(),
                to_type: "validation".to_string(),
                reason: "Timestamp cannot be zero".to_string(),
            });
        }
        
        if let Some(withdrawals) = &attrs.withdrawals {
            if withdrawals.len() > 16 {  // Arbitrary limit for this example
                return Err(CompatError::TypeConversion {
                    from_type: "PayloadAttributes".to_string(),
                    to_type: "validation".to_string(),
                    reason: format!("Too many withdrawals: {} (max: 16)", withdrawals.len()),
                });
            }
        }
        
        Ok(())
    }
}

/// Conversion context for maintaining state during conversion
#[derive(Debug, Clone)]
pub struct ConversionContext {
    /// Conversion options
    pub options: ConversionOptions,
    
    /// Conversion statistics
    pub stats: ConversionStats,
    
    /// Custom field mappings
    pub field_mappings: HashMap<String, String>,
}

/// Options for type conversion
#[derive(Debug, Clone)]
pub struct ConversionOptions {
    /// Allow lossy conversions
    pub allow_lossy: bool,
    
    /// Strict validation
    pub strict_validation: bool,
    
    /// Default values for missing fields
    pub use_defaults: bool,
    
    /// Convert v5 features to v4 equivalents where possible
    pub downgrade_features: bool,
}

/// Statistics about type conversions
#[derive(Debug, Clone, Default)]
pub struct ConversionStats {
    /// Total conversions performed
    pub total_conversions: u64,
    
    /// Successful conversions
    pub successful_conversions: u64,
    
    /// Failed conversions
    pub failed_conversions: u64,
    
    /// Lossy conversions
    pub lossy_conversions: u64,
    
    /// Feature downgrades performed
    pub feature_downgrades: u64,
    
    /// Conversion types performed
    pub conversion_types: HashMap<String, u64>,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            allow_lossy: true,
            strict_validation: false,
            use_defaults: true,
            downgrade_features: false,
        }
    }
}

impl Default for ConversionContext {
    fn default() -> Self {
        Self {
            options: ConversionOptions::default(),
            stats: ConversionStats::default(),
            field_mappings: HashMap::new(),
        }
    }
}

impl ConversionContext {
    /// Create a new conversion context
    pub fn new(options: ConversionOptions) -> Self {
        Self {
            options,
            stats: ConversionStats::default(),
            field_mappings: HashMap::new(),
        }
    }
    
    /// Record a successful conversion
    pub fn record_success(&mut self, conversion_type: &str) {
        self.stats.total_conversions += 1;
        self.stats.successful_conversions += 1;
        *self.stats.conversion_types.entry(conversion_type.to_string()).or_insert(0) += 1;
    }
    
    /// Record a failed conversion
    pub fn record_failure(&mut self, conversion_type: &str) {
        self.stats.total_conversions += 1;
        self.stats.failed_conversions += 1;
        *self.stats.conversion_types.entry(conversion_type.to_string()).or_insert(0) += 1;
    }
    
    /// Record a lossy conversion
    pub fn record_lossy(&mut self) {
        self.stats.lossy_conversions += 1;
    }
    
    /// Record a feature downgrade
    pub fn record_downgrade(&mut self) {
        self.stats.feature_downgrades += 1;
    }
    
    /// Get conversion success rate
    pub fn success_rate(&self) -> f64 {
        if self.stats.total_conversions == 0 {
            0.0
        } else {
            self.stats.successful_conversions as f64 / self.stats.total_conversions as f64
        }
    }
    
    /// Get lossy conversion rate
    pub fn lossy_rate(&self) -> f64 {
        if self.stats.successful_conversions == 0 {
            0.0
        } else {
            self.stats.lossy_conversions as f64 / self.stats.successful_conversions as f64
        }
    }
}

/// Batch conversion utilities for performance
pub mod batch {
    use super::*;
    use futures::future::join_all;
    
    /// Convert multiple payloads in parallel
    pub async fn convert_payloads_v4_to_v5(
        payloads: Vec<lighthouse_wrapper::types::ExecutionPayloadCapella<lighthouse_wrapper::types::MainnetEthSpec>>,
        context: &mut ConversionContext,
    ) -> CompatResult<Vec<ExecutionPayload>> {
        let futures = payloads.into_iter().map(|payload| {
            async move {
                v4_to_v5::convert_execution_payload_capella(payload)
            }
        });
        
        let results = join_all(futures).await;
        
        let mut converted = Vec::new();
        for result in results {
            match result {
                Ok(payload) => {
                    context.record_success("payload_v4_to_v5");
                    converted.push(payload);
                }
                Err(e) => {
                    context.record_failure("payload_v4_to_v5");
                    return Err(e);
                }
            }
        }
        
        Ok(converted)
    }
    
    /// Convert multiple forkchoice states
    pub fn convert_forkchoice_states_v4_to_v5(
        states: Vec<lighthouse_wrapper::execution_layer::ForkchoiceState>,
        context: &mut ConversionContext,
    ) -> Vec<ForkchoiceState> {
        states.into_iter().map(|state| {
            let converted = v4_to_v5::convert_forkchoice_state(state);
            context.record_success("forkchoice_v4_to_v5");
            converted
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    
    #[test]
    fn test_payload_v5_feature_detection() {
        let mut payload = ExecutionPayload::default_test_payload();
        
        // Should not use v5 features initially
        assert!(!payload.uses_v5_features());
        assert!(validation::validate_v4_compatibility(&payload).is_ok());
        
        // Add v5 feature
        payload.blob_gas_used = Some(100);
        assert!(payload.uses_v5_features());
        assert!(validation::validate_v4_compatibility(&payload).is_err());
    }
    
    #[test]
    fn test_forkchoice_conversion() {
        let state = ForkchoiceState::default_test_state();
        
        // Should not use v5 features initially
        assert!(!state.uses_v5_features());
        
        // Add v5 feature
        let mut v5_state = state.clone();
        v5_state.justified_block_hash = Some(H256::from_low_u64_be(1));
        assert!(v5_state.uses_v5_features());
        
        // Convert to v4 compatible
        let v4_compatible = v5_state.to_v4_compatible();
        assert!(!v4_compatible.uses_v5_features());
        assert!(v4_compatible.justified_block_hash.is_none());
    }
    
    #[test]
    fn test_conversion_context() {
        let mut context = ConversionContext::default();
        
        // Record some conversions
        context.record_success("test_type");
        context.record_success("test_type");
        context.record_failure("test_type");
        context.record_lossy();
        
        // Check statistics
        assert_eq!(context.stats.total_conversions, 3);
        assert_eq!(context.stats.successful_conversions, 2);
        assert_eq!(context.stats.failed_conversions, 1);
        assert_eq!(context.stats.lossy_conversions, 1);
        assert_eq!(context.success_rate(), 2.0 / 3.0);
        assert_eq!(context.lossy_rate(), 1.0 / 2.0);
    }
    
    #[test]
    fn test_payload_attributes_validation() {
        let mut attrs = PayloadAttributes::default_test_attributes();
        
        // Valid attributes should pass
        assert!(validation::validate_payload_attributes(&attrs).is_ok());
        
        // Invalid timestamp should fail
        attrs.timestamp = 0;
        assert!(validation::validate_payload_attributes(&attrs).is_err());
        
        // Reset timestamp and test withdrawals
        attrs.timestamp = 1234567890;
        attrs.withdrawals = Some(vec![Withdrawal {
            index: 0,
            validator_index: 0,
            address: Address::zero(),
            amount: 1000,
        }; 20]); // Too many withdrawals
        
        assert!(validation::validate_payload_attributes(&attrs).is_err());
    }
    
    #[test]
    fn test_conversion_options() {
        let options = ConversionOptions {
            allow_lossy: false,
            strict_validation: true,
            use_defaults: false,
            downgrade_features: true,
        };
        
        let context = ConversionContext::new(options);
        assert!(!context.options.allow_lossy);
        assert!(context.options.strict_validation);
        assert!(!context.options.use_defaults);
        assert!(context.options.downgrade_features);
    }
}