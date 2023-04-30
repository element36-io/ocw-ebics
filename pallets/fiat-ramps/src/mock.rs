use crate::{self as fiat_ramps, crypto::Public};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{parameter_types, weights::Weight};
use scale_info::TypeInfo;
use sp_core::{sr25519::Signature, ByteArray, H256};
use sp_runtime::{
	testing::{Header, TestXt},
	traits::{BlakeTwo256, Extrinsic as ExtrinsicT, IdentifyAccount, IdentityLookup, Verify},
};

pub fn get_test_accounts() -> Vec<AccountId> {
	let alice: AccountId = AccountId::from(Public::from_slice(&[1u8; 32]).unwrap());
	let bob: AccountId = AccountId::from(Public::from_slice(&[2u8; 32]).unwrap());
	let charlie: AccountId = AccountId::from(Public::from_slice(&[3u8; 32]).unwrap());

	[alice, bob, charlie].to_vec()
}

pub type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
pub type Block = frame_system::mocking::MockBlock<Test>;

/// Balance of an account.
pub type Balance = u128;

const MILLISECS_PER_BLOCK: u64 = 4000;

//Mock runtime for our tests
frame_support::construct_runtime!(
	pub enum Test
	where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		FiatRampsExample: fiat_ramps,
		Timestamp: pallet_timestamp,
		Sudo: pallet_sudo,
		Balances: pallet_balances,
	}
);

parameter_types! {
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(Weight::from_ref_time(1024));
	pub const BlockHashCount: u64 = 2400;

}
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = sp_core::sr25519::Public;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

pub type Extrinsic = TestXt<RuntimeCall, ()>;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

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
	type RuntimeEvent = RuntimeEvent;
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

impl pallet_sudo::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
}

parameter_types! {
	pub const MinimumInterval: u64 = MILLISECS_PER_BLOCK;
	pub const UnsignedPriority: u64 = 1000;
	/// Maximum number of characters in IBAN
	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
	pub const MaxIbanLength: u32 = 64;
	/// Bound of string length
	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
	pub const MaxStringLength: u32 = 255;
	/// OCW account
	pub OcwAccount: AccountId = AccountId::from(Public::from_slice(&[1u8; 32]).unwrap());
	/// Bound for statements
	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
	pub const MaxStatements: u32 = 255;
}

impl fiat_ramps::Config for Test {
	type AuthorityId = fiat_ramps::crypto::OcwAuthId;
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type TimeProvider = Timestamp;
	type MinimumInterval = MinimumInterval;
	type UnsignedPriority = UnsignedPriority;
	type MaxIbanLength = MaxIbanLength;
	type MaxStringLength = MaxStringLength;
	type OcwAccount = OcwAccount;
	type MaxStatements = MaxStatements;
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Test
where
	RuntimeCall: From<C>,
{
	type OverarchingCall = RuntimeCall;
	type Extrinsic = Extrinsic;
}

impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Test
where
	RuntimeCall: From<LocalCall>,
{
	fn create_transaction<C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>>(
		call: RuntimeCall,
		_public: <Signature as Verify>::Signer,
		_account: AccountId,
		nonce: u64,
	) -> Option<(RuntimeCall, <Extrinsic as ExtrinsicT>::SignaturePayload)> {
		Some((call, (nonce, ())))
	}
}

/// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();

	// Give initial balances for test accounts
	pallet_balances::GenesisConfig::<Test> {
		balances: get_test_accounts()
			.clone()
			.into_iter()
			.map(|x| (x, 100_000_000_000_000))
			.collect::<Vec<_>>(),
	}
	.assimilate_storage(&mut t)
	.unwrap();

	t.into()
}
