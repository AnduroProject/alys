## Manual Setup of Alys Node

The instructions below demonstrate how to run an Alys sidechain consisting of a single local node, and a single-member federation.

> This is intended for local development.

### Bitcoin

```shell
# in a new terminal start bitcoin
bitcoind -regtest -server -rest -rpcport=18443 -rpcuser=rpcuser -rpcpassword=rpcpassword -fallbackfee=0.002 -rpcallowip=127.0.0.1 -debug=rpc
```

> IMPORTANT: Make sure your are running version `28.0.0` or higher of Bitcoin Core.

### Geth

In a new terminal, start Geth. Make sure you are running version `1.14.0` of Geth.

```shell
./scripts/start_geth.sh
```

### Alys node

Next, we start a single Alys node with the federation having exactly one member.

Make sure you are using Rust version `1.87.0` or higher.

```shell
# dev (single node)

# From the Alys root directory
cargo run --bin app -- --dev --jwt-secret <your_jwt_secret>
```

After running the above commands, you will have a local Alys node running with a single-member federation. You can interact with the node by making RPC calls to it. The default RPC port is `3000`.