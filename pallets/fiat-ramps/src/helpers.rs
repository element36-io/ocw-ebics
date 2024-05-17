use sp_runtime::traits::One;
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
) -> (Vec<u8>, Option<QueuedStatementsInfoOf<T>>) {
	let alice_iban = string_to_bounded_vec::<T::MaxIbanLength>("CH2108307000289537320");
	let bob_iban = string_to_bounded_vec::<T::MaxIbanLength>("CH1230116000289537312");
	let charlie_iban = string_to_bounded_vec::<T::MaxIbanLength>("CH2108307000289537313");

	match response {
		ResponseTypes::Empty => {
			return (br#"[]"#.to_vec(), None);
		},
		ResponseTypes::SingleStatement => {
			match statement {
				StatementTypes::Empty => {
					return (br#"[]"#.to_vec(), None);
				},
				StatementTypes::IncomingTransactions => {
					// the transaction is coming from Bob to Alice
					let bytes = br#"[{
						"iban":"CH2108307000289537320","receiptUrl":"abcd.json","balanceCL":449.00,"incomingTransactions":[{"iban":"CH1230116000289537312","name":"Bob","currency":"EUR","amount":100.00,"reference":"Purp:5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY; ourRef:none"}],"outgoingTransactions":[]}]"#.to_vec();
					let parsed_statements = vec![(
						BankAccount {
							iban: alice_iban.clone(),
							balance: 4490000000000,
							last_updated: 0,
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
						].try_into().unwrap(),
					)];
					return (
						bytes,
						Some(QueuedStatementsInfoOf::<T> {
							block_number: One::one(),
							statements: parsed_statements.try_into().unwrap(),
							receipt_url: b"abcd.json".to_vec().try_into().unwrap(),
						}),
					);
				},
				StatementTypes::OutgoingTransactions => {
					// outgoing transaction is from Bob to Alice
					let bytes = br#"[{
							"iban": "CH1230116000289537312",
							"receiptUrl": "abcd.json",
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
						].try_into().unwrap(),
						)
					];
					return (
						bytes,
						Some(QueuedStatementsInfoOf::<T> {
							block_number: One::one(),
							statements: parsed_statements.try_into().unwrap(),
							receipt_url: b"abcd.json".to_vec().try_into().unwrap(),
						}),
					);
				},
				StatementTypes::CompleteTransactions => {
					let bytes = br#"[
						{
							"iban": "CH2108307000289537313",
							"balanceCL": 10000000,
							"receiptUrl": "abcd.json",
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
							].try_into().unwrap(),
						)
					];
					return (
						bytes,
						Some(QueuedStatementsInfoOf::<T> {
							block_number: One::one(),
							statements: parsed_statements.try_into().unwrap(),
							receipt_url: b"abcd.json".to_vec().try_into().unwrap(),
						}),
					);
				},
				StatementTypes::InvalidTransactions => {
					let bytes = br#"[
						{
							"iban": "CH2108307000289537313",
							"balanceCL": 10000000,
							"receiptUrl": "abcd.json",
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
						}]
						.try_into()
						.unwrap(),
					)];
					return (
						bytes,
						Some(QueuedStatementsInfoOf::<T> {
							block_number: One::one(),
							statements: parsed_statements.try_into().unwrap(),
							receipt_url: b"abcd.json".to_vec().try_into().unwrap(),
						}),
					);
				},
			}
		},
		ResponseTypes::MultipleStatements => {
			let bytes = br#"[
				{
					"iban": "CH2108307000289537313",
					"balanceCL": 10000000,
					"receiptUrl": "abcd.json",
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
					"receiptUrl": "abcde.json",
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
					"receiptUrl": "abcdef.json",
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
                    ].try_into().unwrap(),
                ),
                (
                    BankAccount {
                        iban: bob_iban.clone(),
                        balance: 100000000000000000,
                        last_updated: 0,
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
                    ].try_into().unwrap(),
                ),
                (
                    BankAccount {
                        iban: alice_iban.clone(),
                        balance: 100000000000000000,
                        last_updated: 0,
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
                    ].try_into().unwrap(),
                )
            ];
			return (
				bytes,
				Some(QueuedStatementsInfoOf::<T> {
					block_number: One::one(),
					statements: parsed_statements.try_into().unwrap(),
					receipt_url: b"abcd.json".to_vec().try_into().unwrap(),
				}),
			);
		},
	}
}

