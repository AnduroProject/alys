# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Knowledge Graph Documentation

For detailed architectural understanding, refer to these comprehensive knowledge graphs:

- **`docs/knowledge/root.knowledge.md`**: Master system architecture overview synthesizing all components
- **`docs/knowledge/app.knowledge.md`**: Application layer architecture (`app/src/`) with consensus, networking, and mining
- **`docs/knowledge/federation.knowledge.md`**: Federation crate architecture (`crates/federation/`) with two-way peg system  
- **`docs/knowledge/lighthouse.knowledge.md`**: Lighthouse wrapper (`crates/lighthouse_wrapper/`) Ethereum integration

These knowledge graphs provide deep architectural insights, component relationships, data flows, security patterns, and integration points that are essential for understanding and working effectively with the Alys codebase.

## Project Overview

Alys is a merged mined Bitcoin sidechain that uses BTC as its base currency and implements a two-way peg system. The project consists of three main components:

- **Consensus Layer** (`app/`): Contains the consensus client for block production and finalization using optimistic merged mining with federated PoA
- **Smart Contracts** (`contracts/`): Bridge contracts for peg-out operations written in Solidity using Foundry
- **Support Crates** (`crates/`): Federation logic for peg-in/peg-out handling and Bitcoin miner interaction

## Development Commands

### Build and Testing
```bash
# Build all components
cargo build

# Run unit tests (self-contained, no services needed)
cargo test

# Format Rust code
cargo fmt

# Check for compilation errors without building
cargo check
```

### Smart Contract Development
```bash
cd contracts/
forge build     # Build contracts
forge test      # Run contract tests
forge fmt       # Format Solidity code
```

### Local Network Development
```bash
# Start 3-node local network with mining
./scripts/start_network.sh

# Start testnet connection
./scripts/start_testnet_alys.sh

# Individual component scripts
./scripts/start_geth.sh       # Start Ethereum execution layer
./scripts/start_reth.sh       # Alternative execution client
```

### Test Scripts
Located in `scripts/tests/`:
- `1_produce_signed_blocks.sh` - Basic block production
- `2_merged_mining.sh` - Merged mining functionality  
- `3_peg_in.sh` - Peg-in operations
- `4_evm.sh` - EVM compatibility
- `5_peg_out.sh` - Peg-out operations
- `6_network_e2e.sh` - End-to-end network tests

## Architecture

### Consensus Architecture
- **Optimistic Merged Mining**: Federation produces signed blocks optimistically, Bitcoin miners provide PoW finalization
- **Hybrid Consensus**: Separates block production (fast, federated) from finalization (secure, PoW)
- **Aura PoA**: Federation uses Proof-of-Authority for signed block production
- **Block Bundles**: Miners commit to batches of signed blocks for efficiency

### Two-Way Peg System
- **Peg-in**: Bitcoin → Alys via federation-controlled multisig addresses with 6 confirmation requirement
- **Peg-out**: Alys → Bitcoin via bridge contract burn events processed by federation
- **Federation**: Distributed key management using BLS signatures and taproot multisig

### Key Components
- `app/src/engine.rs`: Execution layer interface (Geth/Reth integration)
- `app/src/aura.rs`: Aura PoA consensus implementation
- `app/src/auxpow_miner.rs`: Auxiliary PoW mining coordination
- `app/src/chain.rs`: Core blockchain logic and Bitcoin wallet integration
- `crates/federation/`: Bitcoin signing, UTXO management, and bridge operations
- `crates/miner/`: Mining client for auxiliary PoW

### Network Architecture
- **P2P Layer**: libp2p with Gossipsub for block/transaction propagation
- **RPC Interface**: JSON-RPC compatible with Ethereum tooling (port 8545)
- **Consensus RPC**: Internal federation communication (port 3000)
- **Multiple Execution Clients**: Supports both Geth and Reth

## Key Configuration Files

- `etc/config/chain.json` - Chain specification (authorities, federation, Bitcoin params)
- `etc/config/genesis.json` - Ethereum genesis with pre-deployed bridge contract at `0xbBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB`
- `etc/config/eth-config.toml` - Geth configuration
- `Cargo.toml` - Rust workspace configuration

## Development Notes

### Prerequisites
- Rust 1.87.0+
- Geth 1.14.10+ or Reth
- Bitcoin Core 28.0+
- Foundry for smart contracts
- Standard build tools (clang, cmake, pkg-config, libssl-dev)

### Local Development Flow
1. Use `scripts/start_network.sh` to start multi-node local network
2. Network automatically starts Bitcoin regtest, Geth nodes, and Alys consensus nodes
3. Default dev private key: `0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80`
4. Bridge contract pre-deployed for immediate testing

### Testing Integration
- Peg-in: `./scripts/regtest_pegin.sh [amount] [evm_address]`
- Peg-out: `./scripts/regtest_pegout.sh [private_key] [btc_address]`
- Balance checking: `cast balance [address] --rpc-url localhost:8545`

### Chain Compatibility
- EVM compatible (supports MetaMask, Foundry, Hardhat)
- Chain ID: 263634 (local), 212121 (testnet)
- Conversion: 1 BTC = 10^18 wei (satoshi to wei scaling)

## Important Constants

- **Default Ports**: 8545 (EVM RPC), 3000 (Consensus RPC), 30303 (P2P)
- **Block Time**: 2 seconds (configurable via `slotDuration`)
- **PoW Timeout**: 10 blocks without PoW triggers halt (`maxBlocksWithoutPow`)
- **Bridge Address**: `0xbBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB`
- **Burn Address**: `0x000000000000000000000000000000000000dEaD`
- Never reference claude as an author, contributor, created by, etc. in git commits, jira issues, etc.