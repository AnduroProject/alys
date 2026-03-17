//! EVM Transaction Simulator for Alys V2 Testnet
//!
//! A CLI tool for testing EVM transactions on the Alys V2 testnet.
//! Generates test accounts, manages state, and executes:
//! - ETH transfers between accounts
//! - ERC20 token transfers
//! - ERC20 contract deployments

use chrono::Utc;
use clap::{Parser, Subcommand};
use ethers::{
    abi::{encode, Token},
    prelude::*,
    types::{TransactionRequest, U256},
    utils::hex,
};
use eyre::{eyre, Result, WrapErr};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// ============================================================================
// CLI Definitions
// ============================================================================

#[derive(Parser)]
#[command(name = "tx-simulator")]
#[command(about = "EVM Transaction Simulator for Alys V2 Testnet")]
#[command(version)]
struct Cli {
    /// Number of accounts to generate
    #[arg(short = 'n', long, default_value = "3")]
    num_accounts: usize,

    /// EVM RPC endpoint URL
    #[arg(long, default_value = "http://localhost:8545")]
    rpc_url: String,

    /// Regenerate keys even if they exist
    #[arg(long)]
    overwrite_keys: bool,

    /// Show private keys in output
    #[arg(long)]
    display_priv_keys: bool,

    /// Directory for storing account keys
    #[arg(long, default_value = "keys/tests/tx-simulator")]
    keys_dir: PathBuf,

    /// Directory for storing simulator state
    #[arg(long, default_value = "keys/tests/tx-simulator-state")]
    state_dir: PathBuf,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Transfer ETH between accounts
    EthTransfer {
        /// Source account index (1-based, auto-select if omitted)
        #[arg(long)]
        from: Option<usize>,

        /// Destination account index (1-based, random if omitted)
        #[arg(long)]
        to: Option<usize>,

        /// Amount in wei (default: 0.001 ETH = 1000000000000000 wei)
        #[arg(long)]
        amount: Option<U256>,
    },

    /// Transfer ERC20 tokens between accounts
    Erc20Transfer {
        /// Contract address (uses last deployed if omitted)
        #[arg(long)]
        contract: Option<Address>,

        /// Source account index (1-based, auto-select if omitted)
        #[arg(long)]
        from: Option<usize>,

        /// Destination account index (1-based, random if omitted)
        #[arg(long)]
        to: Option<usize>,

        /// Amount in tokens (with 18 decimals)
        #[arg(long, default_value = "1000000000000000000")]
        amount: U256,
    },

    /// Deploy an ERC20 contract (always from Account 1)
    Erc20Deploy {
        /// Token name
        #[arg(long, default_value = "TestToken")]
        name: String,

        /// Token symbol
        #[arg(long, default_value = "TST")]
        symbol: String,

        /// Initial supply (in whole tokens, will be multiplied by 10^18)
        #[arg(long, default_value = "1000000")]
        initial_supply: u64,
    },

    /// Show accounts, balances, and deployed contracts
    Status,

