//! Swarm factory for creating configured libp2p swarms
//!
//! This module handles the complex setup of libp2p transport,
//! behaviours, and swarm configuration.

use super::{behaviour::AlysNetworkBehaviour, NetworkConfig};
use anyhow::{Context as AnyhowContext, Result};
use libp2p::{core::upgrade, identity, noise, swarm::SwarmBuilder, tcp, yamux, PeerId};

/// Create a fully configured libp2p Swarm
///
/// This function handles:
/// - Keypair generation/loading
/// - Transport creation (TCP + Noise + Yamux)
/// - Protocol configuration (Gossipsub, Request-Response, Identify, mDNS)
/// - Swarm assembly
pub fn create_swarm(config: &NetworkConfig) -> Result<libp2p::Swarm<AlysNetworkBehaviour>> {
    // 1. Generate or load keypair
    let local_key = generate_keypair(config)?;
    let local_peer_id = PeerId::from(local_key.public());

    tracing::info!("Creating libp2p swarm for peer: {}", local_peer_id);

    // 2. Create transport
    let transport = create_transport(&local_key)?;

    // 3. Create behaviour
    let behaviour = create_behaviour(&local_key, config)?;

    // 4. Build swarm
    let swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, local_peer_id).build();

    Ok(swarm)
}

/// Generate or load keypair from config
///
/// If `keypair_path` is set:
///   - Loads existing keypair from file if it exists
///   - Generates new keypair and saves to file if it doesn't exist
/// Otherwise:
///   - Generates ephemeral keypair (changes on each restart)
fn generate_keypair(config: &NetworkConfig) -> Result<identity::Keypair> {
    if let Some(path) = &config.keypair_path {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .context(format!("Failed to create keypair directory: {:?}", parent))?;
        }

        if path.exists() {
            // Load existing keypair
            let bytes = std::fs::read(path)
                .context(format!("Failed to read keypair from {:?}", path))?;
            let keypair = identity::Keypair::from_protobuf_encoding(&bytes)
                .context("Failed to decode keypair from protobuf")?;
            let peer_id = PeerId::from(keypair.public());
            tracing::info!(
                "Loaded persistent V2 keypair from {:?} (peer_id: {})",
                path,
                peer_id
            );
            return Ok(keypair);
        } else {
            // Generate new keypair and save it
            let keypair = identity::Keypair::generate_ed25519();
            let bytes = keypair
                .to_protobuf_encoding()
                .context("Failed to encode keypair to protobuf")?;
            std::fs::write(path, bytes)
                .context(format!("Failed to write keypair to {:?}", path))?;
            let peer_id = PeerId::from(keypair.public());
            tracing::info!(
                "Generated and saved new V2 keypair to {:?} (peer_id: {})",
                path,
                peer_id
            );
            return Ok(keypair);
        }
    }

    // Fallback: ephemeral keypair (not recommended for production)
    let keypair = identity::Keypair::generate_ed25519();
    tracing::warn!(
        "Generated ephemeral V2 keypair (peer_id: {}). \
         Consider setting keypair_path for persistent identity.",
        PeerId::from(keypair.public())
    );
    Ok(keypair)
}

/// Create transport stack: TCP + Noise + Yamux
fn create_transport(
    local_key: &identity::Keypair,
) -> Result<libp2p::core::transport::Boxed<(PeerId, libp2p::core::muxing::StreamMuxerBox)>> {
    use libp2p::Transport;

    let tcp_transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true));

    let transport = tcp_transport
        .upgrade(upgrade::Version::V1Lazy)
        .authenticate(noise::Config::new(local_key).context("Failed to create Noise config")?)
        .multiplex(yamux::Config::default())
        .timeout(std::time::Duration::from_secs(20))
        .boxed();

    Ok(transport)
}

/// Create and configure all network behaviours
fn create_behaviour(
    local_key: &identity::Keypair,
    config: &NetworkConfig,
) -> Result<AlysNetworkBehaviour> {
    use libp2p::{gossipsub, identify, mdns};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Configure Gossipsub for small networks
    // For 2-node networks, we need to relax mesh requirements
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .max_transmit_size(config.message_size_limit)
        .validation_mode(gossipsub::ValidationMode::Strict)
        // Small network mesh parameters
        // TM-B1: Increased mesh_n_low from 1 to 2 for resilience
        // With mesh_n_low=1, single peer loss doesn't trigger repair
        // With mesh_n_low=2, GossipSub actively seeks more peers when mesh drops below 2
        .mesh_n_low(2) // Minimum peers in mesh (default: 4)
        .mesh_n(2) // Target peers in mesh (default: 6)
        .mesh_n_high(3) // Max peers in mesh (default: 12)
        .mesh_outbound_min(1) // Minimum outbound peers (default: 2)
        // Relax gossip parameters for small networks
        .gossip_lazy(3) // Gossip to this many peers (default: 6)
        .gossip_factor(0.5) // Gossip factor (default: 0.25)
        // CRITICAL FIX: Enable flood publishing for small networks
        // This ensures messages are sent to all connected peers immediately,
        // even if the mesh hasn't formed yet. Essential for 2-node networks
        // where mesh formation can be delayed.
        .flood_publish(true) // Flood messages to all connected peers (default: false)
        .message_id_fn(|msg: &gossipsub::Message| {
            // Use first 20 bytes of hash as message ID
            let mut hasher = DefaultHasher::new();
            msg.data.hash(&mut hasher);
            gossipsub::MessageId::from(hasher.finish().to_string())
        })
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build Gossipsub config: {}", e))?;

    let mut gossipsub: gossipsub::Behaviour = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(local_key.clone()),
        gossipsub_config,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create Gossipsub behaviour: {}", e))?;

    // Subscribe to configured topics
    for topic_str in &config.gossip_topics {
        let topic = gossipsub::IdentTopic::new(topic_str);
        gossipsub
            .subscribe(&topic)
            .context(format!("Failed to subscribe to topic: {}", topic_str))?;
        tracing::debug!("Subscribed to gossip topic: {}", topic_str);
    }

    // Configure Identify
    let identify_config = identify::Config::new("/alys/v2/0.1.0".to_string(), local_key.public())
        .with_agent_version(format!("alys-v2/{}", env!("CARGO_PKG_VERSION")));

    let identify = identify::Behaviour::new(identify_config);

    // Configure mDNS
    let mdns =
        mdns::tokio::Behaviour::new(mdns::Config::default(), local_key.public().to_peer_id())
            .context("Failed to create mDNS behaviour")?;

    // Configure Request-Response with BlockCodec
    let request_response = {
        use super::protocols::BlockCodec;
        let protocols = std::iter::once((
            "/alys/block/1.0.0",
            libp2p::request_response::ProtocolSupport::Full,
        ));
        let cfg = libp2p::request_response::Config::default();
        libp2p::request_response::Behaviour::with_codec(BlockCodec::new(), protocols, cfg)
    };

    Ok(AlysNetworkBehaviour {
        gossipsub,
        identify,
        mdns,
        request_response,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swarm_creation() {
        let config = NetworkConfig::default();
        let swarm = create_swarm(&config).expect("Failed to create swarm");

        // Verify swarm is created
        assert_eq!(swarm.connected_peers().count(), 0);
    }
}
