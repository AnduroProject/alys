//! Debug NetworkActor creation to identify stack overflow

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Debugging NetworkActor creation step by step");

    // Step 1: Test config creation
    println!("1. Creating NetworkConfig...");
    let config = app::actors_v2::network_v2::NetworkConfig::default();
    println!("✅ NetworkConfig created");

    // Step 2: Test config validation
    println!("2. Validating config...");
    config.validate()?;
    println!("✅ Config validated");

    // Step 3: Test behaviour creation
    println!("3. Creating AlysNetworkBehaviour...");
    let behaviour = app::actors_v2::network_v2::behaviour::AlysNetworkBehaviour::new(&config)?;
    println!("✅ AlysNetworkBehaviour created");

    // Step 4: Test metrics creation
    println!("4. Creating NetworkMetrics...");
    let _metrics = app::actors_v2::network_v2::NetworkMetrics::new();
    println!("✅ NetworkMetrics created");

    // Step 5: Test peer manager creation
    println!("5. Creating PeerManager...");
    let _peer_manager = app::actors_v2::network_v2::managers::PeerManager::new();
    println!("✅ PeerManager created");

    // Step 6: Test NetworkActor creation (this might cause stack overflow)
    println!("6. Creating NetworkActor (potential stack overflow point)...");
    let _network_actor = app::actors_v2::network_v2::NetworkActor::new(config)?;
    println!("✅ NetworkActor created successfully!");

    println!("🎉 All components created successfully - no stack overflow detected");

    Ok(())
}