//! Withdrawal Collection System for V2 Block Production
//!
//! Implements the data pipeline for collecting peg-in operations and fee distribution
//! that are required for execution payload building.

use ethereum_types::{Address, U256};
use lighthouse_wrapper::types::Withdrawal;
use std::collections::BTreeMap;
use tracing::{debug, info, warn};

use super::{ChainActor, ChainConfig, ChainError};
use super::tendermint::pegin::{PegInInfo, QueuedPegIn};
use crate::engine::ConsensusAmount;

/// Withdrawal collection result
#[derive(Debug, Clone)]
pub struct WithdrawalCollection {
    pub withdrawals: Vec<Withdrawal>,
    pub pegin_count: usize,
    pub total_pegin_amount: U256,
    pub total_fee_amount: U256,
}

/// Standalone withdrawal collection function for use in async handlers
pub async fn collect_withdrawals_standalone(
    queued_pegins: &BTreeMap<bitcoin::Txid, QueuedPegIn>,
    storage_actor: Option<&actix::Addr<crate::actors_v2::storage::StorageActor>>,
    validator_address: Option<ethereum_types::Address>,
    federation: &[ethereum_types::Address],
    head: &Option<crate::actors_v2::storage::actor::BlockRef>,
) -> Result<WithdrawalCollection, ChainError> {
    let mut withdrawals = Vec::new();
    let mut pegin_count = 0;
    let mut total_pegin_amount = U256::zero();

    debug!("Starting standalone withdrawal collection for block production");

    // 1. Process queued peg-ins (QueuedPegIn wrapper, access .info for PegInInfo)
    for (txid, queued_pegin) in queued_pegins {
        let pegin_info = &queued_pegin.info;
        debug!(
            txid = %txid,
            amount = pegin_info.amount,
            evm_account = ?pegin_info.evm_account,
            "Processing peg-in for withdrawal"
        );

        // Basic validation
        if pegin_info.amount > 0 && pegin_info.evm_account != ethereum_types::Address::zero() {
            let withdrawal = lighthouse_wrapper::types::Withdrawal {
                index: withdrawals.len() as u64,
                validator_index: 0,
                address: pegin_info.evm_account,
                amount: crate::engine::ConsensusAmount::from_satoshi(pegin_info.amount).0,
            };

            total_pegin_amount += U256::from(pegin_info.amount);
            withdrawals.push(withdrawal);
            pegin_count += 1;
        }
    }

    // 2. Calculate accumulated fees using V0 pattern
    let accumulated_fees = calculate_accumulated_fees_standalone(storage_actor, head).await?;
    let total_fee_amount = U256::from(accumulated_fees.0);

    if accumulated_fees > crate::engine::ConsensusAmount(0) {
        info!(
            accumulated_fees = accumulated_fees.0,
            "Processing fee distribution for block"
        );

        add_fee_distribution_withdrawals_standalone(
            &mut withdrawals,
            accumulated_fees,
            validator_address,
            federation,
        )?;
    }

    let result = WithdrawalCollection {
        withdrawals,
        pegin_count,
        total_pegin_amount,
        total_fee_amount,
    };

    info!(
        pegin_count = result.pegin_count,
        total_pegin_amount = %result.total_pegin_amount,
        total_fee_amount = %result.total_fee_amount,
        withdrawal_count = result.withdrawals.len(),
        "Completed standalone withdrawal collection"
    );

    Ok(result)
}

