function stop_all_chains() {
    for pid in ${CHAIN_PIDS[*]}; do
        echo "Killing chain $pid"
        kill -INT $pid 2>/dev/null
        wait $pid 2>/dev/null
    done
}

# takes a chain id
function stop_chain() {
    echo "Killing chain ${CHAIN_PIDS[$1]}"
    kill -INT ${CHAIN_PIDS[$1]} 2>/dev/null
    wait ${CHAIN_PIDS[$1]}
}

function resume_chain() {
    NUM=$1

    if [ -z $FULL_NODE ]; then
        APP_ARGS[$NUM]+=$CHAIN_ARGS
        APP_ARGS[$NUM]+=" --aura-secret-key 000000000000000000000000000000000000000000000000000000000000000$(($NUM + 1))"
        APP_ARGS[$NUM]+=" --bitcoin-secret-key 000000000000000000000000000000000000000000000000000000000000000$(($NUM + 1))"
    else
        unset FULL_NODE
    fi

    AUTHRPC_PORT=$((8551 + $NUM * 10))
    $PWD/target/debug/app \
        --chain $PWD/data/chain.json \
        --geth-url "http://localhost:${AUTHRPC_PORT}/" \
        --db-path "$PWD/.alys/chain_db${NUM}" \
        --rpc-port $((3000 + $1)) \
        --wallet-path "$PWD/.alys/wallet${NUM}" \
        --bitcoin-rpc-url http://localhost:18443 \
        --bitcoin-rpc-user rpcuser \
        --bitcoin-rpc-pass rpcpassword \
        --bitcoin-network regtest \
        ${APP_ARGS[$NUM]} \
        > "$PWD/data/logs/app${NUM}.txt" 2>&1 &
    CHAIN_PIDS[$NUM]=$!
}

function start_chain_from_genesis() {
    NUM=$1

    rm -rf "${PWD}/.alys/chain_db${NUM}"
    rm -rf "${PWD}/.alys/wallet${NUM}"
    echo "" > $PWD/data/logs/app${NUM}.txt

    resume_chain $NUM
}

function start_multiple_chain() {
    for (( i=0; i<$1; i++ )); do
        start_chain_from_genesis $i
        echo "Started chain ${CHAIN_PIDS[$i]}"
    done
}

function start_full_node_from_genesis() {
    NUM=$1

    rm -rf "${PWD}/.alys/chain_db${NUM}"
    rm -rf "${PWD}/.alys/wallet${NUM}"
    echo "" > $PWD/data/logs/app${NUM}.txt

    FULL_NODE=1
    resume_chain $NUM
}

function start_testnet_full_node() {
    cargo run --bin app -- --chain "${PWD}/etc/config/chain.json" --geth-url http://localhost:8551/ --db-path "${PWD}/etc/data/consensus/node_0/chain_db" --rpc-port 3000 --wallet-path "${PWD}/etc/data/consensus/node_0" --bitcoin-rpc-url $BTC_RPC_URL --bitcoin-rpc-user $BTC_RPC_USER --bitcoin-rpc-pass $BTC_RPC_PASSWORD --bitcoin-network testnet --p2p-port 55444 --remote-bootnode /ip4/54.224.209.248/tcp/55444/ip4/107.22.120.71/tcp/55444/ip4/54.161.100.208/tcp/55444 > "${PWD}/etc/data/logs/alys_0.txt" 2>&1 &
    CHAIN_PIDS[$NUM]=$!
}

function get_federation_address() {
    curl --silent -H "Content-Type: application/json" \
        -d '{"id":"1", "jsonrpc":"2.0", "method": "getdepositaddress", "params":[]}' \
        http://localhost:3000 | jq -r .result
}