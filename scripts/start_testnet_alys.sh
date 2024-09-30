#!/usr/bin/env bash
# Load utility functions from geth.sh
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &> /dev/null && pwd)
BASE_DIR=$(realpath "$SCRIPT_DIR/../")
. $SCRIPT_DIR/utils/chain.sh

BTC_RPC_USER=${BTC_RPC_USER:-"rpcuser"}
BTC_RPC_PASSWORD=${BTC_RPC_PASSWORD:-"rpcpassword"}
BTC_RPC_URL=${BTC_RPC_URL:-"http://localhost:18332"}


# Initialize logs directory
mkdir -p "${BASE_DIR}/etc/data/logs"

# Start the Geth node(s)
start_testnet_full_node

# Tail the log file to keep the shell open and display logs
tail -f "${PWD}/etc/data/logs/alys_0.txt"
