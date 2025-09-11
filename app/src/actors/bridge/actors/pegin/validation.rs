//! PegIn Deposit Validation
//! 
//! Comprehensive validation logic for Bitcoin deposits

use bitcoin::{Transaction, Address as BtcAddress, Network};
use ethereum_types::H160;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::{info, warn, error, debug};

use crate::actors::bridge::{
    messages::DepositTransaction,
    shared::{validation::*, constants::{MIN_PEGIN_AMOUNT, DUST_LIMIT}},
};

/// Deposit validator for peg-in operations
#[derive(Debug)]
pub struct DepositValidator {
    /// Federation addresses to monitor
    federation_addresses: Vec<BtcAddress>,
    
    /// Address validator
    address_validator: BitcoinTransactionValidator,
    
    /// Processed transactions cache (to avoid duplicates)
    processed_transactions: HashSet<bitcoin::Txid>,
    
    /// Validation statistics
    validation_stats: ValidationStats,
}

/// Validation statistics
#[derive(Debug, Clone, Default)]
pub struct ValidationStats {
    pub total_validations: u64,
    pub valid_deposits: u64,
    pub invalid_deposits: u64,
    pub duplicate_deposits: u64,
    pub validation_errors: u64,
}

/// Enhanced validation result for deposits
#[derive(Debug, Clone)]
pub struct DepositValidationResult {
    pub valid: bool,
    pub extracted_address: Option<H160>,
    pub validated_amount: u64,
    pub federation_output_index: Option<usize>,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub validation_score: f64,
}

impl DepositValidator {
    /// Create new deposit validator
    pub fn new(federation_addresses: Vec<BtcAddress>) -> Result<Self, ValidationError> {
        if federation_addresses.is_empty() {
            return Err(ValidationError::Other("No federation addresses provided".to_string()));
        }

        // Determine network from first address
        let network = federation_addresses[0].network;
        
        // Verify all addresses are on the same network
        for addr in &federation_addresses {
            if addr.network != network {
                return Err(ValidationError::NetworkMismatch {
                    expected: network,
                    found: addr.network,
                });
            }
        }

        let federation_scripts = federation_addresses
            .iter()
            .map(|addr| addr.script_pubkey())
            .collect();

        let address_validator = BitcoinTransactionValidator::new(
            network,
            federation_addresses.clone(),
            federation_scripts,
        );

        Ok(Self {
            federation_addresses,
            address_validator,
            processed_transactions: HashSet::new(),
            validation_stats: ValidationStats::default(),
        })
    }

    /// Validate a deposit transaction
    pub fn validate_deposit(&mut self, deposit: &DepositTransaction) -> Result<DepositValidationResult, String> {
        self.validation_stats.total_validations += 1;
        
        debug!("Validating deposit transaction: {}", deposit.txid);

        // Check for duplicate
        if self.processed_transactions.contains(&deposit.txid) {
            self.validation_stats.duplicate_deposits += 1;
            return Ok(DepositValidationResult {
                valid: false,
                extracted_address: None,
                validated_amount: 0,
                federation_output_index: None,
                errors: vec![ValidationError::DuplicateTransaction { txid: deposit.txid }],
                warnings: vec![],
                validation_score: 0.0,
            });
        }

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut validation_score = 0.0;

        // Basic transaction structure validation
        if deposit.bitcoin_tx.input.is_empty() {
            errors.push(ValidationError::InvalidTransaction("No inputs".to_string()));
        } else {
            validation_score += 20.0; // Has inputs
        }

        if deposit.bitcoin_tx.output.is_empty() {
            errors.push(ValidationError::InvalidTransaction("No outputs".to_string()));
        } else {
            validation_score += 20.0; // Has outputs
        }

        // Federation output validation
        let (federation_output_index, federation_output) = self.find_federation_output(&deposit.bitcoin_tx);
        
        if federation_output.is_none() {
            errors.push(ValidationError::InvalidFederationOutput);
        } else {
            validation_score += 25.0; // Valid federation output
            
            // Amount validation
            let amount = federation_output.unwrap().value;
            if amount < MIN_PEGIN_AMOUNT {
                errors.push(ValidationError::AmountTooSmall {
                    amount,
                    minimum: MIN_PEGIN_AMOUNT,
                });
            } else {
                validation_score += 15.0; // Valid amount
            }

            // Check for dust outputs
            if amount < DUST_LIMIT {
                warnings.push(ValidationWarning::DustOutput {
                    amount,
                    dust_limit: DUST_LIMIT,
                });
            }
        }

        // EVM address extraction
        let extracted_address = match self.extract_and_validate_evm_address(&deposit.bitcoin_tx) {
            Ok(addr) => {
                validation_score += 20.0; // Valid EVM address
                Some(addr)
            }
            Err(e) => {
                errors.push(e);
                None
            }
        };

        // Network consistency check
        for addr in &self.federation_addresses {
            if addr.network != deposit.bitcoin_tx.version.to_consensus() as u8 as Network {
                // This is a simplified check; in practice, you'd validate against expected network
                warnings.push(ValidationWarning::Other("Network consistency check needed".to_string()));
            }
        }

        // Fee analysis
        let fee_analysis = self.analyze_transaction_fees(&deposit.bitcoin_tx);
        if fee_analysis.fee_rate < 1.0 {
            warnings.push(ValidationWarning::LowFee {
                current: fee_analysis.fee_rate as u64,
                recommended: 10,
            });
        } else if fee_analysis.fee_rate > 100.0 {
            warnings.push(ValidationWarning::HighFee {
                current: fee_analysis.fee_rate as u64,
                maximum: 100,
            });
        }

        // Finalize validation
        let valid = errors.is_empty();
        if valid {
            self.validation_stats.valid_deposits += 1;
            self.processed_transactions.insert(deposit.txid);
        } else {
            self.validation_stats.invalid_deposits += 1;
        }

        // Cap validation score at 100
        validation_score = validation_score.min(100.0);

        let result = DepositValidationResult {
            valid,
            extracted_address,
            validated_amount: federation_output.map(|out| out.value).unwrap_or(0),
            federation_output_index,
            errors,
            warnings,
            validation_score,
        };

        debug!("Deposit validation result: valid={}, score={:.1}", result.valid, result.validation_score);
        Ok(result)
    }

