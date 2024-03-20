# includes
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
. $SCRIPT_DIR/../utils/shared.sh

trap stop_all SIGINT

export CHAIN_ARGS="--mine"
start_all 3

export PRIVATE_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
fund_evm_account

(
    cd contracts
    forge build

    # deploy ERC20 contract
    export CONTRACT=$(
        forge create \
            --rpc-url "http://127.0.0.1:8545" \
            --constructor-args "Token" "TKN" 1000 \
            --private-key $PRIVATE_KEY \
            src/MockErc20.sol:MockErc20 \
            --json | jq -r .deployedTo
    )

    # transfer tokens to recipient
    export RECIPIENT="0x70997970C51812dc3A010C7d01b50e0d17dc79C8"
    export AMOUNT=100
    forge script script/TransferErc20.s.sol --rpc-url http://localhost:8545 --broadcast >/dev/null 2>&1
)

stop_all
exit 1