# Alys

Alys is a merged mined Bitcoin sidechain.

- Uses BTC as its base currency.
- Reaches consensus through aux PoW executed by Bitcoin miners and a federation.
- Facilitates a two-way peg between Bitcoin and the Alys sidechain through the federation members.

## Overview

On a high level, the repository consists of three parts:

- [app](./app): Contains a consensus client for block production and finalization and a federation client to process peg-in and peg-out transactions.
- [contracts](.contracts): Contains the smart contract for burning bridged BTC by users to trigger the peg-out process.
- [crates](./crates): Contains the logic for the peg-in and peg-out handling used by the app. It also contains the logic to interact with Bitcoin miners.
- [docs](./docs/src/README.md): Contains more information on the architecture.


## Prerequisites

- Install Rust `1.75.0` or higher: https://www.rust-lang.org/tools/install
- Install Geth `<=1.14.10`: https://geth.ethereum.org/docs/getting-started/installing-geth
- Install Bitcoin Core `25.0` or higher so that you have access to the `bitcoind` and `bitcoin-cli` commands:
  - MacOS: `brew install bitcoin`
  - Ubuntu: `sudo add-apt-repository ppa:bitcoin/bitcoin && sudo apt-get update && sudo apt-get install bitcoind`
  - Arch: `yay bitcoin-core`
  - Download a binary: https://bitcoin.org/en/download
- Install clang
- Install pkg-config
- Install libssl-dev
- Install build-essential
- Install foundry: https://book.getfoundry.sh/getting-started/installation

## Getting Started Guides:

* ### [Running Alys with Docker Compose](./docs/guides/getting_started_docker_setup.md)
* ### [Running Alys - Manual setup](./docs/guides/getting_started_manual_setup.md)

## Alys Testnet4

Anduro operates a public testnet for Alys used for development & testing. Anyone wishing to interact with the Alys testnet, whether it be to query the chain, send transactions, or connect your own node to the
network, can find connection info below.

### Alys Node #1:
```shell
IP: 209.160.175.123
Enode: enode://4a131d635e3b1ab30624912f769a376581087a84eef53f4fccc28bac0a45493bd4e2ee1ff409608c0993dd05e2b8a3d351e65a7697f1ee2b3c9ee9b49529958f@209.160.175.123:30303
```

### Alys Node #2:
```shell
IP: 209.160.175.124
Enode: enode://15d60f94195b361bf20acfd8b025b8f332b79f5752637e225e7c73aca7b17dd978ca94ab825d0f5221210e69ffcd96e910a257e25ff936c918335c44cc7041ba@209.160.175.124:30303
```

### Alys Node #3:
```shell
IP: 209.160.175.125
Enode: enode://53d6af0f549e4f9b4f768bc37145f7fd800fdbe1203652fd3d2ff7444663a4f5cfe8c06d5ed4b25fe3185920c28b2957a0307f1eed8af49566bba7e3f0c95b04@209.160.175.125:30303
```

## Faucet

https://faucet.anduro.io/


### Peg-In

Next, we move funds from Bitcoin to Alys via the peg-in to be able to send transactions on the Alys sidechain.

#### Get the Deposit Address

From the running Alys node, we can get the federation deposit address via the `getdepositaddress` RPC:

```shell
curl --silent -H "Content-Type: application/json" -d '{"id":"1", "jsonrpc":"2.0", "method": "getdepositaddress", "params":[]}' http://localhost:3000 | jq -r .result
```

This returns the federation deposit address of your local Alys node, e.g.:

```
bcrt1p3srvwkq5kyzlxqls43x97ch2vpcp4j278nk8jjuzcgt8k40ttr9s4vj934
```

#### Send BTC to the Deposit Address

Next, we do a bit of bitcoin-cli magic to create an "Alys" wallet. We send some BTC on regtest from the Alys wallet to the federation deposit address and add an EVM account (`0x09Af4E864b84706fbCFE8679BF696e8c0B472201`) in an OP_RETURN field for which we know the private key (`0xb9176fa68b7c590eba66b7d1894a78fad479d6259e9a80d93b9871c232132c01`).

