use crate::error::Error;
use bls::SignatureSet;
use serde_derive::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::typenum::U15;
use std::borrow::Cow;
use tree_hash_derive::TreeHash;
use types::AggregateSignature;
use types::BitList;
use types::Hash256;
use types::PublicKey;
use types::Signature;
use types::Unsigned;

/// upper bound on number of validators
type MaxValidators = U15;

#[derive(Debug, Encode, Decode, TreeHash, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndividualApproval {
    pub signature: Signature,
    pub authority_index: u8,
}

impl IndividualApproval {
    pub fn assume_checked(self) -> CheckedIndividualApproval {
        CheckedIndividualApproval { data: self }
    }
}

#[derive(Debug, Encode, Decode, TreeHash, Clone, PartialEq)]
pub struct CheckedIndividualApproval {
    data: IndividualApproval,
}

impl From<CheckedIndividualApproval> for IndividualApproval {
    fn from(value: CheckedIndividualApproval) -> Self {
        value.data
    }
}

impl CheckedIndividualApproval {
    pub fn into_aggregate(self) -> AggregateApproval {
        let mut aggregate = AggregateApproval::new();
        aggregate.add_approval(self).unwrap();
        aggregate
    }
}

impl IndividualApproval {
    pub fn check(
        self,
        data: Hash256,
        authorities: &[PublicKey],
    ) -> Result<CheckedIndividualApproval, Error> {
        let pubkey = authorities
            .get(self.authority_index as usize)
            .ok_or(Error::UnknownAuthority)?;
        let is_ok = self.signature.verify(pubkey, data);
        if is_ok {
            Ok(CheckedIndividualApproval { data: self })
        } else {
            Err(Error::InvalidSignature)
        }
    }
}

#[derive(Debug, Encode, Decode, Serialize, Deserialize, TreeHash, Clone, PartialEq)]
pub struct AggregateApproval {
    aggregation_bits: BitList<MaxValidators>,
    aggregate_signature: AggregateSignature,
}

impl AggregateApproval {
    pub fn new() -> Self {
        Self {
            aggregation_bits: BitList::with_capacity(MaxValidators::to_usize()).unwrap(),
            aggregate_signature: AggregateSignature::empty(),
        }
    }

    pub fn add_approval(&mut self, approval: CheckedIndividualApproval) -> Result<(), Error> {
        if self
            .aggregation_bits
            .get(approval.data.authority_index.into())
            .map_err(|_| Error::UnknownAuthority)?
        {
            return Ok(()); // already part of aggregate
        }
        self.aggregation_bits
            .set(approval.data.authority_index.into(), true)
            .map_err(|_| Error::UnknownAuthority)?;
        self.aggregate_signature
            .add_assign(&approval.data.signature);
        Ok(())
    }

    pub fn verify(&self, authorities: &[PublicKey], data: Hash256) -> bool {
        let pubkeys = self
            .aggregation_bits
            .iter()
            .zip(authorities.iter())
            .filter_map(|(is_present, pubkey)| {
                if is_present {
                    Some(Cow::Borrowed(pubkey))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        SignatureSet::multiple_pubkeys(&self.aggregate_signature, pubkeys, data).verify()
    }

    pub fn num_approvals(&self) -> usize {
        self.aggregation_bits.num_set_bits()
    }

    pub fn is_signed_by(&self, authority_index: u8) -> bool {
        self.aggregation_bits
            .get(authority_index.into())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use types::SecretKey;

    #[test]
    fn test_aggregate_signatures() {
        let hash = Hash256::random();

        let n = 10;
        let secret_keys = (0..n).map(|_| SecretKey::random()).collect::<Vec<_>>();
        let signatures = secret_keys
            .iter()
            .map(|key| key.sign(hash))
            .collect::<Vec<_>>();

        let mut pubkeys = secret_keys
            .iter()
            .map(|key| key.public_key())
            .collect::<Vec<_>>();

        let individual_approvals = signatures
            .iter()
            .enumerate()
            .map(|(idx, sig)| {
                IndividualApproval {
                    authority_index: idx as u8,
                    signature: sig.clone(),
                }
                .assume_checked()
            })
            .collect::<Vec<_>>();

        let mut aggregate = AggregateApproval::new();

        // we'll check that order of signatures doesn't matter
        let approval_indices = [0, 4, 3];

        for approval_id in approval_indices {
            aggregate
                .add_approval(individual_approvals[approval_id].clone())
                .expect("Should approve");
        }

        // test is_signed_by
        for i in 0..n {
            let expected_result = approval_indices.contains(&i);
            assert_eq!(expected_result, aggregate.is_signed_by(i as u8));
        }

        // test the verify function
        assert!(aggregate.verify(&pubkeys, hash));

        // change one of the pubkeys - this is equivalent to signing with an invalid key.
        // Verify should return false
        pubkeys[0] = pubkeys[1].clone();
        assert!(!aggregate.verify(&pubkeys, hash));
    }
}
