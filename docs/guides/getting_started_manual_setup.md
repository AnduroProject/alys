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

# OR if you wish to disable mining
cargo run --bin app -- --dev --no-mining --jwt-secret <your_jwt_secret>
```

> NOTE: `<your_jwt_secret>` will be a path to a file containing a JWT secret which will most likely be in `etc/config/jwtsecret.hex`

After running the above commands, you will have a local Alys node running with a single-member federation. You can interact with the node by making RPC calls to it. The default RPC port is `3000`.

### Next Steps

Now that you have a local Alys node running, go ahead and try to peg-in funds from your regtest Bitcoin node to Alys.

```shell
./scripts/regtest_pegin.sh 
```

You should see the following output:

```shell
Federation Address: bcrt1p3srvwkq5kyzlxqls43x97ch2vpcp4j278nk8jjuzcgt8k40ttr9s4vj934
User Address: f39Fd6e51aad88F6F4ce6aB8827279cffFb92266
Sending BTC for pegin
Transaction included in 702a22958cc7905b4974933a1df8134f8b43ff5357fc8a0536416309943a2e59
```

You can then check the balance of the Alys account using the following RPC call:

```shell
# Using `cast` from Foundry for simplicity, but you make a raw RPC call to the Alys node
cast balance 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
```