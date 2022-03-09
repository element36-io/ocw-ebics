use codec::Decode;
use frame_support::assert_ok;
use std::sync::Arc;
use sp_core::{
    offchain::{testing, OffchainWorkerExt, TransactionPoolExt},
};
use sp_keystore::{SyncCryptoStore, KeystoreExt};
use sp_runtime::{ 
	RuntimeAppPublic, offchain::http::PendingRequest
};
use httpmock::{
	MockServer, Method::{GET, POST},
};
use mock_server::simulate_standalone_server;
use lite_json::Serialize;

use crate::{types::{
	Transaction, IbanAccount, unpeg_request,
	TransactionType,
}, BurnRequestStatus};
use crate::helpers::{
	ResponseTypes, StatementTypes,
	get_mock_response,
};

use crate::mock::*;

fn test_processing(
    statement_type: StatementTypes,
    response_type: ResponseTypes,
) {
    let (offchain, state) = testing::TestOffchainExt::new();
    let (pool, pool_state) = testing::TestTransactionPoolExt::new();
    let keystore = sp_keystore::testing::KeyStore::new();

    SyncCryptoStore::sr25519_generate_new(
        &keystore,
        crate::crypto::Public::ID,
        Some(&format!("{}/alice", "cup swing hill dinner pioneer mom stick steel sad raven oak practice")),
    ).unwrap();

    let mut t = new_test_ext(); 

	t.register_extension(OffchainWorkerExt::new(offchain));
    t.register_extension(TransactionPoolExt::new(pool));
    t.register_extension(KeystoreExt(Arc::new(keystore)));

	simulate_standalone_server();

	// Mock server
	let mock_server = MockServer::connect("127.0.0.1:8081");
	println!("Mock server listening on {}", mock_server.base_url());


	let (response_bytes, parsed_response) = get_mock_response(
		response_type.clone(), 
		statement_type.clone()
	);

	// Mock response
	mock_server.mock(|when, then| {
		when.method(GET)
			.path("/ebics/api-v1/bankstatements");
		then.status(200)
			.header("content-type", "application/json")
			.body(response_bytes.clone());
	});

	let statements_endpoint = format!("{}/ebics/api-v1/bankstatements", mock_server.base_url());

	ebics_server_response(&mut state.write(),
		testing::PendingRequest {
			method: "GET".to_string(),
			uri: statements_endpoint,
			response: Some(response_bytes),
			sent: true,
			..Default::default()
		}
	);

	t.execute_with(|| {
		let _res = FiatRampsExample::fetch_transactions_and_send_signed();

        match response_type {
            ResponseTypes::Empty => {
                // No transactions should be sent for empty statement
                assert!(pool_state.read().transactions.is_empty());
            },
            ResponseTypes::SingleStatement | ResponseTypes::MultipleStatements => {
                let tx = pool_state.write().transactions.pop().unwrap();

                assert!(pool_state.read().transactions.is_empty());

                let tx = Extrinsic::decode(&mut &*tx).unwrap();
                assert_eq!(tx.signature.unwrap().0, 0);

                assert_eq!(tx.call, Call::FiatRampsExample(crate::Call::process_statements {
                    statements: parsed_response.clone(),
                }));
            }
        }
	})
}

#[test]
fn it_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(1, 1);
	})
}

#[test]
fn should_make_http_call_and_parse() {
	let (offchain, state) = testing::TestOffchainExt::new();
	let mut t = new_test_ext(); 

	simulate_standalone_server();
	
	let mock_server = MockServer::connect("127.0.0.1:8081");

	println!("Mock server listening on {}", mock_server.base_url());

	t.register_extension(OffchainWorkerExt::new(offchain));

	let (response_bytes, parsed_response) = get_mock_response(
		ResponseTypes::SingleStatement, 
		StatementTypes::IncomingTransactions
	);

	// Mock response
	mock_server.mock(|when, then| {
		when.method(GET)
			.path("/ebics/api-v1/bankstatements");
		then.status(200)
			.header("content-type", "application/json")
			.body(response_bytes.clone());
	});

	let statements_endpoint = format!("{}/ebics/api-v1/bankstatements", mock_server.base_url());

	ebics_server_response(&mut state.write(),
		testing::PendingRequest {
			method: "GET".to_string(),
			uri: statements_endpoint,
			response: Some(response_bytes),
			sent: true,
			..Default::default()
		}
	);

	t.execute_with(|| {
		let response = FiatRampsExample::fetch_json(format!("{}/ebics/api-v1", mock_server.base_url()).as_bytes()).unwrap();
		let raw_array = response.as_array();
		
		let statements = match raw_array {
			Some(v) => {
				let mut balances: Vec<(IbanAccount, Vec<Transaction>)> = Vec::with_capacity(v.len());
				for val in v.iter() {
					// extract iban account
					let iban_account = match IbanAccount::from_json_value(&val) {
						Some(account) => account,
						None => Default::default(),
					};

					// extract transactions
					let mut transactions = Transaction::parse_transactions(&val, TransactionType::Outgoing).unwrap_or_default();
					let mut incoming_transactions = Transaction::parse_transactions(&val, TransactionType::Incoming).unwrap_or_default();
					
					transactions.append(&mut incoming_transactions);
					
					balances.push((iban_account, transactions));
				}
				balances
			},
			None => Default::default(),
		};

		assert_eq!(statements.len(), 1);
		assert_eq!(statements[0].0, parsed_response[0].0);
		assert_eq!(statements[0].1, parsed_response[0].1);
	})
}

#[test]
fn test_process_empty_statement() {
    test_processing(
        StatementTypes::IncomingTransactions,
        ResponseTypes::Empty,
    )
}

