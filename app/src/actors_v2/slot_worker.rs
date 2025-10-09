//! Aura Slot Worker V2
//!
//! Simplified slot timing loop that sends messages to ChainActor.
//! Based on V0's proven AuraSlotWorker but adapted for actor model.
//!
//! Key differences from V0:
//! - Uses Addr<ChainActor> instead of Arc<Chain>
//! - Sends ChainMessage::ProduceBlock instead of direct method call
//! - Enables testability through message-based architecture

use actix::prelude::*;
use futures_timer::Delay;
use lighthouse_wrapper::bls::{Keypair, PublicKey};
use std::time::Duration;
use tracing::*;

use crate::actors_v2::chain::{ChainActor, ChainMessage, ChainResponse};
use crate::aura::{duration_now, time_until_next_slot, slot_from_timestamp, slot_author};
use crate::metrics::{AURA_CURRENT_SLOT, AURA_PRODUCED_BLOCKS, AURA_SLOT_CLAIM_TOTALS};

/// Aura Slot Worker V2 - Timing loop for block production
///
/// This worker runs continuously, waiting for slot boundaries and triggering
/// block production by sending messages to the ChainActor when this node is
/// the designated authority for a slot.
pub struct AuraSlotWorkerV2 {
    /// Last slot we processed (to avoid duplicates)
    last_slot: u64,
    /// Duration of each slot (e.g., 3 seconds)
    slot_duration: Duration,
    /// Federation authority public keys
    authorities: Vec<PublicKey>,
    /// Our signing keypair (Some if we're a validator)
    maybe_signer: Option<Keypair>,
    /// Address of ChainActor to send block production requests
    chain_actor: Addr<ChainActor>,
}

impl AuraSlotWorkerV2 {
    /// Create a new V2 slot worker
    ///
    /// # Arguments
    /// * `slot_duration` - Duration of each slot (e.g., Duration::from_secs(3))
    /// * `authorities` - Ordered list of federation validator public keys
    /// * `maybe_signer` - Our validator keypair (None if not a validator)
    /// * `chain_actor` - Address of ChainActor to send messages to
    pub fn new(
        slot_duration: Duration,
        authorities: Vec<PublicKey>,
        maybe_signer: Option<Keypair>,
        chain_actor: Addr<ChainActor>,
    ) -> Self {
        Self {
            last_slot: 0,
            slot_duration,
            authorities,
            maybe_signer,
            chain_actor,
        }
    }

