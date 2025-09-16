# Alys - Bitcoin Sidechain with Two-Way Peg (V2 Migration)

[![CI Status](https://github.com/AnduroProject/alys/workflows/CI/badge.svg)](https://github.com/AnduroProject/alys/actions)
[![Docker](https://github.com/AnduroProject/alys/workflows/Docker/badge.svg)](https://github.com/AnduroProject/alys/pkgs/container/alys)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Migration Progress](https://img.shields.io/badge/V2_Migration-Phase_0_Foundation-yellow.svg)](#v2-migration-status)

Alys is a merged mined Bitcoin sidechain that uses BTC as its base currency and implements a two-way peg system. This repository contains the **V2 migration branch**, which is transitioning from a monolithic architecture to an actor-based system for improved reliability, performance, and maintainability.

## 🚀 Project Overview

### Core Features
- **Merged Mining**: Bitcoin miners can mine Alys blocks alongside Bitcoin blocks
- **Two-Way Peg**: Secure BTC ↔ Alys transfers via federation-controlled multisig
- **EVM Compatibility**: Full Ethereum JSON-RPC compatibility (supports MetaMask, Hardhat, Foundry)
- **Federated Consensus**: Proof-of-Authority with BLS signatures and Bitcoin PoW finalization
- **Actor Architecture** (V2): Message-passing system replacing Arc<RwLock<>> patterns

### Architecture (V2)
- **Consensus Layer**: Optimistic merged mining with federated block production
- **Actor System**: Isolated actors for Chain, Engine, Bridge, Sync, and Network operations
- **Two-Way Peg**: Bitcoin ↔ Alys transfers with 6-confirmation security
- **Smart Contracts**: Solidity-based bridge contracts with automatic peg-out processing

## 🏗️ V2 Migration Status

### Current Phase: **Foundation Setup** 
**Progress: 0% Complete** | **Target Completion: Q1 2025**

#### Migration Overview
The V2 migration is restructuring Alys from a monolithic architecture to an actor-based system to eliminate deadlocks, improve concurrency, and enhance fault tolerance.

#### Epic Status
| Epic | Status | Progress | Subtasks | Estimated Hours |
|------|--------|----------|----------|-----------------|
| [ALYS-001](https://anduroproject.atlassian.net/browse/AN-285) | 🟡 In Progress | 0% | 42 tasks | 24-32h |
| [ALYS-002](docs/v2/jira/issue_2.md) | ⚪ Planned | 0% | 28 tasks | 32-40h |
| [ALYS-003](docs/v2/jira/issue_3.md) | ⚪ Planned | 0% | 24 tasks | 20-24h |
| [ALYS-004](docs/v2/jira/issue_4.md) | ⚪ Planned | 0% | 12 tasks | 12-16h |
| [ALYS-005](docs/v2/jira/issue_5.md) | ⚪ Planned | 0% | 22 tasks | 24-32h |


#### Critical Dependencies
1. **Lighthouse V5 Integration**: Consensus upgrade for improved performance
2. **Anduro Governance**: Secure key management via gRPC streaming  
3. **Actor System Foundation**: Core framework for all migration phases

## 🔧 Repository Structure

### Main Components
- **[app](./app)**: Consensus client for block production and finalization, federation client for peg operations
- **[contracts](./contracts)**: Smart contracts for burning bridged BTC to trigger peg-out process
- **[crates](./crates)**: Peg-in/peg-out logic and Bitcoin miner interaction
- **[docs](./docs/src/README.md)**: Architecture documentation and knowledge base


## 📋 Prerequisites

### System Requirements
- **Rust**: 1.87.0+ with `cargo`, `rustc`, `rustfmt`
- **Bitcoin Core**: 28.0+ (for merged mining and peg operations)
- **Execution Client**: Geth 1.14.10+ or Reth (EVM execution layer)
- **Build Tools**: `clang`, `cmake`, `pkg-config`, `libssl-dev`

### Installation Commands
```bash
# Rust (if not installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Bitcoin Core
# MacOS: brew install bitcoin
# Ubuntu: sudo add-apt-repository ppa:bitcoin/bitcoin && sudo apt-get update && sudo apt-get install bitcoind
# Arch: yay bitcoin-core

# Geth
# https://geth.ethereum.org/docs/getting-started/installing-geth

# Foundry (smart contract development)
curl -L https://foundry.paradigm.xyz | bash && foundryup
```

### Development Tools (Optional)
- **Docker**: Container orchestration for local networks
- **Node.js**: Frontend development and testing tools

## 🛠️ Installation & Setup

### Quick Start (Local Development)
```bash
# Clone repository
git clone https://github.com/AnduroProject/alys.git
cd alys

# Build all components
cargo build --release

# Start 3-node local network (Bitcoin regtest + Geth + Alys)
./scripts/start_network.sh

# Verify network is running
cast balance 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 --rpc-url localhost:8545
```

### Component Build Commands
```bash
# Build consensus client
cargo build --bin alys

# Build smart contracts  
cd contracts/ && forge build

# Run all tests
cargo test --workspace

# Format code
cargo fmt --all
```

## 📖 Getting Started Guides

### Recommended Setup Options
* **[Docker Compose Setup](./docs/guides/getting_started_docker_setup.md)** - Quickest way to get started
* **[Manual Setup](./docs/guides/getting_started_manual_setup.md)** - Full control for local development

## 🚦 Network Configuration

### Local Development
- **Chain ID**: 263634
- **RPC Endpoint**: http://localhost:8545
- **Consensus RPC**: http://localhost:3000
- **P2P Port**: 30303

### Testnet
- **Chain ID**: 212121  
- **RPC Endpoint**: https://testnet-rpc.alys.network
- **Explorer**: http://testnet.alyscan.io/
- **Faucet**: https://faucet.anduro.io/

### Important Addresses
- **Bridge Contract**: `0xbBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB`
- **Burn Address**: `0x000000000000000000000000000000000000dEaD`
- **Dev Private Key**: `0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80`

## 🌐 Connecting to Alys Testnet

Anduro operates a public testnet for development & testing. Connect your node using the peer information below:

### Alys Node #1:
```shell
IP: 209.160.175.123
Enode: enode://4a131d635e3b1ab30624912f769a376581087a84eef53f4fccc28bac0a45493bd4e2ee1ff409608c0993dd05e2b8a3d351e65a7697f1ee2b3c9ee9b49529958f@209.160.175.123:30303
```

### Alys Node #2:
```shell
IP: 209.160.175.124
Enode: enode://15d60f94195b361bf20acfd8b025b8f332b79f5752637e225e7c73aca7b17dd978ca94ab825d0f5221210e69ffcd96e910a257e25ff936c918335c44cc7041ba@209.160.175.124:30303
```

### Alys Node #3:
```shell
IP: 209.160.175.125
Enode: enode://53d6af0f549e4f9b4f768bc37145f7fd800fdbe1203652fd3d2ff7444663a4f5cfe8c06d5ed4b25fe3185920c28b2957a0307f1eed8af49566bba7e3f0c95b04@209.160.175.125:30303
```

### Establishing Peer Connections
```bash
# Connect to any testnet node
cast rpc admin_addTrustedPeer '["<enode_url_of_any_alys_node_listed_above>"]'
```

## 🔧 Development Commands

### Local Network Operations
```bash
# Start full local network
./scripts/start_network.sh

# Start individual components
./scripts/start_geth.sh       # Ethereum execution layer
./scripts/start_reth.sh       # Alternative execution client  
./scripts/start_testnet_alys.sh # Connect to testnet

# Test operations
./scripts/regtest_pegin.sh 0.1 0xYourAddress    # Peg-in 0.1 BTC
./scripts/regtest_pegout.sh $PRIVATE_KEY $BTC_ADDR # Peg-out to Bitcoin
```

### Testing & Validation
```bash
# Unit tests (no external services required)
cargo test --workspace

# Integration tests (requires local network)
./scripts/tests/6_network_e2e.sh

# Specific test suites  
./scripts/tests/1_produce_signed_blocks.sh # Block production
./scripts/tests/2_merged_mining.sh         # Mining integration
./scripts/tests/3_peg_in.sh               # Bitcoin → Alys
./scripts/tests/5_peg_out.sh              # Alys → Bitcoin
```

### Smart Contract Development
```bash
cd contracts/

# Build contracts
forge build

# Run contract tests
forge test -vvv

# Deploy to local network
forge script script/Deploy.s.sol --rpc-url localhost:8545 --broadcast

# Interact with contracts
cast call $BRIDGE_ADDRESS "balanceOf(address)" $YOUR_ADDRESS --rpc-url localhost:8545
```

## 📊 Key Metrics & Monitoring

### Performance Targets
- **Block Time**: 2 seconds (configurable via `slotDuration`)
- **Sync Speed**: >100 blocks/second during catch-up
- **Transaction Throughput**: 1000+ TPS (EVM compatible)
- **Peg Confirmation**: 6 Bitcoin blocks for peg-in security

### Monitoring Endpoints
- **Metrics**: http://localhost:9090/metrics (Prometheus format)
- **Health**: http://localhost:9090/health
- **Chain Status**: `curl localhost:3000/status`

## 🧪 Testing Strategy

### Test Categories
1. **Unit Tests**: Component isolation, fast feedback
2. **Integration Tests**: Multi-component interaction validation
3. **Property Tests**: Randomized input validation with PropTest
4. **Chaos Tests**: Network partitions, Byzantine behavior simulation
5. **Performance Tests**: Throughput and latency benchmarking

### Test Execution
```bash
# Complete test suite
cargo test --all-features --workspace

# Integration tests with Docker environment
docker-compose -f docker-compose.test.yml up -d
cargo test --test integration_tests --features integration
docker-compose -f docker-compose.test.yml down -v

# Property-based testing  
PROPTEST_CASES=10000 cargo test --test property_tests

# Performance benchmarks
cargo bench --features bench
```

## 💰 Two-Way Peg Operations

### Peg-In (Bitcoin → Alys)

#### Get Federation Deposit Address
```bash
curl --silent -H "Content-Type: application/json" \
  -d '{"id":"1", "jsonrpc":"2.0", "method": "getdepositaddress", "params":[]}' \
  http://localhost:3000 | jq -r .result
```

#### Execute Peg-In
```bash
# Automated peg-in with script
EVM_ADDRESS="09Af4E864b84706fbCFE8679BF696e8c0B472201"
./scripts/regtest_pegin.sh "1.0" $EVM_ADDRESS

# Or use default dev address
./scripts/regtest_pegin.sh
```

#### Verify Peg-In Success
```bash
# Check balance (satoshis converted to wei: 1 BTC = 10^18 wei)
cast balance 0x09Af4E864b84706fbCFE8679BF696e8c0B472201 --rpc-url localhost:8545
# Expected: 1000000000000000000 (1 BTC in wei)
```

### Peg-Out (Alys → Bitcoin)

#### Execute Peg-Out
```bash
# Peg-out using bridge contract at 0xbBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB
PRIVATE_KEY=0xb9176fa68b7c590eba66b7d1894a78fad479d6259e9a80d93b9871c232132c01
./scripts/regtest_pegout.sh $PRIVATE_KEY $BTC_ADDRESS

# Or use default dev key
./scripts/regtest_pegout.sh
```

#### Verify Peg-Out Success
```bash
# Check Bitcoin wallet for received transaction
bitcoin-cli -regtest -rpcuser=rpcuser -rpcpassword=rpcpassword \
  listtransactions "*" 3
```

## 🔐 Security Considerations

### Production Security
- **Federation Keys**: Multi-party computation with hardware security modules
- **Bitcoin Integration**: 6-confirmation requirement for peg-in finality
- **Bridge Contracts**: Formally verified Solidity with comprehensive testing
- **Network Security**: BLS signature aggregation with slashing conditions

### Development Security
- **Private Keys**: Never commit keys to repository
- **Test Networks**: Use regtest/testnet for all development
- **Dependencies**: Regular `cargo audit` for vulnerability scanning

## 🛠️ EVM Tooling & Smart Contract Examples

### Example ERC20 Deployment
```bash
cd contracts/

# Deploy example ERC20 contract
PRIVATE_KEY=0xb9176fa68b7c590eba66b7d1894a78fad479d6259e9a80d93b9871c232132c01
forge create --rpc-url http://127.0.0.1:8545 --private-key $PRIVATE_KEY \
  src/MockErc20.sol:MockErc20 --json \
  --constructor-args "HelloBitcoinContract" "HBC" 100000000000000000000000

# Expected output:
# {"deployer":"0x09Af4E864b84706fbCFE8679BF696e8c0B472201","deployedTo":"0x1C36129916E3EA2ACcD516Ae92C8f91deF7c4146","transactionHash":"0x..."}
```

### Interacting with Contracts
```bash
# Transfer ERC20 tokens
cast send --private-key $PRIVATE_KEY --rpc-url localhost:8545 --chain 263634 \
  0x1C36129916E3EA2ACcD516Ae92C8f91deF7c4146 \
  "transfer(address,uint256)" 0xd362E49EE9453Bf414c35288cD090189af2B2C55 100000000

# Transfer native BTC (wei units)
cast send --private-key $PRIVATE_KEY --rpc-url localhost:8545 \
  0xd362E49EE9453Bf414c35288cD090189af2B2C55 --value 16200000000007550
```

### Supported Tools
- **MetaMask**: Full wallet integration support
- **Foundry**: Complete smart contract development suite
- **Hardhat**: JavaScript-based development framework
- **Blockscout**: Blockchain explorer integration

### Setting Up Blockscout Explorer
```bash
# Clone and setup Blockscout
git clone https://github.com/blockscout/blockscout.git
cd blockscout/docker-compose

# Configure for Alys
# Edit docker-compose/envs/common-blockscout.yml:
# SUBNETWORK=Merged ALYS
# CHAIN_ID=263634

# Edit docker-compose/envs/common-frontend.yml:
# NEXT_PUBLIC_NETWORK_NAME=Merged ALYS Alpha
# NEXT_PUBLIC_NETWORK_SHORT_NAME=Merged ALYS Alpha

# Start explorer
docker-compose -f geth.yml up --build

# Access at http://localhost:80

# Reset data if needed
sudo rm -rf services/redis-data services/stats-db-data services/blockscout-db-data services/logs
```

## ⚙️ Configuration Files

### Genesis Configuration
- **[genesis.json](./data/genesis.json)**: Ethereum genesis config for Geth (post-Capella)
- **Bridge Contract**: Pre-deployed at `0xbBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB`

### Chain Specification
- **[chain.json](./etc/config/chain.json)**: Alys consensus configuration
- **Key Parameters**:
  - `slotDuration`: Block time in milliseconds (default: 2000ms)
  - `authorities`: BLS public keys for federation signing
  - `federation`: EVM addresses for fee collection
  - `maxBlocksWithoutPow`: PoW timeout threshold (default: 10 blocks)

### Important Configuration Notes
- All federation members must use identical genesis and chain specs
- Federation EVM addresses receive transaction fees directly
- Bitcoin scanning starts from `bitcoinStartHeight` (0 for development)

## 🚀 Deployment

### Docker Deployment
```bash
# Build Docker image
docker build -t alys:latest .

# Run with Docker Compose  
docker-compose -f docker-compose.yml up -d

# Check deployment status
docker-compose ps
docker logs alys_consensus_1
```

## 📚 Documentation

### Architecture Documentation
- [**Root Architecture**](docs/knowledge/root.knowledge.md) - Complete system overview
- [**App Layer**](docs/knowledge/app.knowledge.md) - Consensus and networking  
- [**Federation**](docs/knowledge/federation.knowledge.md) - Two-way peg system
- [**Lighthouse Integration**](docs/knowledge/lighthouse_wrapper.knowledge.md) - Ethereum consensus

### Migration Documentation
- [**V2 Migration Strategy**](docs/v2/migration-strategy.md) - Complete migration approach
- [**Actor System Guide**](docs/v2/actor-system-guide.md) - Developer guide for actors
- [**Performance Comparison**](docs/v2/performance-analysis.md) - V1 vs V2 benchmarks

### API Documentation
```bash
# Generate API documentation
cargo doc --no-deps --document-private-items --all-features --open
```

## 🤝 Contributing

### Development Workflow
1. **Fork** the repository and create a feature branch
2. **Follow** Rust best practices and existing code style
3. **Test** thoroughly with unit, integration, and property tests  
4. **Document** changes in code comments and architecture docs
5. **Submit** PR with comprehensive description and test evidence

### Code Quality Standards
- **Coverage**: Minimum 80% test coverage for new code
- **Linting**: Zero `clippy` warnings with `cargo clippy --all-targets`
- **Formatting**: Consistent style with `cargo fmt --all`
- **Documentation**: All public APIs documented with examples

### Commit Guidelines
- **Conventional Commits**: Use semantic prefixes (`feat:`, `fix:`, `docs:`)
- **Scope**: Include component scope (`feat(consensus):`, `fix(bridge):`)
- **Tests**: Include test evidence in PR description
- **Breaking Changes**: Clearly document API/behavior changes

## 📄 License

Licensed under the Apache License 2.0. See [LICENSE](LICENSE) for details.

## 🆘 Support & Resources

### Community
- **GitHub Issues**: Bug reports and feature requests
- **Discussions**: Technical discussions and Q&A
- **Discord**: [Real-time community support](https://discord.gg/Me3gjyZ2Nh)

### Development Resources  
- **Claude Code Assistance**: See [CLAUDE.md](CLAUDE.md) for AI development support
- **Knowledge Base**: [docs/knowledge/](docs/knowledge/) for architectural insights
- **Migration Tracking**: [Jira Board](https://anduroproject.atlassian.net/browse/AN-285) for progress updates

### Emergency Procedures
- **Rollback**: `kubectl rollout undo deployment/alys-consensus`
- **Circuit Breaker**: Update feature flags in `config/features-production.toml`
- **Incident Response**: Follow [incident-response.md](docs/incident-response.md)

## 🔗 Important Links

- **[Alys Testnet Explorer](https://testnet.alyscan.io/)**
- **[Alys Faucet](https://faucet.anduro.io/)**
- **[GitHub Repository](https://github.com/anduroproject/alys)**
- **[Twitter](https://twitter.com/andurobtc)**

## 📖 Technical References

- [Eth1-Eth2 Client Relationship](https://ethresear.ch/t/eth1-eth2-client-relationship/7248)
- [Engine API Documentation](https://hackmd.io/@danielrachi/engine_api)
- [Ethereum JSON-RPC API](https://ethereum.org/en/developers/docs/apis/json-rpc/)
- [Aura Consensus Algorithm](https://openethereum.github.io/Aura.html)
- [Merged Mining Specification](https://en.bitcoin.it/wiki/Merged_mining_specification)

---

**V2 Migration Status**: Foundation phase in progress. See [Migration Dashboard](https://anduroproject.atlassian.net/browse/AN-285) for real-time updates.
