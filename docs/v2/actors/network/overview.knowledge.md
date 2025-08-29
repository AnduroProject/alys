SyncActor

  - Blockchain synchronization: Downloads and validates blocks from peers to achieve 99.5% sync threshold
  - Production readiness: Enforces sync requirements before allowing block production participation
  - Federation timing: Respects 2-second Aura PoA block intervals and consensus constraints
  - Checkpoint management: Creates/restores blockchain state snapshots for resilience

  NetworkActor

  - P2P protocol management: Handles libp2p networking stack and protocol negotiations
  - Message propagation: Manages gossipsub for broadcasting blocks and transactions across network
  - Transport layer: Manages TCP/QUIC connections with TLS encryption
  - Federation protocols: Specialized communication channels for consensus operations

  PeerActor

  - Connection management: Establishes, maintains, and monitors peer connections (1000+ concurrent)
  - Peer classification: Categories peers as Federation members or Regular nodes
  - Performance scoring: Tracks reliability, latency, and throughput for peer selection
  - Discovery service: Finds new peers and manages bootstrap connections