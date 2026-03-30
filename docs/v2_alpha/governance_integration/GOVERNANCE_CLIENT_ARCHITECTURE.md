# Governance Client Integration - Architecture Overview

## Summary

Added bidirectional gRPC streaming between Alys validators and a governance service for peg-in verification and validator set management.

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                                                                                 │
│    GOVERNANCE SERVICE                              ALYS VALIDATOR               │
│    ==================                              ==============               │
│                                                                                 │
│    ┌─────────────────────┐                        ┌─────────────────────┐       │
│    │                     │      Bidirectional     │                     │       │
│    │   mock-governance   │◄─────────────────────►│ GovernanceClientActor│       │
│    │      (gRPC)         │      gRPC Stream       │                     │       │
│    │                     │                        └──────────┬──────────┘       │
│    │   Port 50051        │                                   │                  │
│    │                     │                                   │ forwards         │
│    └─────────────────────┘                                   │ updates          │
│                                                              ▼                  │
│                                                   ┌─────────────────────┐       │
│                                                   │                     │       │
│    ┌─────────────────────┐                        │     ChainActor      │       │
│    │  governance-proto   │                        │                     │       │
│    │  (shared protobuf)  │────── used by ────────►│  • Queues updates   │       │
│    └─────────────────────┘                        │  • Processes pegins │       │
│                                                   └─────────────────────┘       │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Key Crates & Actors Introduced

### Crates

| Crate | Location | Purpose |
|-------|----------|---------|
| **governance-proto** | `crates/governance-proto/` | Shared protobuf definitions for gRPC interface. Used by both mock server and validators. Single source of truth for message schemas. |
| **mock-governance** | `crates/mock-governance/` | Standalone mock gRPC server for testing. Simulates governance service behavior with configurable responses. |

### Actors

| Actor | Location | Purpose |
|-------|----------|---------|
| **GovernanceClientActor** | `app/src/actors_v2/governance/` | Manages gRPC client connection to governance. Handles connection lifecycle, request correlation, heartbeats, and automatic reconnection. Forwards governance updates to ChainActor. |

---

## Use Cases Handled

### 1. Peg-in Verification

Validators request verification of Bitcoin peg-in transactions before accepting them into the chain.

```
Validator                          Governance
    │                                  │
    │  PeginVerifyRequest              │
    │  (txid, block_hash, amount)      │
    │─────────────────────────────────►│
    │                                  │
    │         PeginVerifyResponse      │
    │         (verified: true/false)   │
    │◄─────────────────────────────────│
    │                                  │
```

**Flow:** ChainActor → GovernanceClientActor → Governance Service → Response → ChainActor accepts/rejects pegin

### 2. Validator Set Updates

Governance pushes validator set changes to all connected validators.

```
Governance                         Validator
    │                                  │
    │  ValidatorSetUpdate              │
    │  (public_key, power, signature)  │
    │─────────────────────────────────►│
    │                                  │
    │                   Queued at H+2  │
    │                   for activation │
    │                                  │
```

**Flow:** Governance Service → GovernanceClientActor → ChainActor → Tendermint governance queue (activated at height + 2)

---

## Message Flow

```
                    VALIDATOR → GOVERNANCE
    ┌─────────────────────────────────────────────────┐
    │                                                 │
    │   PeginVerifyRequest                            │
    │   ├── txid (Bitcoin tx)                         │
    │   ├── block_hash                                │
    │   ├── evm_account                               │
    │   └── required_confirmations                    │
    │                                                 │
    │   Heartbeat (keep-alive every 30s)              │
    │                                                 │
    └─────────────────────────────────────────────────┘

                    GOVERNANCE → VALIDATOR
    ┌─────────────────────────────────────────────────┐
    │                                                 │
    │   PeginVerifyResponse (ACK/NACK)                │
    │                                                 │
    │   ValidatorSetUpdate   ──► queued at H+2        │
    │   ParameterUpdate      ──► queued at H+1        │
    │   EmergencyAction      ──► immediate (H+0)      │
    │                                                 │
    └─────────────────────────────────────────────────┘
```

---

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Pegin verification** | Blocking | Validator waits for governance ACK before accepting pegin. Simpler flow, matches BFT consensus model where all validators must agree. Non-blocking would require complex state management for pending pegins. |
| **Proto schema** | anduro-governance compatible | Protobuf definitions mirror the production governance service's ChatMessage pattern. Enables migration to real governance with config change only—no code changes required. |
| **Crate separation** | `governance-proto` + `mock-governance` | Proto crate is shared dependency for both mock server and validators, ensuring schema consistency. Mock server is isolated binary that can be deployed independently or excluded from production builds. |
| **GovernanceClientActor** | Dedicated actor with Arc<RwLock<Client>> | Actor pattern provides clean message-based interface for ChainActor. RwLock allows concurrent read access for heartbeats while write access is used for connection management. Automatic reconnection (5s interval) handles transient failures. |
| **Fallback behavior** | Accept all pegins when governance not configured | Backwards compatible for development/testing without governance service. Production deployments should always configure `GOVERNANCE_GRPC_URL`. |

---

## Configuration

```bash
# Environment variables
GOVERNANCE_GRPC_URL=http://mock-governance:50051
GOVERNANCE_AUTH_TOKEN=test-token-123

# CLI flags (alternative)
--governance-grpc-url http://localhost:50051
--governance-auth-token my-secret-token
```

---

## Real Governance Service Integration

### Migration Path

The mock governance service is designed for drop-in replacement with the production anduro-governance service:

| Step | Action | Type |
|------|--------|------|
| 1 | Update `GOVERNANCE_GRPC_URL` to production endpoint | Config |
| 2 | Update `GOVERNANCE_AUTH_TOKEN` with production credentials | Config |
| 3 | Remove mock-governance from docker-compose | Config |
| 4 | Rebuild validators (only if proto schema changed) | Build |

### Proto Compatibility

The `governance.proto` schema matches the anduro-governance ChatMessage pattern:

- `GovernanceRequest` / `GovernanceResponse` wrapper messages
- `chain` field for multi-chain support
- `request_id` for request/response correlation
- Oneof payload for extensible message types

If the production governance service uses an updated schema:

1. Update `crates/governance-proto/proto/governance.proto`
2. Rebuild all validators
3. No changes needed to actor code if message semantics unchanged

### Production Checklist

- [ ] Verify governance service endpoint is reachable from validator network
- [ ] Confirm authentication token is valid
- [ ] Test pegin verification flow end-to-end
- [ ] Monitor logs for connection/reconnection events
- [ ] Verify validator set updates propagate correctly
- [ ] Confirm emergency actions trigger immediate response
