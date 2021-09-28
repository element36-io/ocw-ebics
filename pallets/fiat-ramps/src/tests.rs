use crate as fiat_ramps;
use crate::*;
use codec::Decode;
use frame_support::{parameter_types};
use sp_core::{
    offchain::{testing, OffchainWorkerExt, TransactionPoolExt},
    sr25519::Signature,
    H256
};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;

use sp_runtime::{ testing::{Header, TestXt}, traits::{BlakeTwo256, Extrinsic as ExtrinsicT, IdentifyAccount, IdentityLookup, Verify}};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
pub const MILLISECS_PER_BLOCK: u64 = 6000;

// NOTE: Currently it is not possible to change the slot duration after the chain has started.
//       Attempting to do so will brick block production.
pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

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
		Aura: pallet_aura::{Pallet, Config<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(1024);
}
impl frame_system::Config for Test {
	type BaseCallFilter = ();
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
	type AccountData = ();
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

impl pallet_aura::Config for Test {
	type AuthorityId = AuraId;
}

parameter_types! {
	pub const MinimumPeriod: u64 = SLOT_DURATION / 2;
}

impl pallet_timestamp::Config for Test {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

impl fiat_ramps::Config for Test {
	type AuthorityId = fiat_ramps::crypto::OcwAuthId;
	type Event = Event;
	type Call = Call;
	type UnsignedPriority = UnsignedPriority;
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

parameter_types! {
	// interval in blocks between two consecutive unsigned transactions
	pub const UnsignedInterval: u64 = 3;
	pub const UnsignedPriority: u64 = 1000;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
}

const TEST_API_URL: &[u8] = b"http://localhost:8093/ebics/api-v1/bankstatements";

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

fn ebics_server_response(state: &mut testing::OffchainState) {
	state.expect_request(testing::PendingRequest {
		method: "GET".into(),
		uri: core::str::from_utf8(TEST_API_URL).unwrap().to_string(),
		response: Some(br#"[
			{
			  "iban": "CH4308307000289537312",
			  "balanceOP": 80842.45,
			  "balanceOPCurrency": "CHF",
			  "balanceCL": 80097.2,
			  "balanceCLCurrency": "CHF",
			  "balanceCLDate": "2021-02-28",
			  "bookingDate": "2021-02-28",
			  "validationDate": "2021-02-28",
			  "incomingTransactions": [],
			  "outgoingTransactions": [
				{
				  "iban": null,
				  "name": null,
				  "addrLine": [
					"VISECA CARD SERVICES SA \nHagenholzstrasse 56 \nPostfach 7007 \n8050 Zuerich",
					"ich"
				  ],
				  "currency": "CHF",
				  "amount": 745.25,
				  "reference": null,
				  "endToEndId": null,
				  "instrId": null,
				  "msgId": null,
				  "pmtInfId": null
				}
			  ]
			},
			{
			  "iban": "CH2108307000289537320",
			  "balanceOP": 0,
			  "balanceOPCurrency": "CHF",
			  "balanceCL": 110,
			  "balanceCLCurrency": "CHF",
			  "balanceCLDate": "2021-03-14",
			  "bookingDate": "2021-03-14",
			  "validationDate": "2021-03-14",
			  "incomingTransactions": [
				{
				  "iban": "CH4308307000289537312",
				  "name": null,
				  "addrLine": [
					"element36 AG \nBahnmatt 25 \n6340 Baar"
				  ],
				  "currency": "CHF",
				  "amount": 100,
				  "reference": "Testanweisung",
				  "endToEndId": null,
				  "instrId": null,
				  "msgId": null,
				  "pmtInfId": null
				}
			  ],
			  "outgoingTransactions": []
			}
		  ]
		  "#.to_vec()),
		  sent: true,
		  ..Default::default()
	});
}