use crate as fiat_ramps;
use crate::*;
use codec::Decode;
use frame_support::{
	parameter_types,
	traits::{ConstU32},
};
use sp_core::{
    offchain::{testing, OffchainWorkerExt, TransactionPoolExt},
    sr25519::Signature,
    H256
};

use sp_runtime::{ testing::{Header, TestXt}, traits::{BlakeTwo256, Extrinsic as ExtrinsicT, IdentifyAccount, IdentityLookup, Verify}};

use types::{
	Transaction, IbanAccount, unpeg_request,
	TransactionType, StrVecBytes
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

const TEST_API_URL: &[u8] = b"http://w.e36.io:8093/ebics/api-v1/bankstatements";

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

	t.register_extension(OffchainWorkerExt::new(offchain));

	ebics_server_response(&mut state.write());

	t.execute_with(|| {
		let response = FiatRampsExample::fetch_json(TEST_API_URL).unwrap();
		let iban_balances = match FiatRampsExample::extract_iban_balances(response) {
			Some(iban_balances) => Ok(iban_balances),
			None => {
			 	log::error!("Unable to extract iban balance from response");
				Err("Unable to extract iban balances from response")
			}
		}.unwrap();

		let expected_iban_balances: Vec<IbanBalance> = vec![
			(b"CH4308307000289537312".to_vec(), 8009702),
			(b"CH2108307000289537320".to_vec(), 11000)
		];
		assert_eq!(expected_iban_balances, iban_balances);
	})
}

#[test]
fn test_process_empty_statement() {

} 

#[test]
fn test_process_single_statement() {

}

#[test]
fn should_send_unsigned_transaction() {
	let (offchain, state) = testing::TestOffchainExt::new();
	let mut t = new_test_ext();
	let (pool, pool_state) = testing::TestTransactionPoolExt::new();

	t.register_extension(OffchainWorkerExt::new(offchain));
	t.register_extension(TransactionPoolExt::new(pool));
	ebics_server_response(&mut state.write());

	t.execute_with(|| {
		let block_number: u64 = FiatRampsExample::next_sync_at();
		let _res = FiatRampsExample::fetch_iban_balance_and_send_unsigned(block_number);
		// pop transaction
		let tx = pool_state.write().transactions.pop().unwrap();
		assert!(pool_state.read().transactions.is_empty());

		// decode extrinsic
		let ext = Extrinsic::decode(&mut &*tx).unwrap();
		
		// unsigned extrinsic doesn't have signature
		assert_eq!(ext.signature, None);
		let expected_iban_balances: Vec<IbanBalance> = vec![
			(b"CH4308307000289537312".to_vec(), 8009702),
			(b"CH2108307000289537320".to_vec(), 11000)
		];
		// calls match
		assert_eq!(ext.call, Call::FiatRampsExample(crate::Call::submit_balances_unsigned(block_number, expected_iban_balances)));
	});
}

#[test]
fn should_fail_on_future_blocks() {
	let (offchain, state) = testing::TestOffchainExt::new();
	let mut t = new_test_ext();
	let (pool, pool_state) = testing::TestTransactionPoolExt::new();

	t.register_extension(OffchainWorkerExt::new(offchain));
	t.register_extension(TransactionPoolExt::new(pool));

	t.execute_with(|| {
		let invalid_block: u64 = FiatRampsExample::next_sync_at() - 1;

		let res1 = FiatRampsExample::fetch_iban_balance_and_send_unsigned(invalid_block);

		// transaction pool is empty
		assert!(pool_state.read().transactions.is_empty());

		// result of the transaction
		assert_eq!(&res1, &Err("Too early to send unsigned transaction"));
	})
}

/// Server response types
enum ResponseTypes {
	/// Response is empty
	Empty,
	/// Response contains only one statement
	SingleStatement,
	/// Response contains multiple statements
	MultipleStatements,
}

/// Bank statement types
enum StatementTypes {
	/// Bank statement contains no transactions (usual case)
	Empty,
	/// Bank statement has `incomingTransactions` field populated
	IncomingTransactions,
	/// Bank statement has `outgoingTransactions` field populated
	OutgoingTransactions,
	/// Bank statement has `incomingTransactions` and `outgoingTransactions` fields populated
	CompleteTransactions,
	///
	InvalidTransactions,
}

