#!/usr/bin/env bash
# includes

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
. $SCRIPT_DIR/utils/reth.sh

trap stop_all_reth SIGINT

if [[ -z "${NUM}" ]]; then
    # when running dev mode (single node)
    rm -rf "$PWD/etc/data/consensus/node_${NUM}/chain_db"
    rm -rf "$PWD/etc/data/consensus/node_${NUM}/wallet"
fi

mkdir -p data/logs/

NUM=${NUM:-0}
start_reth $NUM
tail -f "$PWD/etc/data/logs/reth${NUM}.txt"