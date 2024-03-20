function stop_bitcoin() {
    bitcoin-cli -regtest stop
}

function fund_wallet() {
    # create a new wallet
    bitcoin-cli -regtest -rpcwait createwallet $1 >/dev/null 2>&1
    # load the wallet if it already exists
    bitcoin-cli -regtest -rpcwait loadwallet $1 >/dev/null 2>&1
    # mine enough blocks so the funds are spendable
    bitcoin-cli -regtest generatetoaddress 101 $(bitcoin-cli -regtest -rpcwallet=$1 getnewaddress) >/dev/null
}

function maybe_fund_wallet() {
    if ! bitcoin-cli -regtest getwalletinfo >/dev/null 2>&1; then
        if bitcoin-cli -regtest loadwallet $1 >/dev/null 2>&1; then
            echo "Loaded wallet"
        else
            echo "Funding wallet"
            fund_wallet $1
        fi
    fi
}

function start_bitcoin() {
    # clear and start bitcoin regtest
    rm -rf $HOME/.bitcoin/regtest/
    bitcoind -regtest -server -fallbackfee=0.002 -daemon
    # fund default alice wallet
    fund_wallet "alice"
}

# send money to the federation deposit address
function pegin() {
    payment='[{"'$1'":"'$2'"},{"data":"'$3'"}]'
    # Step 1: Generate the transaction
    unfunded=$(bitcoin-cli -regtest -rpcwallet=alice createrawtransaction '[]' $payment)
    # Step 2: Fund the transaction
    funded=$(bitcoin-cli -regtest -rpcwallet=alice fundrawtransaction $unfunded | jq -r '.hex')
    # Step 3: Sign the transaction
    signed=$(bitcoin-cli -regtest -rpcwallet=alice signrawtransactionwithwallet $funded | jq -r '.hex' )
    # Step 4: Send the transaction
    txid=$(bitcoin-cli -regtest -rpcwallet=alice sendrawtransaction $signed)
    # Step 5: Mine the transaction
    block=$(bitcoin-cli -regtest -rpcwallet=alice generatetoaddress 7 bcrt1qewndkwr0evznxz7urnhlv5eav9rx2clsf0lh77 | jq -r '.[0]')
    echo $block
}