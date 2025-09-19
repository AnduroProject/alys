//! Cache Cleanup Demo
//!
//! This example demonstrates the improved cache cleanup functionality
//! that properly removes expired entries from both the LRU cache and
//! the expiration tracking maps.

use app::actors_v2::storage::cache::{StorageCache, CacheConfig, TransactionReceipt};
use lighthouse_wrapper::types::Hash256;
use ethereum_types::H256;
use std::time::Duration;
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧹 Cache Cleanup Demo");
    println!("====================");

    // Create cache with short TTL for demonstration
    let config = CacheConfig {
        max_blocks: 100,
        max_state_entries: 100,
        max_receipts: 50,
        state_ttl: Duration::from_millis(500),    // 500ms for demo
        receipt_ttl: Duration::from_millis(300),  // 300ms for demo
        enable_warming: false,
    };

    let cache = StorageCache::new(config);
    println!("✅ Created cache with short TTL for demonstration");

    // Add some state entries
    println!("\n📝 Adding state entries...");
    for i in 0..5 {
        let key = format!("demo_key_{}", i).into_bytes();
        let value = format!("demo_value_{}", i).into_bytes();
        cache.put_state(key, value).await;
        println!("  Added state entry {}", i);
    }

    // Add some receipt entries
    println!("\n🧾 Adding receipt entries...");
    for i in 0..3 {
        let tx_hash = H256::from_low_u64_be(i as u64);
        let receipt = TransactionReceipt {
            transaction_hash: tx_hash,
            block_hash: Hash256::from_low_u64_be(i as u64 + 1000),
            block_number: i as u64 + 100,
            gas_used: 21000 + i as u64 * 1000,
            status: true,
        };
        cache.put_receipt(tx_hash, receipt).await;
        println!("  Added receipt entry {}", i);
    }

    // Show initial cache stats
    let initial_stats = cache.get_stats().await;
    println!("\n📊 Initial Cache Stats:");
    println!("  Block hits: {}", initial_stats.block_hits);
    println!("  State hits: {}", initial_stats.state_hits);
    println!("  Receipt hits: {}", initial_stats.receipt_hits);
    println!("  State expirations: {}", initial_stats.state_expirations);
    println!("  Receipt expirations: {}", initial_stats.receipt_expirations);
    println!("  Memory usage: {:.2} MB", initial_stats.memory_usage_mb());

    // Wait for some entries to expire
    println!("\n⏳ Waiting 400ms for receipts to start expiring...");
    time::sleep(Duration::from_millis(400)).await;

    // Try to access some entries (will trigger expiration during get)
    println!("\n🔍 Accessing entries (some may have expired)...");
    for i in 0..3 {
        let tx_hash = H256::from_low_u64_be(i as u64);
        match cache.get_receipt(&tx_hash).await {
            Some(_) => println!("  Receipt {} still valid", i),
            None => println!("  Receipt {} expired", i),
        }
    }

    // Wait for all entries to expire
    println!("\n⏳ Waiting 200ms more for all state entries to expire...");
    time::sleep(Duration::from_millis(200)).await;

    // Run manual cleanup
    println!("\n🧹 Running manual cache cleanup...");
    cache.cleanup_expired().await;

    // Show updated stats
    let cleanup_stats = cache.get_stats().await;
    println!("\n📊 After Cleanup Cache Stats:");
    println!("  Block hits: {}", cleanup_stats.block_hits);
    println!("  State hits: {}", cleanup_stats.state_hits);
    println!("  Receipt hits: {}", cleanup_stats.receipt_hits);
    println!("  State expirations: {}", cleanup_stats.state_expirations);
    println!("  Receipt expirations: {}", cleanup_stats.receipt_expirations);
    println!("  Memory usage: {:.2} MB", cleanup_stats.memory_usage_mb());

    // Try accessing expired entries
    println!("\n🔍 Trying to access expired entries...");
    for i in 0..3 {
        let key = format!("demo_key_{}", i).into_bytes();
        match cache.get_state(&key).await {
            Some(_) => println!("  State entry {} still exists", i),
            None => println!("  State entry {} properly cleaned up", i),
        }
    }

    // Show final cache performance
    let hit_rates = cache.get_hit_rates().await;
    println!("\n📈 Cache Performance:");
    println!("  State hit rate: {:.1}%", hit_rates.get("state").unwrap_or(&0.0) * 100.0);
    println!("  Receipt hit rate: {:.1}%", hit_rates.get("receipts").unwrap_or(&0.0) * 100.0);
    println!("  Overall hit rate: {:.1}%", hit_rates.get("overall").unwrap_or(&0.0) * 100.0);

    // Add fresh entries to show cache is still working
    println!("\n🆕 Adding fresh entries to verify cache still works...");
    let fresh_key = b"fresh_key".to_vec();
    let fresh_value = b"fresh_value".to_vec();
    cache.put_state(fresh_key.clone(), fresh_value.clone()).await;

    match cache.get_state(&fresh_key).await {
        Some(value) => {
            println!("  ✅ Fresh entry retrieved: {} bytes", value.len());
            assert_eq!(value, fresh_value);
        }
        None => println!("  ❌ Fresh entry not found"),
    }

    println!("\n🎉 Cache cleanup demo completed successfully!");
    println!("✨ Key improvements demonstrated:");
    println!("  • Proper expiration tracking with separate HashMap");
    println!("  • Efficient cleanup that removes from both cache and tracking");
    println!("  • Comprehensive statistics for monitoring expiration rates");
    println!("  • Memory management to prevent expiration map growth");

    Ok(())
}