    /// Find federation output in transaction
    fn find_federation_output(&self, tx: &Transaction) -> (Option<usize>, Option<&bitcoin::TxOut>) {
        for (index, output) in tx.output.iter().enumerate() {
            for fed_addr in &self.federation_addresses {
                if output.script_pubkey == fed_addr.script_pubkey() {
                    return (Some(index), Some(output));
                }
            }
        }
        (None, None)
    }

    /// Extract and validate EVM address from OP_RETURN
    fn extract_and_validate_evm_address(&self, tx: &Transaction) -> Result<H160, ValidationError> {
        // Find OP_RETURN output
        let op_return_output = tx.output.iter()
            .find(|output| output.script_pubkey.is_op_return())
            .ok_or_else(|| ValidationError::InvalidOpReturn("No OP_RETURN output found".to_string()))?;

        // Extract data
        let script_bytes = op_return_output.script_pubkey.as_bytes();
        if script_bytes.len() < 22 { // OP_RETURN + length + 20 bytes
            return Err(ValidationError::InvalidOpReturn("OP_RETURN too short".to_string()));
        }

        // Parse OP_RETURN structure
        if script_bytes[0] != 0x6a { // OP_RETURN
            return Err(ValidationError::InvalidOpReturn("Not an OP_RETURN script".to_string()));
        }

        // Extract address bytes (skip OP_RETURN and length)
        let addr_bytes = &script_bytes[2..22];
        let address = H160::from_slice(addr_bytes);

        // Validate address (not zero address)
        if address.is_zero() {
            return Err(ValidationError::MissingEthereumAddress);
        }

        Ok(address)
    }

    /// Analyze transaction fees
    fn analyze_transaction_fees(&self, tx: &Transaction) -> FeeAnalysis {
        // This is a simplified analysis
        // In practice, you'd calculate actual input values vs output values
        let estimated_size = tx.vsize() as f64;
        let estimated_fee = 1000.0; // Placeholder
        let fee_rate = estimated_fee / estimated_size;

        FeeAnalysis {
            estimated_fee,
            fee_rate,
            is_reasonable: fee_rate >= 1.0 && fee_rate <= 100.0,
        }
    }

    /// Check if transaction has been processed before
    pub fn is_duplicate(&self, txid: &bitcoin::Txid) -> bool {
        self.processed_transactions.contains(txid)
    }

    /// Get validation statistics
    pub fn get_stats(&self) -> ValidationStats {
        self.validation_stats.clone()
    }

