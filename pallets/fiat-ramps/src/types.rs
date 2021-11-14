/// Contains Pallet types and methods
/// Contains Transaction and IbanAccount types

use crate::*;
/// String vector bytes
pub type StrVecBytes = Vec<u8>;

/// Iban balance type
pub type IbanBalance = (StrVecBytes, u64);

pub trait Deserialize<T> {
    fn deserialize(value: &JsonValue) -> Option<T>;
}

impl Deserialize<Vec<u8>> for Vec<u8> {
    fn deserialize(json: &JsonValue) -> Option<Vec<u8>> {
        json.clone()
            .to_string()
            .map(|v| v.iter().map(|c| *c as u8).collect::<Vec<_>>())
    }
}

impl Deserialize<u128> for u128 {
    fn deserialize(json: &JsonValue) -> Option<u128> {
        json.clone()
            .to_number()
            .map(|num| {
                let exp = num.fraction_length.checked_sub(2).unwrap_or(0);
                let balance = num.integer as u128 + (num.fraction / 10_u64.pow(exp)) as u128;
                balance
            })
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub(crate) struct Payload<Public> {
	number: u64,
	public: Public,
}

impl<T: SigningTypes> SignedPayload<T> for Payload<T::Public> {
	fn public(&self) -> T::Public {
		self.public.clone()
	}
}

/// Utility function for extracting value from json object
///
/// parse value of a given key from json object
pub fn extract_value<T: Deserialize<T> + Default>(key: &str, obj: &[(Vec<char>, lite_json::JsonValue)]) -> T {
	let (_, v) = obj.into_iter().find(|(k, _)| k.iter().copied().eq(key.chars())).unwrap();
    if let Some(value) = T::deserialize(v) {
        return value;
    }
    Default::default()
}

/// Utility function for parsing value from json object
pub fn parse_object(key: &str, obj: &[(Vec<char>, lite_json::JsonValue)]) -> JsonValue {
	let raw_object = obj.into_iter().find(|(k, _)| k.iter().copied().eq(key.chars()));
    if let Some((_, v)) = raw_object {
        return v.clone();
    }
    JsonValue::Null
}


/// 
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
pub enum TransactionType {
	Incoming,
	Outgoing,
	None
}

impl Default for TransactionType {
	fn default() -> Self {
		Self::None
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, RuntimeDebug)]
pub struct Transaction {
	// from
	pub iban: StrVecBytes,
	pub name: StrVecBytes,
	pub currency: StrVecBytes,
	pub amount: u128,
	// to
	pub reference: StrVecBytes,
	pub tx_type: TransactionType
}

impl Transaction {
    pub fn new(
        iban: StrVecBytes,
        name: StrVecBytes,
        currency: StrVecBytes,
        amount: u128,
        reference: StrVecBytes,
        tx_type: TransactionType
    ) -> Self {
        Self {
            iban,
            name,
            currency,
            amount,
            reference,
            tx_type
        }
    }

	// Get single transaction instance from json
	pub fn from_json_statement(json: &JsonValue, tx_type: &TransactionType) -> Option<Self> {
		let raw_object = json.as_object();
        let transaction = match raw_object {
            Some(obj) => {
                let iban = extract_value::<StrVecBytes>("iban", obj);
                let name = extract_value::<StrVecBytes>("name", obj);
                let currency = extract_value::<StrVecBytes>("currency", obj);
                let amount = extract_value::<u128>("amount", obj);
                let reference = extract_value::<StrVecBytes>("reference", obj);
                Self::new(
                    iban,
                    name,
                    currency,
                    amount,
                    reference,
                    *tx_type
                )
            },
            None => return None,
        };
        
		Some(transaction)
	}
	
	/// Parse multiple transactions from JsonValue
	pub fn parse_transactions(json: &JsonValue, transaction_type: TransactionType) -> Option<Vec<Self>> {
		let parsed_transactions = match json {
			JsonValue::Object(obj) => {
				let transactions = match transaction_type {
					TransactionType::Incoming => {
						let incoming_transactions = match parse_object("incomingTransactions", obj) {
							JsonValue::Array(txs) => {
								txs.iter().map(|json| Self::from_json_statement(json, &transaction_type).unwrap_or(Default::default())).collect::<Vec<Transaction>>()
							}
							_ => return None,
						};
						incoming_transactions
					},
					TransactionType::Outgoing => {
						let outgoing_transactions = match parse_object("outgoingTransactions", obj) {
							JsonValue::Array(txs) => {
								txs.iter().map(|json| Self::from_json_statement(json, &transaction_type).unwrap_or(Default::default())).collect::<Vec<Transaction>>()
							}
							_ => return None,
						};
						outgoing_transactions
					},
					_ => Default::default()
				};
				transactions
			},
			_ => return None
		};
		Some(parsed_transactions)
	}
}

/// IbanAccount Type contains the basic information of an account
/// 
#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, RuntimeDebug)]
pub struct IbanAccount {
	pub iban: StrVecBytes,
	pub balance: u128,
	pub last_updated: u64
}

impl IbanAccount {
    pub fn new (
        iban: StrVecBytes,
        balance: u128,
        last_updated: u64
    ) -> Self {
        Self {
            iban,
            balance,
            last_updated
        }
    }
	pub fn from_json_value(json: &JsonValue) -> Option<Self> {
        let raw_object = json.as_object();
		let iban_account = match raw_object {
			Some(obj) => {
				let iban = extract_value::<StrVecBytes>("iban", obj);
				let balance = extract_value::<u128>("balanceCL", obj);

				Self::new(
					iban,
					balance,
					0
                )
			},
			_ => return None,
		};
		Some(iban_account)
	}
}
