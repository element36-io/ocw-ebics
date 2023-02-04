use sp_std::convert::TryInto;

use crate::*;
use sp_std::{vec, vec::Vec};

/// Server response types
#[derive(Clone, Debug, PartialEq)]
pub enum ResponseTypes {
	/// Response is empty
	Empty,
	/// Response contains only one statement
	SingleStatement,
	/// Response contains multiple statements
	MultipleStatements,
}

/// Bank statement types
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StatementTypes {
	/// Bank statement contains no transactions (usual case)
	Empty,
	/// Bank statement has `incomingTransactions` field populated
	IncomingTransactions,
	/// Bank statement has `outgoingTransactions` field populated
	OutgoingTransactions,
	/// Bank statement has `incomingTransactions` and `outgoingTransactions` fields populated
	CompleteTransactions,
	/// Invalid transactions
	InvalidTransactions,
}

/// Convert string to `BoundedVec<u8, T::StringLimit>`
pub(crate) fn string_to_bounded_vec<S: Get<u32>>(string: &str) -> BoundedVec<u8, S> {
	return string.as_bytes().to_vec().try_into().expect("Do not pass more than 255 bytes");
}

/// Get mock server response
///
/// Return a tuple of (response bytes, response parsed to statement)
pub(crate) fn get_mock_response<T: Config>(
	response: ResponseTypes,
	statement: StatementTypes,
) -> (Vec<u8>, Vec<(BankAccountOf<T>, Vec<TransactionOf<T>>)>) {
	let alice_iban = string_to_bounded_vec::<T::MaxIbanLength>("CH2108307000289537320");
	let bob_iban = string_to_bounded_vec::<T::MaxIbanLength>("CH1230116000289537312");
	let charlie_iban = string_to_bounded_vec::<T::MaxIbanLength>("CH2108307000289537313");

	match response {
		ResponseTypes::Empty => {
			return (br#"[]"#.to_vec(), vec![]);
		},
		ResponseTypes::SingleStatement => {
			match statement {
				StatementTypes::Empty => {
					return (br#"[]"#.to_vec(), vec![]);
				},
				StatementTypes::IncomingTransactions => {
					// the transaction is coming from Bob to Alice
					let bytes = br#"[{"iban":"CH2108307000289537320","balanceCL":449.00,"incomingTransactions":[{"iban":"CH1230116000289537312","name":"Bob","currency":"EUR","amount":100.00,"reference":"Purp:5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY; ourRef:none"}],"outgoingTransactions":[]}]"#.to_vec();
					let parsed_statements = vec![(
						BankAccount {
							iban: alice_iban.clone(),
							balance: 4490000000000,
							last_updated: 0,
							behaviour: AccountBehaviour::Keep,
						},
						vec![
							TransactionOf::<T>{
								iban: bob_iban.clone(),
								name: string_to_bounded_vec::<T::MaxStringLength>("Bob"),
								amount: 1000000000000,
								reference: string_to_bounded_vec("Purp:5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY; ourRef:none"),
								currency: string_to_bounded_vec::<T::MaxStringLength>("EUR"),
								tx_type: TransactionType::Incoming,
							}
						]
					)];
					return (bytes, parsed_statements);
				},
				StatementTypes::OutgoingTransactions => {
					// outgoing transaction is from Bob to Alice
					let bytes = br#"[{
							"iban": "CH1230116000289537312",
							"balanceCL": 10000000,
							"incomingTransactions": [],
							"outgoingTransactions": [
								{
									"iban": "CH2108307000289537320",
									"name": "Alice",
									"currency": "EUR",
									"amount": 10000,
									"reference": "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef:none"
								}
							]
						}
					]"#
					.to_vec();
					let parsed_statements = vec![
						(
							BankAccount {
								iban: bob_iban.clone(),
								balance: 100000000000000000,
								last_updated: 0,
								behaviour: AccountBehaviour::Keep,
							},
							vec![
								Transaction{
									iban: alice_iban.clone(),
									name: string_to_bounded_vec::<T::MaxStringLength>("Alice"),
									amount: 100000000000000,
									reference: string_to_bounded_vec::<T::MaxStringLength>("Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef:none"),
									currency: string_to_bounded_vec::<T::MaxStringLength>("EUR"),
									tx_type: TransactionType::Outgoing,
								}
							]
						)
					];
					return (bytes, parsed_statements);
				},
				StatementTypes::CompleteTransactions => {
					let bytes = br#"[
						{
							"iban": "CH2108307000289537313",
							"balanceCL": 10000000,
							"incomingTransactions": [
								{
									"iban": "CH2108307000289537320",
									"name": "Alice",
									"currency": "EUR",
									"amount": 15000,
									"reference": "Purp:5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y; ourRef:none"
								}
							],
							"outgoingTransactions": [
								{
									"iban": "CH1230116000289537312",
									"name": "Bob",
									"currency": "EUR",
									"amount": 15000,
									"reference": "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef:none"
								}
							]
						}	
					]"#
					.to_vec();
					let parsed_statements = vec![
						(
							BankAccount {
								iban: charlie_iban.clone(),
								balance: 100000000000000000,
								last_updated: 0,
								behaviour: AccountBehaviour::Keep,
							},
							vec![
								Transaction{
									iban: alice_iban.clone(),
									name: string_to_bounded_vec::<T::MaxStringLength>("Alice"),
									amount: 150000000000000,
									reference: string_to_bounded_vec::<T::MaxStringLength>("Purp:5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y; ourRef: none"),
									currency: string_to_bounded_vec::<T::MaxStringLength>("EUR"),
									tx_type: TransactionType::Incoming,
								},
								Transaction{
									iban: bob_iban.clone(),
									name: string_to_bounded_vec::<T::MaxStringLength>("Bob"),
									amount: 150000000000000,
									reference: string_to_bounded_vec::<T::MaxStringLength>("Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef: none"),
									currency: string_to_bounded_vec::<T::MaxStringLength>("EUR"),
									tx_type: TransactionType::Outgoing,
								}
							]
						)
					];
					return (bytes, parsed_statements);
				},
				StatementTypes::InvalidTransactions => {
					let bytes = br#"[
						{
							"iban": "CH2108307000289537313",
							"balanceCL": 10000000,
							"incomingTransactions": [
								{
									"iban": "None",
									"name": "Alice",
									"currency": "EUR",
									"amount": 15000,
									"reference": "Purp:None; ourRef: none"
								}
							],
						}
					]"#
					.to_vec();
					let parsed_statements = vec![(
						BankAccount {
							iban: charlie_iban.clone(),
							balance: 100000000000000000,
							last_updated: 0,
							behaviour: AccountBehaviour::Keep,
						},
						vec![Transaction {
							iban: string_to_bounded_vec::<T::MaxIbanLength>(
								"0000000000000000000000000000",
							),
							name: string_to_bounded_vec::<T::MaxStringLength>("Alice"),
							amount: 150000000000000,
							reference: string_to_bounded_vec::<T::MaxStringLength>(
								"Purp:None; ourRef: none",
							),
							currency: string_to_bounded_vec::<T::MaxStringLength>("EUR"),
							tx_type: TransactionType::Incoming,
						}],
					)];
					return (bytes, parsed_statements);
				},
			}
		},
		ResponseTypes::MultipleStatements => {
			let bytes = br#"[
				{
					"iban": "CH2108307000289537313",
					"balanceCL": 10000000,
					"incomingTransactions": [
						{
							"iban": "CH2108307000289537320",
							"name": "Alice",
							"currency": "EUR",
							"amount": 15000,
							"reference": "Purp:5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y; ourRef:none"
						}
					],
					"outgoingTransactions": [
						{
							"iban": "CH1230116000289537312",
							"name": "Bob",
							"currency": "EUR",
							"amount": 15000,
							"reference": "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef:none"
						}
					]
				},
				{
					"iban": "CH1230116000289537312",
					"balanceCL": 10000000,
					"incomingTransactions": [
						{
							"iban": "CH2108307000289537320",
							"name": "Alice",
							"currency": "EUR",
							"amount": 15000,
							"reference": "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef:none"
						}
					],
					"outgoingTransactions": [
						{
							"iban": "CH2108307000289537313",
							"name": "Charlie",
							"currency": "EUR",
							"amount": 15000,
							"reference": "Purp:5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y; ourRef:none"
						}
					]
				},
				{
					"iban": "CH2108307000289537320",
					"balanceCL": 10000000,
					"incomingTransactions": [
						{
							"iban": "CH1230116000289537312",
							"name": "Bob",
							"currency": "EUR",
							"amount": 5000,
							"reference": "Purp:5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY; ourRef:none"
						},
						{
							"iban": "CH1230116000289537312",
							"name": "Bob",
							"currency": "EUR",
							"amount": 10000,
							"reference": "Purp:5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY; ourRef:none"
						}
					],
					"outgoingTransactions": [
						{
							"iban": "CH1230116000289537312",
							"name": "Bob",
							"currency": "EUR",
							"amount": 15000,
							"reference": "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef:none"
						}
					]
				}
			]"#
			.to_vec();

			let parsed_statements = vec![
                (
                    BankAccount {
                        iban: charlie_iban.clone(),
                        balance: 100000000000000000,
                        last_updated: 0,
						behaviour: AccountBehaviour::Keep
                    },
                    vec![
                        Transaction {
                            iban: bob_iban.clone(),
                            name: string_to_bounded_vec::<T::MaxStringLength>("Bob"),
                            amount: 150000000000000,
                            reference: string_to_bounded_vec::<T::MaxStringLength>("Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef:none"),
                            currency: string_to_bounded_vec::<T::MaxStringLength>("EUR"),
                            tx_type: TransactionType::Outgoing,
                        },
						Transaction {
                            iban: alice_iban.clone(),
                            name: string_to_bounded_vec::<T::MaxStringLength>("Alice"),
                            amount: 150000000000000,
                            reference: string_to_bounded_vec::<T::MaxStringLength>("Purp:5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y; ourRef:none"),
                            currency: string_to_bounded_vec::<T::MaxStringLength>("EUR"),
                            tx_type: TransactionType::Incoming,
                        },
                    ]
                ),
                (
                    BankAccount {
                        iban: bob_iban.clone(),
                        balance: 100000000000000000,
                        last_updated: 0,
						behaviour: AccountBehaviour::Keep
                    },
                    vec![
                        Transaction {
                            iban: charlie_iban.clone(),
                            name: string_to_bounded_vec::<T::MaxStringLength>("Charlie"),
                            amount: 150000000000000,
                            reference: string_to_bounded_vec::<T::MaxStringLength>("Purp:5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y; ourRef:none"),
                            currency: string_to_bounded_vec::<T::MaxStringLength>("EUR"),
                            tx_type: TransactionType::Outgoing,
                        },
						Transaction {
                            iban: alice_iban.clone(),
                            name: string_to_bounded_vec::<T::MaxStringLength>("Alice"),
                            amount: 150000000000000,
                            reference: string_to_bounded_vec::<T::MaxStringLength>("Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef:none"),
                            currency: string_to_bounded_vec::<T::MaxStringLength>("EUR"),
                            tx_type: TransactionType::Incoming,
                        },
                    ]
                ),
                (
                    BankAccount {
                        iban: alice_iban.clone(),
                        balance: 100000000000000000,
                        last_updated: 0,
						behaviour: AccountBehaviour::Keep
                    },
                    vec![
						Transaction {
                            iban: bob_iban.clone(),
                            name: string_to_bounded_vec::<T::MaxStringLength>("Bob"),
                            amount: 150000000000000,
                            reference: string_to_bounded_vec::<T::MaxStringLength>("Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef:none"),
                            currency: string_to_bounded_vec::<T::MaxStringLength>("EUR"),
                            tx_type: TransactionType::Outgoing,
                        },
						Transaction {
                            iban: bob_iban.clone(),
                            name: string_to_bounded_vec::<T::MaxStringLength>("Bob"),
                            amount: 50000000000000,
                            reference: string_to_bounded_vec::<T::MaxStringLength>("Purp:5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY; ourRef:none"),
                            currency: string_to_bounded_vec::<T::MaxStringLength>("EUR"),
                            tx_type: TransactionType::Incoming,
                        },
						Transaction {
                            iban: bob_iban.clone(),
                            name: string_to_bounded_vec::<T::MaxStringLength>("Bob"),
                            amount: 100000000000000,
                            reference: string_to_bounded_vec::<T::MaxStringLength>("Purp:5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY; ourRef:none"),
                            currency: string_to_bounded_vec::<T::MaxStringLength>("EUR"),
                            tx_type: TransactionType::Incoming,
                        },
                    ]
                )
            ];
			return (bytes, parsed_statements);
		},
	}
}
