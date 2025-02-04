use crate::block::SignedConsensusBlock;
use crate::chain::Chain;
use crate::error::Error;
use bls::{Keypair, PublicKey};
use futures_timer::Delay;
use std::sync::Arc;
use std::time::Duration;
use store::ItemStore;
use tracing::*;
use types::MainnetEthSpec;

fn slot_from_timestamp(timestamp: u64, slot_duration: u64) -> u64 {
    timestamp / slot_duration
}

// https://github.com/paritytech/substrate/blob/2704ab3d348f18f9db03e87a725e4807b91660d8/client/consensus/aura/src/lib.rs#L127
fn slot_author<AuthorityId>(slot: u64, authorities: &[AuthorityId]) -> Option<(u8, &AuthorityId)> {
    if authorities.is_empty() {
        return None;
    }

    let idx = slot % (authorities.len() as u64);
    assert!(
        idx <= usize::MAX as u64,
        "It is impossible to have a vector with length beyond the address space; qed",
    );

    let current_author = authorities.get(idx as usize).expect(
        "authorities not empty; index constrained to list length; this is a valid index; qed",
    );

    Some((idx as u8, current_author))
}

#[derive(Debug)]
pub(crate) enum AuraError {
    SlotIsInFuture,
    SlotAuthorNotFound,
    BadSignature,
    // InvalidAuthor,
}

#[derive(Clone)]
pub struct Authority {
    pub signer: Keypair,
    pub index: u8,
}

pub struct Aura {
    pub authorities: Vec<PublicKey>,
    pub slot_duration: u64,
    pub authority: Option<Authority>,
}

impl Aura {
    pub fn new(
        authorities: Vec<PublicKey>,
        slot_duration: u64,
        maybe_signer: Option<Keypair>,
    ) -> Self {
        let authority = if let Some(signer) = maybe_signer {
            let index = authorities
                .iter()
                .position(|x| signer.pk.eq(x))
                .expect("Authority not found in set") as u8;
            Some(Authority { index, signer })
        } else {
            None
        };
        Self {
            authorities,
            slot_duration,
            authority,
        }
    }

    // https://github.com/paritytech/substrate/blob/033d4e86cc7eff0066cd376b9375f815761d653c/client/consensus/aura/src/import_queue.rs#L218
    pub fn check_signed_by_author(
        &self,
        block: &SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<(), AuraError> {
        let timestamp =
            Duration::from_secs(block.message.execution_payload.timestamp).as_millis() as u64;
        let slot = block.message.slot;
        let slot_now = slot_from_timestamp(timestamp, self.slot_duration);

        // add drift same as in substrate
        if slot > slot_now + 1 {
            Err(AuraError::SlotIsInFuture)
        } else {
            let (_expected_authority_index, _expected_author) =
                slot_author(slot, &self.authorities[..]).ok_or(AuraError::SlotAuthorNotFound)?;

            debug!("timestamp: {}, slot {slot}", timestamp);

            block
                .verify_signature(&self.authorities[..])
                .then_some(())
                .ok_or(AuraError::BadSignature)?;

            // TODO: Replace with dynamic sourcing for authorities at a given timespan
            // if !block.is_signed_by(expected_authority_index) {
            //     return Err(AuraError::InvalidAuthor);
            // }

            Ok(())
        }
    }

    pub fn majority_approved(
        &self,
        block: &SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<bool, AuraError> {
        self.check_signed_by_author(block)?;

        let required_signatures = if block.message.execution_payload.block_number > 285450 {
            ((self.authorities.len() * 2) + 2) / 3
        } else {
            1
        };
        
        if block.num_approvals() < required_signatures {
            return Ok(false);
        }

        if block.verify_signature(&self.authorities) {
            Ok(true)
        } else {
            Err(AuraError::BadSignature)
        }
    }
}

// https://github.com/paritytech/substrate/blob/033d4e86cc7eff0066cd376b9375f815761d653c/client/consensus/slots/src/slots.rs#L32
pub fn duration_now() -> Duration {
    use std::time::SystemTime;
    let now = SystemTime::now();
    now.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|e| {
            panic!(
                "Current time {:?} is before unix epoch. Something is wrong: {:?}",
                now, e
            )
        })
}

// https://github.com/paritytech/substrate/blob/033d4e86cc7eff0066cd376b9375f815761d653c/client/consensus/slots/src/slots.rs#L41
pub fn time_until_next_slot(slot_duration: Duration) -> Duration {
    let now = duration_now().as_millis();

    let next_slot = (now + slot_duration.as_millis()) / slot_duration.as_millis();
    let remaining_millis = next_slot * slot_duration.as_millis() - now;
    Duration::from_millis(remaining_millis as u64)
}

pub struct AuraSlotWorker<DB> {
    last_slot: u64,
    slot_duration: Duration,
    until_next_slot: Option<Delay>,
    authorities: Vec<PublicKey>,
    maybe_signer: Option<Keypair>,
    chain: Arc<Chain<DB>>,
}

impl<DB: ItemStore<MainnetEthSpec>> AuraSlotWorker<DB> {
    pub fn new(
        slot_duration: Duration,
        authorities: Vec<PublicKey>,
        maybe_signer: Option<Keypair>,
        chain: Arc<Chain<DB>>,
    ) -> Self {
        Self {
            last_slot: 0,
            slot_duration,
            until_next_slot: None,
            authorities,
            maybe_signer,
            chain,
        }
    }