You can run this script to achieve the peg in. The script will automatically fetch the deposit address from the federation nodes.

```shell
# set the btc amount and evm address
EVM_ADDRESS="09Af4E864b84706fbCFE8679BF696e8c0B472201"
./scripts/regtest_pegin.sh "1.0" $EVM_ADDRESS

# OR use the $DEV_PRIVATE_KEY
./scripts/regtest_pegin.sh
```

The Alys node will automatically bridge the BTC.

#### Check that Funds are Allocated in Alys

Run `cast` to check that the funds have been allocated. Note that on peg-in, satoshis (10^8) will be converted to wei (10^18) so you will see a lot more 0s for the bridge 1 BTC, i.e., 1x10^18 wei instead of 1x10^8 satoshis.

```shell
cast balance 0x09Af4E864b84706fbCFE8679BF696e8c0B472201 --rpc-url "localhost:8545"
> 1000000000000000000
```

### Peg-Out

Next up, we want to peg out.

#### Peg-out Funds

We are returning the funds to the Alys wallet we created in Bitcoin.

We can use the peg out contract set the genesis at address `0xbBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB`, see also the [genesis file](./data/genesis.json).

We are doing this from the CLI and will need to define a `PRIVATE_KEY` env.

- `PRIVATE_KEY`: The private key is `0xb9176fa68b7c590eba66b7d1894a78fad479d6259e9a80d93b9871c232132c01`. This is the private key to the address `0x09Af4E864b84706fbCFE8679BF696e8c0B472201` that we set for the peg in.

```shell
# set the private key and btc address
PRIVATE_KEY=0xb9176fa68b7c590eba66b7d1894a78fad479d6259e9a80d93b9871c232132c01
./scripts/regtest_pegout.sh $PRIVATE_KEY $BTC_ADDRESS

# OR just the private key
./scripts/regtest_pegout.sh $PRIVATE_KEY

# OR use the $DEV_PRIVATE_KEY
./scripts/regtest_pegout.sh

# check the last 3 transactions. The 2 last should be the mining reward to alys (with category "immature") and the 3rd last txs should be a normal receive tx from the foundation
bitcoin-cli -regtest -rpcuser=rpcuser -rpcpassword=rpcpassword listtransactions "*" 3
```

<details>
<summary>Expected output</summary>

```shell
  {
    "address": "bcrt1qane4k9ejhhca9w0ez7ale7xru5pnrqmuwqayhc",
    "parent_descs": [
      "wpkh(tpubD6NzVbkrYhZ4XGc5eHTPRieN8p27r6PPNenUPJz5JQeCkav8aZ2wz9zc83xgEUVbpQetH6FXABUZ5LDG9uDWqf7fc9RN2yfJzDAmHnSFHHw/84h/1h/0h/0/*)#t9fj9n6e"
    ],
    "category": "receive",
    "amount": 0.00010000,
    "label": "",
    "vout": 0,
    "abandoned": false,
    "confirmations": 2,
    "blockhash": "78e3a9699277e9dc1da0da5e7f47bded9abdfce673bf1858e18aa6c2089d7d54",
    "blockheight": 792,
    "blockindex": 1,
    "blocktime": 1706691489,
    "txid": "831094cba680a5cbbd622b464eaf69562d53b681400c747cee72caddbc9765b4",
    "wtxid": "0dca63f31e7b873ef29d5ea3124a62f7e40d9f9de5b72e88c39904e9e6750256",
    "walletconflicts": [
    ],
    "time": 1706691488,
    "timereceived": 1706691488,
    "bip125-replaceable": "no"
  },
```

</details>

## Connecting to an Alys Network

### Testnet
- RPC: http://209.160.175.123:8545
- Explorer: http://testnet.alyscan.io/
- Faucet: https://faucet.anduro.io/

- Chain ID: 212121

## Full Node

Running a full node is similar to running a federation node. The main difference is that full nodes do not require an Aura or Bitcoin key since they are not signing blocks or Bitcoin transactions.

