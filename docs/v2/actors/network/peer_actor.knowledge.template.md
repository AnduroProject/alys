# PeerActor Knowledge Template

## Overview

The **PeerActor** is the peer connection management and scoring component responsible for maintaining optimal peer relationships, connection quality assessment, peer discovery coordination, and federation peer prioritization. It manages 1000+ concurrent peer connections with intelligent scoring and selection algorithms.

## Architecture & Core Responsibilities

### Primary Functions
- **Connection Management**: Handles peer connections, disconnections, and connection quality
- **Peer Scoring**: Advanced scoring algorithms for peer selection and prioritization
- **Discovery Coordination**: Works with NetworkActor for peer discovery operations
- **Federation Awareness**: Special handling and prioritization for federation peers
- **Health Monitoring**: Continuous monitoring of peer connection health and performance

### Key Components
```rust
pub struct PeerActor {
    config: PeerConfig,                    // Peer management configuration
    peer_store: PeerStore,                 // Persistent peer information storage
    connection_manager: ConnectionManager, // Active connection management
    scoring_engine: ScoringEngine,         // Peer performance scoring
    discovery_service: DiscoveryService,   // Peer discovery coordination
    health_monitor: HealthMonitor,         // Connection health tracking
    metrics: PeerMetrics,                  // Performance and usage metrics
}
```

### Supporting Systems
- **PeerStore**: Persistent storage for peer information, addresses, and reputation
- **ConnectionManager**: Active connection lifecycle management with priority handling
- **ScoringEngine**: Multi-factor peer scoring with federation prioritization
- **DiscoveryService**: Coordination with NetworkActor for peer discovery
- **HealthMonitor**: Real-time health assessment and proactive issue detection

## Message Handlers

### Connection Management

#### `ConnectToPeer`
**Purpose**: Establishes connections to specific peers with priority handling
- **Parameters**: `peer_id`, `address`, `priority` (Normal, High, Federation)
- **Connection Limits**: Enforces max connection counts per priority level
- **Ban Checking**: Verifies peer is not banned before connection attempt
- **Federation Priority**: Special handling for federation peer connections
- **Response**: `ConnectionResponse` with connection status and timing

#### `DisconnectPeer`
**Purpose**: Cleanly disconnects from specified peers
- **Parameters**: `peer_id`, `reason`, `ban_duration` (optional)
- **Graceful Shutdown**: Allows ongoing operations to complete where possible
- **State Cleanup**: Removes peer from active connections and pending operations
- **Ban Management**: Optional temporary or permanent banning
- **Metrics Update**: Updates connection statistics and peer reputation

#### `GetPeerStatus`
**Purpose**: Retrieves detailed status for specific peers
- **Response**: `PeerStatus` including:
  - Connection state and timing information
  - Performance metrics (latency, bandwidth, success rates)
  - Protocol support and capability information
  - Federation status and priority level
  - Recent activity and interaction history

#### `GetConnectedPeers`
**Purpose**: Lists all currently connected peers with filtering options
- **Parameters**: `filter_criteria` (federation_only, by_protocol, by_performance)
- **Federation Filtering**: Option to return only federation peers
- **Performance Sorting**: Ordered by connection quality and scoring
- **Response**: `ConnectedPeersList` with comprehensive peer information

### Peer Scoring & Selection

#### `UpdatePeerScore`
**Purpose**: Updates peer performance scores based on interactions
- **Parameters**: `peer_id`, `interaction_type`, `performance_data`, `success`
- **Scoring Factors**:
  - **Latency**: Connection response times and message round-trip
  - **Reliability**: Success rates for requests and block delivery
  - **Availability**: Uptime and connection stability
  - **Protocol Support**: Supported features and protocol versions
  - **Federation Status**: Enhanced scoring for verified federation peers
- **Decay Function**: Gradual score decay over time for inactive peers

#### `GetBestPeers`
**Purpose**: Returns optimal peers for specific operations
- **Parameters**: `count`, `operation_type`, `exclude_peers`
- **Operation Types**:
  - `BlockSync`: Peers optimized for block download performance
  - `Transaction`: Fast transaction propagation peers
  - `Discovery`: Good connectivity for peer discovery
  - `Federation`: Federation consensus operations
- **Selection Algorithm**: Multi-factor optimization considering:
  - Current connection quality and latency
  - Historical performance for operation type
  - Geographic and network diversity
  - Federation peer prioritization
- **Response**: `BestPeersList` with ranked peer recommendations

#### `BanPeer`
**Purpose**: Temporarily or permanently bans problematic peers
- **Parameters**: `peer_id`, `duration`, `reason`, `severity`
- **Ban Levels**:
  - `Temporary`: Short-term ban for transient issues (1-24 hours)
  - `Extended`: Longer ban for repeated problems (1-7 days)  
  - `Permanent`: Indefinite ban for malicious behavior
- **Reason Tracking**: Maintains ban reasons for analysis and appeal
- **Automatic Cleanup**: Expired ban removal and periodic review

