# Intro

This document describes in-depth how our chain functions and how to test it out.

## Setup

To get started, obviously make sure you have the necessary setup for Substrate development.

## Demo

Compile and run the node with:

```bash
cargo run --release -- --dev --tmp
```

Open PolkadotJs [interface](https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/explorer) and go to `Developer -> RPC calls` page. Here, we first need to enter keypair for our offchain worker, since it will be signing and submitting transactions to the chain. Choose `author -> insertKey` RPC call and fill out the fields with the following values:

```js
key_type: ramp
suri: cup swing hill dinner pioneer mom stick steel sad raven oak practice
public_key: 5C555czPfaHgYhKhsRg2KNCLGCJ82jVsvweTHAnfvT83uy5T
```

Once you have submitted the call, head over to `Extrinsics -> fiatRamps -> mapIbanAccount` call. Here we need to map Alice, Bob and Charlie's accounts with their respective IBAN numbers, which are given below. Please, make sure to select the correct signer account for each IBAN number.

```json
{
  "accounts" : [ {
    "ownerName" : "Alice",
    "iban" : "CH2108307000289537320",
    "accountId": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    "bic" : "HYPLCH22570",
    "offeredAccountId" : "accountname1",
    "nexusBankAccountId" : "CH2108307000289537320"
  }, {
    "ownerName" : "Bob",
    "iban" : "CH1230116000289537312",
    "accountId": "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty",
    "bic" : "HYPLCH22572",
    "offeredAccountId" : "accountname2",
    "nexusBankAccountId" : "CH1230116000289537312"
  }, {
    "ownerName" : "Charlie",
    "iban" : "CH1230116000289537313",
    "accountId": "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
    "bic" : "HYPLCH22573",
    "offeredAccountId" : "accountname3",
    "nexusBankAccountId" : "CH1230116000289537313"
  } ]
}
```

![Bob connecting his bank account](/assets/bob-map-iban.png)

Once you are done with connecting test accounts to their IBAN numbers, we can proceed to testing how transfering, minting and burning works.

#### Alice transfers to Charlie

To make a transfer from Alice to Charlie, we head over to our EBICS service [API](http://w.e36.io:8093/ebics/swagger-ui/?url=/ebics/v2/api-docs/#/). We open `/ebics/api-v1/createOrder` tab and fill out Charlie's details. Namely, we will `purpose` field with Charlie's on-chain account and `receipientIban` field with his IBAN number. And `sourceIban` field with Alice's IBAN number. We can then specify the amount and other fields. It should look similar to this:

![Alice transfer to Charlie](/assets/alice-transfer-charlie.png)

This will create a new order and will end up in Alice's bank statement as outgoing transaction. And when our offchain worker queries bank statements, it will parse Charlie's on-chain account from `reference` field or query it from storage using his IBAN number. Note that transfer on-chain won't happen instantly, since offchain worker performs activities within a minimum of 5 block times interval (~30 seconds) and there are 3 types of actions. So, there is around 90 seconds of time between each new bank statements processing. 

Once offchain worker has processed new statements, two `Transfer` events occur:

![Transfer from Alice to Charlie](/assets/alice-bob-events.png)

#### Bob 

## Fiat on/off ramp workflow

FiatRamps is a chain that aims to connect EBICS banking interface to Polkadot ecosystem. In order to accomplish it, we utilise Substrate offchain-workers. With the help of offchain-workers, our node syncs with our EBICS server and EBICS Java service. And with mapping our bank account to an `account` chain, we get an easy way to ramp on and off from chain.

Below is the workflow for easily ramping on and off to our chain:

- First and foremost, every user that wants to connect their bank account to FiatRamps, needs to call `map_iban_account` extrinsic
- Once on-chain account is mapped to off-chain bank account, user can perform following actions:
  - Burn funds, aka withdraw from bank account
  - Burn funds to IBAN, i.e transfer funds to another IBAN account
  - Burn funds to account, i.e transfer funds to another account on-chain

In order to move funds from their bank account, EBICS users call `/unpeg` API call providing neccessary recipient details.

### Bank Statement

Each bank statement has at least four fields: `iban`, `balanceCL`, `incomingStatements` and `outgoingStatements`. A bank statement in our pallet is represented as the combination of `IbanAccount` and `Transaction` types:

```rust
pub struct IbanAccount {
    /// IBAN number of the account
	pub iban: StrVecBytes,
	/// Closing balance of the account
	pub balance: u128,
	/// Last time the statement was updated
	pub last_updated: u64
}

pub struct Transaction {
    /// IBAN of the sender, if tx is incoming, IBAN of the receiver, otherwise
	pub iban: StrVecBytes,
    /// Name of the sender, if tx is incoming, name of the receiver, otherwise
	pub name: StrVecBytes,
    /// Currency of the transaction
	pub currency: StrVecBytes,
    /// Amount of the transaction. Note that our token has 6 decimals
	pub amount: u128,
	/// Usually contains the on-chain accountId of the destination and/or burn request nonce
	pub reference: StrVecBytes,
    /// Type of the transaction: Incoming or Outgoing
	pub tx_type: TransactionType
}
```

### Offchain worker

Offchain worker performs two activities: process new statements and process burn requests. Burn requests are 