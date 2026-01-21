use crate::StableCoin;

use super::{
	AccountId, AllPalletsWithSystem, ParachainInfo, ParachainSystem, PolkadotXcm, Runtime,
	RuntimeCall, RuntimeEvent, RuntimeOrigin, WeightToFee, XcmpQueue,
};
use fiat_ramps::types::XcmInterface;
use frame_support::{
	parameter_types,
	traits::{ConstU32, Contains, ContainsPair, Everything, Nothing},
	weights::Weight,
};
use frame_system::EnsureRoot;
use pallet_xcm::XcmPassthrough;
use parachains_common::impls::AssetsFrom;
use polkadot_parachain_primitives::primitives::Sibling;
use sp_core::Get;
use sp_std::{marker::PhantomData, sync::Arc, vec};
use staging_xcm::{
	latest::{prelude::*, Junctions::X1, Location as MultiLocation},
	opaque::latest::WESTEND_GENESIS_HASH,
};
use staging_xcm_builder::WithUniqueTopic;
#[allow(deprecated)]
use staging_xcm_builder::{
	AccountId32Aliases, AllowExplicitUnpaidExecutionFrom, AllowTopLevelPaidExecutionFrom,
	DenyReserveTransferToRelayChain, DenyThenTry, EnsureXcmOrigin, FixedWeightBounds,
	FrameTransactionalProcessor, FungibleAdapter, IsConcrete, NativeAsset, ParentIsPreset,
	RelayChainAsNative, SiblingParachainAsNative, SiblingParachainConvertsVia,
	SignedAccountId32AsNative, SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit,
	TrailingSetTopicAsId, UsingComponents, WithComputedOrigin,
};
use staging_xcm_executor::XcmExecutor;

parameter_types! {
	pub const RelayLocation: MultiLocation = MultiLocation::parent();
	pub const OurLocation: MultiLocation = MultiLocation::here();
	pub AssetHubLocation: MultiLocation = MultiLocation::new(1, X1(Arc::new([Parachain(1000)])));
	pub const RelayNetwork: Option<NetworkId> = Some(NetworkId::ByGenesis(WESTEND_GENESIS_HASH));
	pub RelayChainOrigin: RuntimeOrigin = cumulus_pallet_xcm::Origin::Relay.into();
	pub UniversalLocation: InteriorLocation = Parachain(ParachainInfo::parachain_id().into()).into();
}

parameter_types! {
	/// The location that this chain recognizes as the Relay network's Asset Hub.
	pub SystemAssetHubLocation: Location = Location::new(1, [Parachain(1000)]);
	// ALWAYS ensure that the index in PalletInstance stays up-to-date with
	// the Relay Chain's Asset Hub's Assets pallet index
	pub SystemAssetHubAssetsPalletLocation: Location =
		Location::new(1, [Parachain(1000), PalletInstance(50)]);
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
	pub LocalTeleportableToAssetHub: Location = Location::here();
	pub LocalTeleportableToAssetHubV3: staging_xcm::v3::Location = staging_xcm::v3::Location::new(
		0,
		[staging_xcm::v3::Junction::PalletInstance(11)]
	);

	/// The Penpal runtime is utilized for testing with various environment setups.
	/// This storage item provides the opportunity to customize testing scenarios
	/// by configuring the trusted asset from the `SystemAssetHub`.
	///
	/// By default, it is configured as a `SystemAssetHubLocation` and can be modified using `System::set_storage`.
	pub storage CustomizableAssetFromSystemAssetHub: Location = SystemAssetHubLocation::get();
}

/// Type for specifying how a `MultiLocation` can be converted into an `AccountId`. This is used
/// when determining ownership of accounts for asset transacting and when attempting to use XCM
/// `Transact` in order to determine the dispatch Origin.
pub type LocationToAccountId = (
	// The parent (Relay-chain) origin converts to the parent `AccountId`.
	ParentIsPreset<AccountId>,
	// Sibling parachain origins convert to AccountId via the `ParaId::into`.
	SiblingParachainConvertsVia<Sibling, AccountId>,
	// Straight up local `AccountId32` origins just alias directly to `AccountId`.
	AccountId32Aliases<RelayNetwork, AccountId>,
);

