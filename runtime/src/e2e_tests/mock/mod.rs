//! Mock types for testing XCM.

pub(crate) mod asset_hub;
// pub(crate) mod parachain;
pub(crate) mod relay_chain;

use crate::{AccountId, Runtime as RealParachainRuntime, StableCoin, StableCoinInstance};
use asset_hub::Runtime as MockParachainRuntime;
use frame_system::RawOrigin;
use relay_chain::Runtime as MockRelayChainRuntime;
use sp_runtime::BuildStorage;
use sp_std::sync::Arc;
use staging_xcm::latest::{prelude::*, Location as MultiLocation};
use staging_xcm_executor::traits::ConvertLocation;
use xcm_simulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain, TestExt};

use crate::UNIT;

use self::asset_hub::ForeignAssetsInstance;

use super::transact;

pub const ALICE: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([0xFAu8; 32]);
pub const BOB: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([0xFBu8; 32]);
pub const CHARLIE: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([0xFCu8; 32]);

pub const INITIAL_BALANCE: u128 = 1_000 * UNIT;

decl_test_parachain! {
	pub struct EbicsPara {
		Runtime = RealParachainRuntime,
		XcmpMessageHandler = crate::MockMsgQueue,
		DmpMessageHandler = crate::MockMsgQueue,
		new_ext = para_ext::<RealParachainRuntime>(2000),
	}
}

decl_test_parachain! {
	pub struct AssetHub {
		Runtime = MockParachainRuntime,
		XcmpMessageHandler = asset_hub::MsgQueue,
		DmpMessageHandler = asset_hub::MsgQueue,
		new_ext = para_ext::<MockParachainRuntime>(1000),
	}
}

decl_test_relay_chain! {
	pub struct Relay {
		Runtime = MockRelayChainRuntime,
		RuntimeCall = relay_chain::RuntimeCall,
		RuntimeEvent = relay_chain::RuntimeEvent,
		XcmConfig = relay_chain::XcmConfig,
		MessageQueue = relay_chain::MessageQueue,
		System = relay_chain::System,
		new_ext = relay_ext(),
	}
}

decl_test_network! {
	pub struct MockNet {
		relay_chain = Relay,
		parachains = vec![
			(2000, EbicsPara),
			(1000, AssetHub),
		],
	}
}

pub type EbicsParachainXcm = pallet_xcm::Pallet<RealParachainRuntime>;
pub type AssetHubXcm = pallet_xcm::Pallet<asset_hub::Runtime>;
pub type EbicsParachainStablecoin =
	pallet_balances::Pallet<RealParachainRuntime, StableCoinInstance>;

pub fn parent_account_id() -> asset_hub::AccountId {
	asset_hub::LocationToAccountId::convert_location(&MultiLocation { parents: 1, interior: Here })
		.unwrap()
}

/// Derive parachain sovereign account on relay chain, from parachain Id
pub fn child_para_account_id(para: u32) -> relay_chain::AccountId {
	relay_chain::LocationToAccountId::convert_location(&MultiLocation {
		parents: 0,
		interior: Junctions::X1(Arc::new([Parachain(para)])),
	})
	.unwrap()
}

/// Derive parachain sovereign account on a sibling parachain, from parachain Id
pub fn sibling_para_account_id(para: u32) -> asset_hub::AccountId {
	asset_hub::LocationToAccountId::convert_location(&MultiLocation {
		parents: 1,
		interior: Junctions::X1(Arc::new([Parachain(para)])),
	})
	.unwrap()
}

/// Prepare parachain test externality
pub fn para_ext<Runtime>(para_id: u32) -> sp_io::TestExternalities
where
	Runtime: pallet_balances::Config + staging_parachain_info::Config + pallet_xcm::Config,
	Runtime::AccountId: From<sp_runtime::AccountId32> + Into<sp_runtime::AccountId32>,
	Runtime::Balance: From<u128> + Into<u128>,
	<Runtime as pallet_balances::Config>::Balance: From<u128> + Into<u128>,
{
	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	// Initialize parachain account with some balance
	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![
			(ALICE.into(), INITIAL_BALANCE.into()),
			(BOB.into(), INITIAL_BALANCE.into()),
			(parent_account_id().into(), INITIAL_BALANCE.into()),
			(sibling_para_account_id(1000).into(), INITIAL_BALANCE.into()),
			(sibling_para_account_id(2000).into(), INITIAL_BALANCE.into()),
		],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	staging_parachain_info::GenesisConfig::<Runtime> {
		_config: Default::default(),
		parachain_id: para_id.into(),
	}
	.assimilate_storage(&mut t)
	.unwrap();

	pallet_xcm::GenesisConfig::<Runtime> { _config: Default::default(), safe_xcm_version: Some(3) }
		.assimilate_storage(&mut t)
		.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);

	ext.execute_with(|| {
		asset_hub::System::set_block_number(1);
	});

	ext
}

/// Prepare relay chain test externality
pub fn relay_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<MockRelayChainRuntime>::default()
		.build_storage()
		.unwrap();

	pallet_balances::GenesisConfig::<MockRelayChainRuntime> {
		balances: vec![
			(ALICE, INITIAL_BALANCE),
			(child_para_account_id(1000), INITIAL_BALANCE),
			(child_para_account_id(2000), INITIAL_BALANCE),
		],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	pallet_xcm::GenesisConfig::<MockRelayChainRuntime> {
		_config: Default::default(),
		safe_xcm_version: Some(3),
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		relay_chain::System::set_block_number(1);
		// in AssetHub, we need to set up and register the token of EbicsPara.
		// As a Stablecoin, we assume that the token is a sufficient token in AssetHub,
		// so we should force create the token in AssetHub from Relay chain.
		// NOTE: AssetHub does not have `sudo` pallet, so root calls should come from Relay chain.
		let create_asset = asset_hub::RuntimeCall::ForeignAssets(pallet_assets::Call::<
			asset_hub::Runtime,
			ForeignAssetsInstance,
		>::force_create {
			id: staging_xcm::v5::Location {
				parents: 1,
				interior: [staging_xcm::v5::Junction::Parachain(2000)].into(),
			},
			owner: sibling_para_account_id(2000).into(),
			min_balance: UNIT / 1_000_000_000_000,
			is_sufficient: true,
		});

		transact::<MockRelayChainRuntime>(
			MultiLocation { parents: 0, interior: Junctions::X1(Arc::new([Parachain(1000)])) },
			create_asset.encode(),
		);
		relay_chain::System::reset_events();
	});
	ext
}
