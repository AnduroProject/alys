# Running Alys with Docker Compose

> **NOTE**: If you intend on running Alys using **docker-compose**, we recommend cloning the repo to `/opt/alys/lib` on unix-based systems to be able to use the docker-compose files we provide without changes.

After cloning the Alys repository, you can conveniently find multiple docker-compose files in the `etc` folder:
- `docker-compose.j2.yml` - Runs Alys sidechain with a single node and a federation with a single member.
- `docker-compose.full-node.j2.yml` - Runs an Alys full node which is similar to running a federation node, but does **NOT** require an Aura or Bitcoin key since full nodes are not signing blocks or Bitcoin transactions.
- `docker-compose.local.yml` - Runs a single Alys node + single-member federation + Bitcoin node + Reth node. (This is the recommended setup for local development.)

**IMPORTANT**: Any of the `docker-compose` files that contain `*.j2.yml` are Jinja2 template files which contains placeholders for several configuration arguments. You must replace these placeholders with the actual values before running the docker-compose command. The placeholders are:
- `BTC_NODE_IP`: The IP address of a Bitcoin node
- `BOOTNODE_ARG`: The argument to be passed to the Alys node for the bootnode. This is used to connect to other nodes in the network.
- `AURA_SECRET_KEY`: The secret key used for the Aura consensus algorithm. This is used to sign blocks and transactions.
- `BTC_SECRET_KEY`: The secret key used for the Bitcoin node. This is used to sign Bitcoin transactions.

## *Option* #1: Run Alys Sidechain with single-node setup + single-member Federation

`docker-compose.j2.yml`


#### Step 1. 

Any of the `docker-compose` files that contain `*.j2.yml` are Jinja2 template files which contains placeholders for several configuration arguments. You must replace these placeholders with the actual values before running the docker-compose command.


To have your Alys node connect with other nodes, update the `BOOTNODE_ARG`.

If you want your Alys node to connect to `Alys Testnet4` use the following value(s):

`BOOTNODE_ARG=/ip4/209.160.175.123/tcp/55444`

**IMPORTANT:** Provide valid values for the remaining placeholders in the `docker-compose` file.

#### Step 2.

Edit `etc/config/eth-config.toml` to set `[peers].trusted_nodes`
For example, to connect to `Alys Testnet4` set to the following:
```toml
[peers]
...
trusted_nodes = [
    "enode://4a131d635e3b1ab30624912f769a376581087a84eef53f4fccc28bac0a45493bd4e2ee1ff409608c0993dd05e2b8a3d351e65a7697f1ee2b3c9ee9b49529958f@209.160.175.123:30303"
]
...
```

#### Step 3. 

Run Alys node + single-member federation
```sh
$ docker compose -f etc/docker-compose.j2.yml up -d
```

#### Step 4.

Check the logs to see if everything is running smoothly
```sh
$ docker compose -f etc/docker-compose.j2.yml logs -f
```

> Reference the section [Connecting to an Alys Network](#connecting-to-an-alys-network) for the list of available networks and their respective connection details.



## *Option* #2: Run Alys Fullnode

`docker-compose.full-node.j2.yml`

#### Step 1. 

Alys full nodes do **NOT** sign blocks or Bitcoin transactions. Thus, you do not need to generate secret keys for signing. You only need to update your docker-compose file with a valid `BOOTNODE_ARG`:

`BOOTNODE_ARG=/ip4/209.160.175.123/tcp/55444`

#### Step 2.

Run Alys full node with the following command:
```sh
$ docker compose -f etc/docker-compose.j2.yml up -d
```

#### Step 3.

Check the logs to see if everything is running smoothly.
```sh
$ docker compose -f etc/docker-compose.j2.yml logs -f
```

## *Option* #3: (Local Development) Run entire Alys stack

*coming soon...*