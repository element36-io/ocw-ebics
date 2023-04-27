//! Implementations of traits and types for the pallet
use core::convert::TryInto;

use crate::types::*;
use crate::*;
use sp_std::default::Default;

use self::utils::{extract_value, parse_object};

impl<T: SigningTypes> SignedPayload<T> for Payload<T::Public> {
	fn public(&self) -> T::Public {
		self.public.clone()
	}
}

impl Default for OcwActivity {
	fn default() -> Self {
		OcwActivity::None
	}
}

impl From<u32> for OcwActivity {
	fn from(activity: u32) -> Self {
		match activity {
			0 => OcwActivity::ProcessStatements,
			1 => OcwActivity::ProcessBurnRequests,
			_ => OcwActivity::None,
		}
	}
}

impl Default for TransactionType {
	fn default() -> Self {
		Self::None
	}
}

/// Implement `Deserialize` for common types
impl Deserialize<Vec<u8>> for Vec<u8> {
	fn deserialize(json: &JsonValue) -> Option<Vec<u8>> {
		json.clone().to_string().map(|v| v.iter().map(|c| *c as u8).collect::<Vec<_>>())
	}
}

impl Deserialize<u128> for u128 {
	fn deserialize(json: &JsonValue) -> Option<u128> {
		json.clone().to_number().map(|num| {
			let value_1 = num.integer as u128 * 10_u128.pow(num.exponent as u32 + 10);
			let value_2 =
				num.fraction as u128 * 10_u128.pow(num.exponent as u32 + 10 - num.fraction_length);
			value_1 + value_2
		})
	}
}

/// Functions of `Transaction<T>` type
impl<MaxIbanLength: Get<u32>, MaxStringLength: Get<u32>>
	Transaction<MaxIbanLength, MaxStringLength>
{
	// Get single transaction instance from json
	pub fn from_json_statement(json: &JsonValue, tx_type: &TransactionType) -> Option<Self> {
		if let Some(obj) = json.as_object() {
			let iban = extract_value::<Vec<u8>>("iban", obj).try_into().expect("Invalid IBAN");
			let name = extract_value::<Vec<u8>>("name", obj).try_into().expect("Invalid name");
			let currency =
				extract_value::<Vec<u8>>("currency", obj).try_into().expect("Invalid currency");
			let amount = extract_value::<u128>("amount", obj);
			let reference = extract_value::<Vec<u8>>("reference", obj)
				.try_into()
				.expect("Invalid reference");

			Some(Self { iban, name, currency, amount, reference, tx_type: *tx_type })
		} else {
			None
		}
	}

	/// Parse multiple transactions from `JsonValue`
	pub fn parse_transactions(
		json: &JsonValue,
		transaction_type: TransactionType,
	) -> Option<Vec<Self>> {
		// Get the key string for the transaction type
		let key_string = match transaction_type {
			TransactionType::Incoming => "incomingTransactions",
			TransactionType::Outgoing => "outgoingTransactions",
			_ => return None,
		};

		let mut transactions = Vec::new();

		match json {
			JsonValue::Object(obj) => match parse_object(key_string, &obj) {
				JsonValue::Array(txs) => {
					for json_tx in txs {
						if let Some(tx) = Self::from_json_statement(&json_tx, &transaction_type) {
							transactions.push(tx);
						}
					}
				},
				_ => {},
			},
			_ => {},
		};

		Some(transactions)
	}
}

impl<MaxLength: Get<u32>> From<&Iban<MaxLength>> for BankAccount<MaxLength> {
	fn from(iban: &Iban<MaxLength>) -> Self {
		Self { iban: iban.clone(), balance: 0, last_updated: 0 }
	}
}

impl<MaxLength: Get<u32>> TryFrom<&JsonValue> for BankAccount<MaxLength> {
	type Error = ();

	fn try_from(json: &JsonValue) -> Result<Self, Self::Error> {
		let raw_object = json.as_object();
		if let Some(obj) = raw_object {
			let iban = extract_value::<Vec<u8>>("iban", obj).try_into().unwrap_or_default();
			let balance = extract_value::<u128>("balanceCL", obj);

			Ok(Self { iban, balance, last_updated: 0 })
		} else {
			Err(())
		}
	}
}
