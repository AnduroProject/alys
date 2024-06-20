#!/bin/bash

FLAG_FILE=/opt/alys/execution/data/init.flag

# If init flag not found then run geth init
if [ ! -f $FLAG_FILE ]; then

    mkdir -p /opt/alys/execution/data /opt/alys/execution/logs
    geth init --state.scheme "hash" --datadir "/opt/alys/execution/data" /opt/alys/execution/config/genesis.json > "/opt/alys/execution/logs/geth_init.txt" 2>&1

    touch $FLAG_FILE
fi

# Continue with the normal startup process
exec "$@"