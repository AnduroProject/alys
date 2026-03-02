#![allow(clippy::manual_div_ceil)]

use crate::actors_v2::network::{NetworkMessage, SyncMessage};
use crate::aura::Aura;
use crate::auxpow_miner::spawn_background_miner;
use crate::block_hash_cache::BlockHashCacheInit;
use crate::chain::{BitcoinWallet, Chain};
use crate::engine::*;
use crate::spec::{
    genesis_value_parser, hex_file_parser, ChainSpec, DEV_BITCOIN_SECRET_KEY,
    DEV_REGTEST_AURA_SECRET_KEY_NODE1, DEV_REGTEST_AURA_SECRET_KEY_NODE2,
    DEV_REGTEST_BITCOIN_SECRET_KEY_NODE1, DEV_REGTEST_BITCOIN_SECRET_KEY_NODE2, DEV_SECRET_KEY,
};
use crate::store::{Storage, DEFAULT_ROOT_DIR};
use bridge::{
    bitcoin::Network, BitcoinCore, BitcoinSecretKey, BitcoinSignatureCollector, BitcoinSigner,
    Bridge, Federation,
};
use clap::builder::ArgPredicate;
use clap::Parser;
use eyre::Result;
use futures::pin_mut;
use lighthouse_wrapper::bls::{Keypair, SecretKey};
use lighthouse_wrapper::execution_layer::auth::JwtKey;
use lighthouse_wrapper::store::LevelDB;
use lighthouse_wrapper::types::MainnetEthSpec;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use std::{future::Future, sync::Arc};
use tokio::sync::oneshot;
use tracing::*;
use tracing_subscriber::{prelude::*, EnvFilter};

// V2 RPC imports
use crate::actors_v2::rpc::{RpcActor, RpcConfig, StartRpcServer};
use actix::Actor;

#[inline]
pub fn run() -> Result<()> {
    App::parse().run()
}

pub fn parse_secret_key(s: &str) -> Result<SecretKey, eyre::Error> {
    let secret_key = SecretKey::deserialize(&hex::decode(s)?[..])
        .map_err(|_err| eyre::Error::msg("Failed to deserialize key"))?;
    Ok(secret_key)
}

pub fn parse_bitcoin_secret_key(
    s: &str,
) -> Result<bitcoin::key::secp256k1::SecretKey, eyre::Error> {
    let secret_key = bitcoin::key::secp256k1::SecretKey::from_str(s)
        .map_err(|_err| eyre::Error::msg("Failed to deserialize key"))?;
    Ok(secret_key)
}

#[derive(Parser)]
#[command(author, about = "ALYS", long_about = None)]
pub struct App {
    #[arg(
        long = "chain",
        value_name = "CHAIN_OR_PATH",
        value_parser = genesis_value_parser,
        default_value_if("dev", ArgPredicate::IsPresent, Some("dev")),
        default_value_if("dev_regtest", ArgPredicate::IsPresent, Some("dev-regtest")),
        required_unless_present_any = ["dev", "dev_regtest"]
    )]
    chain_spec: Option<ChainSpec>,

    #[arg(
        long = "aura-secret-key",
        value_parser = parse_secret_key,
        default_value_if("dev", ArgPredicate::IsPresent, Some(DEV_SECRET_KEY)),
        default_value_ifs([
            ("dev_regtest", "true", Some(DEV_REGTEST_AURA_SECRET_KEY_NODE1)),
            ("regtest_node_id", "2", Some(DEV_REGTEST_AURA_SECRET_KEY_NODE2))
        ])
    )]
    pub aura_secret_key: Option<SecretKey>,

    #[arg(
        long = "bitcoin-secret-key",
        value_parser = parse_bitcoin_secret_key,
        default_value_if("dev", ArgPredicate::IsPresent, Some(DEV_BITCOIN_SECRET_KEY)),
        default_value_ifs([
            ("dev_regtest", "true", Some(DEV_REGTEST_BITCOIN_SECRET_KEY_NODE1)),
            ("regtest_node_id", "2", Some(DEV_REGTEST_BITCOIN_SECRET_KEY_NODE2))
        ])
    )]
    pub bitcoin_secret_key: Option<BitcoinSecretKey>,

    #[arg(long)]
    pub wallet_path: Option<String>,

    #[arg(long = "geth-url")]
    pub geth_url: Option<String>,

    #[arg(long = "geth-execution-url")]
    pub geth_execution_url: Option<String>,

    #[arg(long = "db-path")]
    pub db_path: Option<String>,

    /// Flag to enable mining
    #[arg(long = "mine")]
    pub mine: bool,

    /// Flag to disable mining regardless of the `--dev` flags
    #[arg(long = "no-mine", default_value_t = false)]
    pub no_mine: bool,

    #[arg(long = "not-validator", default_value_t = false)]
    pub not_validator: bool,

    /// Disable V0 sync and block production, use V2 Tendermint only
    #[arg(long = "v2-only", default_value_t = false)]
    pub v2_only: bool,

    #[arg(
        long = "full-log-context",
        env = "FULL_LOG_CONTEXT",
        default_value_t = false
    )]
    pub full_log_context: bool,

    #[arg(long, default_value_t = 3000)]
    pub rpc_port: u16,

    #[arg(long, default_value_t = 0)]
    pub p2p_port: u16,

    #[arg(long, default_value = "0.0.0.0")]
    pub p2p_listen_addr: String,

    #[arg(long)]
    pub remote_bootnode: Option<String>,

    #[arg(long)]
    pub dev: bool,

    #[arg(long)]
    pub dev_regtest: bool,

    #[arg(long, default_value_t = 1)]
    pub regtest_node_id: u8,

    #[clap(
        long,
        env = "BITCOIN_RPC_URL",
        default_value_if("dev", ArgPredicate::IsPresent, Some("http://0.0.0.0:18443")),
        default_value_if("dev_regtest", ArgPredicate::IsPresent, Some("http://0.0.0.0:18443")),
        // required_unless_present = "dev"
    )]
    pub bitcoin_rpc_url: Option<String>,

    #[clap(
        long,
        env = "BITCOIN_RPC_USER",
        default_value_if("dev", ArgPredicate::IsPresent, Some("rpcuser")),
        default_value_if("dev_regtest", ArgPredicate::IsPresent, Some("rpcuser")),
        // required_unless_present = "dev"
    )]
    pub bitcoin_rpc_user: Option<String>,

    #[clap(
        long,
        env = "BITCOIN_RPC_PASS",
        default_value_if("dev", ArgPredicate::IsPresent, Some("rpcpassword")),
        default_value_if("dev_regtest", ArgPredicate::IsPresent, Some("rpcpassword")),
        // required_unless_present = "dev"
    )]
    pub bitcoin_rpc_pass: Option<String>,

    #[clap(long, default_value("regtest"))]
    pub bitcoin_network: Network,

    #[clap(long, required = true, value_parser = hex_file_parser)]
    pub jwt_secret: [u8; 32],

    #[clap(long, help = "Port for the metrics server")]
    pub metrics_port: Option<u16>,
}

