## Manual Setup of Alys Node

The instructions below demonstrate how to run an Alys sidechain consisting of a single local node, and a single-member federation.

### Geth and Bitcoin

We will start a single geth node and a Bitcoin regtest node.

```shell
# cleanup, init and run geth
./scripts/start_geth.sh
```

```shell
# in a new terminal start bitcoin
bitcoind -regtest -server -rest -rpcport=18443 -rpcuser=rpcuser -rpcpassword=rpcpassword -fallbackfee=0.002 -rpcallowip=127.0.0.1 -debug=rpc
```

### Alys node

Next, we start a single Alys node with the federation having exactly one member.

```shell
# dev (single node)

# From the Alys root directory
cargo run --bin app -- --dev
```

After running the above commands, you will have a local Alys node running with a single-member federation. You can interact with the node by making RPC calls to it. The default RPC port is `3000`.