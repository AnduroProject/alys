//! Tendermint consensus storage handlers for Storage Actor - V2
//!
//! Handles validator set storage, commit retrieval, and governance parameter persistence.

use crate::actors_v2::storage::{
    actor::{StorageActor, StorageError},
    messages::*,
};
use actix::prelude::*;
use tracing::*;

// ============================================================================
// Validator Set Handlers
// ============================================================================

impl Handler<StoreValidatorSetMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: StoreValidatorSetMessage, _: &mut Context<Self>) -> Self::Result {
        let correlation_id = msg.correlation_id;
        let effective_height = msg.effective_height;
        let validator_set = msg.validator_set;
        let database = self.database.clone();
        let mut metrics = self.metrics.clone();

        debug!(
            correlation_id = ?correlation_id,
            effective_height = effective_height,
            validator_count = validator_set.len(),
            "Handling StoreValidatorSetMessage"
        );

        Box::pin(async move {
            let start_time = std::time::Instant::now();

            database
                .put_validator_set(effective_height, &validator_set)
                .await?;

            let storage_time = start_time.elapsed();
            metrics.record_state_update(storage_time);

            info!(
                correlation_id = ?correlation_id,
                effective_height = effective_height,
                validator_count = validator_set.len(),
                storage_time_ms = storage_time.as_millis(),
                "Successfully stored validator set"
            );

            Ok(())
        })
    }
}

impl Handler<GetValidatorSetForHeightMessage> for StorageActor {
    type Result = ResponseFuture<
        Result<Option<crate::actors_v2::chain::tendermint::ValidatorSet>, StorageError>,
    >;

    fn handle(
        &mut self,
        msg: GetValidatorSetForHeightMessage,
        _: &mut Context<Self>,
    ) -> Self::Result {
        let correlation_id = msg.correlation_id;
        let height = msg.height;
        let database = self.database.clone();
        let mut metrics = self.metrics.clone();

        debug!(
            correlation_id = ?correlation_id,
            height = height,
            "Handling GetValidatorSetForHeightMessage"
        );

        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let result = database.get_validator_set_for_height(height).await?;

            let read_time = start_time.elapsed();
            metrics.record_state_query(read_time, false);

            if let Some(ref vs) = result {
                debug!(
                    correlation_id = ?correlation_id,
                    height = height,
                    validator_count = vs.len(),
                    read_time_ms = read_time.as_millis(),
                    "Found validator set for height"
                );
            } else {
                debug!(
                    correlation_id = ?correlation_id,
                    height = height,
                    read_time_ms = read_time.as_millis(),
                    "No validator set found for height"
                );
            }

            Ok(result)
        })
    }
}

impl Handler<GetValidatorSetAtHeightMessage> for StorageActor {
    type Result = ResponseFuture<
        Result<Option<crate::actors_v2::chain::tendermint::ValidatorSet>, StorageError>,
    >;

    fn handle(
        &mut self,
        msg: GetValidatorSetAtHeightMessage,
        _: &mut Context<Self>,
    ) -> Self::Result {
        let correlation_id = msg.correlation_id;
        let effective_height = msg.effective_height;
        let database = self.database.clone();
        let mut metrics = self.metrics.clone();

        debug!(
            correlation_id = ?correlation_id,
            effective_height = effective_height,
            "Handling GetValidatorSetAtHeightMessage"
        );

        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let result = database.get_validator_set_at_height(effective_height).await?;

            let read_time = start_time.elapsed();
            metrics.record_state_query(read_time, false);

            if let Some(ref vs) = result {
                debug!(
                    correlation_id = ?correlation_id,
                    effective_height = effective_height,
                    validator_count = vs.len(),
                    read_time_ms = read_time.as_millis(),
                    "Found validator set at exact height"
                );
            } else {
                trace!(
                    correlation_id = ?correlation_id,
                    effective_height = effective_height,
                    read_time_ms = read_time.as_millis(),
                    "No validator set at exact height (expected if no change at this height)"
                );
            }

            Ok(result)
        })
    }
}

// ============================================================================
// Commit Retrieval Handler
// ============================================================================

impl Handler<GetCommitForHeightMessage> for StorageActor {
    type Result =
        ResponseFuture<Result<Option<crate::actors_v2::chain::tendermint::Commit>, StorageError>>;

