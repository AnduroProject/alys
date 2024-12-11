#!/usr/bin/env bash
# includes
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
. $SCRIPT_DIR/bitcoin.sh
. $SCRIPT_DIR/chain.sh
. $SCRIPT_DIR/geth.sh

DEV_PRIVATE_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"

function stop_all() {
    if [[ $DONE == 1 ]]; then
        # already terminated, this function is called
        # on SIGINT but also before exit
        return
    fi

    stop_bitcoin
    stop_all_chains
    stop_all_geth

    if [ -n "$MINER_PID" ]; then
        echo "Killing miner $MINER_PID"
        kill -INT $MINER_PID 2>/dev/null
    fi

    DONE=1
}

function start_all() {
    # ready the debug build 
    cargo build || exit 1

    rm -rf data/execution/
    mkdir -p data/logs/

    start_bitcoin

    echo "Starting $1 execution nodes"
    start_multiple_geth $1
    # wait for geth to start
    sleep 2

    echo "Starting $1 consensus nodes"
    start_multiple_chain $1

    # wait for chain to start
    sleep 2
}

function fund_evm_account() {
    FEDERATION_ADDRESS=$(get_federation_address)
    EVM_ADDRESS=$(evm_address $PRIVATE_KEY)

    echo "Sending BTC for pegin"
    pegin $FEDERATION_ADDRESS "1.0" "${EVM_ADDRESS:2}"

    # wait for chain to mint
    sleep 4
}

function evm_address() {
    cast wallet address --private-key=$1
}

# takes the evm private key as input
function pegout() {
    if [ -z $2 ]; then
        # generate a new address for the alice wallet to receive the BTC
        export BTC_ADDRESS=$(bitcoin-cli -rpcuser=rpcuser -rpcpassword=rpcpassword -regtest -rpcwallet=alice getnewaddress)
    else
        export BTC_ADDRESS=$2
    fi
    echo "Requesting pegout to $BTC_ADDRESS"
    (
        # make sure to be in the contracts folder
        cd $PWD/contracts

        # 0.00100000 btc, or 100000 satoshi
        export SATOSHIS="$3"
        export PRIVATE_KEY="$1"
        forge script script/PegOut.s.sol --rpc-url http://localhost:8545 --broadcast --silent
    )

    echo "Waiting for BTC..."
    while true; do
        result=$(bitcoin-cli -rpcuser=rpcuser -rpcpassword=rpcpassword -regtest -rpcwallet=alice listreceivedbyaddress 0 false true $BTC_ADDRESS | jq '.[0].amount' -e)

        if [ $? -eq 0 ]; then
            echo "Received $result BTC"
            break
        else
            sleep 1
        fi
    done
}

function start_miner() {
    rm $PWD/data/logs/miner.txt
    echo "Starting the miner..."
    ./target/debug/miner --url "http://127.0.0.1:3000" > "$PWD/etc/data/logs/miner.txt" 2>&1 &
    MINER_PID=$!
}
