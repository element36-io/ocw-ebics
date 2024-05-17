use codec::Decode;
use frame_support::{assert_err, assert_noop, assert_ok};
use lite_json::Serialize;
use sp_core::offchain::{testing, OffchainWorkerExt, TransactionPoolExt};
use sp_keystore::{Keystore, KeystoreExt};
use sp_runtime::{traits::BadOrigin, DispatchError, RuntimeAppPublic};
use std::sync::Arc;

use crate::{
	helpers::{
		get_mock_receipt, get_mock_response, string_to_bounded_vec, ResponseTypes, StatementTypes,
	},
	types::{
		BankAccountOf, IbanOf, Transaction, TransactionOf, TransactionType, TransferDestination,
	},
	utils::*,
	Config, QueuedStatements,
};

use crate::{mock::*, Error};

/// Utility function to test various scenarios for the `process_statements` extrinsic
fn test_processing(statement_type: StatementTypes, response_type: ResponseTypes) {
	let (offchain, state) = testing::TestOffchainExt::new();
	let (pool, pool_state) = testing::TestTransactionPoolExt::new();
	let keystore = sp_keystore::testing::MemoryKeystore::new();

	keystore
		.sr25519_generate_new(
			crate::crypto::Public::ID,
			Some(&format!(
				"{}/alice",
				"cup swing hill dinner pioneer mom stick steel sad raven oak practice"
			)),
		)
		.unwrap();

	let mut t = new_test_ext();

	t.register_extension(OffchainWorkerExt::new(offchain));
	t.register_extension(TransactionPoolExt::new(pool));
	t.register_extension(KeystoreExt(Arc::new(keystore)));

	let (response_bytes, parsed_response) =
		get_mock_response::<Test>(response_type.clone(), statement_type.clone());

	let statements_endpoint = "http://localhost:8093/ebics/api-v1/bankstatements".to_string();

	ebics_server_response(
		&mut state.write(),
		testing::PendingRequest {
			method: "GET".to_string(),
			uri: statements_endpoint,
			response: Some(response_bytes),
			sent: true,
			..Default::default()
		},
	);

	t.execute_with(|| {
		FiatRampsExample::set_risc0_image_id(RuntimeOrigin::root(), [0u8; 32]).unwrap();

		match response_type {
			ResponseTypes::Empty => {
				let _res =
					FiatRampsExample::fetch_and_send_signed(crate::OcwActivity::FetchStatements);
				// No transactions should be sent for empty statement
				assert!(pool_state.read().transactions.is_empty());
			},
			ResponseTypes::SingleStatement | ResponseTypes::MultipleStatements =>
				match statement_type {
					StatementTypes::InvalidTransactions => {
						assert_noop!(
							FiatRampsExample::fetch_and_send_signed(
								crate::OcwActivity::FetchStatements
							),
							"Error in parsing json"
						);
					},
					_ => {
						assert_ok!(FiatRampsExample::fetch_and_send_signed(
							crate::OcwActivity::FetchStatements
						));
						let tx = pool_state.write().transactions.pop().unwrap();

						assert!(pool_state.read().transactions.is_empty());

						let tx = Extrinsic::decode(&mut &*tx).unwrap();
						assert_eq!(tx.signature.unwrap().0, 0);

						assert_eq!(
							tx.call,
							crate::Call::queue_statements {
								receipt_url: parsed_response.clone().unwrap().receipt_url,
								statements: parsed_response.unwrap().statements,
							}
							.into()
						);
					},
				},
		}
	})
}

/// Mock server response
fn ebics_server_response(
	state: &mut testing::OffchainState,
	pending_request: testing::PendingRequest,
) {
	state.expect_request(pending_request);
}