/// Standalone fee calculation function
async fn calculate_accumulated_fees_standalone(
    storage_actor: Option<&actix::Addr<crate::actors_v2::storage::StorageActor>>,
    head: &Option<crate::actors_v2::storage::actor::BlockRef>,
) -> Result<crate::engine::ConsensusAmount, ChainError> {
    let parent_hash = match head {
        Some(head_ref) => head_ref.hash,
        None => {
            debug!("No parent block found - returning zero fees for genesis");
            return Ok(crate::engine::ConsensusAmount(0));
        }
    };

    if let Some(storage_actor) = storage_actor {
        let get_fees_msg = crate::actors_v2::storage::messages::GetAccumulatedFeesMessage {
            block_root: parent_hash,
            correlation_id: Some(uuid::Uuid::new_v4()),
        };

        match storage_actor.send(get_fees_msg).await {
            Ok(storage_result) => match storage_result {
                Ok(Some(fees_u256)) => {
                    debug!(
                        parent_hash = %parent_hash,
                        accumulated_fees = %fees_u256,
                        "Retrieved accumulated fees from storage"
                    );
                    Ok(crate::engine::ConsensusAmount(
                        fees_u256.low_u64() / 1_000_000_000,
                    ))
                }
                Ok(None) => {
                    debug!(parent_hash = %parent_hash, "No accumulated fees found");
                    Ok(crate::engine::ConsensusAmount(0))
                }
                Err(e) => {
                    warn!(error = ?e, "Failed to get accumulated fees - using zero");
                    Ok(crate::engine::ConsensusAmount(0))
                }
            },
            Err(e) => {
                warn!(error = ?e, "Communication error getting fees - using zero");
                Ok(crate::engine::ConsensusAmount(0))
            }
        }
    } else {
        warn!("StorageActor not available for fee calculation");
        Ok(crate::engine::ConsensusAmount(0))
    }
}

/// Standalone fee distribution function
fn add_fee_distribution_withdrawals_standalone(
    withdrawals: &mut Vec<lighthouse_wrapper::types::Withdrawal>,
    accumulated_fees: crate::engine::ConsensusAmount,
    validator_address: Option<ethereum_types::Address>,
    federation: &[ethereum_types::Address],
) -> Result<(), ChainError> {
    // Alys consensus: 80% to block producer, 20% to federation (matches V0)
    let miner_fee = crate::engine::ConsensusAmount(accumulated_fees.0 * 8 / 10);
    let federation_fee = crate::engine::ConsensusAmount(accumulated_fees.0 * 2 / 10);

    // Get miner address
    let miner_address = validator_address.unwrap_or_else(|| {
        ethereum_types::Address::from_slice(&[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0xde, 0xad,
        ])
    });

    // Add miner fee withdrawal
    withdrawals.push(lighthouse_wrapper::types::Withdrawal {
        index: withdrawals.len() as u64,
        validator_index: 0,
        address: miner_address,
        amount: miner_fee.0,
    });

    // Add federation fee withdrawals
    if !federation.is_empty() {
        let per_member_fee =
            crate::engine::ConsensusAmount(federation_fee.0 / federation.len() as u64);

        for (index, federation_member) in federation.iter().enumerate() {
            withdrawals.push(lighthouse_wrapper::types::Withdrawal {
                index: withdrawals.len() as u64,
                validator_index: 0,
                address: *federation_member,
                amount: per_member_fee.0,
            });

            debug!(
                federation_member = ?federation_member,
                member_index = index,
                member_fee = per_member_fee.0,
                "Added federation member fee withdrawal"
            );
        }
    }

    Ok(())
}

