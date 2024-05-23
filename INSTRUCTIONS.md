# Intro

This document describes in-depth how our chain functions and how to test it out.

## Fiat on/off ramp workflow

FiatRamps is a chain that aims to connect EBICS banking interface to Polkadot ecosystem. In order to accomplish it, we utilise Substrate offchain-workers. With the help of offchain-workers, our node syncs with our EBICS server and EBICS Java service. And with mapping our bank account to an `account` chain, we get an easy way to ramp on and off from chain.

Below is the workflow for easily ramping on and off to our chain:

- First and foremost, every user that wants to connect their bank account to FiatRamps, needs to call `createAccount` extrinsic
- Once on-chain account is mapped to off-chain bank account, user can perform following actions:
  - Burn funds, i.e withdraw from bank account
  - Transfer funds to IBAN, i.e transfer funds to another IBAN account
  - Transfer funds to account, i.e transfer funds to another account on-chain

In order to move funds from their bank account, EBICS users call `/unpeg` API call providing neccessary recipient details.

Our pallet exposes a single extrinsic that can be used to transfer or withdraw funds from the bank account that supports EBICS standard. This extrinsic is called `transfer` and it has following parameters:

- `amount` - specifies the amount of funds to be transferred
- `dest` - a custom enum that specifies the destination of the transfer. It can be either `Address` or `Iban` or `Withdraw`. If `Address` is chosen, then `dest` field should contain an on-chain account address. If `Iban` is chosen, then `dest` field should contain an IBAN number. `Withdraw` does not require any additional parameters.

It is important to note that transferring or withdrawing is not a synchronous process. This is because finality of transactions in EBICS standard is not instant. To handle this issue, our pallet also serves as escrow.

Whenever someone calls one of the above extrinsics, an `amount` of the transfer is transferred to Pallet's account and a new `BurnRequest` instance is created. `BurnRequest` struct contains id, source, destination and amount of the transfer.

The reason why we don't instantly send `unpeg` request to the API, is that we can't send HTTP call outside of Offchain Worker context. Therefore we store requests to *burn* funds from bank account and offchain worker processes it later. For each burn request, an `unpeg` request is sent.

Burn request is removed from the storage once the transaction is confirmed by EBICS API, i.e when it ends up as an outgoing transaction in the bank statement.

Below is a tutorial that demonstrates how our Substrate solo chain works.

## Setup

To get started, obviously make sure you have the necessary setup for Substrate development.

## Ebics Java Service

