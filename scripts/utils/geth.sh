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
         > "$LOG_FILE" 2>&1 &
    GETH_PIDS[$NUM]=$!
}

function start_testnet_geth() {
    local NUM=$1
    local LOG_FILE=$(get_log_path $NUM)


    mkdir -p "${BASE_DIR}/etc/data/execution/node_${NUM}" "${BASE_DIR}/etc/data/logs"
    touch "$LOG_FILE"


    # Port calculations
    local AUTHRPC_PORT=$((8551 + $NUM * 10))
    local HTTP_PORT=$((8545 + $NUM * 10))
    local WS_PORT=$((8546 + $NUM * 10))
    local PORT=$((30303 + $NUM * 10))

    # Initialize and start Geth
    geth init --state.scheme "hash" --datadir "${BASE_DIR}/etc/data/execution/node_${NUM}" "${BASE_DIR}/etc/config/genesis.json" > "$LOG_FILE" 2>&1

    bootnode -genkey "${BASE_DIR}/etc/data/execution/node_${NUM}/bootnode.key"

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
         --maxpeers 20 \
         --nodekey "${BASE_DIR}/etc/data/execution/node_${NUM}/bootnode.key" \
         --bootnodes "enode://f18232ce8d651a06273107f2084a7d0c914712893968ad5b7ad77c324dde2e3d117fe6058b63eae817615bdd354a90217d19ba113a4237080e2527f626b80dcf@54.224.209.248:30303,enode://c24c88c6eef3bb53c8be49e8fe0837088e66e200a3b3a7d097c3af861617de13487cfd665cbc0d313cde6b1aa8159774dc9c29842ffce1d9fc286af44f7eedf4@107.22.120:30303,enode://6f8c2bfe5b83e79d0dfcf2a619af0a05ca178c5c22c30654db80e8e975133797cf704f0707f6b739731c89cf147fd6835500e632484064b048fdad141ccf542c@54.161.100.208:30303"
         > "$LOG_FILE" 2>&1 &
    GETH_PIDS[$NUM]=$!
}

function start_multiple_geth() {
    for (( i=0; i<$1; i++ )); do
        start_geth $i
        echo "Started geth ${GETH_PIDS[$i]}"
    done
}
