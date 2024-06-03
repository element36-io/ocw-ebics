// Copyright 2023-2024 Freeverse.io
// This file is part of LAOS.

// LAOS is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// LAOS is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with LAOS.  If not, see <http://www.gnu.org/licenses/>.

//! Mock Kusama relay chain runtime.

use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{ConstU32, Everything, Nothing, ProcessMessage, ProcessMessageError},
	weights::{Weight, WeightMeter},
};
use frame_system::EnsureRoot;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_runtime_parachains::{
	configuration,
	inclusion::{AggregateMessageOrigin, UmpQueueId},
	origin, shared,
};
use sp_core::{ConstU16, ConstU64, H256};
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	AccountId32,
};
use staging_xcm::latest::{prelude::*, Location as MultiLocation};
use staging_xcm_builder::{
	AccountId32Aliases, AllowUnpaidExecutionFrom, ChildParachainAsNative,
	ChildParachainConvertsVia, ChildSystemParachainAsSuperuser, FixedRateOfFungible,
	FixedWeightBounds, FungibleAdapter as XcmCurrencyAdapter, IsConcrete,
	SignedAccountId32AsNative, SignedToAccountId32, SovereignSignedViaLocation,
};
use staging_xcm_executor::XcmExecutor;

pub type AccountId = AccountId32;
pub type Balance = u128;

construct_runtime!(
	pub struct Runtime
	{
		System: frame_system::{Pallet, Call, Storage, Config<T>, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		ParasOrigin: origin::{Pallet, Origin},
		XcmPallet: pallet_xcm::{Pallet, Call, Storage, Event<T>, Origin},
		MessageQueue: pallet_message_queue::{Pallet, Event<T>},
	}
);

pub type Block = frame_system::mocking::MockBlock<Runtime>;

#[derive_impl(frame_system::config_preludes::ParaChainDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Block = Block;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type AccountData = pallet_balances::AccountData<Balance>;
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type Hashing = BlakeTwo256;
	type Nonce = u64;
	type Hash = H256;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type OnKilledAccount = ();
	type OnNewAccount = ();
	type OnSetCode = ();
	type SS58Prefix = ConstU16<42>;
	type SystemWeightInfo = ();
	type MaxConsumers = ConstU32<16>;
	type RuntimeOrigin = RuntimeOrigin;
}

parameter_types! {
	pub ExistentialDeposit: Balance = 1;
}
impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type RuntimeHoldReason = ();
	type DustRemoval = ();
	type MaxLocks = ConstU32<50>;
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type FreezeIdentifier = [u8; 8];
	type MaxFreezes = ();
	type RuntimeFreezeReason = ();
}

impl shared::Config for Runtime {
	type DisabledValidators = ();
}

impl configuration::Config for Runtime {
	type WeightInfo = configuration::TestWeightInfo;
}

parameter_types! {
	pub const KsmLocation: MultiLocation = Here.into_location();
	pub const KusamaNetwork: NetworkId = NetworkId::Kusama;
	pub UniversalLocation: InteriorLocation = Here;
}

pub type SovereignAccountOf =
	(ChildParachainConvertsVia<ParaId, AccountId>, AccountId32Aliases<KusamaNetwork, AccountId>);

pub type LocalAssetTransactor =
	XcmCurrencyAdapter<Balances, IsConcrete<KsmLocation>, SovereignAccountOf, AccountId, ()>;

type LocalOriginConverter = (
	SovereignSignedViaLocation<SovereignAccountOf, RuntimeOrigin>,
	ChildParachainAsNative<origin::Origin, RuntimeOrigin>,
	SignedAccountId32AsNative<KusamaNetwork, RuntimeOrigin>,
	ChildSystemParachainAsSuperuser<ParaId, RuntimeOrigin>,
);

parameter_types! {
	pub const BaseXcmWeight: Weight = Weight::from_parts(1_000, 0);
	pub KsmPerSecond: (AssetId, u128, u128) = (AssetId(KsmLocation::get()), 1, 1024 * 1024);
	pub const MaxInstructions: u32 = 100;
}

pub type XcmRouter = super::RelayChainXcmRouter;
pub type Barrier = AllowUnpaidExecutionFrom<Everything>;

pub struct XcmConfig;
impl staging_xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	type AssetTransactor = LocalAssetTransactor;
	type OriginConverter = LocalOriginConverter;
	type IsReserve = ();
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = FixedWeightBounds<BaseXcmWeight, RuntimeCall, MaxInstructions>;
	type Trader = FixedRateOfFungible<KsmPerSecond, ()>;
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetClaims = ();
	type SubscriptionService = ();

	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = ConstU32<64>;
	type AssetLocker = ();
	type AssetExchanger = ();
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = ();
}

pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, KusamaNetwork>;

pub type LocationToAccountId = (ChildParachainConvertsVia<ParaId, AccountId>,);

#[cfg(feature = "runtime-benchmarks")]
parameter_types! {
	pub ReachableDest: Option<MultiLocation> = Some(Parachain(1).into());
}

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SendXcmOrigin = staging_xcm_builder::EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	// Anyone can execute XCM messages locally...
	type ExecuteXcmOrigin =
		staging_xcm_builder::EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmExecuteFilter = Nothing;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	type XcmReserveTransferFilter = Everything;
	type Weigher = FixedWeightBounds<BaseXcmWeight, RuntimeCall, MaxInstructions>;
	type UniversalLocation = UniversalLocation;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;

	type Currency = Balances;
	type CurrencyMatcher = ();
	type TrustedLockers = ();
	type SovereignAccountOf = LocationToAccountId;
	type MaxLockers = ConstU32<0>;
	type WeightInfo = pallet_xcm::TestWeightInfo;
	type MaxRemoteLockConsumers = ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();
	#[cfg(feature = "runtime-benchmarks")]
	type ReachableDest = ReachableDest;
	type AdminOrigin = EnsureRoot<AccountId>;
}

parameter_types! {
	pub const FirstMessageFactorPercent: u64 = 100;
}

impl origin::Config for Runtime {}

parameter_types! {
	/// Amount of weight that can be spent per block to service messages.
	pub MessageQueueServiceWeight: Weight = Weight::from_parts(4_000_000_000, 1_000_000);
	pub const MessageQueueHeapSize: u32 = 655_536;
	pub const MessageQueueMaxStale: u32 = 256;
}

/// Message processor to handle any messages that were enqueued into the `MessageQueue` pallet.
pub struct MessageProcessor;
impl ProcessMessage for MessageProcessor {
	type Origin = AggregateMessageOrigin;

	fn process_message(
		message: &[u8],
		origin: Self::Origin,
		meter: &mut WeightMeter,
		id: &mut [u8; 32],
	) -> Result<bool, ProcessMessageError> {
		let para = match origin {
			AggregateMessageOrigin::Ump(UmpQueueId::Para(para)) => para,
		};
		staging_xcm_builder::ProcessXcmMessage::<
			Junction,
			staging_xcm_executor::XcmExecutor<XcmConfig>,
			RuntimeCall,
		>::process_message(message, Junction::Parachain(para.into()), meter, id)
	}
}

impl pallet_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Size = u32;
	type HeapSize = MessageQueueHeapSize;
	type MaxStale = MessageQueueMaxStale;
	type ServiceWeight = MessageQueueServiceWeight;
	type MessageProcessor = MessageProcessor;
	type QueueChangeHandler = ();
	type WeightInfo = ();
	type QueuePausedQuery = ();
}