1. Start Bitcoin with the network you want to connect to via `bitcoind`.
2. Start geth via `NUM=0 ./scripts/start_geth.sh` (assuming you run the full node on a different machine. If running on the same machine as the three nodes, use `NUM=3 ./scripts/start_geth.sh`)
3. Start the full node assuming the full node runs on a seperate machine:

```shell
cargo run --bin app -- \
  --chain etc/config/chain.json \
  --geth-url http://localhost:8551/ \
  --db-path etc/data/consensus/node_0/chain_db/ \
  --rpc-port 3000 \
  --wallet-path etc/data/consensus/node_0/wallet \
  --bitcoin-rpc-url localhost:18332 \
  --bitcoin-rpc-user rpcuser \
  --bitcoin-rpc-pass rpcpassword \
  --bitcoin-network testnet \
  --remote-bootnode /ip4/BOOTNODE_IP/tcp/BOOTNODE_LIBP2P_PORT
```

- `BOOTNODE_IP`: To select a bootnode, get its public or private IP address.
- `BOOTNODE_LIBP2P_PORT`: There are two ways to get the p2p port.
  - You can find the listening port by running `lsof -Pn -i4` on the server running the bootnode.
  - Set the P2P port explicitly: You can set a dedicated p2p port on the federation node/bootnode by adding the argument `--p2p-port 55444` to set this to port `55444`. Make sure to pick a free port.

## Development

### Alys Node (Consensus Layer)

#### Build and Deploy

```shell
# cleanup, init and run geth
./scripts/start_geth.sh

# start bitcoin
bitcoind -regtest -rpcuser=rpcuser -rpcpassword=rpcpassword -fallbackfee=0.002

# dev (single node)
cargo run --bin app -- --dev
```


#### Unit tests

Tests are self-contained such that none of the services need to run.

```shell
cargo test
```

#### Format

```shell
cargo fmt
```

### Smart Contracts

#### Build and Deploy

Go to the contracts folder.

```shell
cd ./contracts
```

The contracts folder contains only the bridge contract for the peg out. However, you can add any other smart contracts you may wish to add here.

Build and deploy.

```shell
forge build
```

#### Example ERC20

We are going to deploy an example ERC20 contract to show how to interact with the sidechain.

We are going to use our private key (`0xb9176fa68b7c590eba66b7d1894a78fad479d6259e9a80d93b9871c232132c01`) as a means to deploy the contract. Make sure the account belonging to this key has received funds via the peg-in procedure.

```shell
PRIVATE_KEY=0xb9176fa68b7c590eba66b7d1894a78fad479d6259e9a80d93b9871c232132c01
# constructor takes the name of the contract, the ticker, and the initial supply that is minted to the creator of the contract
forge create --rpc-url "http://127.0.0.1:8545" --private-key ${PRIVATE_KEY} src/MockErc20.sol:MockErc20 --json --constructor-args "HelloBitcoinContract" "HBC" 100000000000000000000000
```

This should result in something like:

```shell
{"deployer":"0x09Af4E864b84706fbCFE8679BF696e8c0B472201","deployedTo":"0x1C36129916E3EA2ACcD516Ae92C8f91deF7c4146","transactionHash":"0x8478bbed6ba658eecb8e36c143969cf6c11c4517f5f32acf75af5a9c41ac69dd"}
```

Other useful scripts:

```shell
# Send some of the ERC20 tokens from the deployed contract (0x1C36129916E3EA2ACcD516Ae92C8f91deF7c4146) to account 0xd362E49EE9453Bf414c35288cD090189af2B2C55
cast send --private-key ${PRIVATE_KEY} \
  --rpc-url "localhost:8545" \
  --chain 263634 \
  0x1C36129916E3EA2ACcD516Ae92C8f91deF7c4146 \
  "transfer(address,uint256)" 0xd362E49EE9453Bf414c35288cD090189af2B2C55 100000000
# Send 16200000000007550 wei bridged BTC to account 0xd362E49EE9453Bf414c35288cD090189af2B2C55
cast send --private-key ${PRIVATE_KEY} 0xd362E49EE9453Bf414c35288cD090189af2B2C55 --value 16200000000007550
```

