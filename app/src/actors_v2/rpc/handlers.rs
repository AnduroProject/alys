use actix::Addr;
use bitcoin::consensus::Decodable;
use bitcoin::hashes::hex::FromHex;
use bitcoin::BlockHash;
use ethereum_types::Address;
use serde_json::{json, Value};
use std::str::FromStr;
use uuid::Uuid;

use super::error::RpcError;
use super::tendermint_types::{
    ConsensusStateResponse, VotesInfo, ValidatorsResponse, ValidatorInfo, PubKeyInfo,
    EvidenceListResponse, EvidenceItem, ValidatorEvidenceInfo, EvidenceVoteInfo,
};
use crate::actors_v2::chain::messages::{
    CreateAuxBlock, GetChainParams, GetCommit, GetEvidence, GetPendingGovernance,
    GetTendermintState, GetValidatorSet, SubmitAuxBlock,
};
use crate::actors_v2::chain::ChainActor;
use crate::auxpow::AuxPow;

/// createauxblock RPC handler
pub struct CreateAuxBlockHandler;

impl CreateAuxBlockHandler {
    /// Handle createauxblock request
    ///
    /// # Parameters
    /// - params[0]: miner_address (hex string, optional - uses zero address if not provided)
    ///
    /// # Returns
    /// JSON object containing:
    /// - hash: aggregate hash for mining (hex string)
    /// - chainid: chain ID (integer)
    /// - previousblockhash: previous Bitcoin block hash (hex string)
    /// - coinbasevalue: coinbase reward value (integer)
    /// - bits: difficulty target (hex string)
    /// - height: target height after mining (integer)
    pub async fn handle(
        params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        // Parse miner address (optional parameter)
        let miner_address = if params.is_empty() {
            // Use zero address if not provided
            Address::zero()
        } else {
            let addr_str = params[0]
                .as_str()
                .ok_or_else(|| RpcError::InvalidParams("Expected string address".to_string()))?;

            // Remove "0x" prefix if present
            let addr_str = addr_str.trim_start_matches("0x");

            Address::from_slice(
                &hex::decode(addr_str)
                    .map_err(|e| RpcError::InvalidParams(format!("Invalid address hex: {}", e)))?,
            )
        };

        // Create correlation ID
        let correlation_id = Uuid::new_v4();

        tracing::debug!(
            correlation_id = %correlation_id,
            miner_address = %miner_address,
            "createauxblock request received"
        );

        // Send message to ChainActor
        let message = CreateAuxBlock {
            miner_address,
            correlation_id,
        };

        let aux_block = chain_actor
            .send(message)
            .await
            .map_err(|e| RpcError::MailboxError(e.to_string()))?
            .map_err(RpcError::ChainError)?;

        tracing::info!(
            correlation_id = %correlation_id,
            hash = %aux_block.hash,
            "createauxblock completed successfully"
        );

        // Convert AuxBlock to JSON (serde handles the field serialization)
        let response = serde_json::to_value(&aux_block)
            .map_err(|e| RpcError::Internal(format!("Failed to serialize AuxBlock: {}", e)))?;

        Ok(response)
    }
}

/// submitauxblock RPC handler
pub struct SubmitAuxBlockHandler;

