//! Storage Actor V2 Demo
//!
//! This example demonstrates the basic functionality of the ported V2 Storage Actor
//! showing block storage, retrieval, caching, and metrics collection.

use app::actors_v2::storage::actor::{BlockRef, StorageActor, StorageConfig};
use app::auxpow_miner::BlockIndex;
use app::block::{ConsensusBlock, ConvertBlockHash};
use lighthouse_wrapper::types::{ExecutionBlockHash, Hash256, MainnetEthSpec};

/// Create a test consensus block
fn create_test_block(slot: u64, parent_hash: Hash256) -> ConsensusBlock<MainnetEthSpec> {
    let mut block = ConsensusBlock::<MainnetEthSpec>::default();

    // Update basic fields
    block.slot = slot;
    block.parent_hash = parent_hash;

    // Update execution payload with test values
    block.execution_payload.block_number = slot;
    block.execution_payload.timestamp = 1000000 + slot * 12; // Mock timestamp
    block.execution_payload.state_root = Hash256::from_low_u64_be(slot + 1000);
    block.execution_payload.block_hash =
        ExecutionBlockHash::from_root(Hash256::from_low_u64_be(slot + 2000));
    block.execution_payload.parent_hash = ExecutionBlockHash::from_root(parent_hash);

    block
}

/// Demonstrate basic storage operations
async fn demonstrate_storage_operations() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔧 Storage Actor V2 Demo");
    println!("========================");

    // Create storage configuration
    let mut config = StorageConfig::default();
    config.database.main_path = "/tmp/alys_v2_storage_demo".to_string();

    println!("📁 Database path: {}", config.database.main_path);

    // Initialize storage actor (not as an actual actor, just the struct)
    let mut storage = StorageActor::new(config).await?;
    println!("✅ Storage actor initialized");

    // Create test blocks
    let genesis_block = create_test_block(0, Hash256::zero());
    let block_1_parent = genesis_block.block_hash().to_block_hash();
    let block_1 = create_test_block(1, block_1_parent);
    let block_2_parent = block_1.block_hash().to_block_hash();
    let block_2 = create_test_block(2, block_2_parent);

    println!("\n📦 Created test blocks:");
    println!(
        "  Genesis: {} (slot 0)",
        genesis_block.block_hash().to_block_hash()
    );
    println!(
        "  Block 1: {} (slot 1)",
        block_1.block_hash().to_block_hash()
    );
    println!(
        "  Block 2: {} (slot 2)",
        block_2.block_hash().to_block_hash()
    );

    // Store blocks
    println!("\n💾 Storing blocks...");
    storage.store_block(genesis_block.clone(), true).await?;
    println!("  ✅ Genesis block stored");

    storage.store_block(block_1.clone(), true).await?;
    println!("  ✅ Block 1 stored");

    storage.store_block(block_2.clone(), true).await?;
    println!("  ✅ Block 2 stored");

    // Retrieve blocks
    println!("\n🔍 Retrieving blocks...");

    if let Some(retrieved_genesis) = storage
        .get_block(&genesis_block.block_hash().to_block_hash())
        .await?
    {
        println!(
            "  ✅ Genesis block retrieved: slot {}",
            retrieved_genesis.slot
        );
        assert_eq!(retrieved_genesis.slot, genesis_block.slot);
    }

    if let Some(retrieved_block_1) = storage
        .get_block(&block_1.block_hash().to_block_hash())
        .await?
    {
        println!("  ✅ Block 1 retrieved: slot {}", retrieved_block_1.slot);
        assert_eq!(retrieved_block_1.slot, block_1.slot);
    }

    // Test block retrieval by height
    println!("\n🔢 Retrieving blocks by height...");

    if let Some(block_at_height_1) = storage.database.get_block_by_height(1).await? {
        println!(
            "  ✅ Block at height 1: {}",
            block_at_height_1.block_hash().to_block_hash()
        );
        assert_eq!(
            block_at_height_1.block_hash().to_block_hash(),
            block_1.block_hash().to_block_hash()
        );
    }

    // Update chain head
    println!("\n⛓️ Updating chain head...");
    let new_head = BlockRef {
        hash: block_2.block_hash().to_block_hash(),
        number: 2,
    };
    storage.database.put_chain_head(&new_head).await?;
    println!(
        "  ✅ Chain head updated to block {} at height {}",
        new_head.hash, new_head.number
    );

    // Retrieve chain head
    if let Some(current_head) = storage.database.get_chain_head().await? {
        println!(
            "  📋 Current chain head: {} at height {}",
            current_head.hash, current_head.number
        );
        assert_eq!(current_head.hash, block_2.block_hash().to_block_hash());
        assert_eq!(current_head.number, 2);
    }

    // Test state operations
    println!("\n🔧 Testing state operations...");
    let test_key = b"demo_state_key".to_vec();
    let test_value = b"demo_state_value_12345".to_vec();

    storage.database.put_state(&test_key, &test_value).await?;
    println!("  ✅ State stored");

    if let Some(retrieved_value) = storage.database.get_state(&test_key).await? {
        println!("  ✅ State retrieved: {} bytes", retrieved_value.len());
        assert_eq!(retrieved_value, test_value);
    }

    // Test cache operations
    println!("\n💨 Testing cache operations...");
    let cache_test_block = create_test_block(99, Hash256::from_low_u64_be(999));

    // Put in cache only (not database)
    storage
        .cache
        .put_block(
            cache_test_block.block_hash().to_block_hash(),
            cache_test_block.clone(),
        )
        .await;
    println!("  ✅ Block cached");

    if let Some(cached_block) = storage
        .cache
        .get_block(&cache_test_block.block_hash().to_block_hash())
        .await
    {
        println!(
            "  ✅ Block retrieved from cache: slot {}",
            cached_block.slot
        );
        assert_eq!(cached_block.slot, cache_test_block.slot);
    }

    // Display cache stats
    let cache_stats = storage.cache.get_stats().await;
    let hit_rates = storage.cache.get_hit_rates().await;

    println!("\n📊 Cache Statistics:");
    println!("  Block hits: {}", cache_stats.block_hits);
    println!("  Block misses: {}", cache_stats.block_misses);
    println!(
        "  Overall hit rate: {:.2}%",
        hit_rates.get("overall").unwrap_or(&0.0) * 100.0
    );
    println!("  Memory usage: {:.2} MB", cache_stats.memory_usage_mb());

    // Display storage metrics
    println!("\n📈 Storage Metrics:");
    println!("  Blocks stored: {}", storage.metrics.blocks_stored);
    println!("  Blocks retrieved: {}", storage.metrics.blocks_retrieved);
    println!("  State updates: {}", storage.metrics.state_updates);
    println!(
        "  Cache hit rate: {:.2}%",
        storage.metrics.cache_hit_rate() * 100.0
    );

    // Test database stats
    if let Ok(db_stats) = storage.database.get_stats().await {
        println!("\n💿 Database Statistics:");
        println!(
            "  Total size: {:.2} MB",
            db_stats.total_size_bytes as f64 / (1024.0 * 1024.0)
        );
        println!("  Total keys: {}", db_stats.total_keys);
        println!("  Column families: {}", db_stats.column_family_sizes.len());
    }

    println!("\n🎉 Storage Actor V2 Demo completed successfully!");

    Ok(())
}

#[actix::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    match demonstrate_storage_operations().await {
        Ok(()) => {
            println!("\n✅ Demo completed successfully!");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("\n❌ Demo failed: {}", e);
            std::process::exit(1);
        }
    }
}
