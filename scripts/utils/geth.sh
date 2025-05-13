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

    # Define the file path
    BOOTNODE_FILE_PATH="${BASE_DIR}/etc/data/execution/node_${NUM}/bootnode.key"
    NODEKEY_FILE_PATH="${BASE_DIR}/etc/data/execution/node_${NUM}/geth/nodekey"

    # Check if the file exists
    if [ -f "$BOOTNODE_FILE_PATH" ]; then
        # File exists, include the argument
        ARGUMENT="--nodekey $BOOTNODE_FILE_PATH"
    elif [ -f "$NODEKEY_FILE_PATH" ]; then
        ARGUMENT="--nodekey $NODEKEY_FILE_PATH"
    else
        # File does not exist, do not include the argument
        ARGUMENT=""
    fi
    # rm -rf "${BASE_DIR}/etc/data/execution/node_${NUM}"
    # mkdir -p "${BASE_DIR}/etc/data/execution/node_${NUM}" "${BASE_DIR}/etc/data/logs"
    # touch "$LOG_FILE"

    # Port calculations
    local AUTHRPC_PORT=$((8551 + $NUM * 10))
    local HTTP_PORT=$((8545 + $NUM * 10))
    local WS_PORT=$((8546 + $NUM * 10))
    local PORT=$((30303 + $NUM * 10))

    # Initialize and start Geth
    geth init --state.scheme "hash" --datadir "${BASE_DIR}/etc/data/execution/node_${NUM}" "${BASE_DIR}/etc/config/dev-genesis.json" >"$LOG_FILE" 2>&1
    geth --datadir "${BASE_DIR}/etc/data/execution/node_${NUM}" \
        --state.scheme "hash" \
        --networkid 121212 \
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
        --verbosity 5 \
        --log.file $LOG_FILE \
        --syncmode full \
        --maxpeers 20 \
        --gpo.ignoreprice 1 \
        --metrics \
        --metrics.expensive \
        --miner.gasprice 1 \
        --history.state 0 \
        $ARGUMENT &
    GETH_PIDS[$NUM]=$!
}

function start_testnet_geth() {
    local NUM=$1
    local LOG_FILE=$(get_log_path $NUM)

    mkdir -p "${BASE_DIR}/etc/data/execution/node_${NUM}"

    # Port calculations
    local AUTHRPC_PORT=$((8551 + $NUM * 10))
    local HTTP_PORT=$((8545 + $NUM * 10))
    local WS_PORT=$((8546 + $NUM * 10))
    local PORT=$((30303 + $NUM * 10))

    # Initialize and start Geth
    geth init --state.scheme "hash" --datadir "${BASE_DIR}/etc/data/execution/node_${NUM}" "${BASE_DIR}/etc/config/genesis.json" >"$LOG_FILE" 2>&1

    BOOTNODE_FILE_PATH="${BASE_DIR}/etc/data/execution/node_${NUM}/bootnode.key"
    NODEKEY_FILE_PATH="${BASE_DIR}/etc/data/execution/node_${NUM}/geth/nodekey"

    # Check if the file exists
    if [ -f "$BOOTNODE_FILE_PATH" ]; then
        # File exists, include the argument
        ARGUMENT="--nodekey $BOOTNODE_FILE_PATH"
    elif [ -f "$NODEKEY_FILE_PATH" ]; then
        ARGUMENT="--nodekey $NODEKEY_FILE_PATH"
    else
        bootnode -genkey "$BOOTNODE_FILE_PATH"
        ARGUMENT="--nodekey $BOOTNODE_FILE_PATH"
    fi

    geth --datadir "${BASE_DIR}/etc/data/execution/node_${NUM}" \
        --state.scheme "hash" \
        --networkid 727272 \
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
        --verbosity 2 \
        --log.file $LOG_FILE \
        --gpo.ignoreprice 1 \
        --metrics \
        --metrics.expensive \
        --miner.gasprice 1 \
        --history.state 0 \
        --syncmode full \
        --port ${PORT} \
        --gcmode "archive" \
        --maxpeers 20 \
        --bootnodes "enode://53d6af0f549e4f9b4f768bc37145f7fd800fdbe1203652fd3d2ff7444663a4f5cfe8c06d5ed4b25fe3185920c28b2957a0307f1eed8af49566bba7e3f0c95b04@209.160.175.125:30303" \
        $ARGUMENT &
    GETH_PIDS[$NUM]=$!
}

function start_multiple_geth() {
    for ((i = 0; i < $1; i++)); do
        start_geth $i
        echo "Started geth ${GETH_PIDS[$i]}"
    done
}
