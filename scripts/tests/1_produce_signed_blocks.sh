#!/bin/bash
# includes
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
. $SCRIPT_DIR/../utils/shared.sh

trap stop_all SIGINT

export CHAIN_ARGS="--mine"
start_all 3

echo "Chain running, use CTRL+C to exit"
wait "${CHAIN_PIDS[@]}"
stop_all
exit 1