#### Test

```shell
forge test
```

#### Format

```shell
forge fmt
```

## EVM Tooling

Since we use Geth without modification, it is already possible to use most existing EVM tooling out-the-box including MetaMask, Foundry / Hardhat and of course Blockscout!

### Blockscout

To setup [Blockscout](https://github.com/blockscout/blockscout) follow the deployment guides [here](https://docs.blockscout.com/for-developers/deployment). We recommend using [Docker Compose](https://github.com/docker/compose) for simplicity.

```shell
git clone git@github.com:blockscout/blockscout.git
cd ./docker-compose
```

Change the environment variables:

```
# /docker-compose/envs/common-blockscout.yml
SUBNETWORK=Merged ALYS
CHAIN_ID=263634
# /docker-compose/envs/common-frontend.yml
NEXT_PUBLIC_NETWORK_NAME=Merged ALYS Alpha
NEXT_PUBLIC_NETWORK_SHORT_NAME=Merged ALYS Alpha
```

Start the explorer with:

```shell
docker-compose -f geth.yml up --build
```

The explorer runs on [localhost:80](http://localhost/).

If you reset the chain make sure to clear the persistent data in `docker-compose/services/`.

```shell
sudo rm -rf services/redis-data services/stats-db-data services/blockscout-db-data services/logs
```

## Genesis

We provide [`genesis.json`](./data/genesis.json) for local development using Geth but it is also possible to use this other deployments.

It was previously based on the Sepolia genesis with some modifications using [this guide](https://dev.to/q9/how-to-merge-an-ethereum-network-right-from-the-genesis-block-3454):

```shell
geth --sepolia dumpgenesis | jq .
```

Ensure that the chain is configured to start post-capella (set `shanghaiTime` to 0).

The Alys sidechain expects the bridge contract to be pre-deployed at `0xbBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB`, this is set in `alloc`.

## Chain Spec

When you start the Alys sidechain it will use a chain spec to configure it's own genesis block based also on the Geth genesis configured above. We provide [`chain.json`](./etc/config/chain.json) for local development assuming three nodes (instructions above) or using `--chain=dev` will start a single node network. See the annotations below for how to configure a new setup:

```javascript
{
  // the block duration in milliseconds
  "slotDuration": 2000,
  // public keys for bls signing
  "authorities": [],
  // evm addresses for each authority (to receive fees)
  "federation": [],
  // public keys for secp256k1 signing
  "federationBitcoinPubkeys": [],
  // initial PoW mining difficulty
  "bits": 553713663,
  // should be the same as the geth `genesis.json`
  "chainId": 263634,
  // stall block production if no AuxPow is received
  "maxBlocksWithoutPow": 10,
  // set the scanning height, use latest height for testnet or mainnet
  "bitcoinStartHeight": 0,
  "retargetParams": {
    // disable retargeting so we always keep the same target
    "powNoRetargeting": false,
    // the maximum target allowed
    "powLimit": 553713663,
    // expected difficulty adjustment period (in seconds)
    "powTargetTimespan": 12000,
    // expected block time (in seconds)
    "powTargetSpacing": 1000
  }
}
```

Each node should use the same genesis and chain spec, otherwise blocks will be rejected.

Ensure that each federation member has set an EVM address to receive fees - this can be derived from the same secret key used to generate the public key in `"authorities"`. When fees are generated from EVM transactions they are sent directly to that account.

## Resources

- https://ethresear.ch/t/eth1-eth2-client-relationship/7248
- https://hackmd.io/@danielrachi/engine_api
- https://ethereum.org/en/developers/docs/apis/json-rpc/
- https://ceur-ws.org/Vol-2058/paper-06.pdf
- https://openethereum.github.io/Aura.html
- https://en.bitcoin.it/wiki/Merged_mining_specification