/// Get mock server response
fn get_mock_response(
	response: ResponseTypes,
	statement: StatementTypes,
) -> Vec<u8> {
	match response {
		ResponseTypes::Empty => {
			return br#"[]"#.to_vec();
		}
		ResponseTypes::SingleStatement => {
			match statement {
				StatementTypes::Empty => {
					return br#"[]"#.to_vec();
				}
				StatementTypes::IncomingTransactions => {
					// the transaction is coming from Bob to Alice
					return br#"[
						{
							"iban": "CH2108307000289537320",
							"balanceCL": 10000000,
							"incomingTransactions": [
								{
									"iban": "CH4308307000289537312",
									"name: "Bob",
									"currency": "EUR",
									"amount": 10000,
									"reference": "Purp:5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY; ourRef: none",
								}
							],
							outgoingTransactions: []
						}
					]"#.to_vec();
				}
				StatementTypes::OutgoingTransactions => {
					// outgoing transaction is from Bob to Alice
					return br#"[
						{
							"iban": "CH4308307000289537312",
							"balanceCL": 10000000,
							"incomingTransactions": [],
							"outgoingTransactions": [
								{
									"iban": "CH2108307000289537320",
									"name: "Alice",
									"currency": "EUR",
									"amount": 10000,
									"reference": "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef: none",
								}
							]
						}
					]"#.to_vec();
				}
				StatementTypes::CompleteTransactions => {
					return br#"[
						{
							"iban": "CH1230116000289537313",
							"balanceCL": 10000000,
							"incomingTransaction": [
								{
									"iban": "CH2108307000289537320",
									"name: "Alice",
									"currency": "EUR",
									"amount": 15000,
									"reference": "Purp:5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y; ourRef: none",
								}
							],
							"outgoingTransactions": [
								{
									"iban": "CH1230116000289537312",
									"name: "Bob",
									"currency": "EUR",
									"amount": 15000,
									"reference": "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef: none",
								}
							]
						}	
					]"#.to_vec();
				}
			}
		},
		ResponseTypes::MultipleStatements => {
			return br#"[
				{
					"iban": "CH1230116000289537313",
					"balanceCL": 10000000,
					"incomingTransaction": [
						{
							"iban": "CH2108307000289537320",
							"name: "Alice",
							"currency": "EUR",
							"amount": 15000,
							"reference": "Purp:5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y; ourRef: none",
						}
					],
					"outgoingTransactions": [
						{
							"iban": "CH1230116000289537312",
							"name: "Bob",
							"currency": "EUR",
							"amount": 15000,
							"reference": "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef: none",
						}
					]
				},
				{
					"iban": "CH1230116000289537312",
					"balanceCL": 10000000,
					"incomingTransaction": [
						{
							"iban": "CH2108307000289537320",
							"name: "Alice",
							"currency": "EUR",
							"amount": 15000,
							"reference": "Purp:5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y; ourRef: none",
						}
					],
					"outgoingTransactions": [
						{
							"iban": "CH1230116000289537312",
							"name: "Bob",
							"currency": "EUR",
							"amount": 15000,
							"reference": "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef: none",
						}
					]
				},
				{
					"iban": "CH1230116000289537313",
					"balanceCL": 10000000,
					"incomingTransaction": [
						{
							"iban": "CH2108307000289537320",
							"name: "Alice",
							"currency": "EUR",
							"amount": 5000,
							"reference": "Purp:5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y; ourRef: none",
						},
						{
							"iban": "CH1230116000289537312",
							"name: "Bob",
							"currency": "EUR",
							"amount": 10000,
							"reference": "Purp:5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y; ourRef: none",
						}
					],
					"outgoingTransactions": [
						{
							"iban": "CH1230116000289537312",
							"name: "Bob",
							"currency": "EUR",
							"amount": 15000,
							"reference": "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef: none",
						}
					]
				}
			]"#.to_vec();
		}
	}
}

/// Mock server response
fn ebics_server_response(
	state: &mut testing::OffchainState
) {
	state.expect_request(testing::PendingRequest {
		method: "GET".into(),
		uri: core::str::from_utf8(TEST_API_URL).unwrap().to_string(),
		response: Some(br#""#.to_vec()),
		  sent: true,
		  ..Default::default()
	});
}