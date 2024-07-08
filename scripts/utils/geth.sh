if [ -z "$BASE_DIR" ]; then
    BASE_DIR=${PWD}
fi

function get_log_path() {
    local node_NUM=$1
    echo "${BASE_DIR}/etc/data/logs/execution_${node_NUM}.txt"
}

function stop_all_geth() {
    echo "Shutting down all geth processes..."
    for pid in ${GETH_PIDS[*]}; do
        echo "Killing geth $pid"
        kill -INT $pid 2>/dev/null
        wait $pid
    done
}

function start_geth() {
    local NUM=$1
    local LOG_FILE=$(get_log_path $NUM)


    rm -rf "${BASE_DIR}/etc/data/execution/node_${NUM}"
    mkdir -p "${BASE_DIR}/etc/data/execution/node_${NUM}" "${BASE_DIR}/etc/data/logs"
    touch "$LOG_FILE"


    # Port calculations
    local AUTHRPC_PORT=$((8551 + $NUM * 10))
    local HTTP_PORT=$((8545 + $NUM * 10))
    local WS_PORT=$((8546 + $NUM * 10))
    local PORT=$((30303 + $NUM * 10))

    # Initialize and start Geth
    geth init --state.scheme "hash" --datadir "${BASE_DIR}/etc/data/execution/node_${NUM}" "${BASE_DIR}/etc/config/genesis.json" > "$LOG_FILE" 2>&1
    geth --datadir "${BASE_DIR}/etc/data/execution/node_${NUM}" \
         --state.scheme "hash" \
         --networkid 212121 \
         --authrpc.vhosts "*" \
         --authrpc.addr "0.0.0.0" \
         --authrpc.jwtsecret "${BASE_DIR}/etc/config/jwt/jwt" \
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
         >> "$LOG_FILE" 2>&1 &
    GETH_PIDS[$NUM]=$!
}

function start_multiple_geth() {
    for (( i=0; i<$1; i++ )); do
        start_geth $i
        echo "Started geth ${GETH_PIDS[$i]}"
    done
}
