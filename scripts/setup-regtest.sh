#!/bin/bash

# Alys V2 Two-Node Regtest Setup Script
# This script automates the initial setup for the two-node regtest environment

set -e  # Exit on error

echo "====================================="
echo "Alys V2 Regtest Environment Setup"
echo "====================================="
echo ""

# Check prerequisites
echo "Checking prerequisites..."

if ! command -v docker &> /dev/null; then
    echo "ERROR: Docker is not installed or not in PATH"
    exit 1
fi

if ! command -v docker compose &> /dev/null; then
    echo "ERROR: Docker Compose is not installed or not in PATH"
    exit 1
fi

if ! command -v openssl &> /dev/null; then
    echo "ERROR: OpenSSL is not installed or not in PATH"
    exit 1
fi

echo "✓ All prerequisites satisfied"
echo ""

# Create directory structure
echo "Creating directory structure..."

mkdir -p data/node1/{db,wallet}
mkdir -p data/node2/{db,wallet}
mkdir -p data/execution/{data,logs}
mkdir -p logs/{node1,node2,execution}
mkdir -p jwt
mkdir -p config

echo "✓ Directory structure created"
echo ""

# Generate JWT secret if it doesn't exist
if [ -f "jwt/jwt.hex" ]; then
    echo "⚠ JWT secret already exists at jwt/jwt.hex"
    echo "  Skipping JWT generation"
else
    echo "Generating JWT secret..."
    openssl rand -hex 32 > jwt/jwt.hex
    echo "✓ JWT secret generated at jwt/jwt.hex"
fi
echo ""

# Check for execution layer genesis
if [ ! -f "config/genesis.json" ]; then
    echo "⚠ WARNING: No execution layer genesis.json found at config/genesis.json"
    echo "  Services will use --dev flag for genesis generation"
    echo "  If you have a custom genesis, place it at config/genesis.json"
else
    echo "✓ Found execution layer genesis at config/genesis.json"
fi
echo ""

# Verify docker-compose file exists
if [ ! -f "etc/docker-compose.v2-regtest.yml" ]; then
    echo "ERROR: etc/docker-compose.v2-regtest.yml not found"
    echo "Please ensure you're running this script from the alys-v2 project root"
    exit 1
fi

echo "✓ Found etc/docker-compose.v2-regtest.yml"
echo ""

# Display setup summary
echo "====================================="
echo "Setup Complete!"
echo "====================================="
echo ""
echo "Next steps:"
echo ""
echo "1. Start the environment:"
echo "   docker compose -f etc/docker-compose.v2-regtest.yml up -d"
echo ""
echo "2. Monitor startup logs:"
echo "   docker compose -f etc/docker-compose.v2-regtest.yml logs -f"
echo ""
echo "3. Check service health:"
echo "   docker compose -f etc/docker-compose.v2-regtest.yml ps"
echo ""
echo "4. Follow the verification steps in REGTEST_SETUP.md"
echo ""
echo "For detailed instructions, see: REGTEST_SETUP.md"
echo "For architecture details, see: docs/v2_alpha/docker-two-node-testnet-architecture.md"
echo ""

# Offer to start services
read -p "Would you like to start the services now? (y/n) " -n 1 -r
echo ""
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo ""
    echo "Starting services..."
    docker compose -f etc/docker-compose.v2-regtest.yml up -d
    echo ""
    echo "Services started! Checking status..."
    sleep 3
    docker compose -f etc/docker-compose.v2-regtest.yml ps
    echo ""
    echo "Monitor logs with:"
    echo "  docker compose -f etc/docker-compose.v2-regtest.yml logs -f"
fi
