#!/usr/bin/env bash
# includes
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
. $SCRIPT_DIR/utils/shared.sh

trap stop_all SIGINT

export CHAIN_ARGS="--mine"

NODES=${1:-3}
start_all $NODES

# wait for any job to terminate
wait "${CHAIN_PIDS[@]}"
stop_all
exit 1
