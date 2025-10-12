# NetworkActor V2 Operations Guide

## Production Monitoring & Observability (Phase 4)

This guide provides comprehensive operational procedures for monitoring, troubleshooting, and optimizing the NetworkActor V2 in production environments.

---

## Table of Contents

1. [Production Monitoring](#production-monitoring)
2. [Metrics Interpretation](#metrics-interpretation)
3. [Health Check System](#health-check-system)
4. [Troubleshooting Guide](#troubleshooting-guide)
5. [Performance Tuning](#performance-tuning)
6. [DOS Protection](#dos-protection)
7. [Incident Response](#incident-response)

---

## Production Monitoring

### Key Metrics to Monitor

The NetworkActor exposes comprehensive metrics for production monitoring. These metrics are available in Prometheus format via the `GetMetrics` message.

#### Connection Metrics
```
network_connected_peers (gauge)          # Current number of connected peers
network_total_connections (counter)      # Total connections established
network_failed_connections (counter)     # Failed connection attempts
network_connection_errors (counter)      # Connection-level errors
```

**Monitoring Strategy:**
- Alert if `connected_peers < 3` for more than 5 minutes
- Alert if `failed_connections / total_connections > 0.5` over 1 hour
- Track connection error rate: `connection_errors / total_connections < 0.1`

#### Message Flow Metrics
```
network_messages_sent (counter)          # Total messages sent
network_messages_received (counter)      # Total messages received
network_bytes_sent (counter)             # Total bytes sent
network_bytes_received (counter)         # Total bytes received
```

**Monitoring Strategy:**
- Monitor message throughput: `rate(messages_sent[1m])`
- Monitor bandwidth usage: `rate(bytes_sent[1m]) + rate(bytes_received[1m])`
- Alert if message rate drops to 0 for more than 2 minutes

#### Gossipsub Metrics
```
network_gossip_messages_published (counter)  # Gossip messages published
network_gossip_messages_received (counter)   # Gossip messages received
network_gossipsub_mesh_size (gauge)          # Current gossipsub mesh size
network_gossipsub_topics_active (gauge)      # Number of active gossip topics
```

**Monitoring Strategy:**
- Healthy mesh size: `gossipsub_mesh_size >= 6`
- Monitor gossip delivery ratio: `gossip_messages_received / gossip_messages_published`
- Alert if `gossipsub_topics_active < 3` (expected: blocks, transactions, auxpow)

#### Request-Response Metrics
```
network_block_requests_sent (counter)        # Block requests sent
network_block_responses_received (counter)   # Block responses received
network_block_response_errors (counter)      # Block response errors
network_request_response_success_rate (gauge) # Success rate (0.0 to 1.0)
```

**Monitoring Strategy:**
- Target success rate: `request_response_success_rate > 0.8`
- Alert if `block_response_errors > 100` over 5 minutes
- Monitor request latency percentiles (p50, p95, p99)

#### Phase 4: Advanced Reputation Metrics
```
network_peer_reputation_average (gauge)  # Average peer reputation
network_peer_reputation_min (gauge)      # Minimum peer reputation
network_peer_reputation_max (gauge)      # Maximum peer reputation
```

**Monitoring Strategy:**
- Healthy average: `peer_reputation_average > 50.0`
- Alert if `peer_reputation_average < 30.0` for more than 10 minutes
- Monitor reputation distribution over time

#### Phase 4: DOS Protection Metrics
```
network_banned_peers_total (counter)         # Total peers banned
network_rate_limited_messages (counter)      # Messages dropped due to rate limiting
network_rejected_connections (counter)       # Connections rejected due to limits
```

**Monitoring Strategy:**
- Alert if `rate(banned_peers_total[5m]) > 10` (potential attack)
- Alert if `rate_limited_messages > 1000` over 1 minute (DOS attack indication)
- Monitor rejected connection rate: `rate(rejected_connections[1m])`

#### Phase 4: Latency Percentiles
```
network_connection_duration_p50_ms (gauge)   # Connection duration p50
network_connection_duration_p95_ms (gauge)   # Connection duration p95
network_connection_duration_p99_ms (gauge)   # Connection duration p99
network_message_latency_p50_ms (gauge)       # Message latency p50
network_message_latency_p95_ms (gauge)       # Message latency p95
network_message_latency_p99_ms (gauge)       # Message latency p99
```

**Monitoring Strategy:**
- Target latency: `message_latency_p95_ms < 500ms`
- Alert if `message_latency_p99_ms > 2000ms` (2 seconds)
- Monitor connection duration for stability patterns

#### Operational Metrics
```
network_uptime_seconds (gauge)               # Network uptime in seconds
network_last_peer_discovered (gauge)         # Unix timestamp of last peer discovery
```

**Monitoring Strategy:**
- Track uptime for reliability SLOs
- Alert if no peer discovery for more than 300 seconds (5 minutes)

---

## Metrics Interpretation

### Connection Health

**Healthy System:**
```
connected_peers: 10-50
peer_reputation_average: 60-80
connection_errors / total_connections: < 0.05
failed_connections / total_connections: < 0.1
```

**Degraded System:**
```
connected_peers: 3-10
peer_reputation_average: 40-60
connection_errors / total_connections: 0.05-0.15
```

**Unhealthy System:**
```
connected_peers: < 3
peer_reputation_average: < 40
connection_errors / total_connections: > 0.15
rate_limited_messages: > 500/minute
```

### Message Flow Patterns

**Normal Operation:**
- Steady gossip message rate: 10-100 messages/second
- Balanced send/receive ratio: 0.8 to 1.2
- Low error rate: < 1% of total messages

**Under Load:**
- Increased gossip rate: 100-500 messages/second
- Rate limiting may activate
- Success rate should remain > 0.9

**System Stress:**
- Gossip rate: > 500 messages/second
- Active rate limiting: `rate_limited_messages` increasing
- Success rate may drop to 0.7-0.8

### Reputation System Interpretation

**Reputation Score Ranges:**
```
90-100: Excellent peer (trusted, high-performance)
70-90:  Good peer (reliable, normal operation)
50-70:  Average peer (acceptable, monitoring recommended)
30-50:  Poor peer (problems detected, limited trust)
10-30:  Bad peer (frequent issues, disconnect soon)
0-10:   Critical peer (immediate disconnect)
< 0:    Banned peer (actively harmful)
```

**Violation Impact:**
```
InvalidMessage:       -5.0 reputation
ExcessiveRate:        -10.0 reputation
MalformedProtocol:    -8.0 reputation
UnresponsivePeer:     -3.0 reputation
OversizedMessage:     -7.0 reputation
```

---

## Health Check System

The NetworkActor provides a comprehensive health check endpoint via the `HealthCheck` message.

### Health Criteria

**System is Healthy when ALL conditions are met:**
1. Network swarm is running (`is_running: true`)
2. At least 1 connected peer (`connected_peers > 0`)
3. Average peer reputation > 0.0

### Health Check Response Format

```rust
NetworkResponse::Healthy {
    is_healthy: bool,
    connected_peers: usize,
    issues: Vec<String>,
}
```

### Common Health Issues

#### Issue: "Network swarm is not running"
**Cause:** NetworkActor not started or failed initialization
**Action:** Check actor logs, verify StartNetwork message was sent
**Resolution:** Restart NetworkActor with valid configuration

#### Issue: "No connected peers"
**Cause:** Network isolation, bootstrap peer failure, or firewall blocking
**Action:**
- Verify bootstrap peers are reachable
- Check firewall rules allow TCP connections
- Verify listen addresses are valid
**Resolution:** Configure valid bootstrap peers, open required ports

#### Issue: "Low peer count (N peers, recommend >= 3)"
**Cause:** Network partition, poor connectivity, or peer churn
**Action:**
- Monitor peer discovery metrics
- Check peer reputation scores
- Verify network connectivity
**Resolution:** Add more bootstrap peers, investigate network issues

#### Issue: "Critical: Average peer reputation is N (critically low)"
**Cause:** Connected to malicious peers or high violation rate
**Action:**
- Review recent violations in logs
- Check for DOS attack indicators
- Inspect peer behavior patterns
**Resolution:** Ban problematic peers, update bootstrap peer list

#### Issue: "High rate limiting active (N messages dropped)"
**Cause:** DOS attack or legitimate traffic spike
**Action:**
- Analyze message sources
- Review rate limit configuration
- Check for attack patterns
**Resolution:** Adjust rate limits, ban attacking peers

#### Issue: "High connection failure rate (N%)"
**Cause:** Network instability, peer quality issues, or resource constraints
**Action:**
- Check system resources (CPU, memory, file descriptors)
- Review connection error logs
- Monitor peer reputation trends
**Resolution:** Scale resources, improve peer selection criteria

---

## Troubleshooting Guide

### Problem: No Peers Connecting

**Symptoms:**
- `connected_peers = 0` for extended period
- `failed_connections` increasing
- No peer discovery events in logs

**Diagnosis Steps:**
1. Check NetworkActor is started: `GetNetworkStatus`
2. Verify listen addresses are valid and not in use
3. Test bootstrap peer connectivity: `telnet <peer_ip> <peer_port>`
4. Check firewall rules: `sudo iptables -L` or `sudo pfctl -s rules`
5. Review actor logs for connection errors

**Common Causes:**
- **Invalid bootstrap peers:** Update `bootstrap_peers` configuration with working peers
- **Port conflicts:** Change `listen_addresses` to unused ports
- **Firewall blocking:** Configure firewall to allow TCP connections on P2P ports
- **Network isolation:** Verify internet connectivity and DNS resolution

**Resolution:**
```rust
// Update configuration with valid bootstrap peers
let config = NetworkConfig {
    listen_addresses: vec!["/ip4/0.0.0.0/tcp/8000".to_string()],
    bootstrap_peers: vec![
        "/ip4/seed1.alys.network/tcp/8000".to_string(),
        "/ip4/seed2.alys.network/tcp/8000".to_string(),
    ],
    ..Default::default()
};
```

### Problem: High Rate Limiting (DOS Attack)

**Symptoms:**
- `rate_limited_messages` rapidly increasing
- `banned_peers_total` increasing
- Message latency increasing significantly

**Diagnosis Steps:**
1. Check rate limiting metrics: `GetMetrics`
2. Identify attacking peers in logs: `grep "Rate limit exceeded" logs/network.log`
3. Review peer violations: Look for `ExcessiveRate` violations
4. Check bandwidth usage: `rate(bytes_received[1m])`

**Common Causes:**
- **DOS attack:** Malicious peer(s) flooding the network
- **Legitimate burst:** Sudden traffic spike from valid operations
- **Misconfigured peer:** Buggy client sending excessive messages

**Resolution:**
```rust
// Adjust rate limits if legitimate traffic
let config = NetworkConfig {
    max_messages_per_peer_per_second: 200,  // Increased from 100
    max_bytes_per_peer_per_second: 2 * 1024 * 1024,  // 2MB/s
    ..Default::default()
};

// For attacks: Peers are automatically banned after 20+ violations/hour
// Check banned peers: grep "should_be_banned" logs/network.log
```

### Problem: Low Peer Reputation

**Symptoms:**
- `peer_reputation_average < 40.0`
- Frequent peer disconnections
- High `failed_requests` count

**Diagnosis Steps:**
1. Check reputation distribution: `GetMetrics`
2. Review peer violations: `grep "Violation" logs/network.log`
3. Identify problematic peers: `grep "should_disconnect" logs/network.log`
4. Check network quality metrics

**Common Causes:**
- **Poor peer quality:** Connected to unreliable or malicious peers
- **Network instability:** High packet loss or latency
- **Configuration mismatch:** Incompatible protocol versions

**Resolution:**
- Update bootstrap peer list with high-quality peers
- Increase minimum reputation threshold for operations
- Enable automatic peer disconnection based on reputation

### Problem: Gossipsub Not Propagating Messages

**Symptoms:**
- `gossip_messages_published > 0` but `gossip_messages_received = 0`
- Messages not reaching other nodes
- Low `gossipsub_mesh_size`

**Diagnosis Steps:**
1. Check mesh size: Should be >= 6 for healthy gossipsub
2. Verify topic subscriptions: All nodes subscribed to same topics
3. Check peer connectivity: Ensure at least 3 connected peers
4. Review gossipsub logs for mesh maintenance events

**Common Causes:**
- **Insufficient mesh size:** Too few peers connected
- **Topic mismatch:** Nodes subscribed to different topics
- **Network partition:** Isolated from gossipsub mesh

**Resolution:**
- Connect to more peers (target: 10+ peers)
- Verify gossip_topics configuration matches network
- Check for network connectivity issues

### Problem: Memory Leak / High Memory Usage

**Symptoms:**
- Steadily increasing memory consumption
- System eventually OOM (Out of Memory)
- Performance degradation over time

**Diagnosis Steps:**
1. Monitor memory metrics: `ps aux | grep alys`
2. Check for peer accumulation: `connected_peers` growing unbounded
3. Review rate limiter queue sizes
4. Check for unbounded message queues

**Common Causes:**
- **Rate limiter queue growth:** Not cleaning up disconnected peers
- **Peer manager memory leak:** Storing unlimited peer history
- **Message buffering:** Unbounded queues in Actix mailbox

**Resolution:**
- Maintenance task runs every 5 minutes to cleanup rate limiter
- Monitor `max_connections` and enforce limits
- Review and tune Actix mailbox capacity

---

## Performance Tuning

### Connection Limits

**Default Configuration:**
```rust
max_connections: 1000
max_connections_per_ip: 5
max_inbound_connections: 500
max_outbound_connections: 500
```

**Low-Resource Environment (Raspberry Pi, embedded):**
```rust
max_connections: 50
max_connections_per_ip: 2
max_inbound_connections: 25
max_outbound_connections: 25
```

**High-Performance Environment (data center, validator node):**
```rust
max_connections: 5000
max_connections_per_ip: 20
max_inbound_connections: 2500
max_outbound_connections: 2500
```

### Rate Limiting

**Default Configuration:**
```rust
max_messages_per_peer_per_second: 100
max_bytes_per_peer_per_second: 1024 * 1024  // 1MB/s
rate_limit_window: Duration::from_secs(1)
```

**Strict DOS Protection:**
```rust
max_messages_per_peer_per_second: 50
max_bytes_per_peer_per_second: 512 * 1024  // 512KB/s
rate_limit_window: Duration::from_secs(1)
```

**High-Throughput Environment:**
```rust
max_messages_per_peer_per_second: 500
max_bytes_per_peer_per_second: 10 * 1024 * 1024  // 10MB/s
rate_limit_window: Duration::from_secs(1)
```

### Gossipsub Tuning

**Default Topics:**
```rust
gossip_topics: vec![
    "alys-blocks".to_string(),
    "alys-transactions".to_string(),
    "alys-auxpow".to_string(),
]
```

**Message Size Limits:**
```rust
message_size_limit: 1024 * 1024  // 1MB (default)
message_size_limit: 4 * 1024 * 1024  // 4MB (blocks with large payloads)
message_size_limit: 100 * 1024  // 100KB (constrained environments)
```

### Discovery Configuration

**Default Settings:**
```rust
discovery_interval: Duration::from_secs(60)
auto_dial_mdns_peers: true  // Enable for local network discovery
```

**Aggressive Discovery (poor connectivity):**
```rust
discovery_interval: Duration::from_secs(15)
auto_dial_mdns_peers: true
```

**Conservative Discovery (stable network):**
```rust
discovery_interval: Duration::from_secs(300)
auto_dial_mdns_peers: false  // Disable if not needed
```

### Reputation Tuning

**Default Thresholds:**
```rust
// Automatic disconnect if reputation < 10.0 or success_rate < 0.3
// Automatic ban if reputation < -50.0 or violations > 20/hour
```

**Strict Mode (high security):**
```rust
// Custom thresholds via peer_manager
peer_manager.disconnect_threshold = 30.0;
peer_manager.ban_threshold = 0.0;
```

**Lenient Mode (development):**
```rust
peer_manager.disconnect_threshold = -10.0;
peer_manager.ban_threshold = -100.0;
```

---

## DOS Protection

### Multi-Layer Defense

The NetworkActor implements comprehensive DOS protection with multiple defense layers:

#### Layer 1: Connection Limits
```rust
max_connections_per_ip: 5          // Limit connections from single IP
max_inbound_connections: 500       // Total inbound connection limit
max_outbound_connections: 500      // Total outbound connection limit
```

**Protection Against:**
- Connection flooding attacks
- Resource exhaustion
- IP-based attacks

#### Layer 2: Rate Limiting
```rust
max_messages_per_peer_per_second: 100      // Message rate limit
max_bytes_per_peer_per_second: 1MB         // Bandwidth limit
rate_limit_window: 1 second                // Sliding window
```

**Protection Against:**
- Message flooding
- Bandwidth exhaustion
- Amplification attacks

#### Layer 3: Message Validation
```rust
message_size_limit: 1MB            // Maximum message size
```

**Protection Against:**
- Memory exhaustion
- Buffer overflow attempts
- Oversized message attacks

#### Layer 4: Reputation System
```rust
Violation tracking:
- InvalidMessage: -5.0 reputation
- ExcessiveRate: -10.0 reputation
- MalformedProtocol: -8.0 reputation
- Automatic disconnect: < 10.0 reputation
- Automatic ban: < -50.0 or > 20 violations/hour
```

**Protection Against:**
- Persistent attackers
- Low-and-slow attacks
- Coordinated attacks

### Attack Detection Indicators

**DOS Attack Indicators:**
```
rate_limited_messages > 1000/minute
banned_peers_total increasing rapidly (> 10/minute)
connection_errors > 50% of attempts
peer_reputation_average dropping rapidly
```

**Attack Response:**
1. Alert operators via monitoring system
2. Automatically rate limit and ban attacking peers
3. Log attack patterns for analysis
4. Scale connection limits if legitimate traffic

### Manual Intervention

If under severe attack:
```rust
// Emergency rate limit reduction
config.max_messages_per_peer_per_second = 10;
config.max_connections_per_ip = 1;

// Restart NetworkActor with updated config
actor.send(NetworkMessage::StopNetwork { graceful: false }).await;
actor.send(NetworkMessage::StartNetwork {
    listen_addrs: config.listen_addresses,
    bootstrap_peers: config.bootstrap_peers,
}).await;
```

---

## Incident Response

### Incident Severity Levels

**P1 - Critical (Immediate Response Required):**
- Network completely offline (`connected_peers = 0` for > 10 minutes)
- Active DOS attack overwhelming system
- Data corruption or security breach

**P2 - High (Response within 1 hour):**
- Degraded performance (`peer_reputation_average < 30.0`)
- High error rates (`connection_errors > 20%`)
- Partial network partition

**P3 - Medium (Response within 4 hours):**
- Low peer count (`connected_peers < 5`)
- Increased rate limiting activity
- Minor performance degradation

**P4 - Low (Response within 24 hours):**
- Single peer issues
- Configuration optimization needed
- Non-critical warnings

### Response Procedures

#### P1: Network Offline
1. Check actor status: `GetNetworkStatus`
2. Review error logs: Last 1000 lines
3. Verify system resources: CPU, memory, disk
4. Test network connectivity: ping, traceroute to bootstrap peers
5. Restart NetworkActor if necessary
6. Update bootstrap peer list if peers are offline
7. Document incident and root cause

#### P2: DOS Attack
1. Identify attack pattern: Review rate limiting metrics
2. Collect attacker IPs: `grep "Rate limit exceeded" logs/network.log | awk '{print $5}' | sort | uniq -c`
3. Verify automatic bans are working: Check `banned_peers_total`
4. Adjust rate limits if needed (temporary mitigation)
5. Contact network operators to blacklist attacking IPs
6. Document attack vectors and patterns
7. Update DOS protection rules if new attack pattern discovered

#### P3: Performance Degradation
1. Collect full metrics snapshot: `GetMetrics`
2. Analyze reputation distribution: Identify problematic peers
3. Review recent configuration changes
4. Check for resource constraints: CPU, memory, network bandwidth
5. Optimize configuration based on findings
6. Monitor for improvement over 1 hour
7. Document performance issue and resolution

### Logging and Forensics

**Critical Events to Log:**
- All peer violations with timestamps and peer IDs
- Rate limiting activations with source peer
- Connection failures with error messages
- Reputation changes > 10.0 delta
- DOS protection activations

**Log Retention:**
- Real-time logs: Last 24 hours (high volume)
- Aggregated metrics: 30 days
- Security events: 90 days
- Critical incidents: Permanent retention

**Log Analysis Tools:**
```bash
# Find top violators
grep "Violation" network.log | awk '{print $5}' | sort | uniq -c | sort -rn | head -20

# Analyze rate limiting
grep "Rate limit exceeded" network.log | wc -l

# Check connection patterns
grep "Added peer connection" network.log | awk '{print $NF}' | sort | uniq -c

# Monitor reputation changes
grep "reputation change" network.log | grep "Significant"
```

---

## Production Checklist

### Pre-Deployment
- [ ] Configuration validated: `config.validate()`
- [ ] Bootstrap peers tested and reachable
- [ ] Firewall rules configured for P2P ports
- [ ] Monitoring dashboards configured
- [ ] Alerting rules configured
- [ ] Rate limits tuned for expected load
- [ ] Connection limits set appropriately

### Post-Deployment
- [ ] Network successfully started: `is_running = true`
- [ ] Peers connecting: `connected_peers > 3` within 5 minutes
- [ ] Gossipsub mesh formed: `gossipsub_mesh_size >= 6`
- [ ] Messages flowing: `gossip_messages_published > 0`
- [ ] Health check passing: `is_healthy = true`
- [ ] Metrics exporting correctly
- [ ] Alerts not firing

### Daily Operations
- [ ] Review peer count trends
- [ ] Check average reputation score
- [ ] Monitor rate limiting activity
- [ ] Review connection error rate
- [ ] Check for unusual patterns
- [ ] Verify uptime SLOs met

---

## Support and Escalation

For issues not covered in this guide:

1. **Check Logs:** Review NetworkActor logs for detailed error messages
2. **Collect Metrics:** Export full metrics snapshot for analysis
3. **Reproduce Issue:** Try to reproduce in test environment
4. **Report Issue:** Create detailed bug report with logs and metrics
5. **Escalate:** Contact network team for critical issues

**Monitoring Dashboards:** Import Prometheus metrics for visualization
**Alert Management:** Configure alerts based on metrics and thresholds above
**Performance Baselines:** Establish baselines for your specific deployment

---

## Appendix: Prometheus Scrape Configuration

```yaml
scrape_configs:
  - job_name: 'alys_network'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

**Key Grafana Queries:**
```promql
# Peer count over time
network_connected_peers

# Message throughput
rate(network_messages_sent[1m])

# Error rate
rate(network_connection_errors[5m]) / rate(network_total_connections[5m])

# Reputation health
network_peer_reputation_average

# DOS protection activity
rate(network_rate_limited_messages[1m])
```

---

**Document Version:** 1.0
**Last Updated:** 2025-10-12
**Maintained By:** Alys Network Team
