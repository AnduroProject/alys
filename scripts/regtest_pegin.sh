#!/bin/bash
# includes
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
. $SCRIPT_DIR/utils/shared.sh

maybe_fund_wallet alice

BTC_AMOUNT=${1:-"1.0"}
EVM_ADDRESS=${EVM_ADDRESS:-$2}

FEDERATION_ADDRESS=$(get_federation_address)
[ -n "$FEDERATION_ADDRESS" ] || { echo "FEDERATION_ADDRESS not set"; exit 1; }
echo "Federation Address: $FEDERATION_ADDRESS"

if [ -z $EVM_ADDRESS ]; then
    prefixed=$(evm_address $DEV_PRIVATE_KEY)
    # without 0x prefix
    EVM_ADDRESS="${prefixed:2}"
else
    EVM_ADDRESS=$([[ $EVM_ADDRESS == "0x"* ]] && echo ${EVM_ADDRESS:2} || echo $EVM_ADDRESS)
fi

echo "User Address: $EVM_ADDRESS"

echo "Sending BTC for pegin"
BLOCK=$(pegin $FEDERATION_ADDRESS $BTC_AMOUNT $EVM_ADDRESS)
echo "Transaction included in $BLOCK"