impl App {
    pub fn run(self) -> Result<()> {
        // Validate mutual exclusivity of dev and dev_regtest flags
        if self.dev && self.dev_regtest {
            return Err(eyre::Error::msg(
                "Cannot use both --dev and --dev-regtest flags simultaneously",
            ));
        }

        // Validate regtest node ID (supports up to 10 nodes)
        if self.dev_regtest && (self.regtest_node_id < 1 || self.regtest_node_id > 10) {
            return Err(eyre::Error::msg(
                "Invalid --regtest-node-id: must be between 1 and 10",
            ));
        }

        self.init_tracing();
        let tokio_runtime = tokio_runtime()?;

        // Create channels for shutdown coordination
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let (chain_tx, chain_rx) =
            oneshot::channel::<Arc<Chain<LevelDB<MainnetEthSpec>>>>();

        // Run the application with graceful shutdown
        let result = tokio_runtime.block_on(async {
            // Spawn the main application
            let execute_handle = tokio::spawn(self.execute_with_shutdown(shutdown_rx, chain_tx));

            // Wait for shutdown signal
            let signal = run_until_ctrl_c(async {
                // Wait for execute to complete (which only happens on error)
                match execute_handle.await {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(e)) => Err(e),
                    Err(e) => Err(eyre::Error::msg(format!("Execute task panicked: {}", e))),
                }
            })
            .await?;

            // Signal shutdown to the execute task
            let _ = shutdown_tx.send(());

            // Perform graceful shutdown if we have the chain
            if let Ok(chain) = chain_rx.await {
                info!("Performing graceful shutdown...");

                // Sync storage to disk
                if let Err(e) = chain.sync_storage() {
                    error!("Failed to sync storage during shutdown: {:?}", e);
                } else {
                    info!("Storage synced successfully during graceful shutdown");
                }
            } else {
                warn!("Could not retrieve chain for graceful shutdown - storage may not be synced");
            }

            info!("Shutdown complete (signal: {:?})", signal);
            Ok::<(), eyre::Error>(())
        });

