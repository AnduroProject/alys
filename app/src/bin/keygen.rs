//! Key Generation Utility for Alys V2 Regtest
//!
//! Generates cryptographic keys for multi-validator federation setup:
//! - BLS keys for Aura consensus (authorities)
//! - Ethereum addresses for federation
//! - Bitcoin public keys for federation signing

use bitcoin::secp256k1::{PublicKey as BitcoinPublicKey, Secp256k1, SecretKey as BitcoinSecretKey};
use ethereum_types::Address;
use lighthouse_wrapper::bls::SecretKey as BlsSecretKey;
use std::env;
use std::fs::File;
use std::io::Write;

fn generate_ethereum_address_from_bls(bls_pubkey: &lighthouse_wrapper::bls::PublicKey) -> Address {
    // Simple deterministic address generation from BLS public key
    // In production, you might want a more sophisticated scheme
    let pubkey_bytes = bls_pubkey.serialize();

    // Use Blake2 hash (already available) and take first 20 bytes
    use blake2::{Blake2b512, Digest};
    let hash = Blake2b512::digest(&pubkey_bytes);

    let mut addr_bytes = [0u8; 20];
    addr_bytes.copy_from_slice(&hash[0..20]);
    Address::from(addr_bytes)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let num_validators = if args.len() > 1 {
        args[1].parse::<usize>().unwrap_or(3)
    } else {
        3
    };

    let mut output = String::new();

    macro_rules! log {
        ($($arg:tt)*) => {{
            let line = format!($($arg)*);
            println!("{}", line);
            output.push_str(&line);
            output.push('\n');
        }};
    }

    log!("╔════════════════════════════════════════════════════════════════╗");
    log!("║   Alys V2 Federation Key Generator                            ║");
    log!("║   Generating {} validator key sets                             ║", num_validators);
    log!("╚════════════════════════════════════════════════════════════════╝");
    log!("");

    let secp = Secp256k1::new();
    let mut all_bls_pubkeys = Vec::new();
    let mut all_eth_addresses = Vec::new();
    let mut all_btc_pubkeys = Vec::new();

    for i in 0..num_validators {
        log!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        log!("Validator #{} Keys:", i + 1);
        log!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        log!("");

        // Generate BLS keypair for Aura consensus
        let bls_secret = BlsSecretKey::random();
        let bls_public = bls_secret.public_key();

        // Generate Ethereum address from BLS public key
        let eth_address = generate_ethereum_address_from_bls(&bls_public);

        // Generate Bitcoin secp256k1 keypair for federation signing
        let btc_secret = BitcoinSecretKey::new(&mut rand::thread_rng());
        let btc_pubkey = BitcoinPublicKey::from_secret_key(&secp, &btc_secret);

        // Store for summary
        all_bls_pubkeys.push(format!("0x{}", hex::encode(bls_public.serialize())));
        all_eth_addresses.push(format!("{:?}", eth_address));
        all_btc_pubkeys.push(hex::encode(btc_pubkey.serialize()));

        // Print individual keys
        log!("1. BLS Secret Key (Aura Consensus):");
        log!("   {}", hex::encode(bls_secret.serialize()));
        log!("");

        log!("2. BLS Public Key (for spec.rs authorities):");
        log!("   0x{}", hex::encode(bls_public.serialize()));
        log!("");

        log!("3. Ethereum Address (for spec.rs federation):");
        log!("   {:?}", eth_address);
        log!("");

        log!("4. Bitcoin Secret Key (for federation signing):");
        log!("   {}", btc_secret.display_secret());
        log!("");

        log!("5. Bitcoin Public Key (for spec.rs federation_bitcoin_pubkeys):");
        log!("   {}", hex::encode(btc_pubkey.serialize()));
        log!("");

        log!("─────────────────────────────────────────────────────────────────");
        log!("Docker Compose Environment Variables for Node {}:", i + 1);
        log!("─────────────────────────────────────────────────────────────────");
        log!("  - AURA_SECRET_KEY={}", hex::encode(bls_secret.serialize()));
        log!("  - BITCOIN_SECRET_KEY={}", btc_secret.display_secret());
        log!("");
    }

    // Print spec.rs configuration
    log!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    log!("Copy-Paste Configuration for src/spec.rs:");
    log!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    log!("");

    log!("authorities: vec![");
    for (i, pubkey) in all_bls_pubkeys.iter().enumerate() {
        let comma = if i < all_bls_pubkeys.len() - 1 { "," } else { "" };
        log!("    PublicKey::from_str(\"{}\").unwrap(){}", pubkey, comma);
    }
    log!("],");
    log!("");

    log!("federation: vec![");
    for (i, addr) in all_eth_addresses.iter().enumerate() {
        let comma = if i < all_eth_addresses.len() - 1 { "," } else { "" };
        log!("    \"{}\".parse().unwrap(){}", addr.trim_start_matches("0x"), comma);
    }
    log!("],");
    log!("");

    log!("federation_bitcoin_pubkeys: vec![");
    for (i, pubkey) in all_btc_pubkeys.iter().enumerate() {
        let comma = if i < all_btc_pubkeys.len() - 1 { "," } else { "" };
        log!("    BitcoinPublicKey::from_str(\"{}\").unwrap(){}", pubkey, comma);
    }
    log!("],");
    log!("");

    log!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    log!("✓ Key generation complete!");
    log!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    log!("");
    log!("Next steps:");
    log!("1. Save the secret keys securely for each validator node");
    log!("2. Update src/spec.rs with the configuration above");
    log!("3. Configure docker-compose with the environment variables");
    log!("4. Never commit secret keys to version control!");
    log!("");

    // Write to file
    let output_path = "keys/validator-keys.txt";
    if let Err(e) = File::create(output_path)
        .and_then(|mut file| file.write_all(output.as_bytes()))
    {
        eprintln!("Error writing to {}: {}", output_path, e);
    } else {
        println!("✓ Keys written to {}", output_path);
    }
}
