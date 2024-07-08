#!/bin/bash
# Load utility functions from geth.sh
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &> /dev/null && pwd)
BASE_DIR=$(realpath "$SCRIPT_DIR/../")
. "$SCRIPT_DIR/utils/geth.sh"

# Trap SIGINT for a graceful shutdown
trap stop_all_geth SIGINT

# Set default number of nodes if not already set
NUM=${NUM:-0}

# Clear previous blockchain data in dev mode
if [[ -z "${NUM}" ]]; then
    rm -rf "${BASE_DIR}/etc/data/execution/node_${NUM}/chain_db"
    rm -rf "${BASE_DIR}/etc/data/execution/node_${NUM}/wallet"
fi

# Initialize logs directory
mkdir -p "${BASE_DIR}/etc/data/logs"

# Start the Geth node(s)
start_geth $NUM

# Tail the log file to keep the shell open and display logs
tail -f "$(get_log_path $NUM)"