#[test]
fn should_fail_to_update_api_url_non_sudo() {
	let mut t = new_test_ext();
	let test_accounts = get_test_accounts();

	// Alice is a sudo account
	let bob = test_accounts[1].clone();
	let charlie = test_accounts[2].clone();

	let invalid_url = string_to_bounded_vec("http://127.0.0.1:8081/ebics/ap-v2");

	t.execute_with(|| {
		assert_err!(
			FiatRampsExample::set_api_url(Some(bob).into(), invalid_url.clone()),
			DispatchError::BadOrigin
		);

		assert_err!(
			FiatRampsExample::set_api_url(Some(charlie).into(), invalid_url),
			DispatchError::BadOrigin
		);
	})
}

#[test]
fn should_make_http_call_and_parse() {
	let (offchain, state) = testing::TestOffchainExt::new();
	let mut t = new_test_ext();

	t.register_extension(OffchainWorkerExt::new(offchain));

	let (response_bytes, parsed_response) = get_mock_response::<Test>(
		ResponseTypes::SingleStatement,
		StatementTypes::IncomingTransactions,
	);

	let statements_endpoint = "http://localhost:8093/ebics/api-v1/bankstatements".to_string();

	ebics_server_response(
		&mut state.write(),
		testing::PendingRequest {
			method: "GET".to_string(),
			uri: statements_endpoint.clone(),
			response: Some(response_bytes.clone()),
			sent: true,
			..Default::default()
		},
	);

	t.execute_with(|| {
		let response = FiatRampsExample::fetch_json("api-v1/bankstatements").unwrap();
		let raw_array = response.as_array();

		if let Some(v) = raw_array {
			let mut balances: Vec<(BankAccountOf<Test>, Vec<TransactionOf<Test>>)> =
				Vec::with_capacity(v.len());
			for val in v.iter() {
				if let Ok(bank_account) = BankAccountOf::<Test>::try_from(val) {
					let mut transactions =
						Transaction::parse_transactions(&val, TransactionType::Outgoing)
							.unwrap_or_default();
					let mut incoming_transactions =
						Transaction::parse_transactions(&val, TransactionType::Incoming)
							.unwrap_or_default();

					transactions.append(&mut incoming_transactions);
					balances.push((bank_account, transactions));
				}
			}

			assert_eq!(balances.len(), 1);
			assert_eq!(
				b"abcd.json".to_vec(),
				parsed_response.clone().unwrap().receipt_url.into_inner()
			);
			assert_eq!(balances[0].0, parsed_response.clone().unwrap().statements[0].0);
			assert_eq!(
				balances[0].1,
				parsed_response.unwrap().statements[0].clone().1.into_inner()
			);
		}
	})
}

#[test]
fn test_queue_empty_statement() {
	test_processing(StatementTypes::Empty, ResponseTypes::Empty)
}

#[test]
fn test_queue_incoming_transactions() {
	test_processing(StatementTypes::IncomingTransactions, ResponseTypes::SingleStatement)
}

#[test]
fn test_queue_outgoing_transactions() {
	test_processing(StatementTypes::OutgoingTransactions, ResponseTypes::SingleStatement)
}

#[test]
fn test_queue_multiple_statements() {
	test_processing(StatementTypes::CompleteTransactions, ResponseTypes::MultipleStatements)
}

#[test]
fn test_queue_multiple_statements_outgoing() {
	test_processing(StatementTypes::OutgoingTransactions, ResponseTypes::MultipleStatements)
}

#[test]
fn test_queue_invalid_transactions() {
	test_processing(StatementTypes::InvalidTransactions, ResponseTypes::SingleStatement)
}