impl ChainActor {
    /// Collect withdrawals for execution payload building (delegates to standalone function)
    ///
    /// This function processes queued peg-ins and calculates fee distribution
    /// according to the Alys consensus rules (80% miner, 20% federation - matches V0).
    pub async fn collect_withdrawals(&self) -> Result<WithdrawalCollection, ChainError> {
        let mut withdrawals = Vec::new();
        let mut pegin_count = 0;
        let mut total_pegin_amount = U256::zero();

        debug!("Starting withdrawal collection for block production");

        // 1. Process queued peg-ins from bridge (async RwLock access)
        // Note: queued_pegins now stores QueuedPegIn, access .info for PegInInfo fields
        let queued_pegins_snapshot = self.state.queued_pegins.read().await.clone();
        for (txid, queued_pegin) in &queued_pegins_snapshot {
            let pegin_info = &queued_pegin.info;
            debug!(
                txid = %txid,
                amount = pegin_info.amount,
                evm_account = ?pegin_info.evm_account,
                "Processing peg-in for withdrawal"
            );

            // Validate peg-in before including
            if self.validate_pegin_for_withdrawal(pegin_info).await? {
                let withdrawal = Withdrawal {
                    index: withdrawals.len() as u64, // Will be re-indexed by Engine
                    validator_index: 0,              // Not used in our consensus model
                    address: pegin_info.evm_account,
                    amount: ConsensusAmount::from_satoshi(pegin_info.amount).0,
                };

                total_pegin_amount += U256::from(pegin_info.amount);
                withdrawals.push(withdrawal.clone());
                pegin_count += 1;

                debug!(
                    txid = %txid,
                    withdrawal_amount = withdrawal.amount,
                    "Added peg-in withdrawal"
                );
            } else {
                warn!(
                    txid = %txid,
                    amount = pegin_info.amount,
                    "Skipped invalid peg-in for withdrawal"
                );
            }
        }

        // 2. Calculate accumulated fees
        let accumulated_fees = self.calculate_accumulated_fees().await?;
        let total_fee_amount = U256::from(accumulated_fees.0);

        if accumulated_fees > ConsensusAmount(0) {
            info!(
                accumulated_fees = accumulated_fees.0,
                "Processing fee distribution for block"
            );

            self.add_fee_distribution_withdrawals(&mut withdrawals, accumulated_fees)
                .await?;
        } else {
            debug!("No accumulated fees to distribute");
        }

        let result = WithdrawalCollection {
            withdrawals,
            pegin_count,
            total_pegin_amount,
            total_fee_amount,
        };

        info!(
            pegin_count = result.pegin_count,
            total_pegin_amount = %result.total_pegin_amount,
            total_fee_amount = %result.total_fee_amount,
            withdrawal_count = result.withdrawals.len(),
            "Completed withdrawal collection"
        );

        Ok(result)
    }

    /// Add fee distribution withdrawals according to consensus rules
    async fn add_fee_distribution_withdrawals(
        &self,
        withdrawals: &mut Vec<Withdrawal>,
        accumulated_fees: ConsensusAmount,
    ) -> Result<(), ChainError> {
        // Alys consensus: 80% to block producer, 20% to federation (matches V0)
        let miner_fee = ConsensusAmount(accumulated_fees.0 * 8 / 10);
        let federation_fee = ConsensusAmount(accumulated_fees.0 * 2 / 10);

        // Get miner address from configuration
        let miner_address = self.get_miner_address()?;

        // Add miner fee withdrawal
        withdrawals.push(Withdrawal {
            index: withdrawals.len() as u64,
            validator_index: 0,
            address: miner_address,
            amount: miner_fee.0,
        });

        debug!(
            miner_address = ?miner_address,
            miner_fee = miner_fee.0,
            "Added miner fee withdrawal"
        );

        // Add federation fee withdrawals (split among members)
        if !self.state.federation.is_empty() {
            let per_member_fee =
                ConsensusAmount(federation_fee.0 / self.state.federation.len() as u64);

            for (index, federation_member) in self.state.federation.iter().enumerate() {
                withdrawals.push(Withdrawal {
                    index: withdrawals.len() as u64,
                    validator_index: 0,
                    address: *federation_member,
                    amount: per_member_fee.0,
                });

                debug!(
                    federation_member = ?federation_member,
                    member_index = index,
                    member_fee = per_member_fee.0,
                    "Added federation member fee withdrawal"
                );
            }
        } else {
            warn!("No federation members configured - federation fees will be burned");
        }

        Ok(())
    }