    /// Reset state (preserves keys)
    ResetState,
}

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredAccount {
    index: usize,
    address: String,
    private_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AccountsFile {
    accounts: Vec<StoredAccount>,
    generated_at: String, // ISO 8601 timestamp
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeployedContract {
    address: String,
    contract_type: String,
    name: String,
    symbol: String,
    deployed_by: usize,
    tx_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TxRecord {
    tx_hash: String,
    tx_type: String,
    from: usize,
    to: usize,
    amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SimulatorState {
    funded_accounts: Vec<usize>,
    contracts: Vec<DeployedContract>,
    tx_history: Vec<TxRecord>,
    last_updated: Option<String>, // ISO 8601 timestamp
}

// ============================================================================
// Contract Bytecode (MockErc20 compiled with forge)
// ============================================================================

/// MockErc20 bytecode compiled from contracts/src/MockErc20.sol
/// Constructor args: (string name, string symbol, uint256 initialSupply)
const MOCK_ERC20_BYTECODE: &str = "608060405234801561000f575f5ffd5b50604051610b24380380610b2483398101604081905261002e91610264565b8282600361003c8382610355565b5060046100498282610355565b50505061005c338261006460201b60201c565b505050610434565b6001600160a01b0382166100925760405163ec442f0560e01b81525f60048201526024015b60405180910390fd5b61009d5f83836100a1565b5050565b6001600160a01b0383166100cb578060025f8282546100c0919061040f565b9091555061013b9050565b6001600160a01b0383165f908152602081905260409020548181101561011d5760405163391434e360e21b81526001600160a01b03851660048201526024810182905260448101839052606401610089565b6001600160a01b0384165f9081526020819052604090209082900390555b6001600160a01b03821661015757600280548290039055610175565b6001600160a01b0382165f9081526020819052604090208054820190555b816001600160a01b0316836001600160a01b03167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef836040516101ba91815260200190565b60405180910390a3505050565b634e487b7160e01b5f52604160045260245ffd5b5f82601f8301126101ea575f5ffd5b81516001600160401b03811115610203576102036101c7565b604051601f8201601f19908116603f011681016001600160401b0381118282101715610231576102316101c7565b604052818152838201602001851015610248575f5ffd5b8160208501602083015e5f918101602001919091529392505050565b5f5f5f60608486031215610276575f5ffd5b83516001600160401b0381111561028b575f5ffd5b610297868287016101db565b602086015190945090506001600160401b038111156102b4575f5ffd5b6102c0868287016101db565b925050604084015190509250925092565b600181811c908216806102e557607f821691505b60208210810361030357634e487b7160e01b5f52602260045260245ffd5b50919050565b601f82111561035057805f5260205f20601f840160051c8101602085101561032e5750805b601f840160051c820191505b8181101561034d575f815560010161033a565b50505b505050565b81516001600160401b0381111561036e5761036e6101c7565b6103828161037c84546102d1565b84610309565b6020601f8211600181146103b4575f831561039d5750848201515b5f19600385901b1c1916600184901b17845561034d565b5f84815260208120601f198516915b828110156103e357878501518255602094850194600190920191016103c3565b508482101561040057868401515f19600387901b60f8161c191681555b50505050600190811b01905550565b8082018082111561042e57634e487b7160e01b5f52601160045260245ffd5b92915050565b6106e3806104415f395ff3fe608060405234801561000f575f5ffd5b5060043610610090575f3560e01c8063313ce56711610063578063313ce567146100fa57806370a082311461010957806395d89b4114610131578063a9059cbb14610139578063dd62ed3e1461014c575f5ffd5b806306fdde0314610094578063095ea7b3146100b257806318160ddd146100d557806323b872dd146100e7575f5ffd5b806306fdde0314610094578063095ea7b3146100b257806318160ddd146100d557806323b872dd146100e7575f5ffd5b5f5ffd5b61009c610184565b6040516100a99190610553565b60405180910390f35b6100c56100c03660046105a3565b610214565b60405190151581526020016100a9565b6002545b6040519081526020016100a9565b6100c56100f53660046105cb565b61022d565b604051601281526020016100a9565b6100d9610117366004610605565b6001600160a01b03165f9081526020819052604090205490565b61009c610250565b6100c56101473660046105a3565b61025f565b6100d961015a366004610625565b6001600160a01b039182165f90815260016020908152604080832093909416825291909152205490565b60606003805461019390610656565b80601f01602080910402602001604051908101604052809291908181526020018280546101bf90610656565b801561020a5780601f106101e15761010080835404028352916020019161020a565b820191905f5260205f20905b8154815290600101906020018083116101ed57829003601f168201915b5050505050905090565b5f3361022181858561026c565b60019150505b92915050565b5f3361023a85828561027e565b6102458585856102fe565b506001949350505050565b60606004805461019390610656565b5f336102218185856102fe565b610279838383600161035b565b505050565b6001600160a01b038381165f908152600160209081526040808320938616835292905220545f1981146102f857818110156102ea57604051637dc7a0d960e11b81526001600160a01b038416600482015260248101829052604481018390526064015b60405180910390fd5b6102f884848484035f61035b565b50505050565b6001600160a01b03831661032757604051634b637e8f60e11b81525f60048201526024016102e1565b6001600160a01b0382166103505760405163ec442f0560e01b81525f60048201526024016102e1565b61027983838361042d565b6001600160a01b0384166103845760405163e602df0560e01b81525f60048201526024016102e1565b6001600160a01b0383166103ad57604051634a1406b160e11b81525f60048201526024016102e1565b6001600160a01b038085165f90815260016020908152604080832093871683529290522082905580156102f857826001600160a01b0316846001600160a01b03167f8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b9258460405161041f91815260200190565b60405180910390a350505050565b6001600160a01b038316610457578060025f82825461044c919061068e565b909155506104c79050565b6001600160a01b0383165f90815260208190526040902054818110156104a95760405163391434e360e21b81526001600160a01b038516600482015260248101829052604481018390526064016102e1565b6001600160a01b0384165f9081526020819052604090209082900390555b6001600160a01b0382166104e357600280548290039055610501565b6001600160a01b0382165f9081526020819052604090208054820190555b816001600160a01b0316836001600160a01b03167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef8360405161054691815260200190565b60405180910390a3505050565b602081525f82518060208401528060208501604085015e5f604082850101526040601f19601f83011684010191505092915050565b80356001600160a01b038116811461059e575f5ffd5b919050565b5f5f604083850312156105b4575f5ffd5b6105bd83610588565b946020939093013593505050565b5f5f5f606084860312156105dd575f5ffd5b6105e684610588565b92506105f460208501610588565b929592945050506040919091013590565b5f60208284031215610615575f5ffd5b61061e82610588565b9392505050565b5f5f60408385031215610636575f5ffd5b61063f83610588565b915061064d60208401610588565b90509250929050565b600181811c9082168061066a57607f821691505b60208210810361068857634e487b7160e01b5f52602260045260245ffd5b50919050565b8082018082111561022757634e487b7160e01b5f52601160045260245ffdfea2646970667358221220e9defbbe8e614f68bbedfabe102319af7274309cc9b42b653339baba63740cb864736f6c634300081b0033";

/// ERC20 transfer function selector: transfer(address,uint256)
const ERC20_TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

/// ERC20 balanceOf function selector: balanceOf(address)
const ERC20_BALANCE_OF_SELECTOR: [u8; 4] = [0x70, 0xa0, 0x82, 0x31];

// ============================================================================
// File Operations
// ============================================================================

fn load_or_generate_accounts(
    keys_dir: &PathBuf,
    num_accounts: usize,
    overwrite: bool,
) -> Result<AccountsFile> {
    let accounts_path = keys_dir.join("accounts.json");

    // Check if we should load existing accounts
    if accounts_path.exists() && !overwrite {
        let content = fs::read_to_string(&accounts_path)
            .wrap_err_with(|| format!("Failed to read {}", accounts_path.display()))?;
        let accounts: AccountsFile = serde_json::from_str(&content)
            .wrap_err("Failed to parse accounts.json")?;

        // Check if we have enough accounts
        if accounts.accounts.len() >= num_accounts {
            println!("Loaded {} existing accounts from {}", accounts.accounts.len(), accounts_path.display());
            return Ok(accounts);
        }

        println!(
            "Existing accounts ({}) less than requested ({}), regenerating...",
            accounts.accounts.len(),
            num_accounts
        );
    }

    // Generate new accounts
    println!("Generating {} new accounts...", num_accounts);
    let mut accounts = Vec::with_capacity(num_accounts);

    for i in 1..=num_accounts {
        let wallet = LocalWallet::new(&mut rand::thread_rng());
        let address = format!("{:?}", wallet.address());
        let private_key = hex::encode(wallet.signer().to_bytes());

        accounts.push(StoredAccount {
            index: i,
            address,
            private_key,
        });
    }

    let accounts_file = AccountsFile {
        accounts,
        generated_at: Utc::now().to_rfc3339(),
    };

    // Ensure directory exists and save
    fs::create_dir_all(keys_dir)
        .wrap_err_with(|| format!("Failed to create directory {}", keys_dir.display()))?;

    let content = serde_json::to_string_pretty(&accounts_file)?;
    fs::write(&accounts_path, content)
        .wrap_err_with(|| format!("Failed to write {}", accounts_path.display()))?;

    println!("Saved accounts to {}", accounts_path.display());
    Ok(accounts_file)
}

fn load_state(state_dir: &PathBuf) -> Result<SimulatorState> {
    let state_path = state_dir.join("state.json");

    if state_path.exists() {
        let content = fs::read_to_string(&state_path)
            .wrap_err_with(|| format!("Failed to read {}", state_path.display()))?;
        let state: SimulatorState = serde_json::from_str(&content)
            .wrap_err("Failed to parse state.json")?;
        Ok(state)
    } else {
        // Return default state with account 1 considered funded
        Ok(SimulatorState {
            funded_accounts: vec![1],
            ..Default::default()
        })
    }
}

fn save_state(state: &SimulatorState, state_dir: &PathBuf) -> Result<()> {
    fs::create_dir_all(state_dir)
        .wrap_err_with(|| format!("Failed to create directory {}", state_dir.display()))?;

    let state_path = state_dir.join("state.json");
    let mut state = state.clone();
    state.last_updated = Some(Utc::now().to_rfc3339());

    let content = serde_json::to_string_pretty(&state)?;
    fs::write(&state_path, content)
        .wrap_err_with(|| format!("Failed to write {}", state_path.display()))?;

    Ok(())
}

// ============================================================================
// Account Selection Logic
// ============================================================================

fn select_eligible_sender(state: &SimulatorState, num_accounts: usize) -> Option<usize> {
    if state.funded_accounts.is_empty() {
        return Some(1); // Account 1 is always considered funded
    }

    let eligible: Vec<_> = state
        .funded_accounts
        .iter()
        .filter(|&&idx| idx <= num_accounts)
        .collect();

    if eligible.is_empty() {
        Some(1)
    } else {
        let mut rng = rand::thread_rng();
        Some(**eligible.get(rng.gen_range(0..eligible.len()))?)
    }
}

fn select_random_recipient(exclude: usize, num_accounts: usize) -> usize {
    if num_accounts <= 1 {
        return 1;
    }

    let mut rng = rand::thread_rng();
    loop {
        let idx = rng.gen_range(1..=num_accounts);
        if idx != exclude {
            return idx;
        }
    }
}

fn get_wallet(accounts: &AccountsFile, index: usize) -> Result<LocalWallet> {
    let account = accounts
        .accounts
        .iter()
        .find(|a| a.index == index)
        .ok_or_else(|| eyre!("Account {} not found", index))?;

    let bytes = hex::decode(&account.private_key)?;
    let wallet = LocalWallet::from_bytes(&bytes)?;
    Ok(wallet)
}

fn get_address(accounts: &AccountsFile, index: usize) -> Result<Address> {
    let account = accounts
        .accounts
        .iter()
        .find(|a| a.index == index)
        .ok_or_else(|| eyre!("Account {} not found", index))?;

    account.address.parse().wrap_err("Invalid address format")
}

// ============================================================================
// Transaction Operations
// ============================================================================

async fn eth_transfer(
    provider: &Provider<Http>,
    accounts: &AccountsFile,
    state: &mut SimulatorState,
    from_idx: usize,
    to_idx: usize,
    amount: U256,
    chain_id: u64,
) -> Result<TxHash> {
    let wallet = get_wallet(accounts, from_idx)?.with_chain_id(chain_id);
    let to_address = get_address(accounts, to_idx)?;

    let client = SignerMiddleware::new(provider.clone(), wallet);

    let tx = TransactionRequest::new()
        .to(to_address)
        .value(amount);

    println!("Sending {} wei from Account {} to Account {}...", amount, from_idx, to_idx);

    let pending_tx = client
        .send_transaction(tx, None)
        .await
        .wrap_err("Failed to send transaction")?;

    let tx_hash = pending_tx.tx_hash();
    println!("Transaction submitted: {:?}", tx_hash);

    // Wait for confirmation
    let receipt = pending_tx
        .await
        .wrap_err("Failed to get transaction receipt")?
        .ok_or_else(|| eyre!("Transaction dropped"))?;

    if receipt.status == Some(1.into()) {
        println!("Transaction confirmed in block {:?}", receipt.block_number);

        // Mark recipient as funded
        if !state.funded_accounts.contains(&to_idx) {
            state.funded_accounts.push(to_idx);
        }

        // Record transaction
        state.tx_history.push(TxRecord {
            tx_hash: format!("{:?}", tx_hash),
            tx_type: "eth_transfer".to_string(),
            from: from_idx,
            to: to_idx,
            amount: amount.to_string(),
        });
    } else {
        return Err(eyre!("Transaction failed"));
    }

    Ok(tx_hash)
}

async fn deploy_erc20(
    provider: &Provider<Http>,
    accounts: &AccountsFile,
    state: &mut SimulatorState,
    name: &str,
    symbol: &str,
    initial_supply: u64,
    chain_id: u64,
) -> Result<Address> {
    // Always deploy from account 1
    let wallet = get_wallet(accounts, 1)?.with_chain_id(chain_id);
    let client = SignerMiddleware::new(provider.clone(), wallet);

    // Encode constructor arguments
    let supply_with_decimals = U256::from(initial_supply) * U256::exp10(18);
    let constructor_args = encode(&[
        Token::String(name.to_string()),
        Token::String(symbol.to_string()),
        Token::Uint(supply_with_decimals),
    ]);

    // Combine bytecode with constructor args
    let mut bytecode = hex::decode(MOCK_ERC20_BYTECODE)?;
    bytecode.extend(constructor_args);

    let tx = TransactionRequest::new().data(bytecode);

    println!("Deploying ERC20 '{}' ({}) with supply {}...", name, symbol, initial_supply);

    let pending_tx = client
        .send_transaction(tx, None)
        .await
        .wrap_err("Failed to send deployment transaction")?;

    let tx_hash = pending_tx.tx_hash();
    println!("Deployment transaction submitted: {:?}", tx_hash);

    let receipt = pending_tx
        .await
        .wrap_err("Failed to get deployment receipt")?
        .ok_or_else(|| eyre!("Deployment transaction dropped"))?;

    let contract_address = receipt
        .contract_address
        .ok_or_else(|| eyre!("No contract address in receipt"))?;

    if receipt.status == Some(1.into()) {
        println!("Contract deployed at: {:?}", contract_address);
        println!("Confirmed in block {:?}", receipt.block_number);

        // Record deployment
        state.contracts.push(DeployedContract {
            address: format!("{:?}", contract_address),
            contract_type: "ERC20".to_string(),
            name: name.to_string(),
            symbol: symbol.to_string(),
            deployed_by: 1,
            tx_hash: format!("{:?}", tx_hash),
        });
    } else {
        return Err(eyre!("Deployment failed"));
    }

    Ok(contract_address)
}

async fn erc20_transfer(
    provider: &Provider<Http>,
    accounts: &AccountsFile,
    state: &mut SimulatorState,
    contract_address: Address,
    from_idx: usize,
    to_idx: usize,
    amount: U256,
    chain_id: u64,
) -> Result<TxHash> {
    let wallet = get_wallet(accounts, from_idx)?.with_chain_id(chain_id);
    let to_address = get_address(accounts, to_idx)?;

    let client = SignerMiddleware::new(provider.clone(), wallet);

    // Encode transfer(address,uint256) call
    let mut data = Vec::with_capacity(68);
    data.extend_from_slice(&ERC20_TRANSFER_SELECTOR);
    data.extend_from_slice(&encode(&[
        Token::Address(to_address),
        Token::Uint(amount),
    ]));

    let tx = TransactionRequest::new()
        .to(contract_address)
        .data(data);

    println!(
        "Transferring {} tokens from Account {} to Account {}...",
        amount, from_idx, to_idx
    );

    let pending_tx = client
        .send_transaction(tx, None)
        .await
        .wrap_err("Failed to send ERC20 transfer")?;

    let tx_hash = pending_tx.tx_hash();
    println!("Transaction submitted: {:?}", tx_hash);

    let receipt = pending_tx
        .await
        .wrap_err("Failed to get transaction receipt")?
        .ok_or_else(|| eyre!("Transaction dropped"))?;

    if receipt.status == Some(1.into()) {
        println!("Transaction confirmed in block {:?}", receipt.block_number);

        state.tx_history.push(TxRecord {
            tx_hash: format!("{:?}", tx_hash),
            tx_type: "erc20_transfer".to_string(),
            from: from_idx,
            to: to_idx,
            amount: amount.to_string(),
        });
    } else {
        return Err(eyre!("ERC20 transfer failed"));
    }

    Ok(tx_hash)
}

async fn get_eth_balance(provider: &Provider<Http>, address: Address) -> Result<U256> {
    provider
        .get_balance(address, None)
        .await
        .wrap_err("Failed to get ETH balance")
}

async fn get_erc20_balance(
    provider: &Provider<Http>,
    contract: Address,
    account: Address,
) -> Result<U256> {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&ERC20_BALANCE_OF_SELECTOR);
    data.extend_from_slice(&encode(&[Token::Address(account)]));

    let call = TransactionRequest::new()
        .to(contract)
        .data(data);

    let result = provider
        .call(&call.into(), None)
        .await
        .wrap_err("Failed to call balanceOf")?;

    if result.len() >= 32 {
        Ok(U256::from_big_endian(&result[..32]))
    } else {
        Ok(U256::zero())
    }
}

// ============================================================================
// Status Display
// ============================================================================

async fn show_status(
    provider: &Provider<Http>,
    accounts: &AccountsFile,
    state: &SimulatorState,
    display_priv_keys: bool,
) -> Result<()> {
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║   Alys V2 Transaction Simulator - Status                       ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    // Show accounts
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Accounts ({} total):", accounts.accounts.len());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    for account in &accounts.accounts {
        let address: Address = account.address.parse()?;
        let balance = get_eth_balance(provider, address).await.unwrap_or_default();
        let funded = if state.funded_accounts.contains(&account.index) {
            " [FUNDED]"
        } else {
            ""
        };

        println!("Account #{}{}", account.index, funded);
        println!("  Address: {}", account.address);
        println!("  ETH Balance: {} wei ({:.6} ETH)",
            balance,
            balance.as_u128() as f64 / 1e18
        );

        if display_priv_keys {
            println!("  Private Key: {}", account.private_key);
        }

        // Show ERC20 balances for deployed contracts
        for contract in &state.contracts {
            let contract_addr: Address = contract.address.parse()?;
            if let Ok(token_balance) = get_erc20_balance(provider, contract_addr, address).await {
                if !token_balance.is_zero() {
                    println!("  {} Balance: {} ({})",
                        contract.symbol,
                        token_balance,
                        contract.name
                    );
                }
            }
        }
        println!();
    }

    // Show deployed contracts
    if !state.contracts.is_empty() {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Deployed Contracts ({} total):", state.contracts.len());
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        for (i, contract) in state.contracts.iter().enumerate() {
            println!("Contract #{}:", i + 1);
            println!("  Type: {}", contract.contract_type);
            println!("  Name: {} ({})", contract.name, contract.symbol);
            println!("  Address: {}", contract.address);
            println!("  Deployed by: Account {}", contract.deployed_by);
            println!("  Tx Hash: {}", contract.tx_hash);
            println!();
        }
    }

    // Show recent transactions
    if !state.tx_history.is_empty() {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Recent Transactions (last 10):");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        let start = if state.tx_history.len() > 10 {
            state.tx_history.len() - 10
        } else {
            0
        };

        for tx in &state.tx_history[start..] {
            println!("  {} | {} -> {} | {} | {}",
                tx.tx_type,
                tx.from,
                tx.to,
                tx.amount,
                &tx.tx_hash[..20]
            );
        }
        println!();
    }

    if let Some(updated) = &state.last_updated {
        println!("Last updated: {}", updated);
    }

    Ok(())
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load or generate accounts
    let accounts = load_or_generate_accounts(&cli.keys_dir, cli.num_accounts, cli.overwrite_keys)?;
    let mut state = load_state(&cli.state_dir)?;

    // Connect to RPC if needed
    let provider = Provider::<Http>::try_from(&cli.rpc_url)
        .wrap_err_with(|| format!("Failed to connect to {}", cli.rpc_url))?;

    // Get chain ID for signing
    let chain_id = match provider.get_chainid().await {
        Ok(id) => id.as_u64(),
        Err(e) => {
            eprintln!("Warning: Could not get chain ID ({}), using default 1", e);
            1
        }
    };

    match cli.command {
        Some(Commands::EthTransfer { from, to, amount }) => {
            // Determine sender
            let from_idx = match from {
                Some(idx) => {
                    if !state.funded_accounts.contains(&idx) && idx != 1 {
                        return Err(eyre!(
                            "Account {} is not funded. Only funded accounts can send: {:?}",
                            idx,
                            state.funded_accounts
                        ));
                    }
                    idx
                }
                None => select_eligible_sender(&state, accounts.accounts.len())
                    .ok_or_else(|| eyre!("No eligible sender found"))?,
            };

            // Determine recipient
            let to_idx = to.unwrap_or_else(|| select_random_recipient(from_idx, accounts.accounts.len()));

            // Default amount: 0.001 ETH
            let amount = amount.unwrap_or_else(|| U256::from(1_000_000_000_000_000u64));

            eth_transfer(&provider, &accounts, &mut state, from_idx, to_idx, amount, chain_id).await?;
            save_state(&state, &cli.state_dir)?;
        }

        Some(Commands::Erc20Transfer { contract, from, to, amount }) => {
            // Get contract address
            let contract_addr = match contract {
                Some(addr) => addr,
                None => {
                    let last_contract = state.contracts.last()
                        .ok_or_else(|| eyre!(
                            "No ERC20 contract deployed. Use 'erc20-deploy' first."
                        ))?;
                    last_contract.address.parse()?
                }
            };

            // Determine sender (must have tokens)
            let from_idx = match from {
                Some(idx) => idx,
                None => 1, // Default to account 1 (deployer has initial supply)
            };

            // Determine recipient
            let to_idx = to.unwrap_or_else(|| select_random_recipient(from_idx, accounts.accounts.len()));

            erc20_transfer(&provider, &accounts, &mut state, contract_addr, from_idx, to_idx, amount, chain_id).await?;
            save_state(&state, &cli.state_dir)?;
        }

        Some(Commands::Erc20Deploy { name, symbol, initial_supply }) => {
            deploy_erc20(&provider, &accounts, &mut state, &name, &symbol, initial_supply, chain_id).await?;
            save_state(&state, &cli.state_dir)?;
        }

        Some(Commands::Status) => {
            show_status(&provider, &accounts, &state, cli.display_priv_keys).await?;
        }

        Some(Commands::ResetState) => {
            state = SimulatorState {
                funded_accounts: vec![1],
                ..Default::default()
            };
            save_state(&state, &cli.state_dir)?;
            println!("State reset. Keys preserved in {}", cli.keys_dir.display());
        }

        None => {
            // No command - show status by default
            show_status(&provider, &accounts, &state, cli.display_priv_keys).await?;
        }
    }

    Ok(())
}
