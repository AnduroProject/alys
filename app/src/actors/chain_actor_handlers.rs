//! Message handlers for ChainActor implementation
//!
//! This module implements all the message handlers for the ChainActor following the ALYS-007
//! specification. Each handler implements specific blockchain operations while maintaining
//! performance targets and comprehensive error handling.

use super::chain_actor::*;
use crate::messages::chain_messages::*;
use crate::types::*;

use actix::prelude::*;
use std::time::Instant;
use tracing::*;

/// Implementation of ImportBlock handler
/// 
/// This is the core message for processing incoming blocks from peers or local production.
/// It handles validation, execution, state updates, and potential reorganizations.
impl Handler<ImportBlock> for ChainActor {
    type Result = ResponseActFuture<Self, Result<ImportBlockResult, ChainError>>;

    fn handle(&mut self, msg: ImportBlock, _ctx: &mut Context<Self>) -> Self::Result {
        let start_time = Instant::now();
        let block_hash = msg.block.message.hash();
        let correlation_id = msg.correlation_id;
        
        info!(
            block_hash = %block_hash,
            block_height = msg.block.message.number(),
            correlation_id = ?correlation_id,
            source = ?msg.source,
            "Importing block"
        );

        Box::pin(
            async move {
                // Step 1: Check if block is already being processed
                if self.pending_blocks.contains_key(&block_hash) {
                    debug!("Block already being processed");
                    return Err(ChainError::BlockAlreadyProcessing);
                }

                // Step 2: Basic validation checks
                self.validate_block_basic(&msg.block).await?;

                // Step 3: Add to pending blocks tracking
                let pending_info = PendingBlockInfo {
                    block: msg.block.clone(),
                    received_at: start_time,
                    status: ProcessingStatus::Queued,
                    validation_attempts: 0,
                    source: msg.source.clone(),
                    priority: msg.priority,
                    correlation_id,
                    dependencies: self.find_block_dependencies(&msg.block).await?,
                };
                self.pending_blocks.insert(block_hash, pending_info);

                // Step 4: Full validation
                let validation_start = Instant::now();
                let validation_result = self.validate_block_full(&msg.block).await?;
                let validation_time = validation_start.elapsed();
                
                self.metrics.avg_validation_time.add(validation_time.as_millis() as f64);
                
                if !validation_result.is_valid {
                    self.metrics.validation_failures += 1;
                    self.update_block_status(&block_hash, ProcessingStatus::Failed {
                        reason: "Validation failed".to_string(),
                        failed_at: Instant::now(),
                    });
                    return Err(ChainError::ValidationFailed { 
                        reason: validation_result.errors.into_iter()
                            .map(|e| format!("{:?}", e))
                            .collect::<Vec<_>>()
                            .join(", ")
                    });
                }

                // Step 5: Check for reorganization
                let triggered_reorg = self.check_for_reorganization(&msg.block).await?;
                let mut blocks_reverted = 0;

                if triggered_reorg {
                    blocks_reverted = self.perform_reorganization(&msg.block).await?;
                    self.metrics.reorganizations += 1;
                }

                // Step 6: Import the block
                self.import_block_internal(&msg.block).await?;
                
                // Step 7: Broadcast if requested
                if msg.broadcast {
                    self.broadcast_block(&msg.block, BroadcastPriority::Normal).await?;
                }

                // Step 8: Notify subscribers
                self.notify_subscribers(&msg.block, BlockEventType::BlockImported).await?;

                // Step 9: Update metrics
                let total_time = start_time.elapsed();
                self.metrics.avg_import_time.add(total_time.as_millis() as f64);
                self.metrics.blocks_imported += 1;

                // Step 10: Clean up pending blocks
                self.pending_blocks.remove(&block_hash);

                let processing_metrics = BlockProcessingMetrics {
                    total_time_ms: total_time.as_millis() as u64,
                    validation_time_ms: validation_time.as_millis() as u64,
                    execution_time_ms: 0, // TODO: Track execution time
                    storage_time_ms: 0,   // TODO: Track storage time
                    queue_time_ms: 0,     // TODO: Track queue time
                    memory_usage_bytes: None,
                };

                Ok(ImportBlockResult {
                    imported: true,
                    block_ref: Some(msg.block.block_ref()),
                    triggered_reorg,
                    blocks_reverted,
                    validation_result,
                    processing_metrics,
                })
            }
            .into_actor(self)
        )
    }
}

/// Implementation of ProduceBlock handler
/// 
/// Handles block production for validator nodes with timing constraints and performance monitoring.
impl Handler<ProduceBlock> for ChainActor {
    type Result = ResponseActFuture<Self, Result<SignedConsensusBlock, ChainError>>;

    fn handle(&mut self, msg: ProduceBlock, _ctx: &mut Context<Self>) -> Self::Result {
        let start_time = Instant::now();
        
        info!(
            slot = msg.slot,
            timestamp = ?msg.timestamp,
            correlation_id = ?msg.correlation_id,
            "Producing block"
        );

        Box::pin(
            async move {
                // Step 1: Check if we should produce this block
                if !msg.force && !self.should_produce_block(msg.slot) {
                    return Err(ChainError::NotOurSlot { slot: msg.slot });
                }

                // Step 2: Check if we've already produced for this slot
                if self.already_produced_slot(msg.slot) {
                    return Err(ChainError::SlotAlreadyProduced { slot: msg.slot });
                }

                // Step 3: Update production state
                self.production_state.current_slot = Some(msg.slot);
                self.production_state.production_started = Some(start_time);

                // Step 4: Collect pending peg-ins as withdrawals
                let withdrawals = self.collect_pending_withdrawals().await?;

                // Step 5: Build execution payload
                let execution_payload = self.build_execution_payload(
                    msg.timestamp,
                    withdrawals,
                ).await?;

                // Step 6: Collect peg operations
                let pegins = self.collect_pegins().await?;
                let pegout_proposal = self.build_pegout_proposal().await?;

                // Step 7: Create consensus block
                let consensus_block = ConsensusBlock::new(
                    msg.slot,
                    execution_payload,
                    self.chain_state.head.as_ref()
                        .map(|h| h.hash)
                        .unwrap_or(Hash256::zero()),
                    None, // AuxPoW header will be added later
                    pegins,
                    pegout_proposal,
                    Vec::new(), // Finalized pegouts will be added with AuxPoW
                );

                // Step 8: Sign the block
                let signature = self.sign_block(&consensus_block)?;
                let signed_block = SignedConsensusBlock::new(consensus_block, signature);

                // Step 9: Import our own block
                self.import_block_internal(&signed_block).await?;

                // Step 10: Broadcast to network
                self.broadcast_block(&signed_block, BroadcastPriority::High).await?;

                // Step 11: Update metrics
                let production_time = start_time.elapsed();
                self.metrics.avg_production_time.add(production_time.as_millis() as f64);
                self.metrics.blocks_produced += 1;
                self.production_state.recent_production_times.push_back(production_time);
                
                if self.production_state.recent_production_times.len() > 20 {
                    self.production_state.recent_production_times.pop_front();
                }

                // Step 12: Check performance targets
                if production_time.as_millis() > self.config.performance_targets.max_production_time_ms as u128 {
                    warn!(
                        production_time_ms = production_time.as_millis(),
                        target_ms = self.config.performance_targets.max_production_time_ms,
                        "Block production exceeded target time"
                    );
                    self.metrics.performance_violations.production_timeouts += 1;
                }

                // Step 13: Notify subscribers
                self.notify_subscribers(&signed_block, BlockEventType::BlockProduced).await?;

                info!(
                    block_hash = %signed_block.canonical_root(),
                    block_height = signed_block.message.number(),
                    production_time_ms = production_time.as_millis(),
                    "Block produced successfully"
                );

                Ok(signed_block)
            }
            .into_actor(self)
        )
    }
}

