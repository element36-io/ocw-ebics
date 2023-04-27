# Intro

This document describes in-depth how our chain functions and how to test it out.

## Fiat on/off ramp workflow

FiatRamps is a chain that aims to connect EBICS banking interface to Polkadot ecosystem. In order to accomplish it, we utilise Substrate offchain-workers. With the help of offchain-workers, our node syncs with our EBICS server and EBICS Java service. And with mapping our bank account to an `account` chain, we get an easy way to ramp on and off from chain.

Below is the workflow for easily ramping on and off to our chain:

- First and foremost, every user that wants to connect their bank account to FiatRamps, needs to call `map_iban_account` extrinsic
- Once on-chain account is mapped to off-chain bank account, user can perform following actions:
  - Burn funds, aka withdraw from bank account
  - Burn funds to IBAN, i.e transfer funds to another IBAN account
  - Burn funds to account, i.e transfer funds to another account on-chain

In order to move funds from their bank account, EBICS users call `/unpeg` API call providing neccessary recipient details.

Our pallet exposes three extrinsics that can be used to transfer or withdraw funds from the bank account that supports EBICS standard. The following extrinsics are available:

- `burn` - used for simply withdrawing money from the bank account  
- `transferToAddress` - transfers funds to a given address. This will extract IBAN that is mapped to the address from the pallet storage and makes `unpeg` request to the NEXUS API.  
- `transferToIban` - Simply transfers funds to the given IBAN. This also makes `unpeg` request to the NEXUS API.

It is important to note that transferring or withdrawing is not a synchronous process. This is because finality of transactions in EBICS standard is not instant. To handle this issue, our pallet also serves as escrow.

Whenever someone calls one of the above extrinsics, an `amount` of the transfer is transferred to Pallet's account and a new `BurnRequest` instance is created. `BurnRequest` struct contains id, source, destination and amount of the transfer.

The reason why we don't instantly send `unpeg` request to the API, is that we can't send HTTP call outside of Offchain Worker context. Therefore we store requests to *burn* funds from bank account and offchain worker processes it later. For each burn request, an `unpeg` request is sent.

Burn request is removed from the storage once the transaction is confirmed by EBICS API, i.e when it ends up as an outgoing transaction in the bank statement.

Below is a tutorial that demonstrates how our Substrate solo chain works.

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

Once you have submitted the call, head over to `Extrinsics -> fiatRamps -> mapIbanAccount` call. Here we need to map Alice's IBAN number to his on-chain account address. Simply choose Alice as a signer and the  copy and paste value of the IBAN number from the following JSON file and submit the extrinsic.

```json
{
  "accounts" : [ {
    "ownerName" : "Alice",
    "iban" : "CH2108307000289537320",
    "accountId": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
  }, {
    "ownerName" : "John Doe",
    "iban" : "CH2108307000289537313",
    "accountId": "5Hg6mE6QCiqDFH21yjDGe2JSezEZSTn9mBsZa6JsC3wo438c",
    "seed": "0x5108e950fb18a11a372da602c1714f289002204a8003748263bb9c351b57d3aa"
  }, {
    "ownerName" : "Bob",
    "iban" : "CH1230116000289537312",
    "accountId": "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty",
  } ]
}
```

![Alice connecting her bank account](/assets/alice-map-account.png)

#### Stablecoins are minted to Alice

Stablecoins are minted only when offchain worker detects an incoming transaction from an unknown IBAN address, i.e from an IBAN address is not mapped to any on-chain account address. In order to see how it works in action, head over to the EBICS service [API](http://w.e36.io:8093/ebics/swagger-ui/?url=/ebics/v2/api-docs/#/). Open `/ebics/api-v1/createOrder` tab and fill out Charlie's details. Namely, we will `purpose` field with Alice's on-chain account and `receipientIban` field with her IBAN number. Fill out Charlie's IBAN from above JSON file as the `sourceIban`. Finally, execute the call. It should look something like this:

![Alice mints](/assets/ebics-minting.png)

Then wait a little bit until offchain worker picks up the statement. After some time (3-5 blocktimes) you should see that new tokens were minted:

![Mint event happens](/assets/ocw-minting.png)

This is how new stablecoins are minted in our chain.

### Stablecoins are burned from Alice

Now, in order to see how burning works, we can either go to EBICS service again and call `/ebics/api-v1/unpeg` request or submit `transferToAddress`/`transferToIban` extrinsic. Let's use EBICS service again, as extrinsic calls are covered in the next demos. After filling up the `recipientIban` field with Charlie's IBAN, our call should look like this:

![Alice burns](/assets/ebics-burning.png)

Again, we wait for offchain worker to process the statement and shortly after we should see that it emits a Burn event:

![Burn event happens](/assets/ocw-burning.png)

#### Alice transfers to Charlie via EBICS API
For this part of the tutorial we will need to map Charlie and Bob's IBAN numbers to their on-chain account addresses. We submit `mapIbanAccount` extrinsic with IBAN addresses of Charlie and Bob, respectively, making sure that they are signing the extrinsic call. For example, Bob mapping his account would look like this:

![Bob connects his account](/assets/bob-map-iban.png)

It is also very important to know that we can not use PolkadotJS transfer button to move funds in our chain. This would break synchronization between the bank account balance and on-chain balance. In the future it should be disabled and the only way to transfer should be via burn requests.

To make a transfer from Alice to Charlie, we head over to our EBICS service [API](http://w.e36.io:8093/ebics/swagger-ui/?url=/ebics/v2/api-docs/#/). We open `/ebics/api-v1/createOrder` tab and fill out Charlie's details. Namely, we will `purpose` field with Charlie's on-chain account and `receipientIban` field with his IBAN number. And `sourceIban` field with Alice's IBAN number. We can then specify the amount and other fields. It should look similar to this:

![Alice transfer to Charlie](/assets/alice-transfer-charlie.png)

This will create a new order and will end up in Alice's bank statement as an outgoing transaction. And when our offchain worker queries bank statements, it will parse Charlie's on-chain account from `reference` field or query it from storage using his IBAN number. Note that transfer on-chain won't happen instantly, since offchain worker performs activities within a minimum of 5 block times interval (~30 seconds) and there are 3 types of actions. So, there is around 90 seconds of time between each new bank statements processing. 

Once offchain worker has processed new statements, two `Transfer` events occur:

![Transfer from Alice to Charlie](/assets/alice-bob-events.png)

#### Alice transfers to Charlie via Extrinsic

We go to `Extrinsic` tab and choose `transferToAddress` extrinsic call. Fill out the necessary fields and make sure that the amount is a positive number, otherwise extrinsic will fail.

![Extrinsic from Alice to Charlie](/assets/alice-charlie-ext.png)

After we submit extrinsic, we can see that the burn request event is created.

![Extrinsic from Alice to Charlie](/assets/alice-charlie-event-request.png)

Shortly after (approximately 3-4 blocks), we can notice that the burn request has been processed and transfer between Alice and Charlie occurs. Notice that transfer occurs from an unknown wallet to Charlie, not directly from Alice to Charlie. This is offchain worker's account that stores the funds until transaction is finalized by LibEUfin backend.

![Extrinsic from Alice to Charlie](/assets/alice-charlie-transfer.png)
