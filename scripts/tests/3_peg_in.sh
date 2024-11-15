#!/usr/bin/bash
# includes
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
. $SCRIPT_DIR/../utils/shared.sh

trap stop_all SIGINT

export CHAIN_ARGS="--mine"
start_all 3

FEDERATION_ADDRESS=$(get_federation_address)
EVM_ADDRESS="09Af4E864b84706fbCFE8679BF696e8c0B472201"

echo "Sending BTC for pegin"
pegin $FEDERATION_ADDRESS "1.0" $EVM_ADDRESS

# wait for chain to mint
sleep 10

BALANCE=$(cast balance "0x${EVM_ADDRESS}")
echo "$EVM_ADDRESS has $BALANCE"

stop_all
exit 1