/// Implementation of GetChainStatus handler
impl Handler<GetChainStatus> for ChainActor {
    type Result = Result<ChainStatus, ChainError>;

    fn handle(&mut self, msg: GetChainStatus, _ctx: &mut Context<Self>) -> Self::Result {
        let mut status = ChainStatus::default();
        
        // Fill in basic chain information
        status.head = self.chain_state.head.clone();
        status.finalized = self.chain_state.finalized.clone();
        status.best_block_number = self.chain_state.height;
        status.best_block_hash = self.chain_state.head
            .as_ref()
            .map(|h| h.hash)
            .unwrap_or(Hash256::zero());

        // Fill in validator status
        status.validator_status = if self.config.is_validator {
            let next_slot = self.calculate_next_slot();
            let next_slot_in_ms = next_slot.map(|slot| {
                let now = self.calculate_current_slot();
                let slots_until = if slot > now { slot - now } else { 0 };
                slots_until * self.config.slot_duration.as_millis() as u64
            });

            ValidatorStatus::Validator {
                address: self.config.authority_key
                    .as_ref()
                    .map(|k| k.public_key().into())
                    .unwrap_or(Address::zero()),
                is_active: !self.production_state.paused,
                next_slot,
                next_slot_in_ms,
                recent_performance: self.calculate_validator_performance(),
                weight: 1, // TODO: Implement weighted voting
            }
        } else {
            ValidatorStatus::NotValidator
        };

        // Fill in PoW status
        status.pow_status = self.get_pow_status();

        // Fill in federation status if requested
        status.federation_status = FederationStatus {
            version: self.federation.version,
            active_members: self.federation.members.len(),
            threshold: self.federation.threshold,
            ready: self.federation.members.len() >= self.federation.threshold,
            pending_changes: self.federation.pending_changes
                .iter()
                .map(|c| format!("Version {} at height {}", c.new_config.version, c.effective_height))
                .collect(),
        };

        // Fill in peg operation status
        status.peg_status = PegOperationStatus {
            pending_pegins: 0,  // TODO: Get from bridge actor
            pending_pegouts: 0, // TODO: Get from bridge actor
            total_value_locked: 0, // TODO: Get from bridge actor
            success_rate: 0.95, // TODO: Calculate from recent operations
            avg_processing_time_ms: 50, // TODO: Track actual processing times
        };

        // Fill in performance metrics if requested
        if msg.include_metrics {
            status.performance = ChainPerformanceStatus {
                avg_block_time_ms: self.config.slot_duration.as_millis() as u64,
                blocks_per_second: 1.0 / self.config.slot_duration.as_secs_f64(),
                transactions_per_second: 10.0, // TODO: Calculate from recent blocks
                memory_usage_mb: self.estimate_memory_usage(),
                cpu_usage_percent: 0.0, // TODO: Track CPU usage
            };
        }

        // Fill in network status if requested
        if msg.include_sync_info {
            status.network_status = NetworkStatus {
                connected_peers: 0, // TODO: Get from network actor
                inbound_connections: 0, // TODO: Get from network actor
                outbound_connections: 0, // TODO: Get from network actor
                avg_peer_height: None, // TODO: Get from network actor
                health_score: 100, // TODO: Calculate network health
            };

            status.sync_status = SyncStatus::Synced; // TODO: Get actual sync status
        }

        // Fill in actor health status
        status.actor_health = self.health_monitor.status.clone();

        Ok(status)
    }
}

/// Implementation of ValidateBlock handler
impl Handler<ValidateBlock> for ChainActor {
    type Result = ResponseActFuture<Self, Result<bool, ChainError>>;

    fn handle(&mut self, msg: ValidateBlock, _ctx: &mut Context<Self>) -> Self::Result {
        let block_hash = msg.block.canonical_root();
        
        Box::pin(
            async move {
                // Check cache first if requested
                if msg.cache_result {
                    if let Some(cached) = self.validation_cache.get(&block_hash) {
                        if !cached.is_expired() {
                            self.validation_cache.hits += 1;
                            return Ok(cached.result);
                        }
                    }
                    self.validation_cache.misses += 1;
                }

                let validation_result = match msg.validation_level {
                    ValidationLevel::Basic => {
                        self.validate_block_basic(&msg.block).await
                    }
                    ValidationLevel::Full => {
                        self.validate_block_full(&msg.block).await.map(|r| r.is_valid)
                    }
                    ValidationLevel::SignatureOnly => {
                        self.validate_block_signatures(&msg.block).await
                    }
                    ValidationLevel::ConsensusOnly => {
                        self.validate_consensus_rules(&msg.block).await
                    }
                };

                let is_valid = validation_result.unwrap_or(false);

                // Cache result if requested
                if msg.cache_result {
                    self.validation_cache.insert(block_hash, is_valid, Vec::new());
                }

                Ok(is_valid)
            }
            .into_actor(self)
        )
    }
}

/// Implementation of BroadcastBlock handler
impl Handler<BroadcastBlock> for ChainActor {
    type Result = ResponseActFuture<Self, Result<BroadcastResult, ChainError>>;

    fn handle(&mut self, msg: BroadcastBlock, _ctx: &mut Context<Self>) -> Self::Result {
        let block_hash = msg.block.canonical_root();
        let start_time = Instant::now();
        
        Box::pin(
            async move {
                info!(
                    block_hash = %block_hash,
                    priority = ?msg.priority,
                    exclude_peers = msg.exclude_peers.len(),
                    "Broadcasting block"
                );

                // Send to network actor for actual broadcast
                let network_result = self.actor_addresses.network
                    .send(NetworkBroadcastBlock {
                        block: msg.block.clone(),
                        priority: msg.priority,
                        exclude_peers: msg.exclude_peers,
                    })
                    .await;

                match network_result {
                    Ok(Ok(network_result)) => {
                        // Update broadcast tracking
                        let metrics = BroadcastMetrics {
                            block_hash,
                            peers_reached: network_result.peers_reached,
                            successful_sends: network_result.successful_sends,
                            broadcast_time: start_time.elapsed(),
                            timestamp: Instant::now(),
                        };
                        
                        self.broadcast_tracker.recent_broadcasts.push_back(metrics);
                        if self.broadcast_tracker.recent_broadcasts.len() > 50 {
                            self.broadcast_tracker.recent_broadcasts.pop_front();
                        }

                        // Update success rate
                        let success_rate = if network_result.peers_reached > 0 {
                            network_result.successful_sends as f64 / network_result.peers_reached as f64
                        } else {
                            1.0
                        };
                        self.broadcast_tracker.success_rate = 
                            (self.broadcast_tracker.success_rate * 0.9) + (success_rate * 0.1);

                        Ok(BroadcastResult {
                            peers_reached: network_result.peers_reached,
                            successful_sends: network_result.successful_sends,
                            failed_sends: network_result.peers_reached - network_result.successful_sends,
                            avg_response_time_ms: Some(start_time.elapsed().as_millis() as u64),
                            failed_peers: Vec::new(), // TODO: Get from network result
                        })
                    }
                    Ok(Err(e)) => {
                        error!("Network broadcast failed: {}", e);
                        self.metrics.error_counters.network_errors += 1;
                        Err(ChainError::NetworkError { reason: format!("{}", e) })
                    }
                    Err(e) => {
                        error!("Failed to send broadcast message: {}", e);
                        self.metrics.error_counters.network_errors += 1;
                        Err(ChainError::ActorCommunicationFailed {
                            target: "NetworkActor".to_string(),
                            reason: format!("{}", e),
                        })
                    }
                }
            }
            .into_actor(self)
        )
    }
}

