function stop_all_geth() {
    for pid in ${GETH_PIDS[*]}; do
        echo "Killing geth $pid"
        kill -INT $pid 2>/dev/null
        wait $pid
    done
}

function start_geth() {
    NUM=$1

    rm -rf "${PWD}/data/execution/node${NUM}"

    AUTHRPC_PORT=$((8551 + $NUM * 10))
    HTTP_PORT=$((8545 + $NUM * 10))
    WS_PORT=$((8546 + $NUM * 10))
    PORT=$((30303 + $NUM * 10))

    geth init --state.scheme "hash" --datadir "./etc/data/execution/node${NUM}" ./etc/config/genesis.json > "$PWD/etc/data/logs/geth${NUM}.txt" 2>&1
    geth --datadir "./etc/data/execution/node${NUM}" \
        --state.scheme "hash" \
        --networkid 212121 \
        --authrpc.vhosts "*" \
        --authrpc.addr "0.0.0.0" \
        --authrpc.jwtsecret "./etc/config/jwt/jwt" \
        --authrpc.port ${AUTHRPC_PORT} \
        --http \
        --http.addr "0.0.0.0" \
        --http.vhosts "*" \
        --http.port ${HTTP_PORT} \
        --http.api "debug,net,eth,web3,txpool" \
        --http.corsdomain=* \
        --ws.api "eth,net,web3,debug,txpool" \
        --ws \
        --ws.addr "0.0.0.0" \
        --ws.port ${WS_PORT} \
        --ws.origins "*" \
        --port ${PORT} \
        --gcmode "archive" \
        --maxpeers 0 \
        >> "$PWD/etc/data/logs/geth${NUM}.txt" 2>&1 &
    GETH_PIDS[$i]=$!
}

function start_multiple_geth() {
    for (( i=0; i<$1; i++ )); do
        start_geth $i
        echo "Started geth ${GETH_PIDS[$i]}"
    done
}