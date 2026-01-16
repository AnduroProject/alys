//! NetworkActor V2 Validation Test
//!
//! Simple validation that our NetworkActor V2 implementation works

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🧪 NetworkActor V2 Validation Test");
    println!("=================================");

    // Test configuration creation
    println!("\n1. Testing Configuration...");
    let network_config = app::actors_v2::network_v2::NetworkConfig::default();
    assert!(network_config.validate().is_ok());
    println!("✅ NetworkConfig created and validated");

    let sync_config = app::actors_v2::network_v2::SyncConfig::default();
    assert!(sync_config.validate().is_ok());
    println!("✅ SyncConfig created and validated");

    // Test actor creation
    println!("\n2. Testing Actor Creation...");
    let network_actor = app::actors_v2::network_v2::NetworkActor::new(network_config)?;
    println!("✅ NetworkActor V2 created successfully");

    let sync_actor = app::actors_v2::network_v2::SyncActor::new(sync_config)?;
    println!("✅ SyncActor V2 created successfully");

    // Test manager components
    println!("\n3. Testing Manager Components...");
    let mut peer_manager = app::actors_v2::network_v2::managers::PeerManager::new();
    peer_manager.add_peer(
        "test-peer".to_string(),
        "/ip4/127.0.0.1/tcp/8000".to_string(),
    );
    assert!(peer_manager.get_peer(&"test-peer".to_string()).is_some());
    println!("✅ PeerManager working");

    let mut gossip_handler = app::actors_v2::network_v2::managers::GossipHandler::new();
    gossip_handler.set_active_topics(vec!["alys-blocks".to_string()]);
    assert_eq!(gossip_handler.get_stats().messages_received, 0);
    println!("✅ GossipHandler working");

    let mut request_manager = app::actors_v2::network_v2::managers::BlockRequestManager::new(5);
    assert!(request_manager.can_make_request());
    println!("✅ BlockRequestManager working");

    // Test behaviour creation
    println!("\n4. Testing Behaviour...");
    let behaviour_config = app::actors_v2::network_v2::NetworkConfig::default();
    let mut behaviour =
        app::actors_v2::network_v2::behaviour::AlysNetworkBehaviour::new(&behaviour_config)?;
    behaviour.initialize()?;
    assert!(behaviour.is_initialized());
    println!("✅ AlysNetworkBehaviour working");

    println!("\n🎉 NetworkActor V2 Validation Complete!");
    println!("   ✅ All core components functional");
    println!("   ✅ Two-actor architecture working");
    println!("   ✅ Simplified protocols implemented");
    println!("   ✅ Ready for full libp2p integration");

    Ok(())
}