impl SubmitAuxBlockHandler {
    /// Handle submitauxblock request
    ///
    /// # Parameters
    /// - params[0]: hash (aggregate hash from createauxblock, hex string)
    /// - params[1]: auxpow (serialized AuxPoW hex string)
    /// - params[2]: pegins (optional array of peg-in objects)
    /// - params[3]: fee_recipient (optional miner address for peg-in compensation)
    /// - params[4]: verbose (optional bool, default false)
    ///
    /// # Peg-in object format (Document 16):
    /// ```json
    /// {
    ///   "txid": "bitcoin txid hex",
    ///   "block_hash": "bitcoin block hash hex",
    ///   "block_height": 123456,
    ///   "amount": 100000,
    ///   "evm_account": "0x..."
    /// }
    /// ```
    ///
    /// # Returns
    /// - If verbose=false (default): Boolean (true/false) - Bitcoin-compatible format
    /// - If verbose=true: JSON object with {accepted, height, pegins_queued, error?}
    pub async fn handle(
        params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        // Validate minimum parameter count (hash and auxpow required)
        if params.len() < 2 {
            return Err(RpcError::InvalidParams(
                "Expected at least 2 parameters: hash and auxpow".to_string(),
            ));
        }

        // Parse aggregate hash (consensus byte order, matching createauxblock output)
        // Note: The miner sends the hash in consensus format (internal byte order),
        // NOT Bitcoin display format (reversed). We must use consensus_decode to match.
        let hash_str = params[0]
            .as_str()
            .ok_or_else(|| RpcError::InvalidParams("Expected string hash".to_string()))?;

        let hash_bytes = hex::decode(hash_str)
            .map_err(|e| RpcError::InvalidParams(format!("Invalid hash hex: {}", e)))?;
        let aggregate_hash = BlockHash::consensus_decode(&mut hash_bytes.as_slice())
            .map_err(|e| RpcError::InvalidParams(format!("Invalid hash: {:?}", e)))?;

        // Parse AuxPoW hex
        let auxpow_hex = params[1]
            .as_str()
            .ok_or_else(|| RpcError::InvalidParams("Expected string auxpow".to_string()))?;

        let auxpow_bytes = Vec::<u8>::from_hex(auxpow_hex)
            .map_err(|e| RpcError::InvalidParams(format!("Invalid auxpow hex: {}", e)))?;

        // Deserialize AuxPoW using Bitcoin's Decodable trait
        let auxpow = AuxPow::consensus_decode(&mut &auxpow_bytes[..])
            .map_err(|e| RpcError::InvalidParams(format!("Invalid auxpow structure: {:?}", e)))?;

        // Parse optional peg-in data (Document 16)
        // Note: Peg-in validation (amount > 0, reasonable height, etc.) is performed by ChainActor
        let pegins = if let Some(pegins_value) = params.get(2) {
            if pegins_value.is_null() {
                Vec::new()
            } else {
                Self::parse_pegins(pegins_value)?
            }
        } else {
            Vec::new()
        };

        // Parse optional fee recipient address
        let fee_recipient = if let Some(addr_value) = params.get(3) {
            if addr_value.is_null() {
                Address::zero()
            } else {
                Self::parse_address(addr_value)?
            }
        } else {
            Address::zero()
        };

        // Parse optional verbose flag (5th parameter)
        // Default: false for backwards compatibility with existing miners
        let verbose = params
            .get(4)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Create correlation ID
        let correlation_id = Uuid::new_v4();

        tracing::debug!(
            correlation_id = %correlation_id,
            hash = %aggregate_hash,
            auxpow_size = auxpow_bytes.len(),
            pegins_count = pegins.len(),
            fee_recipient = %fee_recipient,
            "submitauxblock request received"
        );

        // Send message to ChainActor
        let message = SubmitAuxBlock {
            aggregate_hash,
            auxpow,
            pegins,
            fee_recipient,
            correlation_id,
        };

        // Attempt submission
        let result = chain_actor
            .send(message)
            .await
            .map_err(|e| RpcError::MailboxError(e.to_string()))?;

        match result {
            Ok(response) => {
                tracing::info!(
                    correlation_id = %correlation_id,
                    hash = %aggregate_hash,
                    height = response.height,
                    pegins_queued = response.pegins_queued,
                    verbose = verbose,
                    "submitauxblock accepted successfully"
                );

                if verbose {
                    // Extended response format (Document 16)
                    Ok(json!({
                        "accepted": true,
                        "height": response.height,
                        "pegins_queued": response.pegins_queued
                    }))
                } else {
                    // Legacy Bitcoin-compatible format
                    Ok(json!(true))
                }
            }
            Err(e) => {
                tracing::warn!(
                    correlation_id = %correlation_id,
                    hash = %aggregate_hash,
                    error = ?e,
                    verbose = verbose,
                    "submitauxblock rejected"
                );

                if verbose {
                    // Extended error response
                    Ok(json!({
                        "accepted": false,
                        "error": format!("{}", e)
                    }))
                } else {
                    // Legacy Bitcoin-compatible format
                    Ok(json!(false))
                }
            }
        }
    }