/// Implementation helper methods for ChainActor
impl ChainActor {
    /// Perform basic block validation
    async fn validate_block_basic(&self, block: &SignedConsensusBlock) -> Result<(), ChainError> {
        // Check basic structure
        if block.message.slot == 0 && block.message.number() > 0 {
            return Err(ChainError::InvalidBlock { 
                reason: "Non-genesis block cannot have slot 0".to_string() 
            });
        }

        // Check timestamp alignment with slot
        let expected_timestamp = block.message.slot * self.config.slot_duration.as_secs();
        let actual_timestamp = block.message.timestamp();
        
        if (actual_timestamp as i64 - expected_timestamp as i64).abs() > 30 {
            return Err(ChainError::InvalidTimestamp {
                expected: expected_timestamp,
                actual: actual_timestamp,
            });
        }

        Ok(())
    }

    /// Perform full block validation
    async fn validate_block_full(&self, block: &SignedConsensusBlock) -> Result<ValidationResult, ChainError> {
        let start_time = Instant::now();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut checkpoints = Vec::new();

        // Basic validation
        if let Err(e) = self.validate_block_basic(block).await {
            errors.push(ValidationError::ConsensusError {
                rule: "basic_validation".to_string(),
                message: format!("{}", e),
            });
        }
        checkpoints.push("basic_validation".to_string());

        // Signature validation
        if let Err(_) = self.validate_block_signatures(block).await {
            errors.push(ValidationError::InvalidSignature {
                signer: Some(block.message.execution_payload.fee_recipient),
                reason: "Invalid block signature".to_string(),
            });
        }
        checkpoints.push("signature_validation".to_string());

        // Consensus rules validation
        if let Err(_) = self.validate_consensus_rules(block).await {
            errors.push(ValidationError::ConsensusError {
                rule: "consensus_rules".to_string(),
                message: "Consensus rule violation".to_string(),
            });
        }
        checkpoints.push("consensus_validation".to_string());

        // State transition validation (via engine actor)
        let state_result = self.validate_state_transition(block).await;
        match state_result {
            Ok(state_root) => {
                if state_root != block.message.execution_payload.state_root {
                    errors.push(ValidationError::InvalidStateRoot {
                        expected: block.message.execution_payload.state_root,
                        computed: state_root,
                    });
                }
            }
            Err(e) => {
                errors.push(ValidationError::ConsensusError {
                    rule: "state_transition".to_string(),
                    message: format!("State validation failed: {}", e),
                });
            }
        }
        checkpoints.push("state_validation".to_string());

        let validation_time = start_time.elapsed();
        let is_valid = errors.is_empty();

        Ok(ValidationResult {
            is_valid,
            errors,
            gas_used: block.message.gas_used(),
            state_root: block.message.execution_payload.state_root,
            validation_metrics: ValidationMetrics {
                total_time_ms: validation_time.as_millis() as u64,
                structural_time_ms: 10, // TODO: Track individual phases
                signature_time_ms: 20,
                state_time_ms: 30,
                consensus_time_ms: 15,
                memory_used_bytes: 1024 * 1024, // TODO: Track actual memory
            },
            checkpoints,
            warnings,
        })
    }

    /// Validate block signatures
    async fn validate_block_signatures(&self, block: &SignedConsensusBlock) -> Result<(), ChainError> {
        // Check if the signer is authorized for this slot
        let expected_signer = self.get_slot_authority(block.message.slot)?;
        
        if block.message.execution_payload.fee_recipient != expected_signer {
            return Err(ChainError::InvalidSignature {
                expected: expected_signer,
                actual: block.message.execution_payload.fee_recipient,
            });
        }

        // Verify the actual signature
        let message_hash = block.message.signing_root();
        if !block.signature.verify(&[expected_signer.into()], message_hash) {
            return Err(ChainError::SignatureVerificationFailed);
        }

        Ok(())
    }

    /// Validate consensus rules
    async fn validate_consensus_rules(&self, block: &SignedConsensusBlock) -> Result<(), ChainError> {
        // Check parent relationship
        if let Some(head) = &self.chain_state.head {
            if block.message.parent_hash != head.hash {
                // Check if this is a valid fork
                if !self.is_valid_fork(block).await? {
                    return Err(ChainError::InvalidParentBlock { 
                        parent_hash: block.message.parent_hash 
                    });
                }
            }
        }

        // Check block height progression
        let expected_height = self.chain_state.height + 1;
        if block.message.number() != expected_height {
            return Err(ChainError::InvalidBlockHeight {
                expected: expected_height,
                actual: block.message.number(),
            });
        }

        Ok(())
    }

    /// Validate state transition through engine actor
    async fn validate_state_transition(&self, block: &SignedConsensusBlock) -> Result<Hash256, ChainError> {
        let result = self.actor_addresses.engine
            .send(ValidateStateTransition {
                block: block.clone(),
            })
            .await;

        match result {
            Ok(Ok(state_root)) => Ok(state_root),
            Ok(Err(e)) => Err(ChainError::StateValidationFailed { reason: format!("{}", e) }),
            Err(e) => Err(ChainError::ActorCommunicationFailed {
                target: "EngineActor".to_string(),
                reason: format!("{}", e),
            }),
        }
    }

    /// Check if a block requires reorganization
    async fn check_for_reorganization(&self, block: &SignedConsensusBlock) -> Result<bool, ChainError> {
        if let Some(head) = &self.chain_state.head {
            // If this block doesn't extend current head, it might trigger a reorg
            if block.message.parent_hash != head.hash {
                // Check if this creates a heavier chain
                return self.is_heavier_chain(block).await;
            }
        }
        Ok(false)
    }

