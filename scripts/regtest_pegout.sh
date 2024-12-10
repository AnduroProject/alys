#!/usr/bin/env bash
# includes
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
. $SCRIPT_DIR/utils/shared.sh

PRIVATE_KEY=${PRIVATE_KEY:-$1}
SATOSHIS=${SATOSHIS:-1000001}
if [ -z $PRIVATE_KEY ]; then
    PRIVATE_KEY=$DEV_PRIVATE_KEY
else
    PRIVATE_KEY=$([[ $PRIVATE_KEY == "0x"* ]] && echo $PRIVATE_KEY || echo "0x$PRIVATE_KEY")
fi

BTC_ADDRESS=${BTC_ADDRESS:-$2}

pegout $PRIVATE_KEY $BTC_ADDRESS $SATOSHIS
