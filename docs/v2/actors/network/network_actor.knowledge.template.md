# NetworkActor Knowledge Template

## Overview

The **NetworkActor** is the core P2P networking component that manages libp2p protocols, message broadcasting, peer connections, and serves as the primary communication gateway for the Alys blockchain network. It implements federation-aware message routing with priority handling for consensus operations.

## Architecture & Core Responsibilities

### Primary Functions
- **P2P Protocol Management**: Orchestrates Gossipsub, Kademlia DHT, mDNS, and custom protocols
- **Message Broadcasting**: Handles block and transaction propagation across the network
- **Federation Coordination**: Priority routing for federation consensus messages
- **Peer Discovery**: Multi-layer peer discovery using DHT and local discovery
- **Network Lifecycle**: Start/stop operations with graceful shutdown support

### Key Components
```rust
pub struct NetworkActor {
    config: NetworkConfig,                           // Network configuration
    swarm: Option<Swarm<AlysNetworkBehaviour>>,     // libp2p swarm instance
    local_peer_id: PeerId,                          // This node's identity
    metrics: NetworkMetrics,                        // Performance statistics
    active_subscriptions: HashMap<String, Instant>, // Topic subscriptions
    pending_requests: HashMap<String, PendingRequest>, // Request tracking
    bootstrap_status: BootstrapStatus,              // DHT bootstrap state
}
```

### Network Behaviour Composition
```rust
#[derive(NetworkBehaviour)]
pub struct AlysNetworkBehaviour {
    gossipsub: Gossipsub,           // Message broadcasting & propagation
    kademlia: Kademlia,             // DHT for peer discovery
    mdns: Mdns,                     // Local network discovery
    identify: Identify,             // Peer identification protocol
    ping: Ping,                     // Connection keepalive
    request_response: RequestResponse, // Direct peer communication
    federation: FederationBehaviour,   // Custom federation logic
}
```

## Message Handlers

### Network Lifecycle Management

#### `StartNetwork`
**Purpose**: Initializes and starts the P2P networking subsystem
- **Parameters**: `listen_addresses`, `bootstrap_peers`, `enable_mdns`
- **Initialization**: Creates libp2p swarm with full protocol stack
- **Bootstrap**: Initiates DHT bootstrap process with configured peers
- **Subscriptions**: Auto-subscribes to essential topics (blocks, transactions, discovery)
- **Response**: `NetworkStartResponse` with peer ID, listening addresses, and protocols

#### `StopNetwork`
**Purpose**: Gracefully or forcefully shuts down networking operations
- **Graceful Shutdown**: 
  - Unsubscribes from all gossipsub topics
  - Disconnects from peers cleanly
  - Maintains connection state for cleanup
- **Force Shutdown**: Immediate termination with actor stop
- **Cleanup**: Clears swarm, pending requests, and resets bootstrap status

#### `GetNetworkStatus`
**Purpose**: Returns comprehensive network operational status
- **Response**: `NetworkStatus` including:
  - Connection counts and peer information
  - Listening addresses and protocol status
  - Bandwidth utilization (in/out bytes)
  - Active gossipsub topics and subscriptions
  - Discovery status (mDNS, Kademlia routing table)

### Message Broadcasting & Gossipsub

#### `BroadcastBlock`
**Purpose**: Propagates new blocks across the network with federation priority
- **Parameters**: `block_data`, `block_height`, `block_hash`, `priority`
- **Topic Selection**: 
  - Priority blocks → `federation_blocks` topic
  - Regular blocks → `blocks` topic
- **Metrics**: Tracks messages sent and peer reach
- **Response**: `BroadcastResponse` with message ID, peer count, and timestamp

#### `BroadcastTransaction`
**Purpose**: Propagates transactions through the network
- **Topic**: `transactions` for all transaction broadcasts
- **Optimization**: Efficient propagation through gossipsub mesh
- **Metrics**: Transaction broadcast tracking and performance monitoring
- **Response**: `BroadcastResponse` with propagation statistics

#### `SubscribeToTopic` / `UnsubscribeFromTopic`
**Purpose**: Dynamic topic subscription management
- **Topic Types**: Blocks, Transactions, FederationMessages, Discovery, Custom
- **Priority Assignment**: Automatic priority based on topic importance
- **Federation Topics**: Special handling for consensus-related subscriptions
- **State Tracking**: Maintains subscription timestamps and activity

### Direct Peer Communication

#### `SendRequest`
**Purpose**: Direct request-response communication with specific peers
- **Protocol**: Custom Alys request-response protocol
- **Timeout Management**: Configurable request timeouts
- **Request Types**: Block requests, sync status, peer info, federation messages
- **Response**: `RequestResponse` with data, peer ID, and duration