    /// Check if this node is the authority for the given slot
    ///
    /// Uses round-robin slot assignment: slot % num_authorities
    /// Returns true if we should produce a block for this slot.
    fn claim_slot(&self, slot: u64) -> bool {
        AURA_SLOT_CLAIM_TOTALS
            .with_label_values(&["called"])
            .inc();

        let expected_author = slot_author(slot, &self.authorities);
        let is_our_slot = expected_author
            .map(|(_, pk)| {
                self.maybe_signer
                    .as_ref()
                    .map(|signer| signer.pk.eq(pk))
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        if is_our_slot {
            AURA_SLOT_CLAIM_TOTALS
                .with_label_values(&["success"])
                .inc();
        } else {
            AURA_SLOT_CLAIM_TOTALS
                .with_label_values(&["failure"])
                .inc();
        }

        is_our_slot
    }

    /// Handle slot tick - send message to ChainActor if we're the authority
    ///
    /// This is called for each slot boundary. If we're the designated authority,
    /// we send a ProduceBlock message to the ChainActor.
    async fn on_slot(&self, slot: u64) {
        AURA_CURRENT_SLOT.set(slot as f64);

        if !self.claim_slot(slot) {
            // Not our slot, nothing to do
            return;
        }

        debug!(slot = slot, "Our slot - requesting block production");

        let msg = ChainMessage::ProduceBlock {
            slot,
            timestamp: duration_now(),
        };

        match self.chain_actor.send(msg).await {
            Ok(Ok(ChainResponse::BlockProduced { block, duration })) => {
                info!(
                    slot = slot,
                    block_hash = ?block.message.execution_payload.block_hash,
                    block_number = block.message.execution_payload.block_number,
                    duration_ms = duration.as_millis(),
                    "Block produced successfully"
                );
                AURA_PRODUCED_BLOCKS
                    .with_label_values(&["success"])
                    .inc();
            }
            Ok(Err(e)) => {
                error!(slot = slot, error = ?e, "Failed to produce block");
                AURA_PRODUCED_BLOCKS
                    .with_label_values(&["error"])
                    .inc();
            }
            Err(e) => {
                error!(slot = slot, error = ?e, "ChainActor mailbox error - actor may be stopped");
                AURA_PRODUCED_BLOCKS
                    .with_label_values(&["error"])
                    .inc();
            }
            _ => {
                warn!(slot = slot, "Unexpected response from ChainActor");
            }
        }
    }

    /// Wait for next slot boundary
    ///
    /// This uses V0's proven timing logic:
    /// 1. Calculate time until next slot boundary
    /// 2. Sleep using futures_timer::Delay for precise timing
    /// 3. Calculate current slot after waking
    /// 4. Only return when slot has advanced (handles clock skew)
    async fn next_slot(&mut self) -> u64 {
        loop {
            let wait_dur = time_until_next_slot(self.slot_duration);
            Delay::new(wait_dur).await;

            let slot = slot_from_timestamp(
                duration_now().as_millis() as u64,
                self.slot_duration.as_millis() as u64,
            );

            if slot > self.last_slot {
                self.last_slot = slot;
                break slot;
            }
            // If slot hasn't advanced, loop again (handles edge cases)
        }
    }

    /// Start the slot worker loop
    ///
    /// This runs indefinitely, waiting for slot boundaries and triggering
    /// block production when appropriate. Only validators (with maybe_signer)
    /// will attempt to produce blocks.
    pub async fn start_slot_worker(mut self) {
        let validator_status = if self.maybe_signer.is_some() {
            "validator"
        } else {
            "observer"
        };

        info!(
            slot_duration_ms = self.slot_duration.as_millis(),
            num_authorities = self.authorities.len(),
            validator_status = validator_status,
            "Starting Aura slot worker V2"
        );

        loop {
            let slot = self.next_slot().await;

            if self.maybe_signer.is_some() {
                self.on_slot(slot).await;
            }
            // Non-validators just track slots for metrics via AURA_CURRENT_SLOT
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lighthouse_wrapper::bls::SecretKey;

    /// Helper to create test authorities
    fn create_test_authorities(count: usize) -> Vec<PublicKey> {
        (0..count)
            .map(|i| {
                let mut secret_bytes = [0u8; 32];
                secret_bytes[0] = (i + 1) as u8;
                let secret = SecretKey::deserialize(&secret_bytes).unwrap();
                secret.public_key()
            })
            .collect()
    }

    #[test]
    fn test_slot_calculation_round_robin() {
        // Test that slot claiming follows round-robin correctly
        let authorities = create_test_authorities(3);

        // Slot 0 -> authority 0
        // Slot 1 -> authority 1
        // Slot 2 -> authority 2
        // Slot 3 -> authority 0 (wraps around)

        for slot in 0..12 {
            let expected_index = (slot % 3) as usize;
            let (actual_index, _) = slot_author(slot, &authorities).unwrap();
            assert_eq!(
                actual_index as usize, expected_index,
                "Slot {} should be assigned to authority {}",
                slot, expected_index
            );
        }
    }

    #[test]
    fn test_claim_slot_not_our_slot() {
        let authorities = create_test_authorities(3);

        // We are authority 0
        let mut secret_bytes = [0u8; 32];
        secret_bytes[0] = 1;
        let secret = SecretKey::deserialize(&secret_bytes).unwrap();
        let keypair = Keypair::from_components(secret.public_key(), secret);

        // Create a mock ChainActor address (we won't actually use it in this test)
        // In a real test, we'd use actix::test and create a proper actor

        // For now, just test the logic without the actor
        // Slot 0 is ours, slot 1 is not
        let slot_0_author = slot_author(0, &authorities).unwrap();
        let slot_1_author = slot_author(1, &authorities).unwrap();

        assert_eq!(slot_0_author.1, &keypair.pk, "Slot 0 should be ours");
        assert_ne!(slot_1_author.1, &keypair.pk, "Slot 1 should not be ours");
    }
}
