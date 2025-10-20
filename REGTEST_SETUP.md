# Alys V2 Two-Node Regtest Setup Guide

## Overview

This guide provides step-by-step instructions for setting up and testing a two-node Alys V2 regtest environment using Docker Compose. This configuration is designed to test the NetworkActor V2 libp2p stack with peer-to-peer communication.

**Architecture:** Both Alys nodes share a single Reth execution instance and Bitcoin Core regtest node.

**Documentation:** See `docs/v2_alpha/docker-two-node-testnet-architecture.md` for detailed architecture specifications.

---

## Prerequisites

- Docker and Docker Compose installed
- OpenSSL (for JWT generation)
- curl (for testing RPC endpoints)
- At least 4-6GB free RAM
- Alys execution layer genesis configuration (or use `--dev` flag)

**Apple Silicon (M1/M2/M3) Note:** All services use `platform: linux/amd64` for compatibility. Docker will use Rosetta 2 emulation, which works well but may have slightly higher resource usage.

---

## Quick Start (Automated Setup)

For automated setup, use the provided script:

```bash
./scripts/setup-regtest.sh
```

This script will:
- Check prerequisites (Docker, Docker Compose, OpenSSL)
- Create all required directories
- Generate JWT secret
- Optionally start services immediately

After setup, verify the environment:

```bash
./scripts/verify-regtest.sh
```

---

## Manual Setup

### 1. Create Directory Structure

```bash
# Create all required directories
mkdir -p data/{node1,node2,execution}/{db,wallet}
mkdir -p data/execution/data
mkdir -p logs/{node1,node2,execution}
mkdir -p jwt config
```

### 2. Generate JWT Secret

The JWT secret is shared by all services for execution layer authentication:

```bash
openssl rand -hex 32 > jwt/jwt.hex
```

### 3. Prepare Execution Layer Configuration

Place your Reth configuration files in the `config/` directory:
- `config/genesis.json` - Execution layer genesis configuration
- `config/eth-config.toml` - Reth node configuration (optional)

**Note:** If using `--dev` flag, genesis will be auto-generated. Otherwise, ensure you have a valid genesis.json.

### 4. (Optional) Generate Shared Genesis for Alys

If not using `--dev` flag, generate a shared genesis file:

```bash
# This step may require running Alys once to generate genesis
# Then copy it to the project root for sharing between nodes
```

---

## Starting the Environment

### Start All Services

```bash
docker-compose -f etc/docker-compose.v2-regtest.yml up -d
```

### Monitor Startup Logs

```bash
# Watch all services
docker-compose -f etc/docker-compose.v2-regtest.yml logs -f

# Watch specific service
docker-compose -f etc/docker-compose.v2-regtest.yml logs -f alys-node-1
docker-compose -f etc/docker-compose.v2-regtest.yml logs -f alys-node-2
```

### Check Service Health

```bash
docker-compose -f etc/docker-compose.v2-regtest.yml ps
```

All services should show as "healthy" after startup (may take 30-60 seconds).

---

## Verification Steps

### 1. Verify Network Startup

**Check Node 1 is listening on V2 P2P port:**

```bash
docker-compose -f etc/docker-compose.v2-regtest.yml logs alys-node-1 | grep "Listening on"
# Expected output: "Listening on: /ip4/0.0.0.0/tcp/10000"
```

**Check Node 2 discovers Node 1 via mDNS:**

```bash
docker-compose -f etc/docker-compose.v2-regtest.yml logs alys-node-2 | grep -i "mdns\|discovered\|peer"
# Expected: Messages about mDNS discovery and peer connection
```

**Verify peer connections established:**

```bash
docker-compose -f etc/docker-compose.v2-regtest.yml logs alys-node-2 | grep -i "newconnection\|established"
# Expected: Connection established with node-1's peer ID
```

### 2. Check RPC Endpoints

**Node 1 V2 RPC (port 3001):**

```bash
curl http://localhost:3001/health
# Expected: Health check response
```

**Node 2 V2 RPC (port 3011):**

```bash
curl http://localhost:3011/health
# Expected: Health check response
```

### 3. Verify Peer Counts

**Query Node 1 network status:**

```bash
curl http://localhost:3001/network/peers
# Expected: peer_count >= 1
```

**Query Node 2 network status:**

```bash
curl http://localhost:3011/network/peers
# Expected: peer_count >= 1
```

---

## Testing Block Broadcasting

### Trigger Block Production

```bash
# Produce block on Node 1
curl -X POST http://localhost:3001/chain/produce_block
```

### Verify Block Broadcast

**Check Node 1 successfully broadcasts:**

```bash
docker-compose -f etc/docker-compose.v2-regtest.yml logs alys-node-1 | grep "BroadcastBlock"
# Should NOT show "InsufficientPeers" error
```

**Check Node 2 receives block via gossip:**