#[test]
fn test_process_incoming_transactions() {
    test_processing(
        StatementTypes::IncomingTransactions,
        ResponseTypes::SingleStatement,
    )
}

#[test]
fn test_process_outgoing_transactions() {
    test_processing(
        StatementTypes::OutgoingTransactions,
        ResponseTypes::SingleStatement,
    )
}

#[test]
fn test_process_multiple_statements() {
    test_processing(
        StatementTypes::IncomingTransactions,
        ResponseTypes::MultipleStatements,
    )
}

#[test]
fn test_process_multiple_statements_outgoing() {
    test_processing(
        StatementTypes::OutgoingTransactions,
        ResponseTypes::MultipleStatements,
    )
}

#[test]
fn test_iban_mapping() {
	let mut t = new_test_ext();

	let test_accounts = get_test_accounts();
	
	let alice = test_accounts[0].clone();
	let bob = test_accounts[1].clone();
	let charlie = test_accounts[2].clone();

	let alice_iban = String::from("DE89370400440532013000").as_bytes().to_vec();
	let bob_iban = String::from("DE89370400440532013001").as_bytes().to_vec();
	let charlie_iban = String::from("DE89370400440532013002").as_bytes().to_vec();

	t.execute_with(|| {
		assert_ok!(FiatRampsExample::map_iban_account(
			Some(alice.clone()).into(),
			IbanAccount {
				iban: alice_iban.clone(),
				balance: 100,
				last_updated: 0,
			}
		));
		assert_ok!(FiatRampsExample::map_iban_account(
			Some(bob.clone()).into(),
			IbanAccount {
				iban: bob_iban.clone(),
				balance: 100,
				last_updated: 0,
			}
		));

		assert_ok!(FiatRampsExample::map_iban_account(
			Some(charlie.clone()).into(),
			IbanAccount {
				iban: charlie_iban.clone(),
				balance: 100,
				last_updated: 0,
			}
		));

		assert_eq!(FiatRampsExample::iban_to_account(alice_iban), alice.clone());
		assert_eq!(FiatRampsExample::iban_to_account(bob_iban), bob.clone());
		assert_eq!(FiatRampsExample::iban_to_account(charlie_iban), charlie.clone());
	})
}

#[test]
fn test_burn_request() {
    let (offchain, state) = testing::TestOffchainExt::new();
    let (pool, pool_state) = testing::TestTransactionPoolExt::new();
    let keystore = sp_keystore::testing::KeyStore::new();

	SyncCryptoStore::sr25519_generate_new(
        &keystore,
        crate::crypto::Public::ID,
        Some(&format!("{}/alice", "cup swing hill dinner pioneer mom stick steel sad raven oak practice")),
    ).unwrap();

    let mut t = new_test_ext(); 

	t.register_extension(OffchainWorkerExt::new(offchain));
    t.register_extension(TransactionPoolExt::new(pool));
    t.register_extension(KeystoreExt(Arc::new(keystore)));

	simulate_standalone_server();

	let test_accounts = get_test_accounts();

	let alice = test_accounts[0].clone();
	let bob = test_accounts[1].clone();
	// let charlie = test_accounts[2].clone();

	let alice_iban = String::from("DE89370400440532013000").as_bytes().to_vec();
	let bob_iban = String::from("DE89370400440532013001").as_bytes().to_vec();
	// let charlie_iban = String::from("DE89370400440532013002").as_bytes().to_vec();

	let mock_unpeg_request = unpeg_request(
	&format!("{:?}", bob),
		10000,
		&bob_iban,
		&"0".to_string(),
	)
	.serialize();

	let unpeg_endpoint = "http://127.0.0.1:8081/ebics/api-v1/unpeg";

	ebics_server_response(&mut state.write(),
		testing::PendingRequest {
			uri: unpeg_endpoint.to_string(),
			method: "POST".to_string(),
			body: mock_unpeg_request.clone(),
			response: Some(mock_unpeg_request),
			headers: [
				("Content-Type".to_string(), "application/json".to_string()), 
				("accept".to_string(), "*/*".to_string())
			].to_vec(),
			sent: true,
			..Default::default()
		}
	);

	t.execute_with(|| {
		// map Alice iban
		assert_ok!(FiatRampsExample::map_iban_account(
			Some(alice.clone()).into(),
			IbanAccount {
				iban: alice_iban.clone(),
				balance: 100,
				last_updated: 0,
			}
		));
		// map Bob iban
		assert_ok!(FiatRampsExample::map_iban_account(
			Some(bob.clone()).into(),
			IbanAccount {
				iban: bob_iban.clone(),
				balance: 100,
				last_updated: 0,
			}
		));

		// call `burn_to_iban` to transfer 10000 from Alice to Bob
		assert_ok!(FiatRampsExample::burn_to_iban(
			Some(alice.clone()).into(),
			10000,
			bob_iban.clone(),
		));

		// Check if burn request counter has been increased
		assert_eq!(FiatRampsExample::burn_request_count(), 1);

		// Check if burn request has been added to the queue
		let burn_request = FiatRampsExample::burn_request(0);
		assert_eq!(burn_request.amount, 10000);
		assert_eq!(burn_request.burner, alice.clone());
		assert_eq!(burn_request.dest_iban, Some(bob_iban.clone()));

		assert_ok!(FiatRampsExample::process_burn_requests());

		// Check if burn request's status has been updated
		let burn_request = FiatRampsExample::burn_request(0);
		assert_eq!(burn_request.status, BurnRequestStatus::Sent);
	})
}

/// Mock server response
fn ebics_server_response(
	state: &mut testing::OffchainState,
	pending_request: testing::PendingRequest
) {
	state.expect_request(pending_request);
}