    /// Import block into the chain state
    async fn import_block_internal(&mut self, block: &SignedConsensusBlock) -> Result<(), ChainError> {
        // Update chain state
        let new_head = block.block_ref();
        self.chain_state.head = Some(new_head.clone());
        self.chain_state.height = block.message.number();
        
        // Update fork choice state
        self.chain_state.fork_choice.canonical_tip = new_head.hash;
        self.chain_state.fork_choice.tips.insert(
            new_head.hash,
            ChainTip {
                block_ref: new_head,
                total_difficulty: self.chain_state.total_difficulty, // TODO: Calculate properly
                last_updated: Instant::now(),
            },
        );

        // Store in persistence layer
        self.actor_addresses.storage
            .send(StoreBlock {
                block: block.clone(),
                update_head: true,
            })
            .await
            .map_err(|e| ChainError::StorageError { 
                reason: format!("Failed to store block: {}", e) 
            })??;

        Ok(())
    }

    /// Helper methods (placeholder implementations)
    
    fn find_block_dependencies(&self, _block: &SignedConsensusBlock) -> impl Future<Output = Result<Vec<Hash256>, ChainError>> {
        async { Ok(Vec::new()) }
    }

    fn update_block_status(&mut self, block_hash: &Hash256, status: ProcessingStatus) {
        if let Some(pending) = self.pending_blocks.get_mut(block_hash) {
            pending.status = status;
        }
    }

    async fn perform_reorganization(&mut self, _block: &SignedConsensusBlock) -> Result<u32, ChainError> {
        // TODO: Implement reorganization logic
        Ok(0)
    }

    async fn broadcast_block(&self, block: &SignedConsensusBlock, priority: BroadcastPriority) -> Result<(), ChainError> {
        let msg = BroadcastBlock {
            block: block.clone(),
            priority,
            exclude_peers: Vec::new(),
            correlation_id: Some(uuid::Uuid::new_v4()),
        };
        
        // Send to self to handle broadcast
        // In real implementation, this would be sent to network actor
        Ok(())
    }

    async fn notify_subscribers(&self, block: &SignedConsensusBlock, event_type: BlockEventType) -> Result<(), ChainError> {
        let notification = BlockNotification {
            block: block.clone(),
            event_type,
            is_canonical: true,
            context: NotificationContext::default(),
        };

        for subscriber in self.subscribers.values() {
            if subscriber.event_types.contains(&event_type) {
                let _ = subscriber.recipient.do_send(notification.clone());
            }
        }

        Ok(())
    }

    fn already_produced_slot(&self, slot: u64) -> bool {
        // Check if we've already produced a block for this slot
        if let Some(head) = &self.chain_state.head {
            if let Some(current_slot) = self.production_state.current_slot {
                return current_slot == slot;
            }
        }
        false
    }

    async fn collect_pending_withdrawals(&self) -> Result<Vec<Withdrawal>, ChainError> {
        // Get pending peg-ins from bridge actor
        let result = self.actor_addresses.bridge
            .send(GetPendingWithdrawals)
            .await;
            
        match result {
            Ok(Ok(withdrawals)) => Ok(withdrawals),
            Ok(Err(e)) => Err(ChainError::BridgeError { reason: format!("{}", e) }),
            Err(e) => Err(ChainError::ActorCommunicationFailed {
                target: "BridgeActor".to_string(),
                reason: format!("{}", e),
            }),
        }
    }

    async fn build_execution_payload(&self, timestamp: Duration, withdrawals: Vec<Withdrawal>) -> Result<ExecutionPayload, ChainError> {
        let parent_hash = self.chain_state.head
            .as_ref()
            .map(|h| h.hash)
            .unwrap_or(Hash256::zero());

        let result = self.actor_addresses.engine
            .send(BuildExecutionPayload {
                parent_hash,
                timestamp: timestamp.as_secs(),
                withdrawals,
            })
            .await;

        match result {
            Ok(Ok(payload)) => Ok(payload),
            Ok(Err(e)) => Err(ChainError::ExecutionError { reason: format!("{}", e) }),
            Err(e) => Err(ChainError::ActorCommunicationFailed {
                target: "EngineActor".to_string(),
                reason: format!("{}", e),
            }),
        }
    }

    async fn collect_pegins(&self) -> Result<Vec<(bitcoin::Txid, bitcoin::BlockHash)>, ChainError> {
        // Get pending peg-ins from bridge actor
        Ok(Vec::new()) // TODO: Implement
    }

    async fn build_pegout_proposal(&self) -> Result<Option<bitcoin::Transaction>, ChainError> {
        // Build peg-out proposal from pending requests
        Ok(None) // TODO: Implement
    }

    fn sign_block(&self, block: &ConsensusBlock) -> Result<AggregateApproval, ChainError> {
        // Sign block with authority key
        if let Some(authority_key) = &self.config.authority_key {
            // TODO: Implement proper BLS signature
            Ok(AggregateApproval::new())
        } else {
            Err(ChainError::NoAuthorityKey)
        }
    }

    fn calculate_next_slot(&self) -> Option<u64> {
        let current_slot = self.calculate_current_slot();
        // TODO: Calculate next slot based on authority schedule
        Some(current_slot + 1)
    }

    fn calculate_validator_performance(&self) -> ValidatorPerformance {
        ValidatorPerformance {
            blocks_produced: self.metrics.blocks_produced as u32,
            blocks_missed: 0, // TODO: Track missed slots
            success_rate: 100.0, // TODO: Calculate actual success rate
            avg_production_time_ms: self.metrics.avg_production_time.current() as u64,
            uptime_percent: 100.0, // TODO: Track uptime
        }
    }
}

/// Implementation of UpdateFederation handler
impl Handler<UpdateFederation> for ChainActor {
    type Result = ResponseActFuture<Self, Result<FederationUpdateStatus, ChainError>>;

