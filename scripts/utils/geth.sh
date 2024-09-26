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


    # rm -rf "${BASE_DIR}/etc/data/execution/node_${NUM}"
    # mkdir -p "${BASE_DIR}/etc/data/execution/node_${NUM}" "${BASE_DIR}/etc/data/logs"
    # touch "$LOG_FILE"


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
         --verbosity 5 \
         --log.file $LOG_FILE \
         --nodiscover \
         --maxpeers 20 \
         --gpo.ignoreprice 1 \
         --metrics \
         --metrics.expensive \
         --miner.gasprice 1 \
         --history.state 0 \
         --nodekey "${BASE_DIR}/etc/data/execution/node_0/bootnode.key"
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
         --verbosity 5 \
         --log.file $LOG_FILE \
         --gpo.ignoreprice 1 \
         --metrics \
         --metrics.expensive \
         --miner.gasprice 1 \
         --history.state 0 \
         --port ${PORT} \
         --gcmode "archive" \
         --maxpeers 20 \
         --nodekey "${BASE_DIR}/etc/data/execution/node_${NUM}/bootnode.key" \
         --bootnodes "enode://6f8c2bfe5b83e79d0dfcf2a619af0a05ca178c5c22c30654db80e8e975133797cf704f0707f6b739731c89cf147fd6835500e632484064b048fdad141ccf542c@54.161.100.208:30303,enode://6fa3a059cde5853f5702fcba00d7d682dfd8af4140fc088fe19ced7aaf245c238377bfa2c3fbb058593c412cfd20b26192b86ba4266770beff79d9fb8a18bc07@107.22.120.71:30303" &
    GETH_PIDS[$NUM]=$!
}

function start_multiple_geth() {
    for (( i=0; i<$1; i++ )); do
        start_geth $i
        echo "Started geth ${GETH_PIDS[$i]}"
    done
}