    fn handle(&mut self, msg: GetCommitForHeightMessage, _: &mut Context<Self>) -> Self::Result {
        let correlation_id = msg.correlation_id;
        let height = msg.height;
        let database = self.database.clone();
        let mut metrics = self.metrics.clone();

        debug!(
            correlation_id = ?correlation_id,
            height = height,
            "Handling GetCommitForHeightMessage"
        );

        Box::pin(async move {
            let start_time = std::time::Instant::now();

            // In Tendermint, the commit for block N is stored in block N+1's last_commit field.
            // So to get the commit for height H, we need to retrieve block H+1.
            let next_height = height.saturating_add(1);

            let block = database.get_block_by_height(next_height).await?;

            let read_time = start_time.elapsed();
            metrics.record_state_query(read_time, false);

            match block {
                Some(b) => {
                    let commit = b.message.last_commit.clone();
                    if commit.is_some() {
                        debug!(
                            correlation_id = ?correlation_id,
                            height = height,
                            next_height = next_height,
                            read_time_ms = read_time.as_millis(),
                            "Found commit for height in block N+1's last_commit"
                        );
                    } else {
                        debug!(
                            correlation_id = ?correlation_id,
                            height = height,
                            next_height = next_height,
                            read_time_ms = read_time.as_millis(),
                            "Block N+1 exists but has no last_commit (may be genesis or Aura block)"
                        );
                    }
                    Ok(commit)
                }
                None => {
                    debug!(
                        correlation_id = ?correlation_id,
                        height = height,
                        next_height = next_height,
                        read_time_ms = read_time.as_millis(),
                        "Block N+1 not found - commit for height not yet available"
                    );
                    Ok(None)
                }
            }
        })
    }
}

// ============================================================================
// Governance Parameter Handlers
// ============================================================================

impl Handler<StoreParameterUpdateMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: StoreParameterUpdateMessage, _: &mut Context<Self>) -> Self::Result {
        let correlation_id = msg.correlation_id;
        let param_id = msg.param_id;
        let effective_height = msg.effective_height;
        let value = msg.value;
        let database = self.database.clone();
        let mut metrics = self.metrics.clone();

        debug!(
            correlation_id = ?correlation_id,
            param_id = ?param_id,
            effective_height = effective_height,
            value_len = value.len(),
            "Handling StoreParameterUpdateMessage"
        );

        Box::pin(async move {
            let start_time = std::time::Instant::now();

            database
                .put_parameter_update(param_id, effective_height, &value)
                .await?;

            let storage_time = start_time.elapsed();
            metrics.record_state_update(storage_time);

            info!(
                correlation_id = ?correlation_id,
                param_id = ?param_id,
                effective_height = effective_height,
                storage_time_ms = storage_time.as_millis(),
                "Successfully stored parameter update"
            );

            Ok(())
        })
    }
}

impl Handler<GetParameterAtHeightMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<Vec<u8>>, StorageError>>;

    fn handle(&mut self, msg: GetParameterAtHeightMessage, _: &mut Context<Self>) -> Self::Result {
        let correlation_id = msg.correlation_id;
        let param_id = msg.param_id;
        let height = msg.height;
        let database = self.database.clone();
        let mut metrics = self.metrics.clone();

        debug!(
            correlation_id = ?correlation_id,
            param_id = ?param_id,
            height = height,
            "Handling GetParameterAtHeightMessage"
        );

        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let result = database.get_parameter_at_height(param_id, height).await?;

            let read_time = start_time.elapsed();
            metrics.record_state_query(read_time, false);

            if let Some(ref value) = result {
                debug!(
                    correlation_id = ?correlation_id,
                    param_id = ?param_id,
                    height = height,
                    value_len = value.len(),
                    read_time_ms = read_time.as_millis(),
                    "Found parameter value at height"
                );
            } else {
                debug!(
                    correlation_id = ?correlation_id,
                    param_id = ?param_id,
                    height = height,
                    read_time_ms = read_time.as_millis(),
                    "No parameter value found at height"
                );
            }

            Ok(result)
        })
    }
}

#[cfg(test)]
mod tests {
    // Tests would require mock database setup
    // For now, integration tests cover these handlers
}
