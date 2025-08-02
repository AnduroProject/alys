# Alys Junior Rust Engineer Onboarding Guide

**🚀 New to Alys? Start with the [Quick Start Guide](./QUICK_START_GUIDE.md) for a 15-minute introduction!**

This comprehensive guide provides in-depth knowledge for junior Rust engineers joining the Alys project. It covers everything from basic setup to advanced contribution patterns.

## Table of Contents

1. [Welcome to Alys](#welcome-to-alys)
2. [Project Overview](#project-overview)
3. [Prerequisites & Setup](#prerequisites--setup)
4. [Understanding the Codebase](#understanding-the-codebase)
5. [Key Concepts](#key-concepts)
6. [Development Workflow](#development-workflow)
7. [Testing Guidelines](#testing-guidelines)
8. [Rust Patterns in Alys](#rust-patterns-in-alys)
9. [Common Tasks](#common-tasks)
10. [Troubleshooting](#troubleshooting)
11. [Contributing Guidelines](#contributing-guidelines)
12. [Learning Resources](#learning-resources)

---

## Welcome to Alys

Welcome to the Alys project! This guide is specifically designed for junior Rust engineers who are new to the codebase. Alys is a sophisticated Bitcoin sidechain project that combines cutting-edge blockchain technology with robust Rust engineering practices.

**What you'll learn:**
- How Bitcoin sidechains work and the role of merged mining
- Advanced Rust patterns used in blockchain development
- How to navigate and contribute to a large, complex Rust codebase
- Testing strategies for distributed systems
- Working with consensus mechanisms and cryptographic operations

**What makes Alys special:**
- **Bitcoin-native:** Uses BTC as the native currency
- **EVM Compatible:** Supports Ethereum smart contracts
- **Merged Mining:** Leverages Bitcoin's security through auxiliary proof-of-work
- **Federation-based:** Uses a federated approach for transaction sequencing and bridge operations

---

## Project Overview

### What is Alys?

Alys is a **merged mined Bitcoin sidechain** that:
- Uses **BTC as its base currency**
- Reaches consensus through **auxiliary Proof-of-Work (AuxPoW)** executed by Bitcoin miners and a federation
- Facilitates a **two-way peg** between Bitcoin and the Alys sidechain through federation members
- Supports **EVM smart contracts** while maintaining Bitcoin compatibility

### High-Level Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Bitcoin Core  │    │   Alys Network  │    │   EVM Layer     │
│                 │◄──►│                 │◄──►│                 │
│ - Merged Mining │    │ - Consensus     │    │ - Smart Contracts│
│ - Block Headers │    │ - Federation    │    │ - DeFi Apps     │
│ - Security      │    │ - Two-way Peg   │    │ - Tooling       │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

### Repository Structure

The Alys repository follows a **Cargo workspace** pattern with these main components:

```
alys/
├── app/                    # Main consensus client & federation client
│   ├── src/
│   │   ├── main.rs        # Entry point
│   │   ├── app.rs         # Main application logic
│   │   ├── aura.rs        # Aura consensus algorithm
│   │   ├── auxpow.rs      # Auxiliary Proof-of-Work handling
│   │   ├── engine.rs      # Consensus engine
│   │   ├── rpc.rs         # JSON-RPC server
│   │   └── ...
│   └── Cargo.toml
├── crates/                 # Shared libraries
│   ├── federation/        # Two-way peg & Bitcoin integration
│   ├── lighthouse_wrapper/ # Lighthouse consensus integration  
│   └── miner/             # Mining-related functionality
├── contracts/             # Smart contracts for bridge operations
├── docs/                  # Architecture documentation
├── scripts/               # Helper scripts for development
└── Cargo.toml            # Workspace configuration
```

---

## Prerequisites & Setup

### Required Software

Before you start, ensure you have the following installed:

#### 1. Rust Toolchain
```bash
# Install Rust 1.87.0 or higher
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Verify installation
rustc --version  # Should be 1.87.0+
cargo --version
```

#### 2. Bitcoin Core (v28.0+)
```bash
# macOS
brew install bitcoin

# Ubuntu
sudo add-apt-repository ppa:bitcoin/bitcoin
sudo apt-get update && sudo apt-get install bitcoind

# Arch Linux
yay bitcoin-core

# Verify installation
bitcoind --version
bitcoin-cli --version
```

#### 3. Ethereum Tools
```bash
# Install Geth 1.14.10+
# Visit: https://geth.ethereum.org/docs/getting-started/installing-geth

# Install Foundry (for smart contracts)
curl -L https://foundry.paradigm.xyz | bash
foundryup

# Verify installations
geth version
forge --version
cast --version
```

#### 4. System Dependencies
```bash
# Ubuntu/Debian
sudo apt-get install build-essential pkg-config libssl-dev cmake clang

# macOS (with Homebrew)
brew install cmake pkg-config openssl

# Arch Linux
sudo pacman -S base-devel cmake clang pkgconf openssl
```

### Environment Setup

#### 1. Clone the Repository
```bash
git clone https://github.com/AnduroProject/alys.git
cd alys
```

#### 2. Build the Project
```bash
# This may take 10-15 minutes on first build
# Use retry setting to handle network issues
CARGO_NET_RETRY=10 cargo build --release
```

**Note for Beginners:** Build failures are common on first attempt due to:
- Complex dependency tree including Lighthouse components
- Network timeouts downloading dependencies
- Missing system certificates (especially in containers)

**If build fails with certificate errors:**
```bash
# Update certificates (Ubuntu/Debian)
sudo apt-get update && sudo apt-get install ca-certificates

# For macOS, update certificates
brew install ca-certificates

# Retry build
CARGO_NET_RETRY=10 cargo build --release
```

**If still having issues, try a simpler approach:**
```bash
# Build just the main binary first
cargo build --bin app

# Or work offline if dependencies are cached
cargo build --offline
```

#### 3. Run Tests
```bash
# Run all tests (may take several minutes)
cargo test

# Run tests for a specific crate
cargo test -p federation
cargo test -p app
```

### Development Environment

#### Recommended IDE Setup
- **VS Code** with the `rust-analyzer` extension
- **IntelliJ IDEA** with the Rust plugin
- **Neovim** with `rust-analyzer` LSP

#### Useful Tools
```bash
# Code formatting
cargo fmt

# Linting
cargo clippy

# Documentation generation
cargo doc --open

# Dependency analysis
cargo tree
```

---

## Understanding the Codebase

### Core Components Deep Dive

#### 1. The `app` Crate - Heart of Alys

**Purpose:** The main binary that runs the Alys node, handling consensus, networking, and federation operations.

**Key Files:**
- `main.rs`: Entry point that calls `app::run()`
- `app.rs`: Main application setup and coordination
- `engine.rs`: Consensus engine implementing the Engine API
- `aura.rs`: Aura (Authority Round) consensus algorithm
- `auxpow.rs`: Auxiliary Proof-of-Work validation and handling
- `rpc.rs`: JSON-RPC server for external communication

**Example: Understanding the main flow**
```rust
// In main.rs
fn main() {
    if let Err(err) = app::run() {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}

// This calls app::run() which:
// 1. Parses CLI arguments
// 2. Sets up logging and metrics
// 3. Initializes the consensus engine
// 4. Starts the RPC server
// 5. Begins block production and validation
```

#### 2. The `federation` Crate - Bitcoin Bridge

**Purpose:** Handles the two-way peg between Bitcoin and Alys, including transaction signing and UTXO management.

**Key Concepts:**
- **Peg-in:** Moving BTC from Bitcoin to Alys
- **Peg-out:** Moving BTC from Alys back to Bitcoin
- **Federation signing:** Multi-signature transactions for security

**Example: Federation structure**
```rust
// From federation/src/lib.rs
pub struct Federation {
    // Bitcoin public keys of federation members
    members: Vec<BitcoinPublicKey>,
    // Threshold for signing (e.g., 2-of-3 multisig)
    threshold: u32,
    // UTXO tracking and management
    utxo_manager: UtxoManager,
}
```

#### 3. The `lighthouse_wrapper` Crate - Consensus Integration

**Purpose:** Provides integration with Lighthouse's consensus layer components while adapting them for the Alys-specific consensus mechanism.

### Data Flow Overview

Understanding how data flows through Alys is crucial for effective development:

```
1. Transaction Submission
   User → JSON-RPC → Engine → Mempool

2. Block Production (by Federation)
   Mempool → Block Builder → Aura Consensus → Block

3. Block Finalization (by Bitcoin Miners)
   Block → AuxPoW Validation → Bitcoin Block → Finalized

4. Peg Operations
   Bitcoin → Federation → Bridge Contract → Alys
   Alys → Bridge Contract → Federation → Bitcoin
```

### Module Dependencies

```rust
// Understanding the dependency relationships
app/                    # Main application
├── Uses: federation    # For peg operations
├── Uses: lighthouse_wrapper  # For consensus components
└── Provides: JSON-RPC, Block production

federation/            # Bitcoin bridge logic
├── Uses: bitcoincore-rpc  # Bitcoin integration
├── Uses: ethers       # Ethereum integration
└── Provides: Peg-in/out, Signing

lighthouse_wrapper/    # Consensus abstractions
├── Uses: lighthouse   # Ethereum consensus
└── Provides: Consensus types
```

---

## Key Concepts

### 1. Merged Mining Explained

**What is Merged Mining?**
Merged mining allows Bitcoin miners to simultaneously mine Bitcoin and Alys without additional computational cost.

**How it works:**
1. **Auxiliary Block Creation:** Alys creates blocks with faster timing (e.g., 2 seconds)
2. **Commitment Inclusion:** Bitcoin miners include Alys block commitments in Bitcoin block headers
3. **Security Inheritance:** Alys inherits Bitcoin's security through this process

**In the Code:**
```rust
// From app/src/auxpow.rs
pub struct AuxPow {
    // The Bitcoin block header that includes our commitment
    pub bitcoin_block_header: BitcoinBlockHeader,
    // Merkle proof showing our commitment is in the Bitcoin block
    pub merkle_proof: MerkleProof,
    // Additional validation data
    pub coinbase_txn: Transaction,
}

impl AuxPow {
    pub fn validate(&self, block_hash: &H256) -> Result<(), AuxPowError> {
        // 1. Verify the Bitcoin block header meets difficulty requirements
        // 2. Verify the merkle proof includes our block commitment
        // 3. Ensure the commitment format is correct
    }
}
```

### 2. Federation Consensus (Aura)

**Authority Round (Aura) Consensus:**
- **Round-robin:** Authorities take turns producing blocks
- **Time-based:** Each authority has a specific time slot
- **Fast finality:** Quick block production for user experience

**In the Code:**
```rust
// From app/src/aura.rs
pub struct AuraConsensus {
    authorities: Vec<Address>,  // List of federation members
    step_duration: Duration,    // Time per authority slot
    current_step: u64,         // Current consensus step
}

impl AuraConsensus {
    pub fn is_authority_at_step(&self, address: &Address, step: u64) -> bool {
        let authority_index = step % self.authorities.len() as u64;
        self.authorities[authority_index as usize] == *address
    }
}
```

### 3. Two-Way Peg Mechanism

**Peg-In Process (Bitcoin → Alys):**
1. User sends BTC to federation-controlled address
2. Federation detects the transaction
3. Federation creates equivalent wrapped BTC on Alys
4. User can now use BTC on Alys for smart contracts

**Peg-Out Process (Alys → Bitcoin):**
1. User burns wrapped BTC on Alys via smart contract
2. Federation detects the burn transaction
3. Federation creates and signs Bitcoin transaction
4. User receives BTC on Bitcoin mainnet

**In the Code:**
```rust
// From federation/src/bitcoin_stream.rs
pub async fn process_peg_in(
    &self, 
    bitcoin_tx: &Transaction,
    alys_address: &Address,
) -> Result<(), Error> {
    // 1. Validate Bitcoin transaction
    // 2. Calculate amount (satoshis to wei conversion)
    // 3. Submit transaction to Alys to mint wrapped BTC
}

pub async fn process_peg_out(
    &self,
    burn_event: &BurnEvent,
) -> Result<(), Error> {
    // 1. Validate burn transaction on Alys
    // 2. Create Bitcoin transaction
    // 3. Collect federation signatures
    // 4. Broadcast signed transaction to Bitcoin
}
```

### 4. Economic Model

**Fee Structure:**
- **Transaction fees:** Paid in wrapped BTC on Alys
- **Mining rewards:** Bitcoin miners receive fees for including AuxPoW
- **Federation fees:** Small fees for peg operations

**Currency Conversion:**
```rust
// From federation/src/lib.rs
pub fn wei_to_sats(wei: U256) -> u64 {
    // Bitcoin has 8 decimals, Ethereum has 18
    // Convert wei (10^18) to satoshis (10^8)
    (wei / U256::from(10_000_000_000u64)).as_u64()
}

pub fn sats_to_wei(sats: u64) -> U256 {
    // Convert satoshis to wei for EVM compatibility
    U256::from(sats) * U256::from(10_000_000_000u64)
}
```

---

## Development Workflow

### Daily Development Cycle

#### 1. Starting Your Development Session
```bash
# Always start with the latest code
git pull origin main

# Check if dependencies need updating
cargo check

# Run tests to ensure everything works
cargo test --lib  # Run library tests only (faster)
```

#### 2. Making Changes

**For new features:**
```bash
# Create a feature branch
git checkout -b feature/your-feature-name

# Make your changes
# ... edit files ...

# Check your changes compile
cargo check

# Run relevant tests
cargo test -p app  # Test specific package
cargo test test_name  # Test specific function
```

**For bug fixes:**
```bash
# Create a bugfix branch
git checkout -b fix/issue-description

# Reproduce the bug with a test first (TDD approach)
# ... write failing test ...

# Fix the issue
# ... implement fix ...

# Verify the test now passes
cargo test
```

#### 3. Code Quality Checks

**Before committing:**
```bash
# Format code
cargo fmt

# Check for common issues
cargo clippy

# Run full test suite
cargo test

# Check documentation builds
cargo doc --no-deps
```

### Working with the Codebase

#### Understanding Error Handling

Alys uses `Result` types extensively with custom error types:

```rust
// From app/src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Consensus error: {0}")]
    Consensus(String),
    
    #[error("Network error: {0}")]
    Network(#[from] NetworkError),
    
    #[error("RPC error: {0}")]
    Rpc(#[from] jsonrpc_core::Error),
}

// Usage pattern throughout the codebase
fn some_operation() -> Result<(), AppError> {
    // Operations that might fail
    validate_input()?;  // The ? operator propagates errors
    network_call().await?;
    Ok(())
}
```

#### Async Programming Patterns

Alys makes heavy use of `async`/`await`:

```rust
use tokio::time::{sleep, Duration};
use futures::stream::StreamExt;

// Typical async function in Alys
pub async fn process_blocks(&self) -> Result<(), Error> {
    let mut block_stream = self.subscribe_to_blocks().await?;
    
    while let Some(block) = block_stream.next().await {
        // Process each block
        self.validate_block(&block).await?;
        self.apply_block(block).await?;
        
        // Small delay to prevent overwhelming the system
        sleep(Duration::from_millis(10)).await;
    }
    
    Ok(())
}
```

#### Configuration Management

Configuration is handled through:
1. **CLI arguments** (using `clap`)
2. **Configuration files** (JSON/TOML)
3. **Environment variables**

```rust
// From app/src/app.rs
#[derive(Debug, clap::Parser)]
pub struct Args {
    #[clap(long, default_value = "info")]
    pub log_level: String,
    
    #[clap(long, default_value = "8545")]
    pub rpc_port: u16,
    
    #[clap(long)]
    pub config_file: Option<PathBuf>,
}
```

### Debugging Techniques

#### 1. Logging and Tracing

Alys uses the `tracing` crate for structured logging:

```rust
use tracing::{info, warn, error, debug, instrument};

#[instrument(skip(self))]  // Automatic span creation
pub async fn process_transaction(&self, tx: Transaction) -> Result<(), Error> {
    info!(
        tx_hash = %tx.hash(),
        tx_value = %tx.value(),
        "Processing transaction"
    );
    
    if tx.value() == 0 {
        warn!("Transaction has zero value");
    }
    
    // ... processing logic ...
    
    debug!("Transaction processed successfully");
    Ok(())
}
```

**Viewing logs during development:**
```bash
# Run with specific log level
RUST_LOG=debug cargo run

# Filter logs by module
RUST_LOG=app::engine=debug cargo run

# JSON output for parsing
RUST_LOG=info cargo run --features json-logs
```

#### 2. Testing Individual Components

```bash
# Test a specific module
cargo test app::aura::tests

# Test with output visible
cargo test -- --nocapture

# Test in release mode (faster)
cargo test --release

# Run tests matching a pattern
cargo test test_block_validation
```

#### 3. Using the Debugger

For complex issues, use `lldb` or `gdb`:

```bash
# Build with debug symbols
cargo build

# Run under debugger
rust-lldb ./target/debug/app
# In debugger:
# (lldb) breakpoint set --name main
# (lldb) run
```

---

## Testing Guidelines

### Test Structure in Alys

Alys follows Rust testing best practices with several types of tests:

#### 1. Unit Tests
Located within the same file as the code being tested:

```rust
// From app/src/auxpow.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_auxpow_validation() {
        let auxpow = AuxPow {
            bitcoin_block_header: create_test_header(),
            merkle_proof: create_test_proof(),
            coinbase_txn: create_test_coinbase(),
        };
        
        let block_hash = H256::random();
        assert!(auxpow.validate(&block_hash).is_ok());
    }
    
    #[test]
    fn test_invalid_auxpow_fails() {
        let invalid_auxpow = AuxPow::default();
        let block_hash = H256::random();
        assert!(invalid_auxpow.validate(&block_hash).is_err());
    }
}
```

#### 2. Integration Tests
Located in `tests/` directories within each crate:

```rust
// tests/integration_test.rs
use federation::{Federation, BitcoinCore};
use tokio_test;

#[tokio::test]
async fn test_peg_in_flow() {
    let federation = Federation::new_test();
    let bitcoin_core = BitcoinCore::new_test();
    
    // Simulate Bitcoin transaction
    let tx = bitcoin_core.create_peg_in_transaction().await;
    
    // Process through federation
    let result = federation.process_peg_in(&tx).await;
    
    assert!(result.is_ok());
}
```

#### 3. Property-Based Tests

For complex validation logic, Alys uses property-based testing:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_wei_sats_conversion_roundtrip(sats in 0u64..21_000_000_00_000_000) {
        let wei = sats_to_wei(sats);
        let converted_back = wei_to_sats(wei);
        prop_assert_eq!(sats, converted_back);
    }
}
```

### Writing Effective Tests

#### 1. Test Organization

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    // Helper functions
    fn create_test_federation() -> Federation {
        // Setup common test data
    }
    
    // Happy path tests
    mod happy_path {
        use super::*;
        
        #[test]
        fn test_normal_operation() {
            // Test expected behavior
        }
    }
    
    // Error cases
    mod error_cases {
        use super::*;
        
        #[test]
        fn test_invalid_input() {
            // Test error handling
        }
    }
    
    // Edge cases
    mod edge_cases {
        use super::*;
        
        #[test]
        fn test_boundary_conditions() {
            // Test limits and boundaries
        }
    }
}
```

#### 2. Async Testing Patterns

```rust
#[tokio::test]
async fn test_async_operation() {
    let mut system = TestSystem::new().await;
    
    // Setup
    system.start().await;
    
    // Test
    let result = system.perform_operation().await;
    
    // Verify
    assert!(result.is_ok());
    
    // Cleanup
    system.shutdown().await;
}
```

#### 3. Mock Objects and Test Doubles

```rust
// Creating mock Bitcoin Core for testing
struct MockBitcoinCore {
    transactions: Vec<Transaction>,
}

impl MockBitcoinCore {
    fn add_transaction(&mut self, tx: Transaction) {
        self.transactions.push(tx);
    }
}

#[async_trait]
impl BitcoinRpc for MockBitcoinCore {
    async fn get_transaction(&self, txid: &Txid) -> Result<Transaction, Error> {
        self.transactions
            .iter()
            .find(|tx| tx.txid() == *txid)
            .cloned()
            .ok_or(Error::TxidNotFound)
    }
}
```

### Running Tests Effectively

```bash
# Run all tests
cargo test

# Run tests for specific package
cargo test -p federation

# Run with multiple threads (faster)
cargo test --jobs 4

# Run tests in release mode (faster execution)
cargo test --release

# Show test output
cargo test -- --nocapture

# Run only integration tests
cargo test --test integration_test

# Run tests matching pattern
cargo test peg_in

# Generate code coverage (requires tarpaulin)
cargo tarpaulin --out Html
```

---

## Rust Patterns in Alys

### 1. Error Handling Patterns

#### Custom Error Types with `thiserror`

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlockError {
    #[error("Invalid block hash")]
    InvalidHash,
    
    #[error("Block validation failed: {reason}")]
    ValidationFailed { reason: String },
    
    #[error("Network error")]
    Network(#[from] NetworkError),  // Automatic conversion
    
    #[error("Serialization error")]
    Serde(#[from] serde_json::Error),
}

// Usage
fn validate_block(block: &Block) -> Result<(), BlockError> {
    if block.hash().is_zero() {
        return Err(BlockError::InvalidHash);
    }
    
    if !block.is_valid() {
        return Err(BlockError::ValidationFailed {
            reason: "Invalid merkle root".to_string(),
        });
    }
    
    Ok(())
}
```

#### Error Propagation with `?` Operator

```rust
async fn process_peg_in(tx: &Transaction) -> Result<Receipt, Error> {
    let validated_tx = validate_transaction(tx)?;  // Propagate validation errors
    let amount = extract_amount(&validated_tx)?;   // Propagate extraction errors
    let receipt = submit_to_alys(amount).await?;   // Propagate submission errors
    Ok(receipt)
}
```

### 2. Async Patterns

#### Stream Processing

```rust
use futures::stream::{Stream, StreamExt};
use tokio::sync::mpsc;

pub async fn process_bitcoin_blocks() -> Result<(), Error> {
    let (tx, mut rx) = mpsc::channel(100);
    
    // Producer task
    tokio::spawn(async move {
        let mut stream = bitcoin_client.subscribe_blocks().await;
        while let Some(block) = stream.next().await {
            let _ = tx.send(block).await;
        }
    });
    
    // Consumer task
    while let Some(block) = rx.recv().await {
        process_block(block).await?;
    }
    
    Ok(())
}
```

#### Timeout Handling

```rust
use tokio::time::{timeout, Duration};

async fn safe_network_call() -> Result<Response, Error> {
    let operation = async {
        // Potentially slow network operation
        network_client.call().await
    };
    
    match timeout(Duration::from_secs(30), operation).await {
        Ok(result) => result.map_err(Error::Network),
        Err(_) => Err(Error::Timeout),
    }
}
```

### 3. Configuration Patterns

#### Builder Pattern for Complex Types

```rust
pub struct FederationConfig {
    members: Vec<PublicKey>,
    threshold: u32,
    bitcoin_network: bitcoin::Network,
    timeout: Duration,
}

impl FederationConfig {
    pub fn builder() -> FederationConfigBuilder {
        FederationConfigBuilder::default()
    }
}

#[derive(Default)]
pub struct FederationConfigBuilder {
    members: Vec<PublicKey>,
    threshold: Option<u32>,
    bitcoin_network: Option<bitcoin::Network>,
    timeout: Option<Duration>,
}

impl FederationConfigBuilder {
    pub fn add_member(mut self, key: PublicKey) -> Self {
        self.members.push(key);
        self
    }
    
    pub fn threshold(mut self, threshold: u32) -> Self {
        self.threshold = Some(threshold);
        self
    }
    
    pub fn build(self) -> Result<FederationConfig, ConfigError> {
        Ok(FederationConfig {
            members: self.members,
            threshold: self.threshold.ok_or(ConfigError::MissingThreshold)?,
            bitcoin_network: self.bitcoin_network.unwrap_or(bitcoin::Network::Regtest),
            timeout: self.timeout.unwrap_or(Duration::from_secs(30)),
        })
    }
}

// Usage
let config = FederationConfig::builder()
    .add_member(pubkey1)
    .add_member(pubkey2)
    .threshold(2)
    .build()?;
```

### 4. State Management Patterns

#### State Machines with Enums

```rust
#[derive(Debug, Clone)]
pub enum PegOutState {
    Pending { amount: u64, destination: Address },
    Collecting { signatures: Vec<Signature> },
    Broadcasting { tx: Transaction },
    Confirmed { txid: Txid },
    Failed { error: String },
}

impl PegOutState {
    pub fn advance(&self, event: PegOutEvent) -> Result<Self, Error> {
        match (self, event) {
            (PegOutState::Pending { amount, destination }, PegOutEvent::StartSigning) => {
                Ok(PegOutState::Collecting { signatures: vec![] })
            }
            (PegOutState::Collecting { mut signatures }, PegOutEvent::SignatureReceived(sig)) => {
                signatures.push(sig);
                if signatures.len() >= THRESHOLD {
                    let tx = create_transaction(*amount, *destination, &signatures)?;
                    Ok(PegOutState::Broadcasting { tx })
                } else {
                    Ok(PegOutState::Collecting { signatures })
                }
            }
            _ => Err(Error::InvalidStateTransition),
        }
    }
}
```

### 5. Serialization Patterns

#### Custom Serialization for Bitcoin Types

```rust
use serde::{Deserialize, Serialize, Deserializer, Serializer};

#[derive(Debug, Clone)]
pub struct BlockHash(bitcoin::BlockHash);

impl Serialize for BlockHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for BlockHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let hash = bitcoin::BlockHash::from_str(&s)
            .map_err(serde::de::Error::custom)?;
        Ok(BlockHash(hash))
    }
}
```

### 6. Type Safety Patterns

#### Newtype Pattern for Domain Types

```rust
// Prevent mixing up different types of addresses
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitcoinAddress(bitcoin::Address);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthereumAddress(ethers::types::Address);

impl BitcoinAddress {
    pub fn from_str(s: &str) -> Result<Self, Error> {
        let addr = bitcoin::Address::from_str(s)?;
        Ok(BitcoinAddress(addr))
    }
    
    pub fn to_script_pubkey(&self) -> bitcoin::Script {
        self.0.script_pubkey()
    }
}

// Now it's impossible to accidentally use an Ethereum address as a Bitcoin address
fn send_bitcoin(to: BitcoinAddress, amount: u64) -> Result<(), Error> {
    // Implementation
}
```

---

## Common Tasks

### 1. Adding a New RPC Method

#### Step 1: Define the Method Interface

```rust
// In app/src/rpc.rs
use jsonrpc_core::{Result as RpcResult, Error as RpcError};

pub trait AlysRpc {
    // Existing methods...
    
    #[rpc(name = "getTransactionStatus")]
    fn get_transaction_status(&self, tx_hash: String) -> RpcResult<TransactionStatus>;
}

#[derive(Serialize, Deserialize)]
pub struct TransactionStatus {
    pub status: String,
    pub confirmations: u32,
    pub block_hash: Option<String>,
}
```

#### Step 2: Implement the Method

```rust
impl AlysRpc for RpcImpl {
    fn get_transaction_status(&self, tx_hash: String) -> RpcResult<TransactionStatus> {
        let hash = H256::from_str(&tx_hash)
            .map_err(|_| RpcError::invalid_params("Invalid transaction hash"))?;
            
        let status = self.chain.get_transaction_status(&hash)
            .map_err(|e| RpcError::internal_error())?;
            
        Ok(TransactionStatus {
            status: status.to_string(),
            confirmations: status.confirmations(),
            block_hash: status.block_hash().map(|h| h.to_string()),
        })
    }
}
```

#### Step 3: Add Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_get_transaction_status() {
        let rpc = create_test_rpc();
        let result = rpc.get_transaction_status("0x123...".to_string());
        assert!(result.is_ok());
    }
}
```

### 2. Adding a New Consensus Rule

#### Step 1: Define the Rule

```rust
// In app/src/consensus.rs
pub fn validate_block_extra_rule(block: &Block, parent: &Block) -> Result<(), ConsensusError> {
    // Example: Blocks must have increasing timestamps
    if block.header().timestamp() <= parent.header().timestamp() {
        return Err(ConsensusError::InvalidTimestamp);
    }
    
    // Example: Block size limit
    if block.size() > MAX_BLOCK_SIZE {
        return Err(ConsensusError::BlockTooLarge);
    }
    
    Ok(())
}
```

#### Step 2: Integrate into Validation Pipeline

```rust
// In app/src/engine.rs
pub fn validate_block(&self, block: &Block) -> Result<(), Error> {
    // Existing validations...
    validate_block_header(&block.header())?;
    validate_block_transactions(&block.transactions())?;
    
    // Add new rule
    if let Some(parent) = self.get_parent_block(&block.parent_hash())? {
        validate_block_extra_rule(block, &parent)?;
    }
    
    Ok(())
}
```

#### Step 3: Add Comprehensive Tests

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_block_timestamp_validation() {
        let parent = create_test_block_with_timestamp(100);
        let valid_child = create_test_block_with_timestamp(101);
        let invalid_child = create_test_block_with_timestamp(99);
        
        assert!(validate_block_extra_rule(&valid_child, &parent).is_ok());
        assert!(validate_block_extra_rule(&invalid_child, &parent).is_err());
    }
}
```

### 3. Adding a New Federation Operation

#### Step 1: Define the Operation

```rust
// In crates/federation/src/operations.rs
#[derive(Debug, Clone)]
pub struct EmergencyWithdrawal {
    pub destination: BitcoinAddress,
    pub amount: u64,
    pub reason: String,
}

impl EmergencyWithdrawal {
    pub async fn execute(&self, federation: &Federation) -> Result<Txid, Error> {
        // 1. Create the Bitcoin transaction
        let tx = federation.create_withdrawal_transaction(
            &self.destination,
            self.amount,
        ).await?;
        
        // 2. Collect signatures from federation members
        let signed_tx = federation.collect_signatures(tx).await?;
        
        // 3. Broadcast to Bitcoin network
        let txid = federation.broadcast_transaction(&signed_tx).await?;
        
        info!(
            txid = %txid,
            amount = self.amount,
            reason = %self.reason,
            "Emergency withdrawal executed"
        );
        
        Ok(txid)
    }
}
```

#### Step 2: Add to Federation Interface

```rust
impl Federation {
    pub async fn emergency_withdrawal(
        &self,
        withdrawal: EmergencyWithdrawal,
    ) -> Result<Txid, Error> {
        // Validate authority
        self.require_majority_consent().await?;
        
        // Execute withdrawal
        withdrawal.execute(self).await
    }
}
```

### 4. Debugging Network Issues

#### Enable Detailed Logging

```bash
# Debug libp2p networking
RUST_LOG=libp2p=debug,app::network=debug cargo run

# Debug RPC calls
RUST_LOG=app::rpc=debug cargo run

# Debug Bitcoin integration
RUST_LOG=federation::bitcoin=debug cargo run
```

#### Common Network Debugging

```rust
// In app/src/network/debug.rs
pub fn diagnose_network_health(&self) -> NetworkDiagnostics {
    NetworkDiagnostics {
        peer_count: self.swarm.connected_peers().count(),
        pending_dials: self.swarm.pending_dial_count(),
        recent_errors: self.recent_errors.clone(),
        bandwidth_stats: self.bandwidth_counter.clone(),
    }
}

// Usage in RPC
#[rpc(name = "networkDiagnostics")]
fn network_diagnostics(&self) -> RpcResult<NetworkDiagnostics> {
    Ok(self.network.diagnose_network_health())
}
```

### 5. Performance Optimization

#### Identifying Bottlenecks

```rust
use std::time::Instant;

pub async fn process_blocks_with_timing(&self) -> Result<(), Error> {
    let start = Instant::now();
    
    // Your processing logic
    self.validate_block(&block).await?;
    let validation_time = start.elapsed();
    
    let start = Instant::now();
    self.apply_block(&block).await?;
    let application_time = start.elapsed();
    
    info!(
        validation_ms = validation_time.as_millis(),
        application_ms = application_time.as_millis(),
        "Block processing timing"
    );
    
    Ok(())
}
```

#### Memory Usage Optimization

```rust
// Use bounded channels to prevent memory leaks
use tokio::sync::mpsc;

let (tx, rx) = mpsc::channel(1000);  // Bounded to 1000 items

// Use streaming for large data sets
pub fn get_all_transactions(&self) -> impl Stream<Item = Transaction> {
    futures::stream::iter(self.transaction_store.iter())
}
```

---

## Troubleshooting

### Common Build Issues

#### 1. Dependency Resolution Failures

**Problem:** Network timeouts during `cargo build`
```
warning: spurious network error (3 tries remaining): [28] Timeout was reached
```

**Solutions:**
```bash
# Increase retry count
CARGO_NET_RETRY=10 cargo build

# Use offline mode if dependencies are cached
cargo build --offline

# Clear and rebuild dependency cache
rm -rf ~/.cargo/registry
cargo build
```

#### 2. Certificate/TLS Issues

**Problem:** SSL certificate verification failures
```
error: failed to download from `https://...`
Caused by: ... SSL certificate verification failed
```

**Solutions:**
```bash
# Update system certificates (Ubuntu/Debian)
sudo apt-get update && sudo apt-get install ca-certificates

# For macOS
brew install ca-certificates

# Set Git to use system certificates
git config --global http.sslCAInfo /etc/ssl/certs/ca-certificates.crt

# Alternative: temporarily disable SSL verification (NOT recommended for production)
CARGO_HTTP_CHECK_REVOKE=false cargo build
```

#### 3. Lighthouse Dependency Issues

**Problem:** Lighthouse-related build failures
```
error: failed to run custom build command for `deposit_contract`
```

**Solutions:**
```bash
# Update git submodules
git submodule update --init --recursive

# Try building lighthouse dependencies separately
cargo build -p lighthouse_wrapper

# If persistent, check for updates to lighthouse revision in Cargo.toml
git pull origin main  # May include dependency updates
```

#### 2. Linking Errors

**Problem:** 
```
error: linking with `cc` failed: exit status: 1
```

**Solutions:**
```bash
# Install system dependencies (Ubuntu)
sudo apt-get install build-essential pkg-config libssl-dev

# For macOS, ensure Xcode tools are installed
xcode-select --install

# Set environment variables for OpenSSL
export OPENSSL_DIR=/usr/local/opt/openssl
export PKG_CONFIG_PATH=/usr/local/opt/openssl/lib/pkgconfig
```

#### 3. Git Submodule Issues

**Problem:** Missing or outdated git submodules

**Solutions:**
```bash
# Initialize and update submodules
git submodule update --init --recursive

# Force update if stuck
git submodule foreach --recursive git reset --hard
git submodule update --init --recursive
```

### Runtime Issues

#### 1. RPC Server Won't Start

**Problem:** Port already in use
```
Error: Address already in use (os error 98)
```

**Debugging:**
```bash
# Check what's using the port
lsof -i :8545
netstat -tulpn | grep 8545

# Kill the process or use a different port
cargo run -- --rpc-port 8546
```

#### 2. Bitcoin Connection Issues

**Problem:** Cannot connect to Bitcoin Core

**Debugging:**
```bash
# Check Bitcoin Core is running
bitcoin-cli -regtest getblockchaininfo

# Verify RPC credentials
bitcoin-cli -regtest -rpcuser=user -rpcpassword=pass getblockchaininfo

# Check network connectivity
telnet localhost 18443
```

**Configuration:**
```rust
// In federation config
BitcoinConfig {
    rpc_url: "http://localhost:18443".to_string(),
    rpc_user: "rpcuser".to_string(),
    rpc_password: "rpcpassword".to_string(),
    network: bitcoin::Network::Regtest,
}
```

#### 3. Memory Issues

**Problem:** High memory usage or leaks

**Debugging:**
```bash
# Run with memory profiling
cargo run --features=jemalloc-profiling

# Use heaptrack (Linux)
heaptrack cargo run

# Monitor with htop/top during runtime
```

**Common Fixes:**
- Implement proper cleanup in Drop traits
- Use bounded channels instead of unbounded
- Regularly clear caches and old data

### Testing Issues

#### 1. Flaky Tests

**Problem:** Tests pass sometimes, fail other times

**Common Causes:**
- Race conditions in async code
- Timing dependencies
- Shared state between tests

**Solutions:**
```rust
// Use proper synchronization
use tokio::sync::Mutex;

// Add timeouts to prevent hanging
#[tokio::test]
async fn test_with_timeout() {
    tokio::time::timeout(
        Duration::from_secs(5),
        actual_test_logic()
    ).await.unwrap();
}

// Use test isolation
#[tokio::test]
async fn isolated_test() {
    let _guard = TEST_LOCK.lock().await;  // Serialize tests
    // Test logic
}
```

#### 2. Test Data Management

**Problem:** Tests interfere with each other

**Solutions:**
```rust
// Use unique test directories
fn create_test_db() -> TempDir {
    tempfile::tempdir().unwrap()
}

// Reset state between tests
fn setup_clean_environment() -> TestEnvironment {
    TestEnvironment::new()
        .with_clean_database()
        .with_fresh_keys()
}
```

### Performance Issues

#### 1. Slow Block Processing

**Debugging:**
```rust
use tracing::{instrument, info};

#[instrument(skip(self))]
async fn process_block(&self, block: Block) -> Result<(), Error> {
    let start = std::time::Instant::now();
    
    // Processing steps with timing
    self.validate_block(&block).await?;
    info!("Validation took: {:?}", start.elapsed());
    
    let start = std::time::Instant::now();
    self.execute_block(&block).await?;
    info!("Execution took: {:?}", start.elapsed());
    
    Ok(())
}
```

**Common Optimizations:**
- Parallel processing where possible
- Caching of expensive computations
- Database query optimization

#### 2. Network Latency

**Debugging:**
```rust
// Add network timing metrics
pub struct NetworkMetrics {
    request_duration: prometheus::Histogram,
    active_connections: prometheus::Gauge,
}

// Implement timeout handling
async fn network_call_with_timeout() -> Result<Response, Error> {
    tokio::time::timeout(
        Duration::from_secs(30),
        make_network_call()
    ).await?
}
```

### Development Environment Issues

#### 1. IDE Integration Problems

**VS Code + rust-analyzer issues:**
```bash
# Restart rust-analyzer
Ctrl+Shift+P -> "Rust Analyzer: Restart Server"

# Check rust-analyzer logs
Ctrl+Shift+P -> "Rust Analyzer: View Logs"

# Rebuild the project index
cargo clean && cargo check
```

#### 2. Git Workflow Issues

**Problem:** Large diffs or merge conflicts

**Solutions:**
```bash
# Use smaller, focused commits
git add -p  # Interactive staging

# Rebase to clean up history
git rebase -i HEAD~3

# Resolve conflicts systematically
git mergetool
```

---

## Contributing Guidelines

### Code Style and Standards

#### 1. Rust Formatting

Always run `cargo fmt` before committing:

```bash
# Format all code
cargo fmt

# Check formatting without changing files
cargo fmt -- --check

# Configure your editor to format on save
```

#### 2. Linting Rules

Use `clippy` to catch common issues:

```bash
# Run clippy with standard rules
cargo clippy

# Run with stricter rules
cargo clippy -- -D warnings

# Fix automatically when possible
cargo clippy --fix
```

#### 3. Documentation Standards

All public APIs must be documented:

```rust
/// Validates an auxiliary proof-of-work (AuxPoW) structure.
///
/// This function verifies that the provided AuxPoW correctly commits to
/// the given block hash and meets the required difficulty target.
///
/// # Arguments
///
/// * `block_hash` - The hash of the Alys block being validated
/// * `difficulty_target` - The minimum difficulty required
///
/// # Returns
///
/// Returns `Ok(())` if the AuxPoW is valid, or an `AuxPowError` describing
/// the validation failure.
///
/// # Examples
///
/// ```rust
/// use app::auxpow::AuxPow;
/// use ethereum_types::H256;
///
/// let auxpow = AuxPow::new(/* ... */);
/// let block_hash = H256::random();
/// assert!(auxpow.validate(&block_hash, 0x1d00ffff).is_ok());
/// ```
pub fn validate(&self, block_hash: &H256, difficulty_target: u32) -> Result<(), AuxPowError> {
    // Implementation
}
```

### Commit Message Guidelines

Use clear, descriptive commit messages:

```bash
# Good commit messages
git commit -m "feat: add emergency withdrawal functionality to federation"
git commit -m "fix: resolve race condition in block processing"
git commit -m "docs: update API documentation for RPC methods"
git commit -m "test: add integration tests for peg-out workflow"

# Bad commit messages (avoid these)
git commit -m "fix stuff"
git commit -m "wip"
git commit -m "changes"
```

**Format:**
```
type(scope): description

[optional body]

[optional footer]
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Formatting changes
- `refactor`: Code restructuring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

### Pull Request Process

#### 1. Before Creating a PR

```bash
# Ensure your branch is up to date
git checkout main
git pull origin main
git checkout your-branch
git rebase main

# Run all checks
cargo fmt
cargo clippy
cargo test
cargo doc --no-deps
```

#### 2. PR Description Template

```markdown
## Description
Brief description of what this PR does.

## Type of Change
- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Documentation update

## Testing
- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] Manual testing performed

## Checklist
- [ ] Code follows the project's style guidelines
- [ ] Self-review of the code has been performed
- [ ] Code is properly documented
- [ ] Tests pass locally
- [ ] Any dependent changes have been merged and published
```

#### 3. Code Review Process

**As a reviewer:**
- Focus on correctness, security, and maintainability
- Suggest improvements, don't just point out problems
- Test the changes locally when possible
- Check for proper error handling and edge cases

**As the author:**
- Respond to all comments
- Make requested changes promptly
- Ask for clarification if feedback is unclear
- Test thoroughly after making changes

### Security Considerations

#### 1. Sensitive Data Handling

```rust
// Never log private keys or other secrets
#[derive(Debug)]
pub struct PrivateKey {
    // Use a wrapper that doesn't implement Debug for sensitive data
    inner: SecretBytes,
}

// Use zeroize for sensitive memory
use zeroize::Zeroize;

impl Drop for PrivateKey {
    fn drop(&mut self) {
        self.inner.zeroize();
    }
}
```

#### 2. Input Validation

```rust
pub fn process_user_input(input: &str) -> Result<ProcessedInput, ValidationError> {
    // Always validate input length
    if input.len() > MAX_INPUT_SIZE {
        return Err(ValidationError::InputTooLarge);
    }
    
    // Validate format
    if !input.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ValidationError::InvalidCharacters);
    }
    
    // Additional validation...
    Ok(ProcessedInput::new(input))
}
```

#### 3. Crypto Operations

```rust
// Use constant-time comparisons for sensitive data
use subtle::ConstantTimeEq;

pub fn verify_signature(signature: &[u8], expected: &[u8]) -> bool {
    signature.ct_eq(expected).into()
}

// Use secure random number generation
use rand::{rngs::OsRng, RngCore};

pub fn generate_nonce() -> [u8; 32] {
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);
    nonce
}
```

### Release Process

#### 1. Version Management

Follow [Semantic Versioning](https://semver.org/):
- `MAJOR.MINOR.PATCH`
- MAJOR: Breaking changes
- MINOR: New features (backward compatible)
- PATCH: Bug fixes (backward compatible)

#### 2. Release Checklist

```bash
# 1. Update version numbers
# Edit Cargo.toml files

# 2. Update CHANGELOG.md
# Document all changes since last release

# 3. Run full test suite
cargo test --all-features

# 4. Build release binaries
cargo build --release

# 5. Create release tag
git tag -a v1.2.3 -m "Release version 1.2.3"
git push origin v1.2.3

# 6. Create GitHub release with binaries
```

---

## Learning Resources

### Rust-Specific Learning

#### 1. Essential Rust Concepts for Alys

**Ownership and Borrowing:**
- Critical for memory safety in blockchain applications
- Understanding `Arc<T>` and `Mutex<T>` for shared state
- Using `Rc<T>` vs `Arc<T>` appropriately

**Async Programming:**
- `async`/`await` syntax and runtime behavior
- `tokio` runtime and ecosystem
- Handling concurrent operations safely

**Error Handling:**
- `Result<T, E>` pattern throughout the codebase
- Custom error types with `thiserror`
- Error propagation with `?` operator

#### 2. Recommended Reading

**Books:**
- "The Rust Programming Language" (The Rust Book)
- "Rust for Rustaceans" by Jon Gjengset
- "Programming Rust" by Jim Blandy and Jason Orendorff

**Online Resources:**
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [The Async Book](https://rust-lang.github.io/async-book/)
- [Rust Patterns](https://rust-unofficial.github.io/patterns/)

### Blockchain Development

#### 1. Bitcoin Concepts

**Essential Knowledge:**
- Bitcoin transaction structure and validation
- UTXO model vs account-based model
- Script language and P2PKH/P2SH/P2WPKH
- Merkle trees and block structure

**Merged Mining:**
- Auxiliary proof-of-work concept
- Namecoin and other merged-mined chains
- Security implications and attack vectors

#### 2. Ethereum/EVM

**Smart Contract Development:**
- Solidity programming language
- Gas model and optimization
- Common patterns (ERC-20, ERC-721, etc.)
- Security considerations (reentrancy, etc.)

**Development Tools:**
- Foundry for smart contract development
- Hardhat as an alternative framework
- Remix for quick prototyping

### Consensus Mechanisms

#### 1. Proof-of-Authority (Aura)

**Understanding Aura:**
- Round-robin block production
- Authority selection and rotation
- Finality and fork choice rules
- Comparison with other PoA mechanisms

#### 2. Hybrid Consensus

**Alys-specific model:**
- Fast finality through federation
- Ultimate security through Bitcoin PoW
- Economic incentives and game theory
- Attack scenarios and mitigation

### Practical Exercises

#### 1. Getting Started Exercises

**Exercise 1: Build and Run**
```bash
# Clone, build, and run the project
git clone https://github.com/AnduroProject/alys.git
cd alys
cargo build --release
cargo test
```

**Exercise 2: Explore the RPC API**
```bash
# Start a local node
cargo run

# In another terminal, test RPC calls
curl -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
  http://localhost:8545
```

**Exercise 3: Read and Understand Code**
- Pick a module (e.g., `app/src/auxpow.rs`)
- Read through the code and comments
- Understand the data structures and functions
- Run the tests for that module

#### 2. Intermediate Exercises

**Exercise 4: Add a Simple RPC Method**
- Add a method to return the current node version
- Follow the patterns in existing RPC methods
- Add tests for your new method

**Exercise 5: Modify Transaction Validation**
- Add a simple validation rule (e.g., minimum transaction value)
- Update the validation logic
- Add tests to ensure it works correctly

**Exercise 6: Extend Federation Operations**
- Add logging to track peg operations
- Implement a simple status query function
- Test with mock Bitcoin transactions

#### 3. Advanced Exercises

**Exercise 7: Performance Optimization**
- Profile a specific operation (e.g., block validation)
- Identify bottlenecks using timing and memory tools
- Implement and measure improvements

**Exercise 8: Security Analysis**
- Review a security-critical module
- Identify potential attack vectors
- Propose and implement improvements

**Exercise 9: Integration Testing**
- Set up a multi-node test environment
- Test consensus and networking behavior
- Implement automated test scenarios

### Community Resources

#### 1. Communication Channels

**Discord:** Join the Alys community Discord for real-time discussion
**GitHub:** Use GitHub Issues for bug reports and feature requests
**Forums:** Participate in technical discussions

#### 2. Contributing to the Ecosystem

**Documentation:** Help improve documentation and guides
**Testing:** Report bugs and help with testing new features
**Code:** Contribute bug fixes and new features
**Mentoring:** Help other new contributors get started

#### 3. Staying Updated

**Release Notes:** Read release notes for each version
**Blog Posts:** Follow project blog for technical deep-dives
**Conference Talks:** Watch presentations about Alys and related tech
**Research Papers:** Read academic papers on sidechains and consensus

---

## Appendix

### Quick Reference

#### Common Commands
```bash
# Build
cargo build --release

# Test
cargo test --all

# Format
cargo fmt

# Lint
cargo clippy

# Documentation
cargo doc --open

# Clean
cargo clean
```

#### Useful Environment Variables
```bash
# Logging
RUST_LOG=debug
RUST_LOG=app::engine=debug

# Networking
CARGO_NET_RETRY=10

# Development
RUST_BACKTRACE=1
RUST_BACKTRACE=full
```

#### Key File Locations
```
app/src/main.rs              # Application entry point
app/src/engine.rs            # Consensus engine
app/src/rpc.rs               # RPC server
crates/federation/src/lib.rs # Bitcoin bridge
docs/                        # Architecture documentation
```

### Glossary

**AuxPoW (Auxiliary Proof-of-Work):** A mechanism allowing miners to simultaneously mine multiple cryptocurrencies.

**Federation:** A group of trusted parties that collectively manage the two-way peg between Bitcoin and Alys.

**Merged Mining:** The process of mining multiple cryptocurrencies simultaneously without additional computational cost.

**Peg-in:** The process of moving Bitcoin from the Bitcoin network to the Alys sidechain.

**Peg-out:** The process of moving Bitcoin from the Alys sidechain back to the Bitcoin network.

**Sidechain:** A separate blockchain that is interoperable with a parent blockchain (Bitcoin in this case).

**Two-way Peg:** A mechanism that allows assets to move between two blockchains in both directions.

---

**Last Updated:** $(date)
**Version:** 1.0.0

This guide is a living document. Please contribute improvements and corrections via GitHub Issues or Pull Requests.