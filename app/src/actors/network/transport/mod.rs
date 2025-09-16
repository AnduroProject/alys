//! Network Transport Layer
//! 
//! Provides transport abstractions and implementations for network communication.
//! This module handles low-level transport concerns for the network actors.

use std::io;
use libp2p::{Transport, Multiaddr};
use serde::{Deserialize, Serialize};

/// Transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Enable TCP transport
    pub enable_tcp: bool,
    /// Enable QUIC transport
    pub enable_quic: bool,
    /// Enable WebRTC transport
    pub enable_webrtc: bool,
    /// Connection timeout in seconds
    pub connection_timeout_secs: u64,
    /// Keep-alive interval in seconds
    pub keep_alive_interval_secs: u64,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            enable_tcp: true,
            enable_quic: false,
            enable_webrtc: false,
            connection_timeout_secs: 30,
            keep_alive_interval_secs: 60,
        }
    }
}

/// Transport types supported by the network layer
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportType {
    /// TCP transport
    Tcp,
    /// QUIC transport
    Quic,
    /// WebRTC transport
    WebRTC,
}

/// Transport information
#[derive(Debug, Clone)]
pub struct TransportInfo {
    /// Transport type
    pub transport_type: TransportType,
    /// Local addresses being listened on
    pub listen_addresses: Vec<Multiaddr>,
    /// Whether transport is active
    pub is_active: bool,
}

/// Transport layer errors
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// I/O error
    #[error("Transport I/O error: {0}")]
    Io(#[from] io::Error),
    
    /// Configuration error
    #[error("Transport configuration error: {0}")]
    Configuration(String),
    
    /// Protocol error
    #[error("Transport protocol error: {0}")]
    Protocol(String),
}

/// Transport layer result type
pub type TransportResult<T> = Result<T, TransportError>;

/// Transport manager for handling multiple transport types
#[derive(Debug)]
pub struct TransportManager {
    /// Transport configuration
    config: TransportConfig,
    /// Active transports
    active_transports: Vec<TransportInfo>,
}

impl TransportManager {
    /// Create a new transport manager
    pub fn new(config: TransportConfig) -> Self {
        Self {
            config,
            active_transports: Vec::new(),
        }
    }

    /// Initialize transports based on configuration
    pub fn initialize_transports(&mut self) -> TransportResult<()> {
        if self.config.enable_tcp {
            let tcp_info = TransportInfo {
                transport_type: TransportType::Tcp,
                listen_addresses: Vec::new(),
                is_active: true,
            };
            self.active_transports.push(tcp_info);
        }

        if self.config.enable_quic {
            let quic_info = TransportInfo {
                transport_type: TransportType::Quic,
                listen_addresses: Vec::new(),
                is_active: true,
            };
            self.active_transports.push(quic_info);
        }

        if self.config.enable_webrtc {
            let webrtc_info = TransportInfo {
                transport_type: TransportType::WebRTC,
                listen_addresses: Vec::new(),
                is_active: true,
            };
            self.active_transports.push(webrtc_info);
        }

        Ok(())
    }

    /// Get active transports
    pub fn get_active_transports(&self) -> &[TransportInfo] {
        &self.active_transports
    }

    /// Check if a transport type is supported
    pub fn supports_transport(&self, transport_type: &TransportType) -> bool {
        match transport_type {
            TransportType::Tcp => self.config.enable_tcp,
            TransportType::Quic => self.config.enable_quic,
            TransportType::WebRTC => self.config.enable_webrtc,
        }
    }

    /// Get transport configuration
    pub fn get_config(&self) -> &TransportConfig {
        &self.config
    }
}

/// Extract transport type from multiaddr
pub fn extract_transport_type(addr: &Multiaddr) -> Option<TransportType> {
    for protocol in addr.iter() {
        match protocol {
            libp2p::multiaddr::Protocol::Tcp(_) => return Some(TransportType::Tcp),
            libp2p::multiaddr::Protocol::Quic => return Some(TransportType::Quic),
            libp2p::multiaddr::Protocol::WebRTCDirect => return Some(TransportType::WebRTC),
            _ => continue,
        }
    }
    None
}

/// Validate multiaddr for transport compatibility
pub fn validate_multiaddr(addr: &Multiaddr, transport_manager: &TransportManager) -> bool {
    if let Some(transport_type) = extract_transport_type(addr) {
        transport_manager.supports_transport(&transport_type)
    } else {
        false
    }
}