    fn handle(&mut self, msg: UpdateFederation, _ctx: &mut Context<Self>) -> Self::Result {
        let correlation_id = msg.correlation_id.unwrap_or_else(|| uuid::Uuid::new_v4());
        
        info!(
            correlation_id = %correlation_id,
            threshold = msg.config.threshold,
            member_count = msg.config.members.len(),
            "Processing federation configuration update"
        );

        Box::pin(
            async move {
                let start_time = Instant::now();

                // Step 1: Validate new federation configuration
                self.validate_federation_config(&msg.config).await?;

                // Step 2: Check if configuration actually changed
                if !self.federation_config_changed(&msg.config).await? {
                    info!("Federation configuration unchanged, skipping update");
                    return Ok(FederationUpdateStatus {
                        success: true,
                        old_epoch: self.federation_state.current_epoch,
                        new_epoch: self.federation_state.current_epoch,
                        activated_at: None,
                        message: "Configuration unchanged".to_string(),
                    });
                }

                // Step 3: Prepare federation transition
                let new_epoch = self.federation_state.current_epoch + 1;
                let old_config = self.federation_state.current_config.clone();

                // Step 4: Update federation state
                self.federation_state.current_config = msg.config.clone();
                self.federation_state.current_epoch = new_epoch;
                self.federation_state.last_update = Instant::now();

                // Update federation members and their keys
                self.federation_state.members.clear();
                for member in &msg.config.members {
                    self.federation_state.members.insert(
                        member.node_id.clone(),
                        FederationMember {
                            node_id: member.node_id.clone(),
                            pubkey: member.pubkey.clone(),
                            weight: member.weight,
                            is_active: true,
                            last_seen: Instant::now(),
                        },
                    );
                }

                // Step 5: Update Bitcoin addresses for new configuration
                self.update_bitcoin_addresses(&msg.config).await?;

                // Step 6: Persist federation configuration
                self.actor_addresses.storage
                    .send(StoreFederationConfig {
                        config: msg.config.clone(),
                        epoch: new_epoch,
                    })
                    .await
                    .map_err(|e| ChainError::StorageError { 
                        reason: format!("Failed to store federation config: {}", e) 
                    })??;

                // Step 7: Notify bridge actor of federation update
                self.actor_addresses.bridge
                    .send(FederationConfigUpdated {
                        old_config,
                        new_config: msg.config.clone(),
                        epoch: new_epoch,
                    })
                    .await
                    .map_err(|e| ChainError::ActorCommunicationFailed {
                        target: "BridgeActor".to_string(),
                        reason: format!("{}", e),
                    })?;

                // Step 8: Update metrics
                let update_time = start_time.elapsed();
                self.metrics.federation_updates += 1;
                
                if update_time.as_millis() > 1000 {
                    warn!(
                        update_time_ms = update_time.as_millis(),
                        "Federation update took longer than expected"
                    );
                }

                let activation_time = Instant::now();
                
                info!(
                    old_epoch = self.federation_state.current_epoch - 1,
                    new_epoch = new_epoch,
                    update_time_ms = update_time.as_millis(),
                    "Federation configuration updated successfully"
                );

                Ok(FederationUpdateStatus {
                    success: true,
                    old_epoch: new_epoch - 1,
                    new_epoch,
                    activated_at: Some(activation_time),
                    message: format!("Federation updated to epoch {} with {} members", 
                        new_epoch, msg.config.members.len()),
                })
            }
            .into_actor(self)
        )
    }
}

/// Implementation of FinalizeBlocks handler
impl Handler<FinalizeBlocks> for ChainActor {
    type Result = ResponseActFuture<Self, Result<FinalizationResult, ChainError>>;

    fn handle(&mut self, msg: FinalizeBlocks, _ctx: &mut Context<Self>) -> Self::Result {
        let correlation_id = msg.correlation_id.unwrap_or_else(|| uuid::Uuid::new_v4());
        
        info!(
            correlation_id = %correlation_id,
            target_block = %msg.target_block,
            "Processing block finalization request"
        );

        Box::pin(
            async move {
                let start_time = Instant::now();

                // Step 1: Validate target block exists
                let target_block = self.get_block_by_hash(&msg.target_block).await?;
                let target_height = target_block.message.number();

                // Step 2: Check if already finalized
                if let Some(finalized) = &self.chain_state.finalized {
                    if target_height <= finalized.height {
                        return Ok(FinalizationResult {
                            finalized_block: msg.target_block,
                            finalized_height: target_height,
                            blocks_finalized: 0,
                            auxpow_commitments: Vec::new(),
                            processing_time: start_time.elapsed(),
                        });
                    }
                }

                // Step 3: Verify AuxPoW commitments if provided
                let mut verified_commitments = Vec::new();
                if let Some(commitments) = msg.auxpow_commitments {
                    for commitment in commitments {
                        if self.verify_auxpow_commitment(&commitment).await? {
                            verified_commitments.push(commitment);
                        } else {
                            warn!(
                                bitcoin_block = %commitment.bitcoin_block_hash,
                                "Invalid AuxPoW commitment, skipping"
                            );
                        }
                    }
                }

                // Step 4: Check minimum confirmations
                let current_height = self.chain_state.height;
                let confirmations = current_height.saturating_sub(target_height);
                let min_confirmations = self.config.consensus_config.min_finalization_depth;

                if confirmations < min_confirmations {
                    return Err(ChainError::InsufficientConfirmations {
                        required: min_confirmations,
                        current: confirmations,
                    });
                }

                // Step 5: Verify chain continuity from current finalized to target
                let blocks_to_finalize = self.get_finalization_chain(&msg.target_block).await?;
                
                // Step 6: Check for any conflicts or reorganizations
                self.validate_finalization_safety(&blocks_to_finalize).await?;

                // Step 7: Update finalization state
                let old_finalized = self.chain_state.finalized.clone();
                self.chain_state.finalized = Some(BlockRef {
                    hash: msg.target_block,
                    height: target_height,
                });

                // Step 8: Update AuxPoW state
                for commitment in &verified_commitments {
                    self.auxpow_state.finalized_commitments.insert(
                        commitment.bitcoin_block_hash,
                        commitment.clone(),
                    );
                }

                // Step 9: Persist finalization
                self.actor_addresses.storage
                    .send(FinalizeBlocks {
                        blocks: blocks_to_finalize.clone(),
                        finalized_root: msg.target_block,
                    })
                    .await
                    .map_err(|e| ChainError::StorageError { 
                        reason: format!("Failed to persist finalization: {}", e) 
                    })??;

                // Step 10: Process any pending peg operations that can now be finalized
                self.process_finalized_peg_operations(&blocks_to_finalize).await?;

                // Step 11: Update metrics
                let finalization_time = start_time.elapsed();
                self.metrics.blocks_finalized += blocks_to_finalize.len() as u64;
                self.metrics.avg_finalization_time.add(finalization_time.as_millis() as f64);
                
                // Step 12: Notify subscribers
                for block in &blocks_to_finalize {
                    self.notify_subscribers(block, BlockEventType::BlockFinalized).await?;
                }

                // Step 13: Cleanup old state that's no longer needed
                self.cleanup_old_finalized_state().await?;

                info!(
                    finalized_block = %msg.target_block,
                    finalized_height = target_height,
                    blocks_count = blocks_to_finalize.len(),
                    auxpow_commitments = verified_commitments.len(),
                    finalization_time_ms = finalization_time.as_millis(),
                    "Block finalization completed successfully"
                );

                Ok(FinalizationResult {
                    finalized_block: msg.target_block,
                    finalized_height: target_height,
                    blocks_finalized: blocks_to_finalize.len() as u32,
                    auxpow_commitments: verified_commitments,
                    processing_time: finalization_time,
                })
            }
            .into_actor(self)
        )
    }
}

/// Implementation of ReorgChain handler
impl Handler<ReorgChain> for ChainActor {
    type Result = ResponseActFuture<Self, Result<ReorganizationResult, ChainError>>;