    /// Parse peg-in array from JSON value
    fn parse_pegins(
        value: &Value,
    ) -> Result<Vec<crate::actors_v2::chain::tendermint::pegin::PegInInfo>, RpcError> {
        use crate::actors_v2::chain::tendermint::pegin::PegInInfo;

        let arr = value
            .as_array()
            .ok_or_else(|| RpcError::InvalidParams("pegins must be an array".to_string()))?;

        let mut pegins = Vec::with_capacity(arr.len());

        for (idx, item) in arr.iter().enumerate() {
            let pegin = Self::parse_single_pegin(item)
                .map_err(|e| RpcError::InvalidParams(format!("pegin[{}]: {}", idx, e)))?;
            pegins.push(pegin);
        }

        Ok(pegins)
    }

    /// Parse a single peg-in object from JSON
    fn parse_single_pegin(
        value: &Value,
    ) -> Result<crate::actors_v2::chain::tendermint::pegin::PegInInfo, String> {
        use bitcoin::{BlockHash as BitcoinBlockHash, Txid};
        use crate::actors_v2::chain::tendermint::pegin::PegInInfo;

        let obj = value.as_object().ok_or("expected object")?;

        // Parse txid
        let txid_str = obj
            .get("txid")
            .and_then(|v| v.as_str())
            .ok_or("missing txid")?;
        let txid = Txid::from_str(txid_str).map_err(|e| format!("invalid txid: {}", e))?;

        // Parse block_hash
        let block_hash_str = obj
            .get("block_hash")
            .and_then(|v| v.as_str())
            .ok_or("missing block_hash")?;
        let block_hash = BitcoinBlockHash::from_str(block_hash_str)
            .map_err(|e| format!("invalid block_hash: {}", e))?;

        // Parse block_height
        let block_height = obj
            .get("block_height")
            .and_then(|v| v.as_u64())
            .ok_or("missing or invalid block_height")? as u32;

        // Parse amount
        let amount = obj
            .get("amount")
            .and_then(|v| v.as_u64())
            .ok_or("missing or invalid amount")?;

        // Parse evm_account
        let evm_account_str = obj
            .get("evm_account")
            .and_then(|v| v.as_str())
            .ok_or("missing evm_account")?;

        let evm_account_str = evm_account_str.trim_start_matches("0x");
        let evm_account_bytes = hex::decode(evm_account_str)
            .map_err(|e| format!("invalid evm_account hex: {}", e))?;

        if evm_account_bytes.len() != 20 {
            return Err("evm_account must be 20 bytes".to_string());
        }

        let evm_account = Address::from_slice(&evm_account_bytes);

        Ok(PegInInfo {
            txid,
            block_hash,
            block_height,
            amount,
            evm_account,
        })
    }

    /// Parse Ethereum address from JSON value
    fn parse_address(value: &Value) -> Result<Address, RpcError> {
        let addr_str = value
            .as_str()
            .ok_or_else(|| RpcError::InvalidParams("fee_recipient must be a string".to_string()))?;

        let addr_str = addr_str.trim_start_matches("0x");
        let addr_bytes = hex::decode(addr_str)
            .map_err(|e| RpcError::InvalidParams(format!("invalid fee_recipient hex: {}", e)))?;

        if addr_bytes.len() != 20 {
            return Err(RpcError::InvalidParams(
                "fee_recipient must be 20 bytes".to_string(),
            ));
        }

        Ok(Address::from_slice(&addr_bytes))
    }
}