        result
    }

    fn init_tracing(&self) {
        let rust_log_level = Level::from_str(
            std::env::var("RUST_LOG")
                .unwrap_or("info".to_string())
                .as_str(),
        )
        .unwrap();

        let filter = if self.full_log_context {
            EnvFilter::builder().parse_lossy(rust_log_level.as_str())
        } else {
            let filter_tag =
                format!("app={rust_log_level},federation={rust_log_level},miner={rust_log_level}");
            EnvFilter::builder().parse_lossy(filter_tag.as_str())
        };

        let main_layer = tracing_subscriber::fmt::layer().with_target(true);

        let layers = if rust_log_level == Level::DEBUG || rust_log_level == Level::TRACE {
            vec![main_layer
                .with_file(true)
                .with_line_number(true)
                .with_filter(filter)
                .boxed()]
        } else {
            vec![main_layer.with_filter(filter).boxed()]
        };

        tracing_subscriber::registry().with(layers).init();
    }

    async fn execute_with_shutdown(
        self,
        shutdown_rx: oneshot::Receiver<()>,
        chain_tx: oneshot::Sender<Arc<Chain<LevelDB<MainnetEthSpec>>>>,
    ) -> Result<()> {
        // Log dev-regtest node information
        if self.dev_regtest {
            info!(
                "Running in dev-regtest mode as Node {}",
                self.regtest_node_id
            );
        }

        // Clone values needed for V2 actor system BEFORE V0 takes ownership
        let v2_db_path = self.db_path.clone();
        let v2_geth_url = self.geth_url.clone();
        let v2_geth_execution_url = self.geth_execution_url.clone();
        let v2_jwt_secret = self.jwt_secret;
        let v2_p2p_listen_addr = self.p2p_listen_addr.clone();
        let v2_p2p_port = self.p2p_port;
        let v2_remote_bootnode = self.remote_bootnode.clone();

        let disk_store = Storage::new_disk(self.db_path);

        info!("Head: {:?}", disk_store.get_head());
        info!("Finalized: {:?}", disk_store.get_latest_pow_block());

        // TODO: Combine instantiation of engine & execution apis into Engine::new
        let http_engine_json_rpc =
            new_http_engine_json_rpc(self.geth_url, JwtKey::from_slice(&self.jwt_secret).unwrap());
        let public_execution_json_rpc = new_http_public_execution_json_rpc(self.geth_execution_url);
        let engine = Engine::new(http_engine_json_rpc, public_execution_json_rpc);

        let chain_spec = self.chain_spec.expect("Chain spec is configured");
        let authorities = chain_spec.authorities.clone();
        let slot_duration = chain_spec.slot_duration;
        let bitcoin_start_height = disk_store
            .get_bitcoin_scan_start_height()
            .unwrap()
            .unwrap_or(chain_spec.bitcoin_start_height);

        let mut bitcoin_addresses = Vec::new();

        fn calculate_threshold(federation_bitcoin_pubkeys_len: usize) -> usize {
            ((federation_bitcoin_pubkeys_len * 2) + 2) / 3
        }

        let threshold = calculate_threshold(chain_spec.federation_bitcoin_pubkeys.len()); // 2rds majority, rounded up
        let bitcoin_federation = Federation::new(
            chain_spec.federation_bitcoin_pubkeys.clone(),
            threshold,
            self.bitcoin_network,
        );
        info!(
            "Using bitcoin deposit address {}",
            bitcoin_federation.taproot_address
        );

        bitcoin_addresses.push(bitcoin_federation.taproot_address.clone());

        let wallet_path = self
            .wallet_path
            .clone()
            .unwrap_or(format!("{DEFAULT_ROOT_DIR}/wallet"));
        let bitcoin_wallet = BitcoinWallet::new(&wallet_path, bitcoin_federation.clone())?;
        let bitcoin_signature_collector =
            BitcoinSignatureCollector::new(bitcoin_federation.clone());

        let (maybe_aura_signer, maybe_bitcoin_signer);
        if chain_spec.is_validator && !self.not_validator {
            (maybe_aura_signer, maybe_bitcoin_signer) =
                match (self.aura_secret_key, self.bitcoin_secret_key) {
                    (Some(aura_sk), Some(bitcoin_sk)) => {
                        let aura_pk = aura_sk.public_key();
                        info!("Using aura public key {aura_pk}");
                        let aura_signer = Keypair::from_components(aura_pk, aura_sk);

                        let bitcoin_pk = bitcoin_sk.public_key(&bitcoin::key::Secp256k1::new());
                        info!("Using bitcoin public key {bitcoin_pk}");
                        let bitcoin_signer = BitcoinSigner::new(bitcoin_sk);

                        info!("Running authority");
                        (Some(aura_signer), Some(bitcoin_signer))
                    }
                    (None, Some(_)) => panic!("Aura secret not configured"),
                    (Some(_), None) => panic!("Bitcoin secret not configured"),
                    (None, None) => {
                        info!("Running full node");
                        (None, None)
                    }
                };
        } else {
            (maybe_aura_signer, maybe_bitcoin_signer) = (None, None);
        }

        let aura = Aura::new(
            authorities.clone(),
            slot_duration,
            maybe_aura_signer.clone(),
        );

        // Log entire chain_spec
        info!("****** Chain spec: {:?}", chain_spec);

        // Clone values for V2 RPC before V0 Chain takes ownership
        let v2_bitcoin_rpc_url = self.bitcoin_rpc_url.clone();
        let v2_bitcoin_rpc_user = self.bitcoin_rpc_user.clone();
        let v2_bitcoin_rpc_pass = self.bitcoin_rpc_pass.clone();
        let v2_bitcoin_addresses = bitcoin_addresses.clone();
        let v2_bitcoin_federation = bitcoin_federation.clone();
        let v2_authorities = authorities.clone();
        let v2_maybe_aura_signer = maybe_aura_signer.clone();
        let v2_maybe_bitcoin_sk = self.bitcoin_secret_key;
        let v2_retarget_params = chain_spec.retarget_params.clone();
        let v2_not_validator = self.not_validator;
        let v2_is_validator = chain_spec.is_validator;
        let v2_federation = chain_spec.federation.clone();
        let v2_max_blocks_without_pow = chain_spec.max_blocks_without_pow;
        let v2_required_confirmations = chain_spec.required_btc_txn_confirmations;
        let v2_slot_duration = slot_duration;
        let v2_wallet_path = format!("{DEFAULT_ROOT_DIR}/wallet_v2"); // wallet_path.clone();

        // TODO: We probably just want to persist the chain_spec struct
        let chain = Arc::new(Chain::new(
            engine,
            disk_store,
            aura,
            chain_spec.max_blocks_without_pow,
            chain_spec.federation.clone(),
            Bridge::new(
                BitcoinCore::new(
                    &self.bitcoin_rpc_url.expect("RPC URL is configured"),
                    self.bitcoin_rpc_user.expect("RPC user is configured"),
                    self.bitcoin_rpc_pass.expect("RPC password is configured"),
                ),
                bitcoin_addresses,
                chain_spec.required_btc_txn_confirmations,
            ),
            bitcoin_wallet,
            bitcoin_signature_collector,
            maybe_bitcoin_signer,
            chain_spec.retarget_params.clone(),
            chain_spec.is_validator && !self.not_validator,
        ));

        // import genesis block without signatures or verification
        chain
            .store_genesis(chain_spec.clone())
            .await
            .expect("Should store genesis");

        // Initialize the block hash cache
        chain.init_block_hash_cache().await?;

        // start json-rpc v0 server
        crate::rpc::run_server(
            chain.clone(),
            bitcoin_federation.taproot_address,
            chain_spec.retarget_params,
            self.rpc_port,
        )
        .await;

        // Start V2 JSON-RPC server on port 3001
        info!("Starting V2 RPC server on port 3001 (sharing state with V0 Chain)...");

        // Spawn V2 actor system in LocalSet (required for Actix !Send actors)
        // Use std::thread instead of spawn_blocking to create a dedicated runtime
        std::thread::spawn(move || {
            // Create a new Tokio runtime for V2 actors
            let rt = tokio::runtime::Runtime::new().expect("Failed to create V2 runtime");
            rt.block_on(async move {
                let local = tokio::task::LocalSet::new();
                local.run_until(async move {
                    info!("🚀 Starting V2 Actor System initialization...");

                    // Clone values for slot worker before Aura consumes them
                    let v2_authorities_for_slot_worker = v2_authorities.clone();
                    let v2_maybe_aura_signer_for_slot_worker = v2_maybe_aura_signer.clone();

                    // Create V2 Aura (separate instance for V2 consensus)
                    let v2_aura = Aura::new(v2_authorities, v2_slot_duration, v2_maybe_aura_signer);

            // STATE SHARING STRATEGY:
            // V0 Chain owns Bridge/Wallet directly (not Arc-wrapped)
            // V2 ChainState expects Arc<RwLock<>> wrappers for async access
            //
            // Current approach: Create separate instances but share filesystem state
            // - Bridge: Separate instances, synced via Bitcoin blockchain state
            // - Wallet: SAME wallet file (disk-level sharing)
            // - SignatureCollector: Separate instances (stateless, deterministic)
            //
            // TODO: Future optimization - wrap V0 Chain's components in Arc<RwLock<>>
            // to enable true in-memory state sharing (requires V0 Chain refactor)

            // Create V2 Bridge (separate instance, eventually consistent via Bitcoin)
            let shared_bridge = Bridge::new(
                BitcoinCore::new(&v2_bitcoin_rpc_url.expect("RPC URL"),
                               v2_bitcoin_rpc_user.expect("RPC user"),
                               v2_bitcoin_rpc_pass.expect("RPC pass")),
                v2_bitcoin_addresses,
                v2_required_confirmations,
            );

            // Create V2 Wallet using SAME filesystem path as V0 (disk-level sharing)
            let shared_wallet = BitcoinWallet::new(
                &v2_wallet_path,  // SAME path as V0 - disk-level state sharing
                v2_bitcoin_federation.clone(),
            )
            .expect("V2 wallet creation");

            let shared_sig_collector = BitcoinSignatureCollector::new(v2_bitcoin_federation);
            let shared_signer = v2_maybe_bitcoin_sk.map(BitcoinSigner::new);

            let v2_state = crate::actors_v2::chain::state::ChainState::new(
                v2_aura,
                v2_federation.clone(),
                shared_bridge,
                shared_wallet,
                shared_sig_collector,
                shared_signer,
                v2_retarget_params,
                v2_is_validator && !v2_not_validator,
                v2_max_blocks_without_pow,
                None,
            );

            let v2_config = crate::actors_v2::chain::ChainConfig {
                is_validator: v2_is_validator && !v2_not_validator,
                validator_address: None,
                federation: v2_federation,
                max_blocks_without_pow: v2_max_blocks_without_pow,
                block_production_timeout: Duration::from_secs(30),
                block_validation_timeout: Duration::from_secs(10),
                enable_auxpow: true,
                enable_peg_operations: true,
                retarget_params: Some(crate::actors_v2::chain::config::BitcoinConsensusParams {
                    target_spacing: Duration::from_secs(600),
                    target_timespan: Duration::from_secs(1209600),
                    retarget_interval: 2016,
                    max_target: 0x1d00ffff,
                }),
                block_hash_cache_size: Some(1000),
                chain_id: 1337,
            };

            // 1. Initialize StorageActor V2
            info!("📦 Initializing StorageActor V2...");
            let v2_data_path = v2_db_path.unwrap_or_else(|| format!("{}/v2", crate::store::DEFAULT_ROOT_DIR));
            let storage_config = crate::actors_v2::storage::StorageConfig {
                database: crate::actors_v2::storage::database::DatabaseConfig {
                    main_path: v2_data_path.clone(),
                    archive_path: None,
                    cache_size_mb: 256,
                    write_buffer_size_mb: 64,
                    max_open_files: 1000,
                    compression_enabled: true,
                },
                cache: crate::actors_v2::storage::cache::CacheConfig {
                    max_blocks: 1000,
                    max_state_entries: 10000,
                    max_receipts: 5000,
                    state_ttl: Duration::from_secs(300),
                    receipt_ttl: Duration::from_secs(300),
                    enable_warming: false,
                },
                write_batch_size: 100,
                sync_interval: Duration::from_millis(100),
                maintenance_interval: Duration::from_secs(300),
                enable_auto_compaction: true,
                metrics_reporting_interval: Duration::from_secs(60),
            };
            let storage_actor = crate::actors_v2::storage::StorageActor::new(storage_config)
                .await
                .expect("Failed to create StorageActor V2")
                .start();
            info!("✓ StorageActor V2 started");

            // 2. Initialize EngineActor V2
            info!("⚙️  Initializing EngineActor V2...");
            let v2_http_engine_json_rpc = new_http_engine_json_rpc(
                v2_geth_url,
                JwtKey::from_slice(&v2_jwt_secret).unwrap()
            );
            let v2_public_execution_json_rpc = new_http_public_execution_json_rpc(v2_geth_execution_url);
            let v2_engine = Engine::new(v2_http_engine_json_rpc, v2_public_execution_json_rpc);
            let engine_actor = crate::actors_v2::engine::EngineActor::new(v2_engine).start();
            info!("✓ EngineActor V2 started");

            // 3. Initialize NetworkActor V2
            info!("🌐 Initializing NetworkActor V2...");
            // Persistent keypair ensures stable peer ID across restarts
            let v2_keypair_path = PathBuf::from(format!("{}/v2_identity/keypair", v2_data_path));
            let network_config = crate::actors_v2::network::NetworkConfig {
                listen_addresses: vec![
                    format!("/ip4/{}/tcp/{}", v2_p2p_listen_addr, if v2_p2p_port == 0 { 0 } else { v2_p2p_port + 1000 })
                ],
                bootstrap_peers: v2_remote_bootnode.map(|b| vec![b]).unwrap_or_default(),
                max_connections: 1000,
                max_inbound_connections: 500,
                max_outbound_connections: 500,
                connection_timeout: Duration::from_secs(30),
                gossip_topics: vec![
                    "alys/blocks".to_string(),              // Regular block gossip
                    "alys/blocks/priority".to_string(),     // Priority block gossip
                    "alys/transactions".to_string(),        // Transaction gossip
                    "alys/auxpow".to_string(),              // AuxPoW mining coordination
                    // Tendermint consensus topics
                    "alys-tendermint-proposals".to_string(),  // Block proposals
                    "alys-tendermint-votes".to_string(),      // Prevotes and precommits
                    "alys-tendermint-timeouts".to_string(),   // Timeout notifications
                    "alys-tendermint-evidence".to_string(),   // Equivocation evidence
                    "alys-tendermint-newround".to_string(),   // New round announcements
                ],
                message_size_limit: 4 * 1024 * 1024, // 4MB
                discovery_interval: Duration::from_secs(60),
                auto_dial_mdns_peers: true, // Phase 2 Task 2.4: Enable mDNS auto-dial
                keypair_path: Some(v2_keypair_path), // Persistent identity for stable peer connections
                ..Default::default() // Phase 4: Use default values for rate limiting & connection limits
            };
            let network_actor = crate::actors_v2::network::NetworkActor::new(network_config.clone())
                .expect("Failed to create NetworkActor V2")
                .start();
            info!("✓ NetworkActor V2 started");

            let network_start_msg = NetworkMessage::StartNetwork {
                listen_addrs: network_config.clone().listen_addresses,
                bootstrap_peers: network_config.clone().bootstrap_peers,
            };
            let _ = network_actor.send(network_start_msg).await.expect("Failed to start NetworkActor V2 Network");
            info!("✓ NetworkActor V2 - network started");

            // 4. Initialize SyncActor V2
            info!("🔄 Initializing SyncActor V2...");
            let sync_data_dir = PathBuf::from(format!("{}/sync", v2_data_path));
            let sync_config = crate::actors_v2::network::SyncConfig {
                max_blocks_per_request: 128,
                sync_timeout: Duration::from_secs(30),
                max_concurrent_requests: 4,
                block_validation_timeout: Duration::from_secs(10),
                max_sync_peers: 8,
                data_dir: sync_data_dir,
                ..Default::default()
            };
            let sync_actor_instance = crate::actors_v2::network::SyncActor::new(sync_config)
                .expect("Failed to create SyncActor V2");

            // Issue 4.2 Step 4.2.6: Get TendermintSyncValidator reference before starting actor
            // This allows sharing with ChainActor for governance notifications
            let tendermint_sync_validator = sync_actor_instance.tendermint_validator();

            let sync_actor = sync_actor_instance.start();
            info!("✓ SyncActor V2 started");

            // 5. Initialize ChainActor V2 and wire up dependencies
            info!("⛓️  Initializing ChainActor V2...");
            let mut chain_actor = crate::actors_v2::chain::ChainActor::new(v2_config, v2_state);

            // Wire actor dependencies
            chain_actor.set_storage_actor(storage_actor.clone());
            chain_actor.set_network_actors(network_actor.clone(), sync_actor.clone());
            chain_actor.set_engine_actor(engine_actor.clone());

            // Configure Tendermint consensus
            info!("🔐 Configuring Tendermint consensus...");
            let validator_set = crate::actors_v2::chain::tendermint::ValidatorSet::with_equal_power(
                v2_authorities_for_slot_worker.clone()
            );
            let wal_path = std::path::PathBuf::from(format!("{}/tendermint_wal", v2_data_path));

            // Create WAL directory if it doesn't exist
            if let Err(e) = std::fs::create_dir_all(&wal_path) {
                error!("Failed to create WAL directory: {:?}", e);
            }

            // Configure Tendermint with validator keypair (if validator)
            if let Err(e) = chain_actor.configure_tendermint(
                v2_maybe_aura_signer_for_slot_worker.clone(),
                validator_set.clone(),
                &wal_path,
            ) {
                error!("✗ Failed to configure Tendermint: {:?}", e);
            } else {
                info!("✓ Tendermint consensus configured");
            }

            // Wrap validator_set in Arc for TendermintDriver (reuse the same set)
            let validator_set_arc = std::sync::Arc::new(validator_set);

            let chain_actor_addr = chain_actor.start();
            info!("✓ ChainActor V2 started with all dependencies wired");

            // Phase 1: Set ChainActor address in NetworkActor for block forwarding
            match network_actor.send(NetworkMessage::SetChainActor {
                addr: chain_actor_addr.clone(),
            }).await {
                Ok(Ok(_)) => info!("✓ ChainActor address configured in NetworkActor for block reception"),
                Ok(Err(e)) => error!("✗ Failed to set ChainActor in NetworkActor: {:?}", e),
                Err(e) => error!("✗ NetworkActor mailbox error during SetChainActor: {:?}", e),
            }

            // Set StorageActor address in NetworkActor for serving block requests to peers
            match network_actor.send(NetworkMessage::SetStorageActor {
                addr: storage_actor.clone(),
            }).await {
                Ok(Ok(_)) => info!("✓ StorageActor address configured in NetworkActor for block request handling"),
                Ok(Err(e)) => error!("✗ Failed to set StorageActor in NetworkActor: {:?}", e),
                Err(e) => error!("✗ NetworkActor mailbox error during SetStorageActor: {:?}", e),
            }

            // Phase 0: Wire ChainActor to SyncActor (CRITICAL FIX for security vulnerability)
            match sync_actor.send(SyncMessage::SetChainActor {
                addr: chain_actor_addr.clone(),
            }).await {
                Ok(Ok(_)) => info!("✓ ChainActor configured in SyncActor - blocks will route through validation"),
                Ok(Err(e)) => error!("✗ Failed to set ChainActor in SyncActor: {:?}", e),
                Err(e) => error!("✗ SyncActor mailbox error during SetChainActor: {:?}", e),
            }

            // Wire NetworkActor to SyncActor - without this, SyncActor cannot query peers for chain heights or request historical blocks
            match sync_actor.send(SyncMessage::SetNetworkActor {
                addr: network_actor.clone(),
            }).await {
                Ok(Ok(_)) => info!("✓ NetworkActor configured in SyncActor - enables peer discovery for sync"),
                Ok(Err(e)) => error!("✗ Failed to set NetworkActor in SyncActor: {:?}", e),
                Err(e) => error!("✗ SyncActor mailbox error during SetNetworkActor: {:?}", e),
            }

            // Wire StorageActor to SyncActor - enables accurate height queries for Active Height Monitoring
            match sync_actor.send(SyncMessage::SetStorageActor {
                addr: storage_actor.clone(),
            }).await {
                Ok(Ok(_)) => info!("✓ StorageActor configured in SyncActor - enables accurate gap calculation"),
                Ok(Err(e)) => error!("✗ Failed to set StorageActor in SyncActor: {:?}", e),
                Err(e) => error!("✗ SyncActor mailbox error during SetStorageActor: {:?}", e),
            }

            // Wire SyncActor to NetworkActor - without this, NetworkActor cannot forward received blocks to SyncActor for processing
            match network_actor.send(NetworkMessage::SetSyncActor {
                addr: sync_actor.clone(),
            }).await {
                Ok(Ok(_)) => info!("✓ SyncActor address configured in NetworkActor for block response forwarding"),
                Ok(Err(e)) => error!("✗ Failed to set SyncActor in NetworkActor: {:?}", e),
                Err(e) => error!("✗ NetworkActor mailbox error during SetSyncActor: {:?}", e),
            }

            // Issue 4.2 Step 4.2.6: Wire TendermintSyncValidator to ChainActor for governance notifications
            // This allows ChainActor to notify the sync validator when validator set changes are processed
            if let Some(validator) = tendermint_sync_validator {
                match chain_actor_addr.send(crate::actors_v2::chain::messages::SetSyncValidator { validator }).await {
                    Ok(()) => info!("✓ TendermintSyncValidator configured in ChainActor for governance notifications"),
                    Err(e) => error!("✗ Failed to set TendermintSyncValidator in ChainActor: {:?}", e),
                }
            }

            // Clone chain_actor_addr for slot worker (before RPC consumes it)
            let chain_actor_addr_for_slot_worker = chain_actor_addr.clone();

            // 6. Initialize RPC server
            info!("🔌 Starting V2 RPC server on port 3001...");
            let rpc_config = RpcConfig {
                bind_address: "127.0.0.1:3001".parse().expect("Valid address"),
                request_timeout: Duration::from_secs(30),
                enable_logging: true,
                enable_metrics: true,
            };

            let rpc_actor = RpcActor::new(rpc_config, chain_actor_addr).start();

            match rpc_actor.send(StartRpcServer).await {
                Ok(Ok(())) => info!("✓ V2 RPC server started successfully on port 3001"),
                Ok(Err(e)) => error!("✗ V2 RPC server failed to start: {:?}", e),
                Err(e) => error!("✗ V2 RPC actor mailbox error: {:?}", e),
            }

            info!("🎉 V2 Actor System fully initialized and operational!");

            // 7. Start Tendermint consensus driver
            {
                use crate::actors_v2::tendermint_driver::{TendermintDriver, TendermintDriverConfig, NodeMode, TendermintDriverMessage};
                use crate::actors_v2::chain::tendermint::TimeoutConfig;
                use crate::actors_v2::chain::messages::ChainMessage;

                info!("🔄 Starting Tendermint consensus driver...");

                // Determine node mode
                let node_mode = if v2_is_validator && !v2_not_validator {
                    NodeMode::Validator
                } else {
                    NodeMode::Observer
                };

                // Create driver config
                let driver_config = TendermintDriverConfig {
                    mode: node_mode.clone(),
                    timeout_config: TimeoutConfig::default(),
                    data_dir: std::path::PathBuf::from(format!("{}/tendermint_driver", v2_data_path)),
                    wal_enabled: true,
                };

                // Create WAL directory for driver
                if let Err(e) = std::fs::create_dir_all(&driver_config.data_dir) {
                    error!("Failed to create Tendermint driver WAL directory: {:?}", e);
                }

                // Get validator public key if we're a validator
                let validator_pubkey = v2_maybe_aura_signer_for_slot_worker.as_ref().map(|kp| kp.pk.clone());

                // Create TendermintDriver (reuse validator_set_arc from above)
                let mut tendermint_driver = TendermintDriver::new(
                    driver_config,
                    validator_pubkey,
                    validator_set_arc,
                );

                // Wire driver to ChainActor (driver -> chain)
                tendermint_driver.set_chain_actor(chain_actor_addr_for_slot_worker.clone());

                // Start the driver as an Actix actor
                let driver_addr = tendermint_driver.start();

                // Wire ChainActor to driver (chain -> driver) for bidirectional communication
                // This allows ChainActor to notify the driver when blocks are committed
                if let Err(e) = chain_actor_addr_for_slot_worker.try_send(
                    ChainMessage::SetTendermintDriver { addr: driver_addr.clone() }
                ) {
                    error!("Failed to set TendermintDriver in ChainActor: {:?}", e);
                } else {
                    info!("✓ Bidirectional wiring complete: ChainActor <-> TendermintDriver");
                }

                info!(
                    mode = ?node_mode,
                    "✓ Tendermint consensus driver started"
                );

                // Query current chain height from storage to determine start height
                let start_height = match storage_actor.send(
                    crate::actors_v2::storage::messages::GetChainHeadMessage { correlation_id: None }
                ).await {
                    Ok(Ok(Some(head))) => {
                        info!(current_height = head.number, "Starting Tendermint at height {}", head.number + 1);
                        head.number + 1
                    }
                    Ok(Ok(None)) => {
                        info!("No chain head found, starting Tendermint at height 1");
                        1
                    }
                    Ok(Err(e)) => {
                        warn!(error = ?e, "Failed to get chain head, starting at height 1");
                        1
                    }
                    Err(e) => {
                        warn!(error = ?e, "Storage actor unreachable, starting at height 1");
                        1
                    }
                };

                // Wait for peers before starting consensus (peer-readiness gating)
                // This prevents nodes from starting consensus before they can communicate
                {
                    use crate::actors_v2::network::messages::{NetworkMessage, NetworkResponse};

                    let min_peers = 1; // Minimum peers required before starting consensus
                    let max_wait_secs = 60; // Maximum time to wait for peers
                    let poll_interval_ms = 500; // How often to check for peers

                    info!(
                        min_peers = min_peers,
                        max_wait_secs = max_wait_secs,
                        "Waiting for peer connections before starting Tendermint consensus..."
                    );

                    let start_time = std::time::Instant::now();
                    let mut peer_count = 0;

                    loop {
                        // Query NetworkActor for connected peers
                        match network_actor.send(NetworkMessage::GetConnectedPeers).await {
                            Ok(Ok(NetworkResponse::Peers(peers))) => {
                                peer_count = peers.len();
                                if peer_count >= min_peers {
                                    info!(
                                        peer_count = peer_count,
                                        elapsed_secs = start_time.elapsed().as_secs(),
                                        "Sufficient peers connected - starting Tendermint consensus"
                                    );
                                    break;
                                }
                            }
                            Ok(Ok(other)) => {
                                warn!("Unexpected response from NetworkActor: {:?}", other);
                            }
                            Ok(Err(e)) => {
                                warn!("NetworkActor error getting peers: {:?}", e);
                            }
                            Err(e) => {
                                warn!("Failed to query NetworkActor for peers: {:?}", e);
                            }
                        }

                        // Check timeout
                        if start_time.elapsed().as_secs() >= max_wait_secs as u64 {
                            warn!(
                                peer_count = peer_count,
                                min_peers = min_peers,
                                elapsed_secs = start_time.elapsed().as_secs(),
                                "Peer wait timeout - starting Tendermint consensus anyway (may have sync issues)"
                            );
                            break;
                        }

                        // Log progress periodically
                        if start_time.elapsed().as_secs() % 5 == 0 && start_time.elapsed().as_millis() % 1000 < poll_interval_ms as u128 {
                            info!(
                                peer_count = peer_count,
                                min_peers = min_peers,
                                elapsed_secs = start_time.elapsed().as_secs(),
                                "Still waiting for peers..."
                            );
                        }

                        tokio::time::sleep(std::time::Duration::from_millis(poll_interval_ms)).await;
                    }
                }

                // Start consensus at the determined height
                if let Err(e) = driver_addr.try_send(TendermintDriverMessage::NewHeight { height: start_height }) {
                    error!("Failed to send initial NewHeight to TendermintDriver: {:?}", e);
                }
            }

                    // Keep actors alive - this task runs indefinitely
                    loop {
                        // tokio::time::sleep(Duration::from_secs(3600)).await;
                        std::future::pending::<()>().await;
                    }
                }).await;
            });
        });

        crate::metrics::start_server(self.metrics_port).await;

        // V0 network stack removed - all networking handled by V2 NetworkActor
        // V0 block production uses V2 Tendermint consensus now
        info!("V0 network removed - using V2 NetworkActor for all P2P communication");

        // Bitcoin block monitoring for peg-ins (still needed for bridge operations)
        if chain_spec.is_validator && !self.not_validator {
            chain
                .clone()
                .monitor_bitcoin_blocks(bitcoin_start_height)
                .await;
        }

        // Send the chain Arc for graceful shutdown handling
        if chain_tx.send(chain.clone()).is_err() {
            warn!("Failed to send chain for graceful shutdown - receiver dropped");
        }

        // Keep the application running until shutdown signal
        info!("Application initialized successfully. Running until shutdown signal...");
        let _ = shutdown_rx.await;
        info!("Shutdown signal received in execute task");

        Ok(())
    }
}

// async code taken from reth, when we add more complexity we should adopt
// the task manager logic to handle thread spawning and graceful shutdown
pub fn tokio_runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

/// Shutdown signal type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShutdownSignal {
    CtrlC,
    Sigterm,
    Normal,
}

async fn run_until_ctrl_c<F, E>(fut: F) -> Result<ShutdownSignal, E>
where
    F: Future<Output = Result<(), E>>,
    E: Send + Sync + 'static + From<std::io::Error>,
{
    let ctrl_c = tokio::signal::ctrl_c();

    let mut stream = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let sigterm = stream.recv();
    pin_mut!(sigterm, ctrl_c, fut);

    let signal = tokio::select! {
        _ = ctrl_c => {
            info!("Received ctrl-c, initiating graceful shutdown...");
            ShutdownSignal::CtrlC
        },
        _ = sigterm => {
            info!("Received SIGTERM, initiating graceful shutdown...");
            ShutdownSignal::Sigterm
        },
        res = fut => {
            res?;
            ShutdownSignal::Normal
        },
    };

    Ok(signal)
}