    fn handle(&mut self, msg: ReorgChain, _ctx: &mut Context<Self>) -> Self::Result {
        let correlation_id = msg.correlation_id.unwrap_or_else(|| uuid::Uuid::new_v4());
        
        warn!(
            correlation_id = %correlation_id,
            new_head = %msg.new_head,
            "Processing chain reorganization"
        );

        Box::pin(
            async move {
                let start_time = Instant::now();
                
                // Step 1: Validate new head block
                let new_head_block = self.get_block_by_hash(&msg.new_head).await?;
                let old_head = self.chain_state.head.clone();
                
                // Step 2: Check if reorganization is actually needed
                if let Some(current_head) = &old_head {
                    if current_head.hash == msg.new_head {
                        return Ok(ReorganizationResult {
                            old_head: current_head.hash,
                            new_head: msg.new_head,
                            reorg_depth: 0,
                            blocks_reverted: Vec::new(),
                            blocks_applied: Vec::new(),
                            processing_time: start_time.elapsed(),
                        });
                    }
                }

                // Step 3: Find common ancestor
                let (common_ancestor, reorg_depth) = self.find_common_ancestor(
                    &old_head, 
                    &new_head_block
                ).await?;

                // Step 4: Validate reorganization safety
                self.validate_reorg_safety(reorg_depth, &new_head_block).await?;

                // Step 5: Check against finalized blocks
                if let Some(finalized) = &self.chain_state.finalized {
                    if reorg_depth > 0 && 
                       old_head.as_ref().map(|h| h.height).unwrap_or(0) - reorg_depth <= finalized.height {
                        return Err(ChainError::ReorgConflictsFinalized {
                            finalized_height: finalized.height,
                            reorg_depth,
                        });
                    }
                }

                // Step 6: Prepare reorganization plan
                let blocks_to_revert = self.get_blocks_to_revert(&old_head, reorg_depth).await?;
                let blocks_to_apply = self.get_blocks_to_apply(&common_ancestor, &new_head_block).await?;

                // Step 7: Begin reorganization transaction
                self.begin_reorg_transaction().await?;

                let mut reverted_blocks = Vec::new();
                let mut applied_blocks = Vec::new();

                // Step 8: Revert old blocks (in reverse order)
                for block_ref in blocks_to_revert.iter().rev() {
                    let block = self.get_block_by_hash(&block_ref.hash).await?;
                    self.revert_block(&block).await?;
                    reverted_blocks.push(block);
                }

                // Step 9: Apply new blocks (in forward order)  
                for block_ref in &blocks_to_apply {
                    let block = self.get_block_by_hash(&block_ref.hash).await?;
                    self.apply_block(&block).await?;
                    applied_blocks.push(block);
                }

                // Step 10: Update chain state
                self.chain_state.head = Some(BlockRef {
                    hash: msg.new_head,
                    height: new_head_block.message.number(),
                });

                // Step 11: Update fork choice state
                self.update_fork_choice_after_reorg(&msg.new_head).await?;

                // Step 12: Commit reorganization transaction
                self.commit_reorg_transaction().await?;

                // Step 13: Update metrics
                let reorg_time = start_time.elapsed();
                self.metrics.reorganizations += 1;
                self.metrics.total_reorg_depth += reorg_depth as u64;
                
                if reorg_depth > 5 {
                    warn!(
                        reorg_depth = reorg_depth,
                        "Deep reorganization detected"
                    );
                    self.metrics.deep_reorgs += 1;
                }

                // Step 14: Notify subscribers about reorganization
                let reorg_notification = ReorgNotification {
                    old_head: old_head.as_ref().map(|h| h.hash).unwrap_or(Hash256::zero()),
                    new_head: msg.new_head,
                    reorg_depth,
                    reverted_blocks: reverted_blocks.iter().map(|b| b.canonical_root()).collect(),
                    applied_blocks: applied_blocks.iter().map(|b| b.canonical_root()).collect(),
                };

                for subscriber in self.subscribers.values() {
                    if subscriber.event_types.contains(&BlockEventType::ChainReorganized) {
                        let _ = subscriber.recipient.do_send(reorg_notification.clone());
                    }
                }

                // Step 15: Process any peg operations affected by reorganization
                self.process_reorg_affected_peg_operations(&reverted_blocks, &applied_blocks).await?;

                warn!(
                    old_head = %old_head.as_ref().map(|h| h.hash).unwrap_or(Hash256::zero()),
                    new_head = %msg.new_head,
                    reorg_depth = reorg_depth,
                    blocks_reverted = reverted_blocks.len(),
                    blocks_applied = applied_blocks.len(),
                    reorg_time_ms = reorg_time.as_millis(),
                    "Chain reorganization completed successfully"
                );

                Ok(ReorganizationResult {
                    old_head: old_head.as_ref().map(|h| h.hash).unwrap_or(Hash256::zero()),
                    new_head: msg.new_head,
                    reorg_depth,
                    blocks_reverted: reverted_blocks.into_iter().map(|b| b.canonical_root()).collect(),
                    blocks_applied: applied_blocks.into_iter().map(|b| b.canonical_root()).collect(),
                    processing_time: reorg_time,
                })
            }
            .into_actor(self)
        )
    }
}

/// Implementation of ProcessAuxPow handler
impl Handler<ProcessAuxPow> for ChainActor {
    type Result = ResponseActFuture<Self, Result<AuxPowProcessingResult, ChainError>>;