// ============================================================================
// Tendermint RPC Handlers (Phase 4: Document 12)
// ============================================================================

// Note: Aura RPC handlers have been removed. Tendermint is the only consensus mechanism.
// Use tendermint_* RPC methods instead.

/// tendermint_consensusState RPC handler
pub struct ConsensusStateHandler;

impl ConsensusStateHandler {
    /// Handle tendermint_consensusState request
    ///
    /// Returns current consensus state including height, round, step, and votes.
    pub async fn handle(
        _params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        let correlation_id = Uuid::new_v4();

        tracing::debug!(
            correlation_id = %correlation_id,
            "tendermint_consensusState request received"
        );

        let message = GetTendermintState {
            correlation_id: Some(correlation_id),
        };

        let state = chain_actor
            .send(message)
            .await
            .map_err(|e| RpcError::MailboxError(e.to_string()))?
            .map_err(RpcError::ChainError)?;

        // Convert internal response to RPC response format
        let response = ConsensusStateResponse {
            height: state.height,
            round: state.round,
            step: state.step.clone(),
            start_time: chrono::Utc::now().to_rfc3339(), // TODO: Track actual start time
            proposal_block_hash: state.proposal_block_hash.map(|h| format!("{:?}", h)),
            locked_block_hash: state.locked_block_hash.map(|h| format!("{:?}", h)),
            locked_round: state.locked_round,
            valid_block_hash: state.valid_block_hash.map(|h| format!("{:?}", h)),
            valid_round: state.valid_round,
            votes: VotesInfo {
                prevotes: Vec::new(), // TODO: Include individual votes when needed
                precommits: Vec::new(),
                prevotes_bit_array: format!("BA{{{}:{}}}", state.total_validators,
                    "x".repeat(state.prevotes_count.min(state.total_validators) as usize) +
                    &"_".repeat(state.total_validators.saturating_sub(state.prevotes_count) as usize)),
                precommits_bit_array: format!("BA{{{}:{}}}", state.total_validators,
                    "x".repeat(state.precommits_count.min(state.total_validators) as usize) +
                    &"_".repeat(state.total_validators.saturating_sub(state.precommits_count) as usize)),
            },
        };

        tracing::debug!(
            correlation_id = %correlation_id,
            height = state.height,
            round = state.round,
            step = %state.step,
            "tendermint_consensusState completed"
        );

        serde_json::to_value(&response)
            .map_err(|e| RpcError::Internal(format!("Serialization error: {}", e)))
    }
}

/// tendermint_validators RPC handler
pub struct ValidatorsHandler;

impl ValidatorsHandler {
    /// Handle tendermint_validators request
    ///
    /// Returns validator set for a given height (or current if not specified).
    /// Supports pagination via page and per_page parameters.
    pub async fn handle(
        params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        let correlation_id = Uuid::new_v4();

        // Parse optional parameters
        let height = params.get(0).and_then(|v| v.as_u64());
        let page = params.get(1).and_then(|v| v.as_u64()).unwrap_or(1) as u32;
        let per_page = params.get(2).and_then(|v| v.as_u64()).unwrap_or(30) as u32;

        tracing::debug!(
            correlation_id = %correlation_id,
            height = ?height,
            page = page,
            per_page = per_page,
            "tendermint_validators request received"
        );

        let message = GetValidatorSet {
            height,
            page,
            per_page,
            correlation_id: Some(correlation_id),
        };

        let result = chain_actor
            .send(message)
            .await
            .map_err(|e| RpcError::MailboxError(e.to_string()))?
            .map_err(RpcError::ChainError)?;

        // Convert to RPC format
        let validators: Vec<ValidatorInfo> = result
            .validators
            .iter()
            .enumerate()
            .map(|(i, v)| ValidatorInfo {
                address: v.address.clone(),
                pub_key: PubKeyInfo {
                    type_: "bls12-381".to_string(),
                    value: v.public_key.clone(),
                },
                voting_power: v.voting_power,
                proposer_priority: i as i64, // Simplified priority based on index
            })
            .collect();

        let response = ValidatorsResponse {
            block_height: result.height,
            validators,
            count: result.count,
            total: result.total,
        };

        tracing::debug!(
            correlation_id = %correlation_id,
            height = result.height,
            count = result.count,
            "tendermint_validators completed"
        );

        serde_json::to_value(&response)
            .map_err(|e| RpcError::Internal(format!("Serialization error: {}", e)))
    }
}

