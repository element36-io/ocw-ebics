## Fiat on/off ramp workflow

Every time offchain worker runs, it queries the `/bankstatements` endpoint to get the latest updated bank statements. 

### Bank Statement

Each bank statement has at least four fields: `iban`, `balanceCL`, 'incomingStatements' and `outgoingStatements`. A bank statement in our pallet is represented as the combination of `IbanAccount` and `Transaction` types:

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

Offchain worker fetches the new bank statements every N blocks (currently it's 4 blocks, for better testing). Whenever it detects non-empty bank statements it sends a signed transaction to call `process_statements` call of the pallet. This call can only be called by offchain worker itself.