#[test]
fn test_verify_queued_statements_works() {
	let (offchain, state) = testing::TestOffchainExt::new();
	let (pool, pool_state) = testing::TestTransactionPoolExt::new();
	let keystore = sp_keystore::testing::MemoryKeystore::new();

	keystore
		.sr25519_generate_new(
			crate::crypto::Public::ID,
			Some(&format!(
				"{}/alice",
				"cup swing hill dinner pioneer mom stick steel sad raven oak practice"
			)),
		)
		.unwrap();

	let public = keystore.sr25519_public_keys(crate::crypto::Public::ID).pop().unwrap().clone();

	let mut t = new_test_ext();

	t.register_extension(OffchainWorkerExt::new(offchain));
	t.register_extension(TransactionPoolExt::new(pool));
	t.register_extension(KeystoreExt(Arc::new(keystore)));

	let (response_bytes, parsed_response) = get_mock_response::<Test>(
		ResponseTypes::SingleStatement,
		StatementTypes::OutgoingTransactions,
	);

	let statements_endpoint = "http://localhost:8093/ebics/api-v1/bankstatements".to_string();

	ebics_server_response(
		&mut state.write(),
		testing::PendingRequest {
			method: "GET".to_string(),
			uri: statements_endpoint.clone(),
			response: Some(response_bytes.clone()),
			sent: true,
			..Default::default()
		},
	);

	let (receipt_response_bytes, _receipt_response) = get_mock_receipt();

	let receipt_endpoint = "http://localhost:8093/ebics/abcd.json".to_string();

	ebics_server_response(
		&mut state.write(),
		testing::PendingRequest {
			method: "GET".to_string(),
			uri: receipt_endpoint.clone(),
			response: Some(receipt_response_bytes.clone()),
			sent: true,
			..Default::default()
		},
	);

	t.execute_with(|| {
		assert_ok!(FiatRampsExample::set_risc0_image_id(RuntimeOrigin::root(), [0u8; 32]));
		assert_ok!(FiatRampsExample::fetch_and_send_signed(crate::OcwActivity::FetchStatements));

		let tx_in_pool = pool_state.write().transactions.pop().unwrap();
		let tx = Extrinsic::decode(&mut &*tx_in_pool).unwrap();

		assert_eq!(tx.signature.unwrap().0, 0);

		let statements = if let RuntimeCall::FiatRampsExample(crate::Call::queue_statements {
			receipt_url,
			statements,
		}) = tx.call
		{
			assert_eq!(receipt_url, parsed_response.clone().unwrap().receipt_url);
			statements
		} else {
			panic!("Unexpected call: {:?}", tx.call);
		};

		assert!(parsed_response.clone().unwrap().statements.len() > 0);
		assert_eq!(statements, parsed_response.clone().unwrap().statements);

		assert_ok!(FiatRampsExample::queue_statements(
			RuntimeOrigin::signed(public),
			parsed_response.clone().unwrap().receipt_url,
			parsed_response.unwrap().statements
		));

		assert_ok!(FiatRampsExample::fetch_and_send_signed(
			crate::OcwActivity::VerifyAndProcessStatements
		));

		// check if transaction is in the pool
		let tx_in_pool = pool_state.write().transactions.pop().unwrap();
		let tx = Extrinsic::decode(&mut &*tx_in_pool).unwrap();

		assert!(matches!(
			tx.call,
			RuntimeCall::FiatRampsExample(crate::Call::process_statements { .. })
		));
	});
}

#[test]
fn test_iban_mapping() {
	let mut t = new_test_ext();

	let test_accounts = get_test_accounts();

	let alice = test_accounts[0].clone();
	let bob = test_accounts[1].clone();
	let charlie = test_accounts[2].clone();

	let alice_iban: IbanOf<Test> = string_to_bounded_vec("CH2108307000289537320");
	let bob_iban: IbanOf<Test> = string_to_bounded_vec("CH1230116000289537312");
	let charlie_iban: IbanOf<Test> = string_to_bounded_vec("CH1230116000289537313");

	t.execute_with(|| {
		assert_ok!(FiatRampsExample::create_account(
			Some(alice.clone()).into(),
			alice_iban.clone(),
		));
		assert_ok!(FiatRampsExample::create_account(Some(bob.clone()).into(), bob_iban.clone(),));

		assert_ok!(FiatRampsExample::create_account(
			Some(charlie.clone()).into(),
			charlie_iban.clone(),
		));

		assert_eq!(FiatRampsExample::get_account_id(&alice_iban).unwrap(), alice.clone());
		assert_eq!(FiatRampsExample::get_account_id(&bob_iban).unwrap(), bob.clone());
		assert_eq!(FiatRampsExample::get_account_id(&charlie_iban).unwrap(), charlie.clone());

		// Unmapping should work
		assert_ok!(FiatRampsExample::unmap_iban_account(
			Some(alice.clone()).into(),
			alice_iban.clone()
		));
		// Should be mapped to None
		assert_eq!(FiatRampsExample::get_account_id(&alice_iban), None);
	})
}