/// tendermint_commit RPC handler
pub struct CommitHandler;

impl CommitHandler {
    /// Handle tendermint_commit request
    ///
    /// Returns commit proof for a given height (or latest if not specified).
    pub async fn handle(
        params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        let correlation_id = Uuid::new_v4();

        // Parse optional height parameter
        let height = params.get(0).and_then(|v| v.as_u64());

        tracing::debug!(
            correlation_id = %correlation_id,
            height = ?height,
            "tendermint_commit request received"
        );

        let message = GetCommit {
            height,
            correlation_id: Some(correlation_id),
        };

        let result = chain_actor
            .send(message)
            .await
            .map_err(|e| RpcError::MailboxError(e.to_string()))?
            .map_err(RpcError::ChainError)?;

        // Build full commit response with signed_header structure
        let signatures_json: Vec<Value> = result.signatures.iter().map(|s| {
            json!({
                "block_id_flag": s.block_id_flag,
                "validator_address": s.validator_address,
                "timestamp": s.timestamp,
                "signature": s.signature
            })
        }).collect();

        let response = json!({
            "signed_header": {
                "header": {
                    "height": result.height,
                    "hash": format!("{:?}", result.block_hash),
                    "parent_hash": format!("{:?}", result.parent_hash),
                    "timestamp": result.timestamp,
                    "proposer_index": result.proposer_index,
                    "last_commit_hash": result.last_commit_hash.map(|h| format!("{:?}", h))
                },
                "commit": {
                    "height": result.height,
                    "round": result.round,
                    "block_id": {
                        "hash": format!("{:?}", result.block_hash)
                    },
                    "signatures": signatures_json
                }
            },
            "canonical": result.canonical,
            "commit_available": result.commit_available
        });

        tracing::debug!(
            correlation_id = %correlation_id,
            height = result.height,
            "tendermint_commit completed"
        );

        Ok(response)
    }
}

/// tendermint_params RPC handler
pub struct ParamsHandler;

impl ParamsHandler {
    /// Handle tendermint_params request
    ///
    /// Returns consensus and governance parameters.
    pub async fn handle(
        _params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        let correlation_id = Uuid::new_v4();

        tracing::debug!(
            correlation_id = %correlation_id,
            "tendermint_params request received"
        );

        let message = GetChainParams {
            correlation_id: Some(correlation_id),
        };

        let result = chain_actor
            .send(message)
            .await
            .map_err(|e| RpcError::MailboxError(e.to_string()))?
            .map_err(RpcError::ChainError)?;

        let response = json!({
            "block_height": result.height,
            "consensus_params": {
                "block": {
                    "max_bytes": result.max_block_bytes,
                    "max_gas": result.max_gas
                },
                "evidence": {
                    "max_age_num_blocks": result.evidence_max_age_blocks,
                    "max_age_duration_ms": 86400000_u64, // 24 hours default
                    "max_bytes": 1048576_u64 // 1MB default
                },
                "validator": {
                    "pub_key_types": ["bls12-381"]
                }
            },
            "governance_params": {
                "pegin_minimum_satoshis": result.pegin_minimum_satoshis,
                "pegin_confirmation_depth": result.pegin_confirmation_depth,
                "bridge_fee_rate_bps": result.miner_fee_bps,
                "emergency_pause_enabled": false
            }
        });

        tracing::debug!(
            correlation_id = %correlation_id,
            height = result.height,
            "tendermint_params completed"
        );

        Ok(response)
    }
}

