function stop_all_reth() {
    for pid in ${RETH_PIDS[*]}; do
        echo "Killing reth $pid"
        kill -INT $pid 2>/dev/null
        wait $pid
    done
}

function start_reth() {
    NUM=$1

    rm -rf "${PWD}/data/execution/node${NUM}"

    AUTHRPC_PORT=$((8551 + $NUM * 10))
    HTTP_PORT=$((8545 + $NUM * 10))
    WS_PORT=$((8546 + $NUM * 10))
    PORT=$((30303 + $NUM * 10))

    reth init \
    --config ./etc/config/eth-config.toml \
    --datadir "$PWD/etc/data/execution/node${NUM}" \
    --chain "$PWD/etc/config/genesis.json" \
    > "$PWD/etc/data/logs/reth${NUM}.txt" 2>&1

    reth node \
    --datadir "$PWD/etc/data/execution/node${NUM}" \
    --config "$PWD/etc/config/eth-config.toml" \
    --chain "$PWD/etc/config/genesis.json" \
    --metrics 0.0.0.0:9001 \
    --log.file.directory "$PWD/etc/data/logs/reth${NUM}.txt" \
    --authrpc.addr 0.0.0.0 \
    --authrpc.port ${AUTHRPC_PORT} \
    --authrpc.jwtsecret "$PWD/etc/jwttoken/jwt.hex" \
    --http --http.addr 0.0.0.0 \
    --http.port ${HTTP_PORT} \
    --http.api "debug,net,eth,web3,txpool" \
    --http.corsdomain "*" \
    --ws.api "eth,net,web3,debug,txpool" \
    --ws --ws.addr "0.0.0.0" \
    --ws.port ${WS_PORT} \
    --ws.origins "*" \
    --port ${PORT} &
    RETH_PIDS[$i]=$!
}

function start_multiple_reth() {
    for (( i=0; i<$1; i++ )); do
        start_reth $i
        echo "Started reth ${RETH_PIDS[$i]}"
    done
}