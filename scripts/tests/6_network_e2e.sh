#!/bin/bash
# includes
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
. $SCRIPT_DIR/../utils/shared.sh

trap stop_all SIGINT

start_all 3

#  START TEST
# ************

start_miner

read -p "Chain is running..."

echo "Killing nodes 1 and 2 (0 is still running)"

stop_chain 1
stop_chain 2

read -p "Chain has stopped producing blocks..."

echo "Re-starting node 1..."

resume_chain 1

read -p "Chain has resumed producing blocks..."

echo "Killing the miner..."
kill $MINER_PID 2>/dev/null
wait $MINER_PID

read -p "Chain will produce up to 5 more blocks, and then stop..."

start_miner

read -p "Chain has resumed block production..."

echo "Restarting node 2..."
resume_chain 2

read -p "Node 2 re-syncs and participates in block production..."

echo "Killing all nodes..."
stop_all_chains

# echo "Starting all nodes again..."
for (( i=0; i<3; i++ )); do
    resume_chain $i
    echo "Started chain ${CHAIN_PIDS[$i]}"
done

read -p "Nodes have resumed block production where they left off before the restart..."

echo "Finished testing script, shutting down"

# **********
#  END TEST

stop_all
exit 1