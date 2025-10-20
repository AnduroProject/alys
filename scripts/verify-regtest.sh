#!/bin/bash

# Alys V2 Two-Node Regtest Verification Script
# This script automates the verification and testing workflow

set -e  # Exit on error

COMPOSE_FILE="etc/docker-compose.v2-regtest.yml"

echo "==========================================="
echo "Alys V2 Regtest Environment Verification"
echo "==========================================="
echo ""

# Check if services are running
echo "Checking service status..."
if ! docker-compose -f "$COMPOSE_FILE" ps | grep -q "Up"; then
    echo "ERROR: Services are not running"
    echo "Start services with: docker-compose -f $COMPOSE_FILE up -d"
    exit 1
fi

echo "✓ Services are running"
echo ""

# Check service health
echo "Checking service health..."
echo ""
docker-compose -f "$COMPOSE_FILE" ps
echo ""

# Wait for services to be healthy
echo "Waiting for services to become healthy (max 60 seconds)..."
TIMEOUT=60
ELAPSED=0
while [ $ELAPSED -lt $TIMEOUT ]; do
    UNHEALTHY=$(docker-compose -f "$COMPOSE_FILE" ps | grep -c "unhealthy" || true)
    STARTING=$(docker-compose -f "$COMPOSE_FILE" ps | grep -c "starting" || true)

    if [ "$UNHEALTHY" -eq 0 ] && [ "$STARTING" -eq 0 ]; then
        echo "✓ All services are healthy"
        break
    fi

    sleep 5
    ELAPSED=$((ELAPSED + 5))
    echo "  Waiting... ($ELAPSED seconds)"
done

if [ $ELAPSED -ge $TIMEOUT ]; then
    echo "⚠ WARNING: Some services may not be healthy yet"
    echo "  Check logs with: docker-compose -f $COMPOSE_FILE logs"
fi

echo ""

# Test 1: Check Node 1 V2 Network Listening
echo "Test 1: Verifying Node 1 V2 P2P port..."
if docker-compose -f "$COMPOSE_FILE" logs alys-node-1 2>/dev/null | grep -q "Listening on.*10000"; then
    echo "✓ Node 1 is listening on V2 P2P port 10000"
else
    echo "⚠ Node 1 V2 P2P port not confirmed in logs"
    echo "  This may be normal if the service just started"
fi
echo ""

# Test 2: Check mDNS Discovery
echo "Test 2: Checking Node 2 mDNS peer discovery..."
if docker-compose -f "$COMPOSE_FILE" logs alys-node-2 2>/dev/null | grep -qi "mdns\|discovered"; then
    echo "✓ Node 2 shows mDNS discovery activity"
else
    echo "⚠ No mDNS discovery messages found yet"
    echo "  This may take a few moments after startup"
fi
echo ""

# Test 3: Check Peer Connections
echo "Test 3: Verifying peer connections..."
if docker-compose -f "$COMPOSE_FILE" logs alys-node-2 2>/dev/null | grep -qi "newconnection\|connection.*established"; then
    echo "✓ Node 2 has established peer connections"
else
    echo "⚠ No peer connections confirmed yet"
    echo "  Check logs with: docker-compose -f $COMPOSE_FILE logs alys-node-2"
fi
echo ""

# Test 4: Check RPC Endpoints
echo "Test 4: Testing RPC endpoints..."

# Node 1 V2 RPC
if curl -s -f http://localhost:3001/health > /dev/null 2>&1; then
    echo "✓ Node 1 V2 RPC responding (port 3001)"
else
    echo "⚠ Node 1 V2 RPC not responding on port 3001"
fi

# Node 2 V2 RPC
if curl -s -f http://localhost:3011/health > /dev/null 2>&1; then
    echo "✓ Node 2 V2 RPC responding (port 3011)"
else
    echo "⚠ Node 2 V2 RPC not responding on port 3011"
fi

echo ""

# Test 5: Check Network Peers
echo "Test 5: Querying network peer counts..."

# Node 1 peers
echo -n "  Node 1 peer count: "
PEER_COUNT_1=$(curl -s http://localhost:3001/network/peers 2>/dev/null | grep -o '"peer_count":[0-9]*' | cut -d':' -f2 || echo "N/A")
echo "$PEER_COUNT_1"

# Node 2 peers
echo -n "  Node 2 peer count: "
PEER_COUNT_2=$(curl -s http://localhost:3011/network/peers 2>/dev/null | grep -o '"peer_count":[0-9]*' | cut -d':' -f2 || echo "N/A")
echo "$PEER_COUNT_2"

if [ "$PEER_COUNT_1" != "N/A" ] && [ "$PEER_COUNT_1" -gt 0 ] && [ "$PEER_COUNT_2" != "N/A" ] && [ "$PEER_COUNT_2" -gt 0 ]; then
    echo "✓ Both nodes have peers connected"
else
    echo "⚠ Peer count may be low or unavailable"
    echo "  This is expected if nodes just started"
fi

echo ""

# Summary
echo "==========================================="
echo "Verification Summary"
echo "==========================================="
echo ""
echo "Next steps:"
echo ""
echo "1. Monitor logs for detailed activity:"
echo "   docker-compose -f $COMPOSE_FILE logs -f"
echo ""
echo "2. Test block broadcasting:"
echo "   curl -X POST http://localhost:3001/chain/produce_block"
echo ""
echo "3. Check for 'InsufficientPeers' errors:"
echo "   docker-compose -f $COMPOSE_FILE logs alys-node-1 | grep BroadcastBlock"
echo ""
echo "4. Verify block reception on Node 2:"
echo "   docker-compose -f $COMPOSE_FILE logs alys-node-2 | grep -i 'received.*block'"
echo ""
echo "For detailed testing workflow, see: REGTEST_SETUP.md"
echo ""
