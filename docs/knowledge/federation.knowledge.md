# Federation Crate Knowledge Graph

## Overview
The `crates/federation/` directory implements the core federation functionality for Alys's two-way peg system. This crate provides Bitcoin multisignature wallet management, peg-in/peg-out processing, and Bitcoin network monitoring capabilities. It serves as the critical bridge between the Bitcoin mainnet and the Alys sidechain.

## Core Architecture

### 1. Module Structure
```
lib.rs (public interface) → bitcoin_signing.rs (cryptography) → bitcoin_stream.rs (monitoring)
```

**Key Dependencies:**
- **BDK (Bitcoin Dev Kit)**: Wallet functionality, UTXO management, fee calculation
- **bitcoincore-rpc**: Bitcoin Core RPC client integration
- **ethers**: Ethereum types and event parsing for peg-out detection
- **secp256k1**: Schnorr signatures and taproot cryptography

### 2. Public API Surface (lib.rs)

**Core Types Exported:**
```rust
// Bitcoin Signing Infrastructure
pub use BitcoinSignatureCollector, BitcoinSigner, Federation
pub use PartiallySignedTaprootTransaction, SingleMemberTransactionSignatures
pub use PublicKey as BitcoinPublicKey, SecretKey as BitcoinSecretKey
pub use Tree, UtxoManager, FeeRate

// Bitcoin Network Interface  
pub use BitcoinCore

// Utility Functions
pub fn wei_to_sats(wei: U256) -> u64  // Convert Ethereum wei to Bitcoin satoshis
```

**Main Bridge Component:**
- **Bridge**: Central coordinator for peg-in/peg-out operations
- **PegInInfo**: Structured peg-in transaction data
- **Error**: Comprehensive error types for federation operations

## Component Deep Dive

### 1. Bitcoin Signing System (bitcoin_signing.rs)

#### Federation Structure
```rust
pub struct Federation {
    pub taproot_address: Address,           // Multisig deposit address
    spend_info: TaprootSpendInfo,          // Taproot spending conditions
    redeem_script: ScriptBuf,              // Multisig redemption script
    threshold: usize,                       // Required signatures (m-of-n)
    pubkeys: Vec<PublicKey>,               // Federation member public keys
    satisfaction_weight: usize,             // Transaction weight for fee calculation
}
```

**Key Features:**
- **Taproot Integration**: Uses Bitcoin's taproot for efficient multisig
- **Unspendable Internal Key**: Uses nothing-up-my-sleeve number to disable keypath spending
- **Threshold Signatures**: Configurable m-of-n signature requirements
- **Script Path Spending**: Federation members sign via script path (not keypath)

#### UTXO Management
```rust
pub struct UtxoManager<T: Database> {
    tree: T,                    // Database backend (Sled in production, Memory for testing)
    federation: Federation,     // Associated federation configuration
    secp: Secp256k1<All>,      // Cryptographic context
}
```

**Core Capabilities:**
- **UTXO Tracking**: Register peg-ins and mark spent outputs for peg-outs
- **Payment Creation**: Coin selection, fee calculation, and unsigned transaction building
- **Missing UTXO Recovery**: Fetch UTXOs from Bitcoin network during sync issues
- **Signature Verification**: Validate transaction signatures against federation rules

#### Signature Collection Process
```rust
pub struct BitcoinSignatureCollector {
    partial_txs: HashMap<Txid, PartiallySignedTaprootTransaction>,
    federation: Federation,
}
```

**Workflow:**
1. **Unsigned Transaction**: Created by `UtxoManager::create_payment()`
2. **Individual Signing**: Each federation member signs with `BitcoinSigner`
3. **Signature Aggregation**: `BitcoinSignatureCollector` accumulates signatures
4. **Transaction Finalization**: Once threshold met, creates fully signed transaction

### 2. Bitcoin Network Monitoring (bitcoin_stream.rs)

#### BitcoinCore Client
```rust
pub struct BitcoinCore {
    pub rpc: Arc<Client>,  // Thread-safe RPC client
}
```

**Features:**
- **Block Streaming**: Continuous monitoring from specified height with confirmation requirements
- **Error Handling**: Comprehensive Bitcoin RPC error code mapping
- **Retry Logic**: Automatic retry with backoff for temporary network issues
- **Confirmation Safety**: Configurable minimum confirmations before processing

#### Block Streaming Implementation
```rust
pub async fn stream_blocks(
    rpc: BitcoinCore,
    from_height: u32,
    num_confirmations: u32,
) -> impl Stream<Item = Result<(Block, u32), Error>> + Unpin
```

**Stream Characteristics:**
- **Never-ending**: Continuously monitors for new blocks
- **Stateful**: Tracks next expected height internally
- **Async**: Non-blocking operation with proper error propagation
- **Fork Awareness**: Includes TODO to handle Bitcoin forks properly