### Event Processing

#### `PeerConnected`
**Purpose**: Handles new peer connection events
- **Federation Detection**: Identifies and prioritizes federation peers
- **Metrics Update**: Connection tracking and bandwidth monitoring
- **Priority Setting**: Enhanced handling for federation peer connections
- **Logging**: Detailed connection information and protocol support

#### `PeerDisconnected`
**Purpose**: Manages peer disconnection cleanup
- **Request Cleanup**: Removes pending requests for disconnected peers
- **Metrics Cleanup**: Cleans up latency and performance data
- **State Updates**: Updates connection counts and peer listings

#### `MessageReceived`
**Purpose**: Processes incoming gossipsub messages by topic
- **Topic Routing**:
  - `Blocks` → Forward to ChainActor/SyncActor
  - `Transactions` → Forward to TransactionPool
  - `FederationMessages` → Federation consensus handling
  - `Discovery` → Peer discovery information processing
- **Metrics**: Message counting and bandwidth tracking
- **Validation**: Basic message validation and filtering

#### `NetworkEvent`
**Purpose**: Handles system-wide network events
- **Event Types**:
  - `BootstrapCompleted` → DHT bootstrap success
  - `PartitionDetected/Recovered` → Network partition handling
  - `ProtocolUpgrade` → Protocol version management
  - `BandwidthLimitExceeded` → Rate limiting triggers
  - `SecurityViolation` → Security incident handling

## libp2p Protocol Implementations

### Gossipsub Protocol (`protocols/gossip.rs`)

#### **AlysGossipsub Features**
- **Federation-Aware Routing**: Priority handling for federation messages
- **Custom Message ID**: SHA256-based deduplication
- **Message Validation**: Size limits and content validation
  - Blocks: 1MB maximum
  - Transactions: 256KB maximum  
  - Federation: 2MB maximum
- **Priority Levels**: Critical (Federation) > High (Blocks) > Normal (Transactions)

#### **Topic Management**
- **Default Topics**: `alys/blocks/v1`, `alys/transactions/v1`, `alys/discovery/v1`
- **Federation Topics**: `alys/federation/consensus/v1`, `alys/federation/blocks/v1`, `alys/federation/emergency/v1`
- **Subscription Tracking**: Timestamp and message count per topic
- **Automatic Cleanup**: Message cache cleanup with TTL

### Discovery Protocol (`protocols/discovery.rs`)

#### **AlysDiscovery Features**
- **Dual Discovery**: Kademlia DHT + mDNS for comprehensive peer finding
- **Bootstrap Management**: Automated bootstrap process with status tracking
- **Federation Priority**: Special handling for federation peer discovery
- **Peer Caching**: Multi-source peer information with cleanup

#### **Discovery Operations**
- **Bootstrap**: DHT network joining with configurable bootstrap peers
- **Peer Queries**: Find closest peers for specific operations
- **Record Operations**: Store/retrieve federation configuration in DHT
- **Local Discovery**: mDNS for same-network peer finding

### Request-Response Protocol (`protocols/request_response.rs`)

#### **AlysRequestResponse Features**
- **Custom Codec**: Bincode serialization for efficient message encoding
- **Request Types**: Block downloads, sync coordination, federation messages
- **Timeout Management**: Per-request timeout with cleanup
- **Handler System**: Pluggable request handlers for different message types

#### **Request Handlers**
- **BlockRequestHandler**: Serves block download requests
- **SyncStatusHandler**: Provides sync status information
- **FederationHandler**: Processes federation consensus messages
- **PeerInfoHandler**: Returns peer capability and status information

## Configuration

### NetworkConfig Key Parameters
```rust
pub struct NetworkConfig {
    listen_addresses: Vec<Multiaddr>,     // Network listening addresses
    bootstrap_peers: Vec<Multiaddr>,      // DHT bootstrap peer list
    connection_timeout: Duration,         // Connection establishment timeout
    gossip_config: GossipConfig,         // Gossipsub-specific settings
    discovery_config: DiscoveryConfig,   // DHT and mDNS configuration
    federation_config: FederationNetworkConfig, // Federation networking
}
```

### Federation Configuration
```rust
pub struct FederationNetworkConfig {
    federation_discovery: bool,           // Enable federation peer discovery
    federation_topics: Vec<String>,       // Federation gossipsub topics
    consensus_config: ConsensusConfig,    // Timing and coordination settings
}
```

## Performance Characteristics

