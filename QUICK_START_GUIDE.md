# Quick Start Guide for Junior Rust Engineers

## 🚀 Welcome to Alys!

This is a condensed guide to get you productive quickly on the Alys Bitcoin sidechain project.

## What is Alys?

Alys is a **Bitcoin sidechain** that:
- Uses **BTC as native currency**
- Supports **Ethereum smart contracts** (EVM compatible)
- Secured by **Bitcoin miners** through merged mining
- Managed by a **federation** for fast transactions

## 15-Minute Setup

### 1. Install Prerequisites

```bash
# Rust (required)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# System dependencies (Ubuntu/Debian)
sudo apt-get install build-essential pkg-config libssl-dev cmake clang

# macOS
brew install cmake pkg-config openssl
```

### 2. Clone and Build

```bash
git clone https://github.com/AnduroProject/alys.git
cd alys

# Initial build (may take 10-15 minutes)
CARGO_NET_RETRY=10 cargo build --release
```

**Note:** If build fails with network errors, this is normal due to complex dependencies. Try again or see troubleshooting below.

### 3. Run Tests

```bash
# Run a subset of fast tests first
cargo test --lib

# If that works, run all tests
cargo test
```

## Project Structure (5-Minute Overview)

```
alys/
├── app/                    # Main node software
│   ├── src/main.rs        # Entry point
│   ├── src/engine.rs      # Consensus engine
│   ├── src/rpc.rs         # JSON-RPC API
│   └── src/auxpow.rs      # Bitcoin merged mining
├── crates/federation/     # Bitcoin bridge logic
├── crates/miner/          # Mining integration
├── contracts/             # Smart contracts
└── docs/                  # Architecture docs
```

## Key Concepts (5 Minutes)

### 1. Merged Mining
- Bitcoin miners can mine Alys "for free"
- Alys blocks are committed to Bitcoin blocks
- Provides Bitcoin-level security to Alys

### 2. Federation
- Trusted group managing Bitcoin bridge
- Handles peg-in (Bitcoin → Alys) and peg-out (Alys → Bitcoin)
- Uses multi-signature for security

### 3. EVM Compatibility
- Run Ethereum smart contracts on Alys
- Use existing tools: MetaMask, Foundry, Hardhat
- Pay fees in BTC instead of ETH

## Your First Task (15 Minutes)

Let's explore the RPC API:

### 1. Find the RPC Implementation
```bash
# Look at the RPC methods
cat app/src/rpc.rs | grep "fn.*rpc"
```

### 2. Find a Simple Test
```bash
# Look at auxpow tests
grep -A 10 "#\[test\]" app/src/auxpow.rs
```

### 3. Understand Error Handling
```bash
# See how errors are defined
cat app/src/error.rs
```

**Real Example - Understanding AuxPoW:**
The file `app/src/auxpow.rs` contains the core merged mining logic. Let's look at one test:

```rust
#[test]
fn empty_merkle_branch() {
    let hash_aux_block = BlockHash::from_byte_array([1; 32]);
    let blockchain_branch = MerkleBranch {
        branch_hash: vec![],
        branch_side_mask: 0,
    };
    let root_hash = blockchain_branch
        .check_merkle_branch(TxMerkleNode::from_raw_hash(hash_aux_block.to_raw_hash()));
    // tests that if the mining pool is only including a single hash
    // we get that as the merkle root if the branch is empty
    assert_eq!(hash_aux_block.to_raw_hash(), root_hash.to_raw_hash());
}
```

This test shows how Alys validates merkle proofs from Bitcoin miners. The `empty_merkle_branch` test verifies that when a mining pool includes only one transaction, the merkle root equals the transaction hash itself.

**Try this:**
```bash
# Run just this test
cargo test empty_merkle_branch

# Run all auxpow tests
cargo test -p app auxpow
```

## Common Patterns You'll See

### Error Handling
```rust
// Alys uses Result types everywhere
fn validate_block(block: &Block) -> Result<(), BlockError> {
    if block.is_empty() {
        return Err(BlockError::EmptyBlock);
    }
    Ok(())
}

// The ? operator propagates errors
fn process_transaction(tx: Transaction) -> Result<Receipt, Error> {
    let validated = validate_transaction(tx)?;  // Returns early if error
    let receipt = submit_transaction(validated)?;
    Ok(receipt)
}
```

### Async Programming
```rust
// Most operations are async
async fn get_block_by_hash(hash: H256) -> Result<Block, Error> {
    let block = self.database.get_block(hash).await?;
    Ok(block)
}

// Use .await for async calls
let block = get_block_by_hash(hash).await?;
```

### Configuration
```rust
// CLI args with clap
#[derive(Parser)]
struct Args {
    #[clap(long, default_value = "8545")]
    rpc_port: u16,
}
```

## Troubleshooting Quick Fixes

### Build Issues

**Network timeouts:**
```bash
# Retry with more attempts
CARGO_NET_RETRY=10 cargo build

# If still failing, work offline with cached deps
cargo build --offline
```

**Missing system libraries:**
```bash
# Ubuntu/Debian
sudo apt-get install build-essential pkg-config libssl-dev cmake clang

# If OpenSSL issues persist
export PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig
```

**Git submodule issues:**
```bash
git submodule update --init --recursive
```

### Development Issues

**rust-analyzer not working:**
```bash
# In VS Code: Ctrl+Shift+P → "Rust Analyzer: Restart Server"
cargo clean && cargo check
```

**Tests failing:**
```bash
# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture

# Run single-threaded (helps with race conditions)
cargo test -- --test-threads=1
```

## Daily Development Workflow

### 1. Starting Work
```bash
git pull origin main
cargo check  # Quick compile check
```

### 2. Making Changes
```bash
# Create branch
git checkout -b your-feature

# Make changes, then check they compile
cargo check

# Run tests for your changes
cargo test -p app  # Test specific package
```

### 3. Before Committing
```bash
cargo fmt      # Format code
cargo clippy   # Lint code
cargo test     # Run tests
```

## Next Steps

1. **Read the full guide:** `JUNIOR_RUST_ENGINEER_GUIDE.md` (comprehensive version)
2. **Explore the docs:** `docs/src/README.md` for architecture
3. **Join the community:** Discord, GitHub discussions
4. **Try the exercises:** Start with RPC exploration
5. **Read existing code:** Pick a module and understand it deeply

## Key Files to Bookmark

- `app/src/main.rs` - Application entry point
- `app/src/engine.rs` - Core consensus logic
- `app/src/rpc.rs` - API interface
- `crates/federation/src/lib.rs` - Bitcoin bridge
- `docs/src/README.md` - Architecture overview

## Common Commands Reference

```bash
# Build and test
cargo build --release
cargo test

# Code quality
cargo fmt
cargo clippy

# Documentation
cargo doc --open

# Specific testing
cargo test -p federation
cargo test test_auxpow_validation

# Debug builds (faster)
cargo build
cargo test --lib
```

## Getting Help

- **Build issues:** Check `JUNIOR_RUST_ENGINEER_GUIDE.md` troubleshooting section
- **Concept questions:** Read `docs/` folder
- **Code questions:** Look for similar patterns in existing code
- **Stuck?** Ask in Discord or create GitHub issue

---

**Remember:** This is a complex project. Don't expect to understand everything immediately. Focus on one component at a time, and gradually build your understanding.

Good luck! 🦀⚡