/// Means for transacting assets on this chain.
pub type LocalAssetTransactor = FungibleAdapter<
	// Use this currency:
	StableCoin,
	// Use this currency when it is a fungible asset matching the given location or name:
	IsConcrete<OurLocation>,
	// Do a simple punn to convert an AccountId32 MultiLocation into a native chain account ID:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We don't track any teleports.
	(),
>;

/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which can
/// biases the kind of local `Origin` it will become.
pub type XcmOriginToTransactDispatchOrigin = (
	// Sovereign account converter; this attempts to derive an `AccountId` from the origin location
	// using `LocationToAccountId` and then turn that into the usual `Signed` origin. Useful for
	// foreign chains who want to have a local sovereign account on this chain which they control.
	SovereignSignedViaLocation<LocationToAccountId, RuntimeOrigin>,
	// Native converter for Relay-chain (Parent) location; will convert to a `Relay` origin when
	// recognized.
	RelayChainAsNative<RelayChainOrigin, RuntimeOrigin>,
	// Native converter for sibling Parachains; will convert to a `SiblingPara` origin when
	// recognized.
	SiblingParachainAsNative<cumulus_pallet_xcm::Origin, RuntimeOrigin>,
	// Native signed account converter; this just converts an `AccountId32` origin into a normal
	// `RuntimeOrigin::Signed` origin of the same 32-byte value.
	SignedAccountId32AsNative<RelayNetwork, RuntimeOrigin>,
	// Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
	XcmPassthrough<RuntimeOrigin>,
);

parameter_types! {
	pub const Stablecoin: AssetFilter = Wild(AllOf { fun: WildFungible, id: AssetId(MultiLocation::here()) });
	// One XCM operation is 1_000_000_000 weight - almost certainly a conservative estimate.
	pub UnitWeightCost: Weight = Weight::from_parts(1_000_000, 64 * 1024);
	pub const MaxInstructions: u32 = 100;
	pub const MaxAssetsIntoHolding: u32 = 64;
	pub AssetHubTrustedTeleporter: (AssetFilter, MultiLocation) = (Stablecoin::get(), AssetHubLocation::get());
}

pub struct ParentOrParentsExecutivePlurality;
impl Contains<Location> for ParentOrParentsExecutivePlurality {
	fn contains(location: &Location) -> bool {
		matches!(location.unpack(), (1, []) | (1, [Plurality { id: BodyId::Executive, .. }]))
	}
}

pub type Barrier = TrailingSetTopicAsId<
	DenyThenTry<
		DenyReserveTransferToRelayChain,
		(
			TakeWeightCredit,
			WithComputedOrigin<
				(
					AllowTopLevelPaidExecutionFrom<Everything>,
					AllowExplicitUnpaidExecutionFrom<ParentOrParentsExecutivePlurality>,
					// ^^^ Parent and its exec plurality get free execution
				),
				UniversalLocation,
				ConstU32<8>,
			>,
		),
	>,
>;

// /// Accepts asset with ID `AssetLocation` and is coming from `Origin` chain.
// pub struct AssetFromChain<AssetLocation, Origin>(PhantomPinned<(AssetLocation, Origin)>);
// impl<AssetLocation: Get<Location>, Origin: Get<Location>> ContainsPair<Asset, Location>
// 	for AssetFromChain<AssetLocation, Origin>
// {
// 	fn contains(asset: &Asset, origin: &Location) -> bool {
// 		log::trace!(target: "xcm::contains", "AssetFromChain asset: {:?}, origin: {:?}", asset, origin);
// 		*origin == Origin::get() &&
// 			matches!(asset.id.clone(), AssetId(id) if id == AssetLocation::get())
// 	}
// }

pub type TrustedReserves = (NativeAsset, AssetsFrom<AssetHubLocation>);