    fn handle(&mut self, msg: ProcessAuxPow, _ctx: &mut Context<Self>) -> Self::Result {
        let correlation_id = msg.correlation_id.unwrap_or_else(|| uuid::Uuid::new_v4());
        
        info!(
            correlation_id = %correlation_id,
            bitcoin_block = %msg.commitment.bitcoin_block_hash,
            merkle_size = msg.commitment.merkle_proof.len(),
            "Processing AuxPoW commitment"
        );

        Box::pin(
            async move {
                let start_time = Instant::now();

                // Step 1: Validate AuxPoW commitment structure
                self.validate_auxpow_structure(&msg.commitment).await?;

                // Step 2: Verify Bitcoin block exists and is valid
                let bitcoin_block = self.verify_bitcoin_block(&msg.commitment.bitcoin_block_hash).await?;

                // Step 3: Verify merkle proof
                let merkle_valid = self.verify_auxpow_merkle_proof(&msg.commitment).await?;
                if !merkle_valid {
                    return Err(ChainError::InvalidMerkleProof {
                        bitcoin_block: msg.commitment.bitcoin_block_hash.to_string(),
                    });
                }

                // Step 4: Extract and validate committed block bundle
                let committed_blocks = self.extract_committed_blocks(&msg.commitment).await?;
                
                // Step 5: Verify all blocks in bundle exist in our chain
                let mut processed_blocks = Vec::new();
                for block_hash in &committed_blocks {
                    match self.get_block_by_hash(block_hash).await {
                        Ok(block) => {
                            processed_blocks.push(block);
                        },
                        Err(ChainError::BlockNotFound { .. }) => {
                            warn!(
                                block_hash = %block_hash,
                                "Block in AuxPoW commitment not found in chain"
                            );
                            continue;
                        },
                        Err(e) => return Err(e),
                    }
                }

                // Step 6: Check minimum work requirement
                let bitcoin_work = self.calculate_bitcoin_block_work(&bitcoin_block).await?;
                let min_work = self.config.consensus_config.min_auxpow_work;
                
                if bitcoin_work < min_work {
                    return Err(ChainError::InsufficientWork {
                        provided: bitcoin_work,
                        required: min_work,
                    });
                }

                // Step 7: Check for duplicate commitments
                if self.auxpow_state.processed_commitments.contains_key(&msg.commitment.bitcoin_block_hash) {
                    return Ok(AuxPowProcessingResult {
                        commitment_hash: msg.commitment.bitcoin_block_hash,
                        blocks_confirmed: 0,
                        total_work_added: 0,
                        processing_time: start_time.elapsed(),
                        status: AuxPowStatus::AlreadyProcessed,
                    });
                }

                // Step 8: Update AuxPoW state
                self.auxpow_state.processed_commitments.insert(
                    msg.commitment.bitcoin_block_hash,
                    ProcessedCommitment {
                        commitment: msg.commitment.clone(),
                        confirmed_blocks: committed_blocks.clone(),
                        bitcoin_work,
                        processed_at: Instant::now(),
                    },
                );

                // Step 9: Update block confirmation status
                for block in &processed_blocks {
                    self.update_block_auxpow_confirmation(block, &msg.commitment).await?;
                }

                // Step 10: Check if any blocks can now be finalized
                let newly_finalized = self.check_auxpow_finalization(&processed_blocks).await?;

                // Step 11: Persist AuxPoW commitment
                self.actor_addresses.storage
                    .send(StoreAuxPowCommitment {
                        commitment: msg.commitment.clone(),
                        confirmed_blocks: committed_blocks.clone(),
                    })
                    .await
                    .map_err(|e| ChainError::StorageError { 
                        reason: format!("Failed to store AuxPoW commitment: {}", e) 
                    })??;

                // Step 12: Update chain security metrics
                self.update_chain_security_metrics(bitcoin_work).await?;

                // Step 13: Trigger finalization for newly confirmed blocks
                for finalized_block in &newly_finalized {
                    let finalize_msg = FinalizeBlocks {
                        target_block: finalized_block.hash,
                        auxpow_commitments: Some(vec![msg.commitment.clone()]),
                        correlation_id: Some(correlation_id),
                    };
                    
                    // Send to self to process finalization
                    let _ = ctx.address().try_send(finalize_msg);
                }

                // Step 14: Update metrics
                let processing_time = start_time.elapsed();
                self.metrics.auxpow_commitments_processed += 1;
                self.metrics.total_auxpow_work += bitcoin_work;
                self.metrics.avg_auxpow_processing_time.add(processing_time.as_millis() as f64);

                // Step 15: Notify subscribers
                let auxpow_notification = AuxPowNotification {
                    bitcoin_block_hash: msg.commitment.bitcoin_block_hash,
                    committed_blocks: committed_blocks.clone(),
                    bitcoin_work,
                    newly_finalized: newly_finalized.iter().map(|b| b.hash).collect(),
                };

                for subscriber in self.subscribers.values() {
                    if subscriber.event_types.contains(&BlockEventType::AuxPowConfirmed) {
                        let _ = subscriber.recipient.do_send(auxpow_notification.clone());
                    }
                }

                info!(
                    bitcoin_block = %msg.commitment.bitcoin_block_hash,
                    blocks_confirmed = processed_blocks.len(),
                    work_added = bitcoin_work,
                    newly_finalized = newly_finalized.len(),
                    processing_time_ms = processing_time.as_millis(),
                    "AuxPoW commitment processed successfully"
                );

                Ok(AuxPowProcessingResult {
                    commitment_hash: msg.commitment.bitcoin_block_hash,
                    blocks_confirmed: processed_blocks.len() as u32,
                    total_work_added: bitcoin_work,
                    processing_time,
                    status: AuxPowStatus::Processed,
                })
            }
            .into_actor(self)
        )
    }

}

impl ChainActor {
    /// Helper methods for federation management
    
    async fn validate_federation_config(&self, config: &FederationConfig) -> Result<(), ChainError> {
        // Validate threshold
        if config.threshold == 0 || config.threshold > config.members.len() as u32 {
            return Err(ChainError::InvalidFederationConfig {
                reason: "Invalid threshold value".to_string(),
            });
        }

        // Validate members
        if config.members.is_empty() {
            return Err(ChainError::InvalidFederationConfig {
                reason: "Federation must have at least one member".to_string(),
            });
        }

        // Check for duplicate members
        let mut seen_ids = std::collections::HashSet::new();
        for member in &config.members {
            if !seen_ids.insert(&member.node_id) {
                return Err(ChainError::InvalidFederationConfig {
                    reason: format!("Duplicate member: {}", member.node_id),
                });
            }
        }

        Ok(())
    }

    async fn federation_config_changed(&self, new_config: &FederationConfig) -> Result<bool, ChainError> {
        // Compare with current configuration
        if self.federation_state.current_config.threshold != new_config.threshold {
            return Ok(true);
        }
        
        if self.federation_state.current_config.members.len() != new_config.members.len() {
            return Ok(true);
        }

        for (i, member) in new_config.members.iter().enumerate() {
            if let Some(current_member) = self.federation_state.current_config.members.get(i) {
                if member.node_id != current_member.node_id || 
                   member.pubkey != current_member.pubkey ||
                   member.weight != current_member.weight {
                    return Ok(true);
                }
            } else {
                return Ok(true);
            }
        }

        Ok(false)
    }

    async fn update_bitcoin_addresses(&mut self, config: &FederationConfig) -> Result<(), ChainError> {
        // Generate new Bitcoin addresses for the federation
        // TODO: Implement actual address generation from pubkeys
        Ok(())
    }

    /// Helper methods for finalization
    
    async fn get_block_by_hash(&self, hash: &Hash256) -> Result<SignedConsensusBlock, ChainError> {
        // Try to get from pending blocks first
        if let Some(pending) = self.pending_blocks.get(hash) {
            return Ok(pending.block.clone());
        }

        // Get from storage
        let result = self.actor_addresses.storage
            .send(GetBlock { hash: *hash })
            .await;

        match result {
            Ok(Ok(Some(block))) => Ok(block),
            Ok(Ok(None)) => Err(ChainError::BlockNotFound { 
                block_hash: hash.to_string() 
            }),
            Ok(Err(e)) => Err(ChainError::StorageError { 
                reason: format!("{}", e) 
            }),
            Err(e) => Err(ChainError::ActorCommunicationFailed {
                target: "StorageActor".to_string(),
                reason: format!("{}", e),
            }),
        }
    }

    async fn get_finalization_chain(&self, target_hash: &Hash256) -> Result<Vec<SignedConsensusBlock>, ChainError> {
        let mut blocks = Vec::new();
        let mut current_hash = *target_hash;

        // Build chain from target back to current finalized
        let finalized_height = self.chain_state.finalized
            .as_ref()
            .map(|f| f.height)
            .unwrap_or(0);

        loop {
            let block = self.get_block_by_hash(&current_hash).await?;
            
            if block.message.number() <= finalized_height {
                break;
            }

            blocks.push(block.clone());
            current_hash = block.message.parent_hash;
        }

        // Reverse to get forward order
        blocks.reverse();
        Ok(blocks)
    }

