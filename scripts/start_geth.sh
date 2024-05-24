#!/bin/bash
# includes
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
. $SCRIPT_DIR/utils/geth.sh

trap stop_all_geth SIGINT

if [[ -z "${NUM}" ]]; then
    # when running dev mode (single node)
    rm -rf ".bob/chain_db"
    rm -rf ".bob/wallet"
fi

mkdir -p data/logs/

NUM=${NUM:-0}
start_geth $NUM
tail -f "$PWD/data/logs/geth${NUM}.txt"