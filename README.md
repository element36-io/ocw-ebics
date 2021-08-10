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

The provided `cargo run` command will launch a temporary node and its state will be discarded after
you terminate the process. After the project has been built, there are other ways to launch the
node.

Or you can run the development node with temporary storage:

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