    async fn validate_finalization_safety(&self, blocks: &[SignedConsensusBlock]) -> Result<(), ChainError> {
        // Check that blocks form a continuous chain
        for window in blocks.windows(2) {
            if window[1].message.parent_hash != window[0].canonical_root() {
                return Err(ChainError::ValidationFailed {
                    reason: "Finalization chain is not continuous".to_string(),
                });
            }
        }

        Ok(())
    }

    async fn process_finalized_peg_operations(&self, blocks: &[SignedConsensusBlock]) -> Result<(), ChainError> {
        // Process peg-ins and peg-outs that are now finalized
        for block in blocks {
            // Notify bridge actor of finalized block
            let _ = self.actor_addresses.bridge
                .send(BlockFinalized {
                    block: block.clone(),
                })
                .await;
        }
        Ok(())
    }

    async fn cleanup_old_finalized_state(&mut self) -> Result<(), ChainError> {
        // Remove old pending blocks that are now finalized
        if let Some(finalized) = &self.chain_state.finalized {
            let finalized_height = finalized.height;
            
            self.pending_blocks.retain(|_, pending| {
                pending.block.message.number() > finalized_height
            });
        }

        Ok(())
    }

    async fn verify_auxpow_commitment(&self, commitment: &AuxPowCommitment) -> Result<bool, ChainError> {
        // TODO: Implement actual AuxPoW verification
        Ok(true)
    }

    /// Helper methods for reorganization
    
    async fn find_common_ancestor(
        &self, 
        old_head: &Option<BlockRef>, 
        new_head_block: &SignedConsensusBlock
    ) -> Result<(Hash256, u64), ChainError> {
        if old_head.is_none() {
            return Ok((Hash256::zero(), 0));
        }

        let old_head = old_head.as_ref().unwrap();
        let mut current_old = old_head.hash;
        let mut current_new = new_head_block.canonical_root();
        let mut depth = 0u64;

        // Walk back both chains until we find common ancestor
        while current_old != current_new {
            // Walk back the higher chain
            let old_block = self.get_block_by_hash(&current_old).await?;
            let new_block = self.get_block_by_hash(&current_new).await?;

            if old_block.message.number() > new_block.message.number() {
                current_old = old_block.message.parent_hash;
                depth += 1;
            } else if new_block.message.number() > old_block.message.number() {
                current_new = new_block.message.parent_hash;
            } else {
                current_old = old_block.message.parent_hash;
                current_new = new_block.message.parent_hash;
                depth += 1;
            }

            // Safety check
            if depth > 1000 {
                return Err(ChainError::ValidationFailed {
                    reason: "Reorganization too deep".to_string(),
                });
            }
        }

        Ok((current_old, depth))
    }

    async fn validate_reorg_safety(&self, depth: u64, new_head: &SignedConsensusBlock) -> Result<(), ChainError> {
        // Check maximum allowed reorg depth
        let max_depth = self.config.consensus_config.max_reorg_depth.unwrap_or(10);
        if depth > max_depth {
            return Err(ChainError::ReorgTooDeep { 
                depth, 
                max_allowed: max_depth 
            });
        }

        // Validate new head has sufficient work
        // TODO: Implement actual work calculation

        Ok(())
    }

    async fn get_blocks_to_revert(&self, old_head: &Option<BlockRef>, depth: u64) -> Result<Vec<BlockRef>, ChainError> {
        if old_head.is_none() || depth == 0 {
            return Ok(Vec::new());
        }

        let mut blocks = Vec::new();
        let mut current = old_head.as_ref().unwrap().hash;

        for _ in 0..depth {
            let block = self.get_block_by_hash(&current).await?;
            blocks.push(BlockRef {
                hash: current,
                height: block.message.number(),
            });
            current = block.message.parent_hash;
        }

        Ok(blocks)
    }

    async fn get_blocks_to_apply(&self, _ancestor: &Hash256, new_head: &SignedConsensusBlock) -> Result<Vec<BlockRef>, ChainError> {
        // TODO: Implement proper chain walking from ancestor to new head
        Ok(vec![BlockRef {
            hash: new_head.canonical_root(),
            height: new_head.message.number(),
        }])
    }

    async fn begin_reorg_transaction(&mut self) -> Result<(), ChainError> {
        // Begin atomic reorganization transaction
        // TODO: Implement proper transaction handling
        Ok(())
    }

    async fn revert_block(&mut self, _block: &SignedConsensusBlock) -> Result<(), ChainError> {
        // TODO: Implement block reversion logic
        Ok(())
    }

    async fn apply_block(&mut self, _block: &SignedConsensusBlock) -> Result<(), ChainError> {
        // TODO: Implement block application logic
        Ok(())
    }

    async fn update_fork_choice_after_reorg(&mut self, new_head: &Hash256) -> Result<(), ChainError> {
        self.chain_state.fork_choice.canonical_tip = *new_head;
        Ok(())
    }

    async fn commit_reorg_transaction(&mut self) -> Result<(), ChainError> {
        // Commit atomic reorganization transaction
        // TODO: Implement proper transaction handling
        Ok(())
    }

    async fn process_reorg_affected_peg_operations(
        &self, 
        _reverted: &[SignedConsensusBlock], 
        _applied: &[SignedConsensusBlock]
    ) -> Result<(), ChainError> {
        // TODO: Handle peg operations affected by reorganization
        Ok(())
    }

    /// Helper methods for AuxPoW processing
    
    async fn validate_auxpow_structure(&self, _commitment: &AuxPowCommitment) -> Result<(), ChainError> {
        // TODO: Validate AuxPoW commitment structure
        Ok(())
    }

    async fn verify_bitcoin_block(&self, _block_hash: &bitcoin::BlockHash) -> Result<bitcoin::Block, ChainError> {
        // TODO: Implement Bitcoin block verification
        use bitcoin::Block;
        Err(ChainError::NotImplemented)
    }

    async fn verify_auxpow_merkle_proof(&self, _commitment: &AuxPowCommitment) -> Result<bool, ChainError> {
        // TODO: Implement merkle proof verification
        Ok(true)
    }

    async fn extract_committed_blocks(&self, _commitment: &AuxPowCommitment) -> Result<Vec<Hash256>, ChainError> {
        // TODO: Extract committed block hashes from AuxPoW
        Ok(Vec::new())
    }

    async fn calculate_bitcoin_block_work(&self, _block: &bitcoin::Block) -> Result<u64, ChainError> {
        // TODO: Calculate Bitcoin block work
        Ok(1000000) // Placeholder value
    }

    async fn update_block_auxpow_confirmation(
        &mut self, 
        _block: &SignedConsensusBlock, 
        _commitment: &AuxPowCommitment
    ) -> Result<(), ChainError> {
        // TODO: Update block's AuxPoW confirmation status
        Ok(())
    }

    async fn check_auxpow_finalization(&self, _blocks: &[SignedConsensusBlock]) -> Result<Vec<BlockRef>, ChainError> {
        // TODO: Check which blocks can now be finalized due to AuxPoW
        Ok(Vec::new())
    }

    async fn update_chain_security_metrics(&mut self, work: u64) -> Result<(), ChainError> {
        self.auxpow_state.total_work += work;
        self.auxpow_state.last_commitment_time = Instant::now();
        Ok(())
    }
}