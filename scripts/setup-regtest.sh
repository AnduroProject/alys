#!/bin/bash

# Alys V2 Multi-Node Regtest Setup Script
# This script automates the initial setup for the multi-node regtest environment
# including cryptographic key generation for federation validators

set -e  # Exit on error

# Configuration
NUM_VALIDATORS=${NUM_VALIDATORS:-3}  # Default to 3 validators
KEYS_DIR="keys"
KEYS_OUTPUT_FILE="${KEYS_DIR}/validator-keys.txt"

echo "====================================="
echo "Alys V2 Regtest Environment Setup"
echo "====================================="
echo ""
echo "Configuration:"
echo "  Validators: ${NUM_VALIDATORS}"
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

if ! command -v cargo &> /dev/null; then
    echo "ERROR: Cargo (Rust toolchain) is not installed or not in PATH"
    echo "  Install from: https://rustup.rs/"
    exit 1
fi

echo "✓ All prerequisites satisfied"
echo ""

# Create directory structure
echo "Creating directory structure..."

# Create directories for N validators
for i in $(seq 1 ${NUM_VALIDATORS}); do
    mkdir -p "data/node${i}/{db,wallet}"
    mkdir -p "logs/node${i}"
done

mkdir -p data/execution/{data,logs}
mkdir -p logs/execution
mkdir -p jwt
mkdir -p config
mkdir -p "${KEYS_DIR}"

echo "✓ Directory structure created (${NUM_VALIDATORS} validator nodes)"
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

# Generate validator keys
if [ -f "${KEYS_OUTPUT_FILE}" ]; then
    echo "⚠ Validator keys already exist at ${KEYS_OUTPUT_FILE}"
    echo ""
    read -p "Regenerate keys? This will overwrite existing keys! (y/N) " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "  Skipping key generation"
        echo "  Using existing keys from ${KEYS_OUTPUT_FILE}"
        SKIP_KEYGEN=true
    else
        SKIP_KEYGEN=false
    fi
else
    SKIP_KEYGEN=false
fi

if [ "${SKIP_KEYGEN}" != "true" ]; then
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "Generating cryptographic keys for ${NUM_VALIDATORS} validators..."
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "Building keygen utility..."

    cd app
    cargo build --bin keygen
    BUILD_EXIT_CODE=$?
    cd ..

    if [ $BUILD_EXIT_CODE -ne 0 ] || [ ! -f "./target/debug/keygen" ]; then
        echo "ERROR: Failed to build keygen utility"
        echo "Try building manually with: cd app && cargo build --bin keygen"
        exit 1
    fi

    echo "✓ Keygen utility built"
    echo ""
    echo "Generating keys..."
    echo ""

    # Run keygen and save output (binary is in workspace target directory)
    ./target/debug/keygen ${NUM_VALIDATORS} | tee "${KEYS_OUTPUT_FILE}"

    echo ""
    echo "✓ Keys saved to ${KEYS_OUTPUT_FILE}"
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "⚠  SECURITY WARNING:"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  • Secret keys have been generated and saved to ${KEYS_OUTPUT_FILE}"
    echo "  • This file contains private keys - keep it secure!"
    echo "  • DO NOT commit this file to version control"
    echo "  • Consider encrypting this file if storing long-term"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
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
echo "Generated Keys Summary:"
echo "  • ${NUM_VALIDATORS} validator key sets created"
echo "  • Keys saved to: ${KEYS_OUTPUT_FILE}"
echo ""
echo "Next steps:"
echo ""
echo "1. Update src/spec.rs with the generated keys:"
echo "   • Copy the 'authorities' configuration"
echo "   • Copy the 'federation' configuration"
echo "   • Copy the 'federation_bitcoin_pubkeys' configuration"
echo "   • See ${KEYS_OUTPUT_FILE} for the exact values"
echo ""
echo "2. Update docker-compose configuration with secret keys:"
echo "   • Add environment variables for each node's AURA_SECRET_KEY"
echo "   • Add environment variables for each node's BITCOIN_SECRET_KEY"
echo "   • See ${KEYS_OUTPUT_FILE} for the values"
echo ""
echo "3. Rebuild the application with new spec:"
echo "   cd app && cargo build --release"
echo ""
echo "4. Start the environment:"
echo "   docker compose -f etc/docker-compose.v2-regtest.yml up -d"
echo ""
echo "5. Monitor startup logs:"
echo "   docker compose -f etc/docker-compose.v2-regtest.yml logs -f"
echo ""
echo "6. Verify block production rotation:"
echo "   • With ${NUM_VALIDATORS} validators, each gets every ${NUM_VALIDATORS}th slot"
echo "   • Check logs to see round-robin block production"
echo ""
echo "For detailed instructions, see: REGTEST_SETUP.md"
echo "For architecture details, see: docs/v2_alpha/docker-two-node-testnet-architecture.md"
echo "For Aura authority rotation, see: docs/v2_alpha/aura-federation-authority-rotation.md"
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