### 3. Bridge Operations (lib.rs)

#### Peg-in Processing
```rust
pub struct Bridge {
    pegin_addresses: Vec<BitcoinAddress>,  // Federation multisig addresses
    bitcoin_core: BitcoinCore,             // Bitcoin network interface
    required_confirmations: u16,           // Safety threshold
}
```

**Peg-in Flow:**
1. **Address Generation**: Federation creates taproot multisig address
2. **Bitcoin Transaction**: User sends BTC with EVM address in OP_RETURN
3. **Detection**: Bridge monitors federation addresses for incoming transactions
4. **Validation**: Ensures proper format and confirmation count
5. **EVM Address Extraction**: Parses destination address from OP_RETURN data
6. **Information Packaging**: Creates `PegInInfo` for consensus layer processing

**OP_RETURN Parsing Logic:**
- Attempts UTF-8 string parsing first
- Falls back to direct hex interpretation
- Validates EVM address format (H160)
- Handles both prefixed and non-prefixed address formats

#### Peg-out Processing
```rust
#[derive(Clone, Debug, EthEvent)]
pub struct RequestPegOut {
    #[ethevent(indexed)]
    pub evm_address: Address,      // Source EVM address
    pub bitcoin_address: Bytes,    // Destination Bitcoin address  
    pub value: U256,              // Amount in wei
}
```

**Peg-out Flow:**
1. **Event Detection**: Monitor bridge contract for `RequestPegOut` events
2. **Amount Validation**: Ensure minimum threshold (1M sats) for economic viability
3. **Address Parsing**: Convert bytes to valid Bitcoin address
4. **UTXO Creation**: Generate `TxOut` for Bitcoin transaction
5. **Fee Estimation**: Dynamic fee calculation from Bitcoin network
6. **Transaction Building**: Federation creates and signs Bitcoin transaction

## Critical Security Features

### 1. Cryptographic Security
- **Schnorr Signatures**: Modern signature scheme with better privacy/efficiency
- **Taproot Multisig**: Script path spending prevents single point of failure
- **Threshold Security**: Requires m-of-n signatures, not just m signatures
- **Unspendable Internal Key**: Prevents keypath spending attacks

### 2. Transaction Validation
- **UTXO Verification**: Validates inputs are spendable and owned by federation
- **Output Validation**: Ensures peg-out addresses and amounts match requests
- **Fee Validation**: Prevents fee attacks that could drain federation funds
- **Confirmation Requirements**: Prevents double-spend attacks via reorg protection

### 3. Error Handling
- **Comprehensive Error Types**: 20+ specific error variants for different failure modes
- **Bitcoin RPC Errors**: Detailed mapping of all Bitcoin Core error codes
- **Graceful Degradation**: Missing UTXO recovery and circuit breaker patterns
- **Network Resilience**: Retry logic with exponential backoff

## Dependencies and Integration Points

### 1. External Crate Dependencies
```toml
bitcoincore-rpc = "0.17"    # Bitcoin Core RPC client
bdk = "0.29.0"              # Bitcoin wallet functionality
ethers = "2.0.11"           # Ethereum event parsing
serde = "1.0"               # Serialization
futures = "0.3.26"          # Async streams
tokio = "1.0"               # Async runtime
```

### 2. Integration with Main Application
- **Chain Integration**: Used by `app/src/chain.rs` for peg-in/peg-out processing
- **RPC Integration**: Provides endpoints via `app/src/rpc.rs`
- **Network Integration**: Broadcasts signed transactions via P2P network
- **Storage Integration**: Persists UTXO state and transaction history

### 3. Configuration Requirements
- **Bitcoin RPC**: Requires Bitcoin Core node with RPC access
- **Network Selection**: Supports mainnet, testnet, and regtest
- **Federation Setup**: Requires public keys and threshold configuration
- **Address Management**: Manages multiple peg-in addresses

## Performance Characteristics

### 1. Scaling Considerations
- **UTXO Set Growth**: Linear with peg-in volume
- **Signature Collection**: O(n) with federation size
- **Block Processing**: Dependent on Bitcoin block time and confirmation requirements
- **Database Operations**: Optimized with Sled B-tree storage

### 2. Monitoring and Metrics
- Integration with Prometheus metrics (imported from workspace)
- Stream processing statistics
- Transaction success/failure rates
- Fee estimation accuracy

### 3. Testing Infrastructure
- Comprehensive unit tests with Bitcoin Core integration
- End-to-end peg-in/peg-out simulation
- Keypath vs script path spending verification
- Multi-federation member signature aggregation tests

This federation crate represents a sophisticated Bitcoin bridge implementation that securely handles the cryptographic and network complexities of maintaining a two-way peg between Bitcoin and the Alys sidechain, with robust error handling and security measures throughout.