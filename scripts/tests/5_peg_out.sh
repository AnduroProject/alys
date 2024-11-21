#!/usr/bin/env bash
# includes
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
. $SCRIPT_DIR/../utils/shared.sh

trap stop_all SIGINT

export CHAIN_ARGS="--mine"
start_all 3

export PRIVATE_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
fund_evm_account
EVM_BALANCE=$(cast balance "${EVM_ADDRESS}")
echo "$EVM_ADDRESS has $EVM_BALANCE"

pegout $PRIVATE_KEY

stop_all
exit 1