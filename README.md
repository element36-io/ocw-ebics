# Ebics Off-chain Worker Substrate pallet

Contains a fork of Substrate node-template and a offchain worker pallet.

### Getting Started

This is the modified `substrate-node-template` with an offchain worker for `EbicsService` called `Fiat-ramps`. 

`Fiat-ramps` is located inside `/pallets` folder. It is an offchain-worker pallet that polls `EbicsService` api to get the latest bank statements. Current workflow of the offchain worker:

1. Get all statements from the api
2. Extract `iban` and `balanceCL` (closing balance) of each statement
3. Store balance of each `iban` in the offchain-worker local storage

### Rust Setup

First, complete the [basic Rust setup instructions](./docs/rust-setup.md).

### Build

Use the following command to build the node without launching it:

```sh
cargo build --release
```

Note: the above code might take long to compile depending on your machine specs (~30-45 minutes)

## Run

The OWC needs the backend which connects to the bank account, which is provided by
(this project)[https://github.com/element36-io/ebics-java-service]. Start the backend:

```sh
docker run -p 8093:8093 e36io/ebics-service 
```

Now start the OCW. The provided `cargo run` command will launch a temporary node and its state will be discarded after
you terminate the process. After the project has been built, there are other ways to launch the
node.

```sh
./target/release/node-template --dev --tmp
```

You can run the development node with temporary storage:

```sh
./target/release/node-template --dev --tmp
```

### Single-Node Development Chain

This command will start the single-node development chain with persistent state:

```bash
./target/release/node-template --dev
```

Purge the development chain's state:

```bash
./target/release/node-template purge-chain --dev
```

Start the development chain with detailed logging:

```bash
RUST_LOG=debug RUST_BACKTRACE=1 ./target/release/node-template -lruntime=debug --dev
```

### Connect with Polkadot-JS Apps Front-end

Once the node template is running locally, you can connect it with **Polkadot-JS Apps** front-end
to interact with your chain. [Click here](https://polkadot.js.org/apps/#/explorer?rpc=ws://localhost:9944) connecting the Apps to your local node template.

First, we will need to inject our types in the PolkadotJs interface [here:](https://polkadot.js.org/apps/#/settings/developer). Paste the contents of [`types.json`](https://github.com/element36-io/ocw-ebics/blob/main/pallets/fiat-ramps/src/types.json) in the text area and click `Save`.

Now you will be able to open the block explorer and see transactions, events that have occured in the blockchain.

For example, you can take a look at chain storage of `fiat-ramps` [here](https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/chainstate)

### Inject Signer Account

For the offchain worker to sign transactions, we need to insert authority keys into the chain. This can be done using RPC call `author_insert_key` which is located in PolkadotJs Apps interface RPC calls section.

For testing puroposes, we can use this account:

```js
key_type: ramp
suri: cup swing hill dinner pioneer mom stick steel sad raven oak practice
public_key: 5C555czPfaHgYhKhsRg2KNCLGCJ82jVsvweTHAnfvT83uy5T
```

### New API url

You can set the new url for the `ebics-service` via PolkadotJS interface. Follow this link to the `Sudo` [tab](https://polkadot.js.org/apps/#/sudo) and choose `FiatRamps.setApiUrl` extrinsic. Paste the new url for the API and click `Submit transaction`. If everything is good, i.e you are the Sudo account and you have the necessary rights, you should see the transaction included in the block and offchain worker starts querying the new API.

### Run in Docker

First, install [Docker](https://docs.docker.com/get-docker/) and
[Docker Compose](https://docs.docker.com/compose/install/).

Then run the following command to start a single node development chain.

```bash
./scripts/docker_run.sh
```

This command will firstly compile your code, and then start a local development network. You can
also replace the default command (`cargo build --release && ./target/release/node-template --dev --ws-external`)
by appending your own. A few useful ones are as follow.

```bash
# Run Substrate node without re-compiling
./scripts/docker_run.sh ./target/release/node-template --dev --ws-external

# Purge the local dev chain
./scripts/docker_run.sh ./target/release/node-template purge-chain --dev

# Check whether the code is compilable
./scripts/docker_run.sh cargo check
```