/// tendermint_pendingGovernanceUpdates RPC handler
pub struct PendingGovernanceHandler;

impl PendingGovernanceHandler {
    /// Handle tendermint_pendingGovernanceUpdates request
    ///
    /// Returns list of pending governance updates awaiting activation.
    pub async fn handle(
        _params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        let correlation_id = Uuid::new_v4();

        tracing::debug!(
            correlation_id = %correlation_id,
            "tendermint_pendingGovernanceUpdates request received"
        );

        let message = GetPendingGovernance {
            correlation_id: Some(correlation_id),
        };

        let result = chain_actor
            .send(message)
            .await
            .map_err(|e| RpcError::MailboxError(e.to_string()))?
            .map_err(RpcError::ChainError)?;

        let response = json!({
            "updates": result.updates.iter().map(|u| json!({
                "update_type": u.update_type,
                "activation_height": u.activation_height,
                "proposed_at_height": u.proposed_at_height
            })).collect::<Vec<_>>()
        });

        tracing::debug!(
            correlation_id = %correlation_id,
            pending_count = result.updates.len(),
            "tendermint_pendingGovernanceUpdates completed"
        );

        Ok(response)
    }
}

/// tendermint_evidence RPC handler
pub struct EvidenceHandler;

impl EvidenceHandler {
    /// Handle tendermint_evidence request
    ///
    /// Returns list of detected equivocation evidence.
    /// Used by chaos testing to verify equivocation detection.
    ///
    /// # Parameters (optional)
    /// - params[0].max_age_blocks: Maximum age in blocks to include
    ///
    /// # Returns
    /// JSON object with:
    /// - evidence: Array of detected equivocation evidence
    /// - total: Total evidence count
    pub async fn handle(
        params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        let correlation_id = Uuid::new_v4();

        // Parse optional max_age_blocks parameter
        let max_age_blocks = params
            .get(0)
            .and_then(|v| v.as_object())
            .and_then(|o| o.get("max_age_blocks"))
            .and_then(|v| v.as_u64());

        tracing::debug!(
            correlation_id = %correlation_id,
            max_age_blocks = ?max_age_blocks,
            "tendermint_evidence request received"
        );

        let message = GetEvidence {
            max_age_blocks,
            correlation_id: Some(correlation_id),
        };

        let result = chain_actor
            .send(message)
            .await
            .map_err(|e| RpcError::MailboxError(e.to_string()))?
            .map_err(RpcError::ChainError)?;

        // Convert to RPC format
        let evidence_items: Vec<EvidenceItem> = result
            .evidence
            .iter()
            .map(|e| EvidenceItem {
                evidence_type: e.evidence_type.clone(),
                validator: ValidatorEvidenceInfo {
                    address: e.validator_address.clone(),
                    power: 1, // Default voting power (to be populated from state)
                },
                height: e.height,
                round: e.round,
                vote_a: EvidenceVoteInfo {
                    block_hash: e.vote_a_block_hash.clone(),
                    signature: String::new(), // Signature not exposed in summary
                    timestamp: e.detected_at.clone(),
                },
                vote_b: EvidenceVoteInfo {
                    block_hash: e.vote_b_block_hash.clone(),
                    signature: String::new(),
                    timestamp: e.detected_at.clone(),
                },
                detected_at: e.detected_at.clone(),
                total_voting_power: 1,
            })
            .collect();

        let response = EvidenceListResponse {
            evidence: evidence_items,
            total: result.total as u32,
        };

        tracing::debug!(
            correlation_id = %correlation_id,
            evidence_count = result.total,
            "tendermint_evidence completed"
        );

        serde_json::to_value(&response)
            .map_err(|e| RpcError::Internal(format!("Serialization error: {}", e)))
    }
}