```bash
docker-compose -f etc/docker-compose.v2-regtest.yml logs alys-node-2 | grep -i "received.*block"
# Expected: Messages showing block reception via gossip
```

---

## Service Configuration

### Port Mappings

| Service | Container Port | Host Port | Description |
|---------|---------------|-----------|-------------|
| **alys-node-1** |
| | 3000 | 3000 | V0 RPC |
| | 3001 | 3001 | V2 RPC |
| | 9000 | 9000 | V0 P2P |
| | 10000 | 10000 | V2 P2P |
| **alys-node-2** |
| | 3000 | 3010 | V0 RPC |
| | 3001 | 3011 | V2 RPC |
| | 9000 | 9001 | V0 P2P |
| | 10000 | 10001 | V2 P2P |
| **execution** |
| | 8545 | 8545 | HTTP RPC |
| | 8551 | 8551 | Engine API |
| | 8456 | 8456 | WebSocket |
| | 19001 | 19001 | Metrics |
| | 30303 | 30303 | ETH P2P |
| **bitcoin-core** |
| | 18333 | 18333 | P2P |
| | 18443 | 18443 | RPC |

### Static IP Assignments

| Service | IP Address |
|---------|------------|
| alys-node-1 | 172.20.0.10 |
| alys-node-2 | 172.20.0.11 |
| execution | 172.20.0.20 |
| bitcoin-core | 172.20.0.30 |

---

## Stopping and Cleanup

### Stop All Services

```bash
docker-compose -f etc/docker-compose.v2-regtest.yml down
```

### Clean Up Data (CAUTION: This deletes all blockchain state)

```bash
# Remove all data directories
rm -rf data/ logs/

# Keep JWT and config
# rm -rf jwt/ config/
```

---

## Troubleshooting

### Issue: Services Not Starting

**Check health status:**

```bash
docker-compose -f etc/docker-compose.v2-regtest.yml ps
```

**View service logs:**

```bash
docker-compose -f etc/docker-compose.v2-regtest.yml logs <service-name>
```

### Issue: Nodes Not Discovering Each Other

**Verify mDNS discovery is enabled:**

The NetworkActor V2 config has `auto_dial_mdns_peers: true` by default (see `app/src/actors_v2/network/config.rs:62`).

**Check Docker bridge network supports multicast:**

```bash
docker network inspect alys-regtest
```

**Fallback: Use explicit bootstrap peer**

If mDNS fails, you may need to implement pre-generated libp2p keys (see architecture document Issue #2 fallback solution).

### Issue: "InsufficientPeers" Error on Block Broadcast

**Verify peer count > 0:**

```bash
curl http://localhost:3001/network/peers
curl http://localhost:3011/network/peers
```

**Check peer connection logs:**

```bash
docker-compose -f etc/docker-compose.v2-regtest.yml logs alys-node-2 | grep -i "connection\|peer"
```

### Issue: Permission Denied on Volume Mounts

Ensure the directories have correct permissions:

```bash
chmod -R 755 data/ logs/
```

### Issue: JWT Authentication Failure

Verify JWT file exists and is readable:

```bash
cat jwt/jwt.hex
# Should output 64 hex characters (32 bytes)
```

---

## Development Workflow

### Restart Single Service

```bash
docker-compose -f etc/docker-compose.v2-regtest.yml restart alys-node-1
```

### View Real-Time Logs

```bash
docker-compose -f etc/docker-compose.v2-regtest.yml logs -f --tail=100 alys-node-1
```

### Execute Commands Inside Container

```bash
docker exec -it alys-node-1 /bin/sh
```

### Rebuild After Code Changes

```bash
# Rebuild and restart services
docker-compose -f etc/docker-compose.v2-regtest.yml up -d --build
```

---

## Network Testing Checklist

- [ ] All services start and become healthy
- [ ] Node 1 listens on V2 P2P port 10000
- [ ] Node 2 discovers Node 1 via mDNS
- [ ] Both nodes show peer_count >= 1
- [ ] Block production succeeds on Node 1
- [ ] Block broadcast does NOT show "InsufficientPeers" error
- [ ] Node 2 receives block via gossip
- [ ] V2 RPC endpoints respond on both nodes

---

## Next Steps

After successful network testing:

1. Test block synchronization between nodes
2. Test concurrent block production
3. Test network resilience (restart nodes, check reconnection)
4. Measure resource usage (should be ~4-6GB RAM)
5. Test with higher peer counts (add more nodes)

---

## References

- **Architecture Document:** `docs/v2_alpha/docker-two-node-testnet-architecture.md`
- **NetworkActor V2:** `app/src/actors_v2/network/`
- **Network Config:** `app/src/actors_v2/network/config.rs`
- **App Initialization:** `app/src/app.rs:478-500`

---

## Support

If you encounter issues not covered in this guide:

1. Check the architecture document for detailed specifications
2. Review service logs for error messages
3. Verify all prerequisites are met
4. Ensure Docker has sufficient resources allocated