#### `GetPeerScore`
**Purpose**: Retrieves detailed scoring information for peers
- **Response**: `PeerScore` including:
  - Overall composite score (0.0-1.0)
  - Individual factor scores (latency, reliability, availability)
  - Score history and trend analysis
  - Federation bonus scoring
  - Comparison to peer average scores

### Discovery Operations

#### `StartDiscovery`
**Purpose**: Initiates peer discovery operations
- **Parameters**: `discovery_type`, `target_count`, `filters`
- **Discovery Types**:
  - `Bootstrap`: Initial network joining
  - `Maintenance`: Ongoing peer set optimization
  - `Federation`: Federation-specific peer discovery
  - `Emergency`: Rapid peer acquisition during network issues
- **Coordination**: Works with NetworkActor discovery protocols
- **Response**: `DiscoveryResponse` with operation ID and initial results

#### `StopDiscovery`
**Purpose**: Halts active discovery operations
- **Graceful Stop**: Completes current discovery queries
- **State Cleanup**: Clears pending discovery operations
- **Resource Release**: Frees discovery-related resources

## Peer Store & Persistence

### Peer Information Storage
```rust
pub struct StoredPeer {
    peer_id: PeerId,                    // Unique peer identifier
    addresses: Vec<Multiaddr>,          // Known peer addresses
    last_seen: Instant,                 // Last successful interaction
    reputation: f64,                    // Long-term reputation score
    capabilities: PeerCapabilities,     // Supported protocols and features
    is_federation_peer: bool,           // Federation peer status
    connection_history: ConnectionHistory, // Historical connection data
    performance_metrics: PerformanceMetrics, // Aggregated performance data
}
```

### Persistence Features
- **Durable Storage**: Survives actor restarts and system reboots
- **Reputation Tracking**: Long-term peer behavior assessment
- **Address Management**: Multiple address tracking with freshness
- **Federation Registry**: Persistent federation peer identification

## Connection Management

### Connection Lifecycle
1. **Discovery**: Peer found through discovery protocols
2. **Validation**: Check against ban list and connection limits
3. **Connection**: Establish libp2p connection with timeout
4. **Handshake**: Protocol negotiation and capability exchange
5. **Active**: Full operational peer relationship
6. **Monitoring**: Continuous health and performance tracking
7. **Cleanup**: Graceful disconnection and state cleanup

### Connection Priorities
```rust
pub enum ConnectionPriority {
    Low,        // Background connections
    Normal,     // Standard peer connections
    High,       // Important peer connections (good performers)
    Federation, // Federation consensus peers (highest priority)
}
```

### Connection Limits
- **Total Connections**: Maximum concurrent peer connections (default: 100)
- **Federation Slots**: Reserved slots for federation peers (default: 20)
- **Outbound Ratio**: Minimum outbound connection percentage (default: 30%)
- **Discovery Buffer**: Extra slots for discovery operations (default: 10)

## Scoring Algorithm

### Multi-Factor Scoring
The peer scoring system uses weighted factors to compute an overall peer quality score:

```rust
fn calculate_peer_score(peer: &PeerData) -> f64 {
    let latency_score = 1.0 - (peer.avg_latency.as_secs_f64() / MAX_ACCEPTABLE_LATENCY);
    let reliability_score = peer.success_rate;
    let availability_score = peer.uptime_percentage;
    let freshness_score = time_decay_factor(peer.last_interaction);
    
    let base_score = (latency_score * 0.3) + 
                     (reliability_score * 0.4) + 
                     (availability_score * 0.2) + 
                     (freshness_score * 0.1);
    
    // Federation peer bonus
    let final_score = if peer.is_federation_peer {
        base_score * FEDERATION_BONUS_MULTIPLIER // 1.5x bonus
    } else {
        base_score
    };
    
    final_score.clamp(0.0, 1.0)
}
```

### Scoring Factors
- **Latency (30%)**: Connection speed and responsiveness
- **Reliability (40%)**: Success rate for requests and operations
- **Availability (20%)**: Uptime and connection stability  
- **Freshness (10%)**: Recent activity and interaction recency
- **Federation Bonus**: 50% score boost for verified federation peers

## Health Monitoring

### Health Metrics
- **Connection Quality**: Latency, packet loss, connection drops
- **Performance Trends**: Historical performance tracking and analysis
- **Resource Usage**: Bandwidth consumption and connection overhead
- **Protocol Compliance**: Adherence to Alys network protocols

### Proactive Health Management
- **Automatic Remediation**: Disconnection of consistently poor performers
- **Preventive Actions**: Early detection of connection degradation
- **Load Balancing**: Distribution of operations across healthy peers
- **Recovery Procedures**: Automatic reconnection and peer replacement

## Configuration