/// Accepts asset with ID `AssetLocation` and is coming from `Origin` chain.
pub struct AssetFromChain<AssetLocation, Origin>(PhantomData<(AssetLocation, Origin)>);
impl<AssetLocation: Get<Location>, Origin: Get<Location>> ContainsPair<Asset, Location>
	for AssetFromChain<AssetLocation, Origin>
{
	fn contains(asset: &Asset, origin: &Location) -> bool {
		log::trace!(target: "xcm::contains", "AssetFromChain asset: {:?}, origin: {:?}", asset, origin);
		*origin == Origin::get() &&
			matches!(asset.id.clone(), AssetId(id) if id == AssetLocation::get())
	}
}

// pub type TrustedTeleporters = (staging_xcm_builder::Case<AssetHubTrustedTeleporter>,);
pub type TrustedTeleporters =
	(AssetFromChain<LocalTeleportableToAssetHub, SystemAssetHubLocation>,);
pub struct XcmConfig;

impl staging_xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	// How to withdraw and deposit an asset.
	type AssetTransactor = LocalAssetTransactor;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	type IsReserve = TrustedReserves;
	type IsTeleporter = TrustedTeleporters; // Teleporting is disabled.
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type Trader = UsingComponents<WeightToFee, OurLocation, AccountId, StableCoin, ()>;
	type ResponseHandler = PolkadotXcm;
	type AssetTrap = PolkadotXcm;
	type AssetClaims = PolkadotXcm;
	type SubscriptionService = PolkadotXcm;
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type AssetLocker = ();
	type AssetExchanger = ();
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = FrameTransactionalProcessor;
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = PolkadotXcm;
}

/// No local origins on this chain are allowed to dispatch XCM sends/executions.
pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
#[cfg(not(test))]
pub type XcmRouter = WithUniqueTopic<(
	// Two routers - use UMP to communicate with the relay chain:
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, ()>,
	// ..and XCMP to communicate with the sibling chains.
	XcmpQueue,
)>;

/// Use different router in `xcm-simulator` tests.
#[cfg(test)]
pub type XcmRouter = crate::e2e_tests::ParachainXcmRouter<ParachainInfo>;

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmExecuteFilter = Nothing;
	// ^ Disable dispatchable execute on the XCM pallet.
	// Needs to be `Everything` for local testing.
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	type XcmReserveTransferFilter = Everything;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type UniversalLocation = UniversalLocation;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;

	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	// ^ Override for AdvertisedXcmVersion default
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
	type Currency = StableCoin;
	type CurrencyMatcher = ();
	type TrustedLockers = ();
	type SovereignAccountOf = LocationToAccountId;
	type MaxLockers = ConstU32<8>;
	type WeightInfo = pallet_xcm::TestWeightInfo;
	type AdminOrigin = EnsureRoot<AccountId>;
	type MaxRemoteLockConsumers = ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();
}

impl cumulus_pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}

/// Simple wrapper around `PolkadotXcm` to provide a `XcmInterface` implementation.
pub struct EbicsXcmInterface<T>(sp_std::marker::PhantomData<T>);
impl<R, A, B, T> XcmInterface<R, B, A> for EbicsXcmInterface<T>
where
	A: From<[u8; 32]> + Into<[u8; 32]>,
	B: From<u128> + Into<u128>,
	T: pallet_xcm::Config,
	<T as frame_system::Config>::RuntimeOrigin: From<R>,
{
	fn burn(_origin: R, _amount: B, _dest: A) -> frame_support::dispatch::DispatchResult {
		// To be implemented.
		Ok(())
	}
	fn mint(origin: R, amount: B, dest: A) -> frame_support::dispatch::DispatchResult {
		pallet_xcm::Pallet::<T>::teleport_assets(
			origin.into(),
			sp_std::boxed::Box::new(AssetHubLocation::get().into()),
			sp_std::boxed::Box::new(
				MultiLocation {
					parents: 0,
					interior: staging_xcm::latest::Junction::AccountId32 {
						network: Some(NetworkId::ByGenesis(WESTEND_GENESIS_HASH)),
						id: dest.into(),
					}
					.into(),
				}
				.into(),
			),
			sp_std::boxed::Box::new(
				vec![Asset {
					id: AssetId(MultiLocation { parents: 0, interior: Here }),
					fun: Fungible(amount.into()),
				}]
				.into(),
			),
			0,
		)?;

		Ok(())
	}
}
