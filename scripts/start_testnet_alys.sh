#!/usr/bin/env bash
# Load utility functions from geth.sh
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &> /dev/null && pwd)
BASE_DIR=$(realpath "$SCRIPT_DIR/../")
. $SCRIPT_DIR/utils/chain.sh

BTC_RPC_USER=${BTC_RPC_USER:-"rpcuser"}
BTC_RPC_PASSWORD=${BTC_RPC_PASSWORD:-"rpcpassword"}
BTC_RPC_URL=${BTC_RPC_URL:-"http://localhost:18332"}
GETH_JSON_RPC_URL=${GETH_JSON_RPC_URL:-"http://localhost:8545"}
# Set default number of nodes if not already set
NUM=${NUM:-0}

echo "${NUM}"

# Initialize directories & log path
mkdir -p "${BASE_DIR}/etc/data/logs"
mkdir -p "$(get_data_path $NUM)"
touch "$(get_log_path $NUM)"

# Start the Geth node(s)
start_testnet_full_node $NUM

# Tail the log file to keep the shell open and display logs
tail -f "$(get_log_path $NUM)"
