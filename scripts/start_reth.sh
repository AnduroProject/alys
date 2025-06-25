#!/usr/bin/env bash
# Load utility functions from reth.sh
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &> /dev/null && pwd)
BASE_DIR=$(realpath "$SCRIPT_DIR/../")
. "$SCRIPT_DIR/utils/shared.sh"

# Set default number of nodes if not already set
NUM=${NUM:-0}

echo "${NUM}"

# Initialize logs directory
mkdir -p "${BASE_DIR}/etc/data/logs"
touch "$(get_log_path $NUM)"

start_reth $NUM

tail -f "$(get_log_path $NUM)"