    /// Clear processed transactions cache (for memory management)
    pub fn clear_old_transactions(&mut self, keep_recent: usize) {
        if self.processed_transactions.len() > keep_recent {
            // In practice, you'd keep track of timestamps and clear based on age
            // For now, just clear excess entries
            let excess = self.processed_transactions.len() - keep_recent;
            let txids_to_remove: Vec<bitcoin::Txid> = self.processed_transactions
                .iter()
                .take(excess)
                .cloned()
                .collect();
            
            for txid in txids_to_remove {
                self.processed_transactions.remove(&txid);
            }
        }
    }

    /// Update federation addresses
    pub fn update_federation_addresses(&mut self, new_addresses: Vec<BtcAddress>) -> Result<(), ValidationError> {
        if new_addresses.is_empty() {
            return Err(ValidationError::Other("No federation addresses provided".to_string()));
        }

        // Verify network consistency
        let network = new_addresses[0].network;
        for addr in &new_addresses {
            if addr.network != network {
                return Err(ValidationError::NetworkMismatch {
                    expected: network,
                    found: addr.network,
                });
            }
        }

        self.federation_addresses = new_addresses;
        
        // Update address validator
        let federation_scripts = self.federation_addresses
            .iter()
            .map(|addr| addr.script_pubkey())
            .collect();

        self.address_validator = BitcoinTransactionValidator::new(
            network,
            self.federation_addresses.clone(),
            federation_scripts,
        );

        info!("Updated federation addresses: {} addresses", self.federation_addresses.len());
        Ok(())
    }
}

/// Fee analysis result
#[derive(Debug, Clone)]
pub struct FeeAnalysis {
    pub estimated_fee: f64,
    pub fee_rate: f64, // sat/vB
    pub is_reasonable: bool,
}

/// Validation rule engine for advanced checks
pub struct ValidationRuleEngine {
    rules: Vec<Box<dyn ValidationRule>>,
}

/// Validation rule trait
pub trait ValidationRule: Send + Sync {
    fn name(&self) -> &str;
    fn validate(&self, tx: &Transaction, deposit: &DepositTransaction) -> ValidationResult<()>;
}

/// Minimum amount validation rule
pub struct MinimumAmountRule {
    min_amount: u64,
}

impl ValidationRule for MinimumAmountRule {
    fn name(&self) -> &str {
        "minimum_amount"
    }

    fn validate(&self, _tx: &Transaction, deposit: &DepositTransaction) -> ValidationResult<()> {
        if deposit.amount >= self.min_amount {
            ValidationResult {
                valid: true,
                result: Some(()),
                errors: vec![],
                warnings: vec![],
            }
        } else {
            ValidationResult {
                valid: false,
                result: None,
                errors: vec![ValidationError::AmountTooSmall {
                    amount: deposit.amount,
                    minimum: self.min_amount,
                }],
                warnings: vec![],
            }
        }
    }
}

/// OP_RETURN format validation rule
pub struct OpReturnFormatRule;

impl ValidationRule for OpReturnFormatRule {
    fn name(&self) -> &str {
        "op_return_format"
    }

    fn validate(&self, tx: &Transaction, _deposit: &DepositTransaction) -> ValidationResult<()> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check for OP_RETURN output
        let has_op_return = tx.output.iter().any(|out| out.script_pubkey.is_op_return());
        
        if !has_op_return {
            warnings.push(ValidationWarning::Other("No OP_RETURN output found".to_string()));
        } else {
            // Validate OP_RETURN format
            for output in &tx.output {
                if output.script_pubkey.is_op_return() {
                    let script_bytes = output.script_pubkey.as_bytes();
                    if script_bytes.len() < 22 {
                        errors.push(ValidationError::InvalidOpReturn("OP_RETURN too short".to_string()));
                    }
                    break;
                }
            }
        }

        ValidationResult {
            valid: errors.is_empty(),
            result: if errors.is_empty() { Some(()) } else { None },
            errors,
            warnings,
        }
    }
}

impl ValidationRuleEngine {
    /// Create new rule engine
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
        }
    }

    /// Add validation rule
    pub fn add_rule(&mut self, rule: Box<dyn ValidationRule>) {
        self.rules.push(rule);
    }

    /// Run all validation rules
    pub fn validate(&self, tx: &Transaction, deposit: &DepositTransaction) -> Vec<ValidationResult<()>> {
        self.rules.iter()
            .map(|rule| rule.validate(tx, deposit))
            .collect()
    }

    /// Create default rule engine
    pub fn default_rules() -> Self {
        let mut engine = Self::new();
        engine.add_rule(Box::new(MinimumAmountRule { min_amount: MIN_PEGIN_AMOUNT }));
        engine.add_rule(Box::new(OpReturnFormatRule));
        engine
    }
}