pub(crate) fn get_mock_receipt() -> (Vec<u8>, Receipt) {
	let bytes = br#"{"inner":{"Fake":{"claim":{"pre":{"Value":{"pc":2464100,"merkle_root":[3568380161,2516590490,672836492,3447306562,1550888505,3425089559,3327538353,1021151634]}},"post":{"Value":{"pc":2467860,"merkle_root":[4091246138,1465949765,2210553808,715373872,1116733518,3068971712,24521862,1313121345]}},"exit_code":{"Halted":0},"input":[0,0,0,0,0,0,0,0],"output":{"Value":{"journal":{"Value":[225,5,0,0,123,34,104,111,115,116,105,110,102,111,34,58,34,104,111,115,116,58,109,97,105,110,34,44,34,105,98,97,110,34,58,34,67,72,52,51,48,56,51,48,55,48,48,48,50,56,57,53,51,55,51,49,50,34,44,34,112,117,98,95,98,97,110,107,95,112,101,109,34,58,34,45,45,45,45,45,66,69,71,73,78,32,80,85,66,76,73,67,32,75,69,89,45,45,45,45,45,92,110,77,73,73,66,73,106,65,78,66,103,107,113,104,107,105,71,57,119,48,66,65,81,69,70,65,65,79,67,65,81,56,65,77,73,73,66,67,103,75,67,65,81,69,65,105,73,88,56,103,104,119,106,108,75,101,70,79,57,90,70,109,50,84,85,92,110,114,80,72,90,72,110,114,85,78,83,102,66,110,86,111,107,89,68,78,100,107,110,82,43,76,68,70,114,116,55,78,68,65,86,104,88,66,85,70,117,119,56,74,112,77,66,72,69,119,50,75,65,56,80,52,110,106,106,70,89,54,112,104,52,92,110,70,78,99,103,89,116,97,72,52,102,104,79,98,90,89,69,54,73,55,120,122,68,72,69,49,51,111,74,112,120,66,84,49,121,104,121,108,103,85,116,79,71,82,54,54,107,119,54,101,119,74,122,81,50,50,107,97,47,86,119,51,104,103,118,92,110,100,117,43,108,65,66,67,113,89,74,56,87,69,81,56,90,119,81,55,85,114,87,50,88,73,110,115,85,98,65,52,67,98,109,80,110,50,72,54,118,110,108,90,52,55,54,97,51,110,106,73,73,109,110,117,75,117,48,106,51,66,100,116,66,92,110,113,108,77,70,117,54,116,78,103,55,71,52,74,114,57,81,70,71,57,71,43,50,83,72,105,49,112,100,107,111,108,66,86,108,105,108,118,99,99,121,50,78,69,81,67,97,54,89,79,106,106,66,97,97,54,52,80,50,80,81,77,57,104,110,92,110,48,100,80,112,117,120,83,116,100,83,74,113,70,110,71,88,80,106,48,90,101,47,51,83,116,67,85,105,113,71,79,68,73,122,88,97,56,72,84,56,115,66,87,85,114,104,50,120,82,99,74,73,105,103,54,82,51,54,55,43,81,53,70,122,92,110,74,119,73,68,65,81,65,66,92,110,45,45,45,45,45,69,78,68,32,80,85,66,76,73,67,32,75,69,89,45,45,45,45,45,92,110,34,44,34,112,117,98,95,119,105,116,110,101,115,115,95,112,101,109,34,58,34,45,45,45,45,45,66,69,71,73,78,32,80,85,66,76,73,67,32,75,69,89,45,45,45,45,45,92,110,77,73,73,66,73,106,65,78,66,103,107,113,104,107,105,71,57,119,48,66,65,81,69,70,65,65,79,67,65,81,56,65,77,73,73,66,67,103,75,67,65,81,69,65,110,103,110,111,76,101,99,51,81,87,122,72,107,103,71,87,55,85,106,50,92,110,105,50,121,70,112,56,54,75,68,117,75,114,70,85,117,115,54,112,88,72,74,109,67,110,90,73,76,84,65,79,105,75,122,78,67,65,66,53,113,73,68,66,119,97,57,104,53,48,47,79,84,90,54,112,118,49,88,53,109,103,86,77,50,83,92,110,80,78,75,118,90,111,85,114,102,79,85,54,74,103,53,109,49,98,51,71,107,121,76,106,47,51,65,102,100,83,43,110,74,98,106,85,88,70,108,121,77,87,73,105,53,99,50,54,87,118,118,87,50,70,115,113,115,69,111,101,104,65,71,70,92,110,81,112,117,114,90,86,54,81,75,87,83,75,69,107,49,54,84,75,111,73,50,107,99,68,56,115,69,65,85,98,53,84,86,119,120,43,55,68,53,107,122,56,90,103,85,88,48,103,47,75,113,77,43,111,50,107,85,120,66,105,83,75,100,83,92,110,49,112,57,67,68,69,104,119,87,87,101,48,77,82,48,106,97,52,69,104,54,43,112,70,121,73,73,106,86,115,114,121,98,66,57,117,102,66,117,117,66,67,51,49,114,101,100,70,71,90,52,110,66,88,52,51,120,116,115,53,68,111,54,90,92,110,54,51,85,49,108,88,49,53,103,78,105,74,116,86,120,108,100,66,102,75,109,57,111,50,111,102,80,77,120,100,80,117,51,75,88,69,103,55,102,51,90,109,50,110,57,101,65,49,70,120,85,75,117,114,119,97,99,55,97,51,49,86,56,100,92,110,76,119,73,68,65,81,65,66,92,110,45,45,45,45,45,69,78,68,32,80,85,66,76,73,67,32,75,69,89,45,45,45,45,45,92,110,34,44,34,112,117,98,95,99,108,105,101,110,116,95,112,101,109,34,58,34,45,45,45,45,45,66,69,71,73,78,32,80,85,66,76,73,67,32,75,69,89,45,45,45,45,45,92,110,77,73,73,66,73,106,65,78,66,103,107,113,104,107,105,71,57,119,48,66,65,81,69,70,65,65,79,67,65,81,56,65,77,73,73,66,67,103,75,67,65,81,69,65,105,101,113,84,57,119,114,114,73,115,83,105,107,114,77,55,86,76,88,104,92,110,107,101,49,119,70,49,51,80,75,108,48,114,65,80,101,109,71,102,73,99,80,55,78,75,101,100,47,80,71,122,106,50,121,67,76,110,103,108,105,102,76,76,47,117,79,71,101,54,70,55,54,102,118,83,97,49,86,68,86,112,117,80,110,89,92,110,84,55,81,85,103,56,106,90,79,65,43,120,103,97,65,106,117,49,47,108,107,113,117,48,105,114,103,43,57,111,53,101,50,117,110,87,112,113,110,118,118,81,56,97,97,117,113,51,56,83,101,67,43,114,100,75,78,82,102,57,57,86,109,118,92,110,97,65,66,97,117,69,119,84,48,111,108,106,86,115,43,109,50,120,78,43,120,115,88,83,122,82,89,118,98,72,97,66,86,49,53,103,74,55,55,88,111,70,57,55,51,71,102,54,82,109,43,98,98,79,86,90,99,78,98,107,73,106,117,110,92,110,110,75,70,114,67,85,79,82,104,66,116,77,53,43,98,79,83,106,68,87,69,52,105,105,113,48,111,82,83,101,75,103,88,100,66,118,71,117,87,89,49,122,115,116,76,114,75,108,52,82,77,77,99,122,76,84,54,89,85,77,118,50,105,66,92,110,87,53,74,81,105,69,74,74,76,88,65,77,113,80,100,84,110,100,76,108,90,57,71,122,52,102,51,104,56,103,99,98,72,69,54,77,104,113,65,88,110,67,51,66,70,87,98,71,104,111,76,47,116,116,84,112,100,117,71,114,77,107,116,56,92,110,88,81,73,68,65,81,65,66,92,110,45,45,45,45,45,69,78,68,32,80,85,66,76,73,67,32,75,69,89,45,45,45,45,45,92,110,34,44,34,115,116,109,116,115,34,58,91,93,125,0,0,0]},"assumptions":{"Value":[]}}}}}},"journal":{"bytes":[225,5,0,0,123,34,104,111,115,116,105,110,102,111,34,58,34,104,111,115,116,58,109,97,105,110,34,44,34,105,98,97,110,34,58,34,67,72,52,51,48,56,51,48,55,48,48,48,50,56,57,53,51,55,51,49,50,34,44,34,112,117,98,95,98,97,110,107,95,112,101,109,34,58,34,45,45,45,45,45,66,69,71,73,78,32,80,85,66,76,73,67,32,75,69,89,45,45,45,45,45,92,110,77,73,73,66,73,106,65,78,66,103,107,113,104,107,105,71,57,119,48,66,65,81,69,70,65,65,79,67,65,81,56,65,77,73,73,66,67,103,75,67,65,81,69,65,105,73,88,56,103,104,119,106,108,75,101,70,79,57,90,70,109,50,84,85,92,110,114,80,72,90,72,110,114,85,78,83,102,66,110,86,111,107,89,68,78,100,107,110,82,43,76,68,70,114,116,55,78,68,65,86,104,88,66,85,70,117,119,56,74,112,77,66,72,69,119,50,75,65,56,80,52,110,106,106,70,89,54,112,104,52,92,110,70,78,99,103,89,116,97,72,52,102,104,79,98,90,89,69,54,73,55,120,122,68,72,69,49,51,111,74,112,120,66,84,49,121,104,121,108,103,85,116,79,71,82,54,54,107,119,54,101,119,74,122,81,50,50,107,97,47,86,119,51,104,103,118,92,110,100,117,43,108,65,66,67,113,89,74,56,87,69,81,56,90,119,81,55,85,114,87,50,88,73,110,115,85,98,65,52,67,98,109,80,110,50,72,54,118,110,108,90,52,55,54,97,51,110,106,73,73,109,110,117,75,117,48,106,51,66,100,116,66,92,110,113,108,77,70,117,54,116,78,103,55,71,52,74,114,57,81,70,71,57,71,43,50,83,72,105,49,112,100,107,111,108,66,86,108,105,108,118,99,99,121,50,78,69,81,67,97,54,89,79,106,106,66,97,97,54,52,80,50,80,81,77,57,104,110,92,110,48,100,80,112,117,120,83,116,100,83,74,113,70,110,71,88,80,106,48,90,101,47,51,83,116,67,85,105,113,71,79,68,73,122,88,97,56,72,84,56,115,66,87,85,114,104,50,120,82,99,74,73,105,103,54,82,51,54,55,43,81,53,70,122,92,110,74,119,73,68,65,81,65,66,92,110,45,45,45,45,45,69,78,68,32,80,85,66,76,73,67,32,75,69,89,45,45,45,45,45,92,110,34,44,34,112,117,98,95,119,105,116,110,101,115,115,95,112,101,109,34,58,34,45,45,45,45,45,66,69,71,73,78,32,80,85,66,76,73,67,32,75,69,89,45,45,45,45,45,92,110,77,73,73,66,73,106,65,78,66,103,107,113,104,107,105,71,57,119,48,66,65,81,69,70,65,65,79,67,65,81,56,65,77,73,73,66,67,103,75,67,65,81,69,65,110,103,110,111,76,101,99,51,81,87,122,72,107,103,71,87,55,85,106,50,92,110,105,50,121,70,112,56,54,75,68,117,75,114,70,85,117,115,54,112,88,72,74,109,67,110,90,73,76,84,65,79,105,75,122,78,67,65,66,53,113,73,68,66,119,97,57,104,53,48,47,79,84,90,54,112,118,49,88,53,109,103,86,77,50,83,92,110,80,78,75,118,90,111,85,114,102,79,85,54,74,103,53,109,49,98,51,71,107,121,76,106,47,51,65,102,100,83,43,110,74,98,106,85,88,70,108,121,77,87,73,105,53,99,50,54,87,118,118,87,50,70,115,113,115,69,111,101,104,65,71,70,92,110,81,112,117,114,90,86,54,81,75,87,83,75,69,107,49,54,84,75,111,73,50,107,99,68,56,115,69,65,85,98,53,84,86,119,120,43,55,68,53,107,122,56,90,103,85,88,48,103,47,75,113,77,43,111,50,107,85,120,66,105,83,75,100,83,92,110,49,112,57,67,68,69,104,119,87,87,101,48,77,82,48,106,97,52,69,104,54,43,112,70,121,73,73,106,86,115,114,121,98,66,57,117,102,66,117,117,66,67,51,49,114,101,100,70,71,90,52,110,66,88,52,51,120,116,115,53,68,111,54,90,92,110,54,51,85,49,108,88,49,53,103,78,105,74,116,86,120,108,100,66,102,75,109,57,111,50,111,102,80,77,120,100,80,117,51,75,88,69,103,55,102,51,90,109,50,110,57,101,65,49,70,120,85,75,117,114,119,97,99,55,97,51,49,86,56,100,92,110,76,119,73,68,65,81,65,66,92,110,45,45,45,45,45,69,78,68,32,80,85,66,76,73,67,32,75,69,89,45,45,45,45,45,92,110,34,44,34,112,117,98,95,99,108,105,101,110,116,95,112,101,109,34,58,34,45,45,45,45,45,66,69,71,73,78,32,80,85,66,76,73,67,32,75,69,89,45,45,45,45,45,92,110,77,73,73,66,73,106,65,78,66,103,107,113,104,107,105,71,57,119,48,66,65,81,69,70,65,65,79,67,65,81,56,65,77,73,73,66,67,103,75,67,65,81,69,65,105,101,113,84,57,119,114,114,73,115,83,105,107,114,77,55,86,76,88,104,92,110,107,101,49,119,70,49,51,80,75,108,48,114,65,80,101,109,71,102,73,99,80,55,78,75,101,100,47,80,71,122,106,50,121,67,76,110,103,108,105,102,76,76,47,117,79,71,101,54,70,55,54,102,118,83,97,49,86,68,86,112,117,80,110,89,92,110,84,55,81,85,103,56,106,90,79,65,43,120,103,97,65,106,117,49,47,108,107,113,117,48,105,114,103,43,57,111,53,101,50,117,110,87,112,113,110,118,118,81,56,97,97,117,113,51,56,83,101,67,43,114,100,75,78,82,102,57,57,86,109,118,92,110,97,65,66,97,117,69,119,84,48,111,108,106,86,115,43,109,50,120,78,43,120,115,88,83,122,82,89,118,98,72,97,66,86,49,53,103,74,55,55,88,111,70,57,55,51,71,102,54,82,109,43,98,98,79,86,90,99,78,98,107,73,106,117,110,92,110,110,75,70,114,67,85,79,82,104,66,116,77,53,43,98,79,83,106,68,87,69,52,105,105,113,48,111,82,83,101,75,103,88,100,66,118,71,117,87,89,49,122,115,116,76,114,75,108,52,82,77,77,99,122,76,84,54,89,85,77,118,50,105,66,92,110,87,53,74,81,105,69,74,74,76,88,65,77,113,80,100,84,110,100,76,108,90,57,71,122,52,102,51,104,56,103,99,98,72,69,54,77,104,113,65,88,110,67,51,66,70,87,98,71,104,111,76,47,116,116,84,112,100,117,71,114,77,107,116,56,92,110,88,81,73,68,65,81,65,66,92,110,45,45,45,45,45,69,78,68,32,80,85,66,76,73,67,32,75,69,89,45,45,45,45,45,92,110,34,44,34,115,116,109,116,115,34,58,91,93,125,0,0,0]}}"#;
	let receipt = serde_json_core::from_slice(&bytes.as_slice()).unwrap();

	(bytes.to_vec(), receipt.0)
}