First, we need to run the EBICS Java service. This service is responsible for connecting to the bank account and providing an API for our offchain worker to interact with. You can find instructions for running the service [here](https://github.com/element36-io/ebics-java-service/blob/hyperfridge/docs/TEST.md#run-and-test-with-docker):

Or manually, make sure you cloned [`ebics-java-service`](https://github.com/element36-io/ebics-java-service) and switch to `hyperfridge` branch:

```
docker compose pull
docker compose up -d
# optional
docker compose logs -f
```

### Image ID

And you also need to get the image ID of the `hyperfridge` `riscv0` module. You can get it by running the following command:

```bash
docker compose run hyperfridge cat /app/IMAGE_ID.hex

dcaba464d4909890d6638dd14e7a25853a8dd2cad14639d0d310987b32a43957
```

Copy that image ID and pass it to the sudo extrinsic when the chain is running.

## Demo

Compile and run the node with:

```bash
cargo run --release -- --dev --tmp
```

Insert image ID with `fiatRamps::setRisc0ImageId` extrinsic call. This is necessary for offchain worker to know which image to use when running the `riscv0` module. Go to `Sudo` tab and choose `fiatRamps -> setRisc0ImageId` extrinsic and paste the image ID from the previous step, make sure to prepend the image ID with `0x`. Click `Submit transaction`. Sudo account is a development account `Dave`.

<img width="1722" alt="Set Image ID" src="https://github.com/element36-io/ocw-ebics/assets/88332432/c099cd03-4003-4c88-8b24-6099a3145f8f">


Open PolkadotJs [interface](https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/explorer) and go to `Developer -> RPC calls` page. Here, we first need to enter keypair for our offchain worker, since it will be signing and submitting transactions to the chain. Choose `author -> insertKey` RPC call and fill out the fields with the following values:

```js
key_type: ramp
suri: cup swing hill dinner pioneer mom stick steel sad raven oak practice
public_key: 5C555czPfaHgYhKhsRg2KNCLGCJ82jVsvweTHAnfvT83uy5T
```

Then, choose `FiatRamps.setApiUrl` extrinsic and paste the new url for the API and click `Submit transaction`. This is only necessary if you have a different URL than the default one with Ebics Java service.

Once you have submitted the call, head over to `Extrinsics -> fiatRamps -> createAccount` call. Here we need to map Alice's IBAN number to his on-chain account address. Simply choose Alice as a signer, copy and paste value of the IBAN number from the following JSON file and submit the extrinsic.

```json
{
  "accounts" : [ {
    "ownerName" : "Alice",
    "iban" : "CH2108307000289537320",
    "accountId": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
  }, {
    "ownerName" : "Jack",
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

`Jack's` IBAN comes mapped in genesis, so we don't need to map it again.

![Alice connecting her bank account](/assets/alice-map-account.png)

#### Stablecoins are minted to Alice

Stablecoins are minted only when offchain worker detects an incoming transaction from an unknown IBAN address, i.e from an IBAN address is not mapped to any on-chain account address. In order to see how it works in action, head over to the EBICS service [API](http://w.e36.io:8093/ebics/swagger-ui/?url=/ebics/v2/api-docs/#/). Open `/ebics/api-v1/createOrder` tab and fill out Bob's details. Namely, we will fill `purpose` field with Alice's on-chain account and `receipientIban` field with her IBAN number. Fill out Bob's IBAN from above JSON file as the `sourceIban`. Finally, execute the call. It should look something like this:

![Alice mints](/assets/ebics-minting-zk.png)

Then wait a little bit until offchain worker picks up the statement. After some time (3-5 blocktimes) you should see that new tokens were minted:

![Mint event happens](/assets/ocw-minting-zk.png)

This is how new stablecoins are minted in our chain.

### Stablecoins are burned from Alice

Now, in order to see how burning works, we can either go to EBICS service again and call `/ebics/api-v1/unpeg` request or submit `fiatRamps.transfer` extrinsic. Let's use EBICS service again, as extrinsic calls are covered in the next demos. After filling up the `recipientIban` field with Bob's IBAN, our call should look like this:

![Alice burns](/assets/ebics-burning-zk.png)

Again, we wait for offchain worker to process the statement and shortly after we should see that it emits a Burn event:

![Burn event happens](/assets/ocw-burning-zk.png)

#### Alice transfers to Jack via EBICS API

For this part of the tutorial we will need to map Jack and Bob's IBAN numbers to their on-chain account addresses. We submit `createAccount` extrinsic with IBAN addresses of Jack and Bob, respectively, making sure that they are signing the extrinsic call. For example, Bob mapping his account would look like this:

![Bob connects his account](/assets/bob-map-iban.png)

It is also very important to know that we can not use PolkadotJS transfer button to move funds in our chain. This would break synchronization between the bank account balance and on-chain balance. In the future it should be disabled and the only way to transfer should be via burn requests.

To make a transfer from Alice to Jack, we head over to our EBICS service [API](http://w.e36.io:8093/ebics/swagger-ui/?url=/ebics/v2/api-docs/#/). We open `/ebics/api-v1/createOrder` tab and fill out Jack's details. Namely, we will `purpose` field with Jack's on-chain account and `receipientIban` field with his IBAN number. And `sourceIban` field with Alice's IBAN number. We can then specify the amount and other fields. It should look similar to this:

![Alice transfer to Jack](/assets/alice-transfers-jack-zk.png)

This will create a new order and will end up in Alice's bank statement as an outgoing transaction. And when our offchain worker queries bank statements, it will parse Jack's on-chain account from `reference` field or query it from storage using his IBAN number. Note that transfer on-chain won't happen instantly, since offchain worker performs activities within a minimum of 5 block times interval (~30 seconds) and there are 3 types of actions. So, there is around 90 seconds of time between each new bank statements processing. 

Once offchain worker has processed new statements, two `Transfer` events occur:

![Transfer from Alice to Jack](/assets/jack-to-alice-zk.png)

#### Alice transfers to Jack via Extrinsic

We go to `Extrinsic` tab, choose `fiatRamps.transfer` extrinsic call and choose `destination` as `Address`. Fill out the necessary fields and make sure that the amount is a positive number and more than 1 UNIT (10 decimals), otherwise extrinsic will fail.

![Extrinsic from Alice to Jack](/assets/alice-jack-ext-zk.png)

After we submit extrinsic, we can see that the burn request event is created.

Shortly after (approximately 3-4 blocks), we can notice that the burn request has been processed and transfer between Alice and Jack occurs. Notice that transfer occurs from an unknown wallet to Jack, not directly from Alice to Jack. This is offchain worker's account that stores the funds until transaction is finalized by LibEUfin backend.

![Extrinsic from Alice to Jack](/assets/alice-jack-event-zk.png)
 