### Optimizations
- **Connection Pooling**: Efficient connection reuse and management
- **Message Deduplication**: SHA256-based message ID for duplicate detection
- **Bandwidth Monitoring**: Real-time bandwidth usage tracking
- **Peer Prioritization**: Federation peers get enhanced service

### Metrics Tracking
```rust
pub struct NetworkMetrics {
    messages_sent: u64,                   // Total messages broadcast
    messages_received: u64,               // Total messages received
    total_bandwidth_in: u64,              // Bytes received
    total_bandwidth_out: u64,             // Bytes sent
    peer_latencies: HashMap<PeerId, Duration>, // Per-peer latency tracking
}
```

## Error Handling & Recovery

### Connection Management
- **Automatic Reconnection**: Built-in libp2p connection recovery
- **Peer Rotation**: Automatic switching to better performing peers
- **Bootstrap Recovery**: Re-bootstrap on DHT connection loss
- **Graceful Degradation**: Continued operation with reduced peer set

### Protocol Resilience
- **Message Retry**: Automatic retry for failed broadcasts
- **Timeout Handling**: Proper cleanup of expired requests
- **Partition Recovery**: Detection and recovery from network partitions
- **Security Measures**: Protection against malicious peers and messages

## Integration Points

### SyncActor Coordination
- **Block Broadcasts**: Propagates newly produced blocks
- **Block Requests**: Handles block download requests from sync operations
- **Progress Updates**: Coordinates sync status across the network

### ChainActor Integration
- **Block Production**: Broadcasts blocks after successful mining
- **Transaction Pool**: Propagates transactions for inclusion in blocks
- **Consensus Messages**: Handles federation consensus coordination

### PeerActor Integration
- **Discovery Results**: Provides discovered peers to PeerActor
- **Connection Events**: Notifies PeerActor of connection changes
- **Performance Data**: Shares peer performance metrics

## Usage Examples

### Basic Network Startup
```rust
// Start networking with bootstrap peers
let start_msg = StartNetwork {
    listen_addresses: vec![
        "/ip4/0.0.0.0/tcp/30303".parse()?,
        "/ip6/::/tcp/30303".parse()?,
    ],
    bootstrap_peers: vec![
        "/ip4/bootstrap.alys.network/tcp/30303/p2p/12D3...".parse()?,
    ],
    enable_mdns: true,
};
let response = network_actor.send(start_msg).await?;
```

### Block Broadcasting
```rust
// Broadcast high-priority federation block
let broadcast_msg = BroadcastBlock {
    block_data: block_bytes,
    block_height: 1001,
    block_hash: "0x123...".to_string(),
    priority: true, // Federation priority
};
let response = network_actor.send(broadcast_msg).await?;
println!("Block reached {} peers", response.peers_reached);
```

### Topic Management
```rust
// Subscribe to federation consensus messages
let subscribe_msg = SubscribeToTopic {
    topic: GossipTopic::FederationMessages,
};
network_actor.send(subscribe_msg).await?;

// Direct peer communication
let request_msg = SendRequest {
    peer_id: target_peer,
    request_data: request_bytes,
    timeout_ms: 30000,
};
let response = network_actor.send(request_msg).await?;
```

## Testing & Validation

### Protocol Testing
- **Gossipsub Validation**: Message propagation and deduplication
- **Discovery Testing**: Peer finding across different network topologies
- **Request-Response**: Direct communication reliability and performance
- **Federation Features**: Priority message handling and routing

### Integration Testing
- **Multi-Node Networks**: Real-world network simulation
- **Partition Testing**: Network split and recovery scenarios
- **Load Testing**: High-throughput message broadcasting
- **Security Testing**: Malicious peer and message handling

## Deployment Considerations

### Production Settings
- **Bootstrap Peers**: Configure reliable bootstrap nodes
- **Listen Addresses**: Proper port and interface configuration
- **Federation Topics**: Enable federation-specific topics for validator nodes
- **Resource Limits**: Connection and bandwidth limits

### Monitoring
- **Connection Health**: Monitor peer counts and connection stability
- **Message Metrics**: Track broadcast success rates and latency
- **Bandwidth Usage**: Monitor network resource consumption
- **Discovery Performance**: DHT and mDNS effectiveness metrics

### Security
- **Message Validation**: Implement strict message validation rules
- **Peer Authentication**: Verify federation peer identities
- **Rate Limiting**: Protect against spam and DoS attacks
- **Transport Security**: TLS encryption for sensitive communications

This NetworkActor serves as the robust P2P communication backbone for the Alys blockchain, with special emphasis on federation-aware networking and reliable message propagation for consensus operations.