#[test]
fn test_burn_request() {
	let (offchain, state) = testing::TestOffchainExt::new();
	let (pool, _pool_state) = testing::TestTransactionPoolExt::new();
	let keystore = sp_keystore::testing::MemoryKeystore::new();

	keystore
		.sr25519_generate_new(
			crate::crypto::Public::ID,
			Some(&format!(
				"{}/alice",
				"cup swing hill dinner pioneer mom stick steel sad raven oak practice"
			)),
		)
		.unwrap();

	let mut t = new_test_ext();

	t.register_extension(OffchainWorkerExt::new(offchain));
	t.register_extension(TransactionPoolExt::new(pool));
	t.register_extension(KeystoreExt(Arc::new(keystore)));

	let test_accounts = get_test_accounts();

	let alice = test_accounts[0].clone();
	let bob = test_accounts[1].clone();
	let charlie = test_accounts[2].clone();

	let alice_iban: IbanOf<Test> = string_to_bounded_vec("CH2108307000289537320");
	let bob_iban: IbanOf<Test> = string_to_bounded_vec("CH1230116000289537312");
	let charlie_iban: IbanOf<Test> = string_to_bounded_vec("CH1230116000289537313");

	{
		let mock_unpeg_request =
			unpeg_request::<Test>(&format!("{:?}", bob), 10000, &bob_iban, &"0".to_string())
				.serialize();

		let mock_unpeg_request_1 =
			unpeg_request::<Test>(&format!("{:?}", charlie), 100, &charlie_iban, &"1".to_string())
				.serialize();

		let mock_unpeg_request_2 =
			unpeg_request::<Test>(&format!("{:?}", charlie), 1000, &charlie_iban, &"2".to_string())
				.serialize();

		let unpeg_endpoint = "http://localhost:8093/ebics/api-v1/unpeg";

		ebics_server_response(
			&mut state.write(),
			testing::PendingRequest {
				uri: unpeg_endpoint.to_string(),
				method: "POST".to_string(),
				body: mock_unpeg_request.clone(),
				response: Some(mock_unpeg_request),
				headers: [
					("Content-Type".to_string(), "application/json".to_string()),
					("accept".to_string(), "*/*".to_string()),
				]
				.to_vec(),
				sent: true,
				..Default::default()
			},
		);

		ebics_server_response(
			&mut state.write(),
			testing::PendingRequest {
				uri: unpeg_endpoint.to_string(),
				method: "POST".to_string(),
				body: mock_unpeg_request_1.clone(),
				response: Some(mock_unpeg_request_1),
				headers: [
					("Content-Type".to_string(), "application/json".to_string()),
					("accept".to_string(), "*/*".to_string()),
				]
				.to_vec(),
				sent: true,
				..Default::default()
			},
		);
		ebics_server_response(
			&mut state.write(),
			testing::PendingRequest {
				uri: unpeg_endpoint.to_string(),
				method: "POST".to_string(),
				body: mock_unpeg_request_2.clone(),
				response: Some(mock_unpeg_request_2),
				headers: [
					("Content-Type".to_string(), "application/json".to_string()),
					("accept".to_string(), "*/*".to_string()),
				]
				.to_vec(),
				sent: true,
				..Default::default()
			},
		);
	}

	t.execute_with(|| {
		// Local counter to keep track of the number of burn requests
		fn check_burn_request(
			initial_pallet_balance: u128,
			request_counter: u64,
			amount: u128,
			burner: &AccountId,
			dest_iban: &IbanOf<Test>,
		) {
			// Check if burn request has been added to the queue
			let maybe_burn_request = FiatRampsExample::burn_requests(request_counter);

			assert!(maybe_burn_request.is_some());

			let burn_request = maybe_burn_request.unwrap();

			assert_eq!(burn_request.amount, amount);
			assert_eq!(
				FiatRampsExample::get_account_id(&burn_request.burner).unwrap(),
				burner.clone()
			);
			assert_eq!(burn_request.dest_iban, *dest_iban);

			// Burn amount should be transfered to Pallet's account
			// Pallet's accounts serves as the treasury of unpegged funds
			// Once the burn request is confirmed as an outgoing transaction in the bank statement,
			// We can tag the burn request as confirmed and send funds to the destination account
			assert_eq!(
				Balances::free_balance(FiatRampsExample::account_id()),
				initial_pallet_balance + amount
			);

			// Trigger processing of burn requests
			assert_ok!(FiatRampsExample::process_burn_requests());
		}

		// map Alice iban
		assert_ok!(FiatRampsExample::create_account(
			Some(alice.clone()).into(),
			alice_iban.clone(),
		));
		// map Bob iban
		assert_ok!(FiatRampsExample::create_account(Some(bob.clone()).into(), bob_iban.clone(),));

		// map Charlie iban
		assert_ok!(FiatRampsExample::create_account(
			Some(charlie.clone()).into(),
			charlie_iban.clone(),
		));

		// Pallet's balance before unpeg request
		let initial_pallet_balance = Balances::free_balance(FiatRampsExample::account_id());
		// call `burn_to_iban` to transfer 10000 from Alice to Bob
		assert_ok!(FiatRampsExample::transfer(
			Some(alice.clone()).into(),
			10000,
			TransferDestination::Iban(bob_iban.clone())
		));

		check_burn_request(initial_pallet_balance, 0, 10000, &alice, &bob_iban);

		// Pallet's balance before unpeg request
		let initial_pallet_balance = Balances::free_balance(FiatRampsExample::account_id());
		// make burn to address
		assert_ok!(FiatRampsExample::transfer(
			Some(bob.clone()).into(),
			100,
			TransferDestination::Address(charlie.clone())
		));

		check_burn_request(initial_pallet_balance, 1, 100, &bob, &charlie_iban);

		// Pallet's balance before unpeg request
		let initial_pallet_balance = Balances::free_balance(FiatRampsExample::account_id());

		// Make a generic burn, similar to withdrawin money from the bank
		assert_ok!(FiatRampsExample::transfer(
			Some(charlie.clone()).into(),
			1000,
			TransferDestination::Withdraw
		));

		check_burn_request(initial_pallet_balance, 2, 1000, &charlie, &charlie_iban);
	})
}

#[test]
fn process_statements_is_permissioned() {
	new_test_ext().execute_with(|| {
		let test_accounts = get_test_accounts();

		assert_noop!(
			FiatRampsExample::process_statements(RuntimeOrigin::signed(test_accounts[2]),),
			Error::<Test>::UnauthorizedCall,
		);

		QueuedStatements::<Test>::put(crate::QueuedStatementsInfo {
			statements: vec![].try_into().unwrap(),
			block_number: 0,
			receipt_url: vec![0u8; 32].try_into().unwrap(),
		});

		assert_ok!(FiatRampsExample::process_statements(RuntimeOrigin::signed(
			<Test as Config>::OcwAccount::get()
		),));
	});
}

#[test]
fn set_risc0_image_id() {
	new_test_ext().execute_with(|| {
		let test_accounts = get_test_accounts();

		assert_noop!(
			FiatRampsExample::set_risc0_image_id(
				RuntimeOrigin::signed(test_accounts[2]),
				[0u8; 32]
			),
			BadOrigin
		);

		assert_ok!(FiatRampsExample::set_risc0_image_id(RuntimeOrigin::root(), [0u8; 32]));
	});
}