### PeerConfig Key Parameters
```rust
pub struct PeerConfig {
    max_connections: usize,              // Maximum concurrent connections
    max_federation_peers: usize,         // Reserved federation peer slots
    connection_timeout: Duration,        // Connection establishment timeout
    health_check_interval: Duration,     // Health monitoring frequency
    score_decay_interval: Duration,      // Score aging frequency
    ban_check_interval: Duration,        // Ban list cleanup frequency
    discovery_config: DiscoveryConfig,   // Discovery coordination settings
    scoring_config: ScoringConfig,       // Scoring algorithm parameters
}
```

### Scoring Configuration
```rust
pub struct ScoringConfig {
    latency_weight: f64,                 // Latency factor weight (0.3)
    reliability_weight: f64,             // Reliability factor weight (0.4)
    availability_weight: f64,            // Availability factor weight (0.2)
    freshness_weight: f64,               // Freshness factor weight (0.1)
    federation_bonus: f64,               // Federation peer bonus (1.5)
    score_decay_rate: f64,               // Score decay over time
    min_interactions: u32,               // Minimum interactions for reliable scoring
}
```

## Integration Points

### NetworkActor Coordination
- **Discovery Integration**: Receives peer discovery results from NetworkActor
- **Connection Events**: Notifies NetworkActor of connection state changes
- **Performance Feedback**: Provides peer performance data for network optimization

### SyncActor Integration
- **Peer Selection**: Provides optimal peers for sync operations
- **Performance Reporting**: Receives sync performance feedback for scoring
- **Connection Management**: Manages connections for sync-specific operations

### ChainActor Integration
- **Federation Peers**: Maintains connections to federation authority peers
- **Block Propagation**: Provides high-quality peers for block broadcasting
- **Consensus Support**: Ensures reliable connections for consensus operations

## Performance Characteristics

### Scalability
- **1000+ Peers**: Designed for large-scale peer management
- **Efficient Storage**: Optimized data structures for peer information
- **Background Processing**: Non-blocking health monitoring and scoring
- **Memory Management**: Automatic cleanup of stale peer data

### Optimization Features
- **Connection Pooling**: Efficient connection reuse and management
- **Lazy Loading**: On-demand peer information retrieval
- **Batch Operations**: Batched scoring updates and health checks
- **Caching**: Frequently accessed peer data caching

## Usage Examples

### Basic Peer Operations
```rust
// Connect to a federation peer with high priority
let connect_msg = ConnectToPeer {
    peer_id: Some(federation_peer_id),
    address: "/ip4/fed.alys.network/tcp/30303".parse()?,
    priority: ConnectionPriority::Federation,
};
let response = peer_actor.send(connect_msg).await?;

// Get best peers for block synchronization
let best_peers_msg = GetBestPeers {
    count: 8,
    operation_type: OperationType::BlockSync,
    exclude_peers: vec![],
};
let peers = peer_actor.send(best_peers_msg).await?;
```

### Peer Scoring and Management
```rust
// Update peer score based on successful block download
let score_update_msg = UpdatePeerScore {
    peer_id: peer_id,
    interaction_type: InteractionType::BlockDownload,
    performance_data: PerformanceData {
        latency: Duration::from_millis(150),
        success: true,
        bytes_transferred: 1024 * 1024, // 1MB block
    },
};
peer_actor.send(score_update_msg).await?;

// Ban a misbehaving peer temporarily
let ban_msg = BanPeer {
    peer_id: problematic_peer,
    duration: BanDuration::Hours(24),
    reason: "Repeated connection failures".to_string(),
    severity: BanSeverity::Moderate,
};
peer_actor.send(ban_msg).await?;
```

## Testing & Validation

### Unit Tests
- **Scoring Algorithm**: Correctness of multi-factor scoring
- **Connection Management**: Proper connection lifecycle handling
- **Ban System**: Ban duration and cleanup functionality
- **Federation Prioritization**: Enhanced federation peer handling

### Integration Tests
- **Network Coordination**: Integration with NetworkActor discovery
- **Performance Under Load**: Large-scale peer management (1000+ peers)
- **Failover Scenarios**: Peer failure and replacement handling
- **Scoring Accuracy**: Real-world performance correlation

## Deployment Considerations

### Production Settings
- **Connection Limits**: Adjust based on available system resources
- **Scoring Weights**: Tune based on network characteristics
- **Federation Peers**: Configure known federation peer identities
- **Health Monitoring**: Set appropriate check intervals for network conditions

### Monitoring
- **Connection Metrics**: Track connection counts and quality
- **Scoring Distribution**: Monitor peer score distributions and trends
- **Ban Statistics**: Track ban rates and effectiveness
- **Discovery Performance**: Monitor peer discovery success rates

### Resource Management
- **Memory Usage**: Monitor peer store size and cleanup efficiency
- **CPU Usage**: Track scoring computation and health check overhead
- **Network Usage**: Monitor discovery and health check bandwidth consumption
- **Storage Growth**: Manage persistent peer information storage

This PeerActor serves as the intelligent peer management system for the Alys blockchain, ensuring optimal peer selection, connection quality, and special support for federation consensus operations through advanced scoring and prioritization algorithms.