    fn claim_slot(&self, slot: u64, authorities: &[PublicKey]) -> Option<PublicKey> {
        let expected_author = slot_author(slot, authorities);
        expected_author.and_then(|(_, p)| {
            if self
                .maybe_signer
                .as_ref()
                .expect("Only called by signer")
                .pk
                .eq(p)
            {
                Some(p.clone())
            } else {
                None
            }
        })
    }

    async fn on_slot(&self, slot: u64) -> Option<Result<(), Error>> {
        let _ = self.claim_slot(slot, &self.authorities[..])?;
        debug!("My turn");

        self.chain
            .produce_block(slot, duration_now())
            .await
            .expect("Should produce block");
        Some(Ok(()))
    }

    async fn next_slot(&mut self) -> u64 {
        loop {
            self.until_next_slot
                .take()
                .unwrap_or_else(|| {
                    let wait_dur = time_until_next_slot(self.slot_duration);
                    Delay::new(wait_dur)
                })
                .await;

            let wait_dur = time_until_next_slot(self.slot_duration);
            self.until_next_slot = Some(Delay::new(wait_dur));

            // https://github.com/paritytech/substrate/blob/033d4e86cc7eff0066cd376b9375f815761d653c/bin/node/cli/src/service.rs#L462-L468
            let slot = slot_from_timestamp(
                duration_now().as_millis() as u64,
                self.slot_duration.as_millis() as u64,
            );

            if slot > self.last_slot {
                self.last_slot = slot;

                break slot;
            }
        }
    }

    pub async fn start_slot_worker(&mut self) {
        loop {
            let slot_info = self.next_slot().await;
            if self.maybe_signer.is_some() {
                let _ = self.on_slot(slot_info).await;
            } else {
                // nothing to do
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bls::SecretKey;

    #[test]
    fn should_find_slot_author() {
        let slot_now = slot_from_timestamp(1703256299459, 5000);
        assert_eq!(slot_now, 340651259);
        assert_eq!(*slot_author(slot_now, &[1, 2, 3, 4, 5, 6]).unwrap().1, 6);
    }

    #[test]
    fn should_find_authority() {
        // Replace with your secret key
        let secret_key_hex = "0000000000000000000000000000000000000000000000000000000000000001";

        // Convert the secret key from hex to bytes
        let secret_key_bytes = hex::decode(secret_key_hex).unwrap();

        // Create a SecretKey instance from the bytes
        let aura_sk = SecretKey::deserialize(&secret_key_bytes[..]).unwrap();

        let aura_pk = aura_sk.public_key();

        let aura_signer = Keypair::from_components(aura_pk, aura_sk);

        let aura_authority_key_hex = "97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb";

        let aura_authority_key_bytes = hex::decode(aura_authority_key_hex).unwrap();

        let aura_authority_key = PublicKey::deserialize(&aura_authority_key_bytes[..]).unwrap();

        let authorities = [aura_authority_key];

        let _index = authorities
            .iter()
            .position(|x| aura_signer.pk.eq(x))
            .expect("Authority not found in set") as u8;
    }
}