    /// Validate peg-in for inclusion in withdrawal collection
    async fn validate_pegin_for_withdrawal(
        &self,
        pegin_info: &PegInInfo,
    ) -> Result<bool, ChainError> {
        // Basic validation for withdrawal inclusion
        // More comprehensive validation would be performed elsewhere

        // Check amount is within reasonable bounds
        if pegin_info.amount == 0 {
            debug!(
                txid = %pegin_info.txid,
                "Peg-in has zero amount - excluding from withdrawals"
            );
            return Ok(false);
        }

        // Check EVM account is valid (non-zero address)
        if pegin_info.evm_account == Address::zero() {
            debug!(
                txid = %pegin_info.txid,
                "Peg-in has zero EVM account - excluding from withdrawals"
            );
            return Ok(false);
        }

        // Additional validation could be added here:
        // - Check Bitcoin confirmation status
        // - Validate against double-spending
        // - Check signature validity
        // For now, accept valid basic structure

        Ok(true)
    }

    /// Calculate accumulated fees since last block (V0-compatible implementation)
    async fn calculate_accumulated_fees(&self) -> Result<ConsensusAmount, ChainError> {
        // Implementation matches V0 chain.rs:1637-1643 pattern

        // Get parent block for fee accumulation lookup
        let parent_hash = {
            let head_guard = self.state.head.read().await;
            match head_guard.as_ref() {
                Some(head_ref) => head_ref.hash,
                None => {
                    debug!("No parent block found - returning zero fees for genesis");
                    return Ok(ConsensusAmount(0));
                }
            }
        };

        // Query accumulated fees from storage (matches V0 storage.get_accumulated_block_fees)
        let accumulated_fees = if let Some(ref storage_actor) = self.storage_actor {
            let get_fees_msg = crate::actors_v2::storage::messages::GetAccumulatedFeesMessage {
                block_root: parent_hash,
                correlation_id: Some(uuid::Uuid::new_v4()),
            };

            match storage_actor.send(get_fees_msg).await {
                Ok(storage_result) => {
                    match storage_result {
                        Ok(Some(fees_u256)) => {
                            debug!(
                                parent_hash = %parent_hash,
                                accumulated_fees = %fees_u256,
                                "Retrieved accumulated fees from storage"
                            );
                            // Convert U256 to ConsensusAmount (wei to gwei conversion)
                            ConsensusAmount(fees_u256.low_u64() / 1_000_000_000)
                            // Convert wei to gwei
                        }
                        Ok(None) => {
                            debug!(parent_hash = %parent_hash, "No accumulated fees found - first block");
                            ConsensusAmount(0)
                        }
                        Err(e) => {
                            warn!(error = ?e, "Failed to get accumulated fees from storage - using zero");
                            ConsensusAmount(0)
                        }
                    }
                }
                Err(e) => {
                    warn!(error = ?e, "Communication error with StorageActor for fees - using zero");
                    ConsensusAmount(0)
                }
            }
        } else {
            warn!("StorageActor not available for fee calculation - using zero");
            ConsensusAmount(0)
        };

        // TODO: Add current block transaction fees (would need access to execution receipts)
        // For Phase 2, use the accumulated fees from storage
        // Phase 3 will add: fees += total_fees(execution_block, execution_receipts)

        debug!(
            parent_hash = %parent_hash,
            accumulated_fees = accumulated_fees.0,
            "Calculated accumulated fees for block production"
        );

        Ok(accumulated_fees)
    }

    /// Get miner address for fee distribution
    fn get_miner_address(&self) -> Result<Address, ChainError> {
        // This would come from configuration
        // For now, use a placeholder address

        // In production, this would be:
        // - The validator's configured fee recipient address
        // - Or a configured mining reward address

        if let Some(validator_address) = self.config.get_validator_address() {
            Ok(validator_address)
        } else {
            // Fallback to a burn address if no miner address configured
            Ok(Address::from_slice(&[
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0xde, 0xad,
            ]))
        }
    }
}

/// Helper extension for ChainConfig to get validator address
impl ChainConfig {
    /// Get validator fee recipient address
    pub fn get_validator_address(&self) -> Option<Address> {
        // Return configured validator address for block production rewards
        self.validator_address
    }
}
