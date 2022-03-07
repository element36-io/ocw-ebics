/// Contains Pallet types and methods
/// Contains Transaction and IbanAccount types

use crate::*;
/// String vector bytes
pub type StrVecBytes = Vec<u8>;

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
                let value_1 = num.integer as u128 * 10_u128.pow(num.exponent as u32 + 10);
				let value_2 = num.fraction as u128 * 10_u128.pow(
					num.exponent as u32 + 10 - num.fraction_length
				);
				value_1 + value_2
            })
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
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
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo)]
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

#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, RuntimeDebug, TypeInfo)]
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
#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, RuntimeDebug, TypeInfo)]
pub struct IbanAccount {
	/// IBAN number of the account
	pub iban: StrVecBytes,
	/// Closing balance of the account
	pub balance: u128,
	/// Last time the statement was updated
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


/// Unpeq request template
/// 
/// # Arguments
/// 
/// `account_id` - Sender of the unpeq request
/// `amount` - Amount of the unpeq request
/// `iban` - IBAN of the receiver
/// `reference` - Reference of the unpeq request, we save request id in this field
pub fn unpeg_request(
	dest: &str, 
	amount: u128, 
	iban: &StrVecBytes,
	reference: &str,
) -> JsonValue {

	// First step is to convert amount to NumberValue type
	let integer = amount / 1_000_000_0000;
	let fraction = amount % 1_000_000_0000;

	// Mutable copy of `fraction` that will be used to calculate length of the fraction
	let mut fraction_copy = fraction.clone();

	let fraction_length = {
		let mut len = 0;
		
		while fraction_copy > 0 {
			fraction_copy /= 10;
			len += 1;
		}
		len
	};

	let amount_json = JsonValue::Number(NumberValue {
		integer: integer as i64,
		fraction: fraction as u64,
		fraction_length,
		exponent: 0,
	});

	let iban_json = JsonValue::String(
		iban[..].iter().map(|b| *b as char).collect::<Vec<char>>()
	);

	JsonValue::Object(
		vec![
			(
				"amount".chars().into_iter().collect(), 
				amount_json
			),
			(
				"clearingSystemMemberId".chars().into_iter().collect(),
				JsonValue::String(vec!['H', 'Y', 'P', 'L', 'C', 'H', '2', '2'])
			),
			(
				"currency".chars().into_iter().collect(), 
				JsonValue::String(vec!['E', 'U', 'R'])
			),
			(
				"nationalPayment".chars().into_iter().collect(),
				JsonValue::Boolean(true)
			),
			(
				"ourReference".chars().into_iter().collect(), 
				JsonValue::String(reference.chars().into_iter().collect())
			),
			(
				"purpose".chars().into_iter().collect(), 
				JsonValue::String(dest.chars().into_iter().collect())
			),
			(
				"receipientBankName".chars().into_iter().collect(),
				JsonValue::String(vec!['H', 'y', 'p'])
			),
			(
				"receipientCity".chars().into_iter().collect(),
				JsonValue::String(vec!['e'])
			),
			(
				"receipientCountry".chars().into_iter().collect(),
				JsonValue::String(vec!['C', 'H'])
			),
			(
				"receipientName".chars().into_iter().collect(),
				JsonValue::String(vec!['e'])
			),
			(
				"receipientIban".chars().into_iter().collect(),
				iban_json
			),
			(
				"receipientStreet".chars().into_iter().collect(),
				JsonValue::String(vec!['e'])
			),
			(
				"receipientStreetNr".chars().into_iter().collect(),
				JsonValue::String(vec!['2', '5'])
			),
			(
				"receipientZip".chars().into_iter().collect(),
				JsonValue::String(vec!['6', '3', '4', '0'])
			)
		]
	)
}
