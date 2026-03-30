# Governance Update Verification

This document describes how validators verify governance updates to prevent rogue governance clients from pushing malicious changes.

## Overview

**Each validator independently verifies governance updates** using BLS aggregate signatures before applying them. A rogue governance client cannot push malicious updates because it would need the private BLS keys of 2/3+ validators to create a valid aggregate signature.

## Verification Flow

### 1. Signature Requirement

Every governance update carries a cryptographic signature (`governance.rs:132-133`):

```rust
pub struct ValidatorUpdate {
    pub public_key: PublicKey,
    pub power: VotingPower,
    /// Governance threshold signature proving authorization
    pub governance_signature: Signature,  // BLS signature required
}
```

### 2. Verification Before Application

Validators verify signatures before applying any update (`tendermint_handlers.rs:1411-1426`):

```rust
// Verify governance signature (unless skipped for testing)
if !self.config.skip_governance_signature_verification {
    let validator_set = self.load_validator_set_for_height(current_height).await?;
    let chain_id = format!("alys-{}", self.config.chain_id);

    // Verify the signature - requires 2/3+ aggregate signature from validators
    update.verify_governance_signature(&validator_set, &chain_id).map_err(|e| {
        ChainError::GovernanceVerificationFailed(...)
    })?;
}
```

### 3. BLS Signature Verification

The signature is verified against the current validator set (`governance.rs:168-200`):

```rust
pub fn verify_governance_signature(
    &self,
    validator_set: &ValidatorSet,
    chain_id: &str,
) -> Result<(), GovernanceError> {
    let signing_root = self.signing_root(chain_id);

    // Reject empty signatures
    if self.governance_signature == Signature::empty() {
        return Err(GovernanceError::MissingSignature);
    }

    // Verify BLS aggregate signature against all validators
    let aggregate_pubkey = validator_set.aggregate_public_key();
    if !self.governance_signature.verify(&aggregate_pubkey, signing_root) {
        return Err(GovernanceError::InvalidSignature);
    }
    Ok(())
}
```

### 4. Replay Protection

The signing root includes domain separation and chain ID to prevent replay attacks (`governance.rs:146-161`):

```rust
pub fn signing_root(&self, chain_id: &str) -> H256 {
    let mut hasher = Keccak::v256();

    // Domain separation prefix
    hasher.update(b"governance-validator-update");
    // Chain ID prevents cross-network replay
    hasher.update(chain_id.as_bytes());
    // Update-specific data
    hasher.update(&self.public_key.serialize());
    hasher.update(&self.power.to_le_bytes());

    let mut output = [0u8; 32];
    hasher.finalize(&mut output);
    H256::from(output)
}
```

## Security Properties

| Property | Implementation |
|----------|---------------|
| **Authentication** | BLS aggregate signature from validators |
| **Replay Protection** | Chain ID included in signing root |
| **Domain Separation** | Unique prefix per update type (`governance-validator-update`, etc.) |
| **Independent Verification** | Each validator verifies before applying |
| **Threshold Security** | Requires 2/3+ validator signatures (see MVP limitation below) |

## Current MVP Limitation

The current implementation requires **ALL validators** to sign (100% threshold). The code includes comments noting what production would need:

```rust
// In production, we would need to:
// 1. Track which validators signed (e.g., using a bitfield)
// 2. Aggregate only participating validator public keys
// 3. Verify that participating power >= 2/3+ threshold
//
// For now, aggregate all validator public keys and verify
```

### Production Requirements

To support 2/3+ threshold signing:

1. **Bitfield**: Include a bitfield in the update indicating which validators signed
2. **Selective Aggregation**: Aggregate only the public keys of participating validators
3. **Threshold Check**: Verify that participating voting power >= 2/3+ total power
4. **Partial Signature Verification**: Verify the aggregate signature against the partial aggregate public key

## Testing Mode

For testing and development, signature verification can be bypassed using the `--skip-governance-signature-verification` flag or `SKIP_GOVERNANCE_SIG_VERIFY=true` environment variable.

**WARNING**: This should NEVER be used in production as it removes all governance security.

The mock governance server generates invalid signatures (`vec![0u8; 96]`), which is why this flag is required for chaos testing.

## Attack Resistance

| Attack | Mitigation |
|--------|------------|
| Rogue governance client | Cannot create valid BLS aggregate signature without validator private keys |
| Replay attack (same chain) | Updates are applied once and stored; duplicate detection possible |
| Replay attack (cross-chain) | Chain ID in signing root prevents cross-network replay |
| Man-in-the-middle | BLS signatures are unforgeable; tampering invalidates signature |
| Validator impersonation | Requires validator's BLS private key |

## Related Files

- `app/src/actors_v2/chain/tendermint/governance.rs` - Governance update types and verification
- `app/src/actors_v2/chain/tendermint_handlers.rs` - Handler that calls verification
- `app/src/actors_v2/governance/` - Governance client actor
- `crates/mock-governance/` - Mock governance server for testing
