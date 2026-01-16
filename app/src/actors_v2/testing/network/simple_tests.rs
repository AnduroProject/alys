//! NetworkActor V2 Simple Tests
//!
//! Basic tests to verify the testing framework is working

use crate::actors_v2::network::{NetworkConfig, SyncConfig};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_config_creation() {
        let config = NetworkConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_sync_config_creation() {
        let config = SyncConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_basic_config_validation() {
        let mut config = NetworkConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Invalid config should fail
        config.listen_addresses.clear();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_peer_manager_basic() {
        use crate::actors_v2::network::managers::PeerManager;

        let mut peer_manager = PeerManager::new();

        // Add a peer
        peer_manager.add_peer("test-peer".to_string(), "/ip4/127.0.0.1/tcp/8000".to_string());

        // Check peer exists
        assert!(peer_manager.get_peer(&"test-peer".to_string()).is_some());

        // Remove peer
        peer_manager.remove_peer(&"test-peer".to_string());

        // Peer should no longer be connected
        assert!(peer_manager.get_peer(&"test-peer".to_string()).is_none());
    }

    #[test]
    fn test_gossip_handler_basic() {
        use crate::actors_v2::network::managers::GossipHandler;

        let mut handler = GossipHandler::new();
        handler.set_active_topics(vec!["test-topic".to_string()]);

        let stats = handler.get_stats();
        assert_eq!(stats.messages_received, 0);
    }

    #[test]
    fn test_block_request_manager_basic() {
        use crate::actors_v2::network::managers::BlockRequestManager;

        let manager = BlockRequestManager::new(5);
        assert!(manager.can_make_request());
        assert_eq!(manager.get_available_capacity(), 5);
    }
}