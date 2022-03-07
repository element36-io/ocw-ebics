use crate::{self as fiat_ramps, crypto::Public};
use codec::Decode;
use std::sync::Arc;
use frame_support::{
	parameter_types,
};
use sp_core::{
    offchain::{testing, OffchainWorkerExt, TransactionPoolExt},
    sr25519::Signature,
    H256
};
use sp_keystore::{SyncCryptoStore, KeystoreExt};
use sp_runtime::{ testing::{Header, TestXt}, traits::{BlakeTwo256, Extrinsic as ExtrinsicT, IdentifyAccount, IdentityLookup, Verify}, offchain::DbExternalities, RuntimeAppPublic};
use httpmock::{
	MockServer, Method::GET,
};
use mock_server::simulate_standalone_server;

use crate::types::{
	Transaction, IbanAccount, unpeg_request,
	TransactionType,
};
use crate::helpers::{
	ResponseTypes, StatementTypes,
	get_mock_response,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
/// Balance of an account.
pub type Balance = u128;

const MILLISECS_PER_BLOCK: u64 = 4000;

//Mock runtime for our tests
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
        FiatRampsExample: fiat_ramps::{Pallet, Call, Storage, Event<T>, ValidateUnsigned},
		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
		Balances: pallet_balances::{Pallet, Call, Storage, Event<T>},
	}
);

parameter_types! {
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(1024);
	pub const BlockHashCount: u64 = 2400;

}
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type Origin = Origin;
	type Call = Call;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = sp_core::sr25519::Public;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
}

type Extrinsic = TestXt<Call, ()>;
type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

impl frame_system::offchain::SigningTypes for Test {
	type Public = <Signature as Verify>::Signer;
	type Signature = Signature;
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 10;
	pub const MaxLocks: u32 = 50;
}

impl pallet_balances::Config for Test {
	type MaxLocks = MaxLocks;
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	/// The type for recording an account's balance.
	type Balance = Balance;
	/// The ubiquitous event type.
	type Event = Event;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Test>;
}

parameter_types! {
	pub const MinimumPeriod: u64 = 2;
}

impl pallet_timestamp::Config for Test {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

parameter_types!{
	pub const MinimumInterval: u64 = MILLISECS_PER_BLOCK * 5;
	pub const UnsignedPriority: u64 = 1000;
	/// We set decimals for fiat currencies to 2
	/// (e.g. 1 EUR = 1.00 EUR)
	pub const Decimals: u8 = 2;
}

impl fiat_ramps::Config for Test {
	type AuthorityId = fiat_ramps::crypto::OcwAuthId;
	type Event = Event;
	type Call = Call;
	type Currency = Balances;
	type TimeProvider = Timestamp;
	type MinimumInterval = MinimumInterval;
	type UnsignedPriority = UnsignedPriority;
	type Decimals = Decimals;
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Test
where
	Call: From<C>,
{
	type OverarchingCall = Call;
	type Extrinsic = Extrinsic;
}

impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Test
where
	Call: From<LocalCall>,
{
	fn create_transaction<C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>>(
		call: Call,
		_public: <Signature as Verify>::Signer,
		_account: AccountId,
		nonce: u64,
	) -> Option<(Call, <Extrinsic as ExtrinsicT>::SignaturePayload)> {
		Some((call, (nonce, ())))
	}
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
}

// const TEST_API_URL: &[u8] = b"http://w.e36.io:8093/ebics/api-v1/bankstatements";

const API_URL: &str = "127.0.0.1:8081";

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

	ebics_server_response(
		&mut state.write(),
		&statements_endpoint,
		Some(response_bytes)
	);

	t.execute_with(|| {
		let _res = FiatRampsExample::fetch_transactions_and_send_signed();

        match response_type {
            ResponseTypes::Empty => {
                // No transactions should be sent for empty statement
                assert!(pool_state.read().transactions.is_empty());
            },
            ResponseTypes::SingleStatement => {
                let tx = pool_state.write().transactions.pop().unwrap();

                assert!(pool_state.read().transactions.is_empty());

                let tx = Extrinsic::decode(&mut &*tx).unwrap();
                assert_eq!(tx.signature.unwrap().0, 0);
                assert_eq!(tx.call, Call::FiatRampsExample(crate::Call::process_statements {
                    statements: parsed_response
                }));
            },
            ResponseTypes::MultipleStatements => {
                let tx = pool_state.write().transactions.pop().unwrap();

                assert!(pool_state.read().transactions.is_empty());

                let tx = Extrinsic::decode(&mut &*tx).unwrap();
                assert_eq!(tx.signature.unwrap().0, 0);
                assert_eq!(tx.call, Call::FiatRampsExample(crate::Call::process_statements {
                    statements: parsed_response
                }));
            },
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

	ebics_server_response(
		&mut state.write(),
		&statements_endpoint,
		Some(response_bytes)
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

/// Mock server response
fn ebics_server_response(
	state: &mut testing::OffchainState,
	url: &str,
	response: Option<Vec<u8>>,
) {
	state.expect_request(testing::PendingRequest {
		method: "GET".into(),
		uri: url.to_string(),
		response,
		sent: true,
		..Default::default()
	});
}
