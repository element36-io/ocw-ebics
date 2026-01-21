//! Hyperfridge parachain e2e tests

#![cfg(test)]
pub mod mock;

use self::xcm_config::Stablecoin;

use super::{
	e2e_tests::mock::{
		asset_hub, asset_hub::ForeignAssets, AssetHub, EbicsPara, EbicsParachainXcm, MockNet,
		Relay, ALICE, BOB, CHARLIE, INITIAL_BALANCE,
	},
	*,
};
use crate::{
	e2e_tests::mock::{AssetHubXcm, EbicsParachainStablecoin},
	AccountId, Runtime,
};
use frame_support::{
	assert_noop, assert_ok,
	traits::{fungible::Mutate, tokens::fungibles::Inspect},
	weights::Weight,
};
use frame_system::RawOrigin;
pub use mock::ParachainXcmRouter;
use sp_std::sync::Arc;
use staging_xcm::latest::{
	Asset as MultiAsset,
	Instruction::{BuyExecution, DepositAsset, Transact, WithdrawAsset},
	Junction::{AccountId32, Parachain},
	Junctions::X1,
	Location as MultiLocation,
	NetworkId,
	WeightLimit::Unlimited,
	WildAsset::All,
	Xcm,
	WESTEND_GENESIS_HASH,
};
use xcm_simulator::{Junctions::Here, TestExt};

/// Our location in the network
fn our_location() -> MultiLocation {
	MultiLocation {
		parents: 1,
		interior: [Parachain(2000)].into(),
	}
}

// Cross-chain asset transfer tests
//
// The scenario for these tests is as follows:
//
// - EbicsPara is our parachain's runtime.
// - AssetHub is a mock Asset Hub parachain.

/// Utility function for transacting a call to other chain
///
/// - Buys 1 UNIT of execution weight
/// - Transacts the call
/// - Deposits surplus asset to ALITH
pub(crate) fn transact<Runtime: pallet_xcm::Config>(dest: MultiLocation, encoded_call: Vec<u8>) {
	let xcm_call = staging_xcm::opaque::VersionedXcm::V5(Xcm(vec![
		WithdrawAsset(
			vec![MultiAsset {
				id: MultiLocation { parents: 1, interior: Here }.into(),
				fun: cumulus_primitives_core::Fungibility::Fungible(UNIT * 10),
			}]
			.into(),
		),
		BuyExecution {
			fees: MultiAsset {
				id: MultiLocation { parents: 1, interior: Here }.into(),
				fun: cumulus_primitives_core::Fungibility::Fungible(UNIT * 10),
			},
			weight_limit: Unlimited,
		},
		Transact {
			origin_kind: OriginKind::Superuser,
			fallback_max_weight: Some(Weight::from_parts(20_000_000_000, 1024 * 1024)),
			call: encoded_call.into(),
		},
		DepositAsset {
			assets: Wild(All),
			beneficiary: MultiLocation {
				parents: 0,
				interior: AccountId32 { network: None, id: ALICE.into() }.into(),
			},
		},
	]));

	assert_ok!(pallet_xcm::Pallet::<Runtime>::send(
		RawOrigin::Root.into(),
		Box::new(dest.into()),
		Box::new(xcm_call),
	));
}

/// This tests following scenario:
///
/// - Alice already has stablecoins minted in `Hyperfridge` parachain, i.e she linked her
/// bank account with the parachain.
/// - Now, Alice wants to use this stablecoin to buy swap or trade some other asset in the
/// `AssetHub` parachain.
/// - Alice sends a `FiatRamps::teleport_to_asset_hub` extrinsic to `Hyperfridge` parachain.
/// - `Hyperfridge` parachain sends (teleports) the stablecoin to `AssetHub` parachain.
/// - `AssetHub` parachain receives the stablecoin and mints the same amount of stablecoin in
/// `AssetHub` parachain, represented by the foreign asset. Transaction fees are deducted from
/// the stablecoin, since it is a sufficient asset in `AssetHub` parachain.
/// - Alice can now use this stablecoin to buy other assets in `AssetHub` parachain.
///
/// Now, reverse of this scenario:
/// - Alice wants to off ramp her stablecoins
/// - Alice sends a `AssetHubXcm::teleport_assets` extrinsic to `AssetHub` parachain.
/// - `AssetHub` parachain sends (teleports) the stablecoin to `Hyperfridge` parachain.
/// - `Hyperfridge` parachain receives the stablecoin and mints the same amount of stablecoin in
/// `Hyperfridge` parachain, represented by the foreign asset. Transaction fees are deducted from
/// the stablecoin, since it is a sufficient asset in `Hyperfridge` parachain.
/// - Alice can now off ramp using the `FiatRamps::burn_request` extrinsic.
#[test]
fn ebics_para_teleport_to_asset_hub_and_back() {
	MockNet::reset();

	Relay::execute_with(|| {});

	AssetHub::execute_with(|| {
		use asset_hub::{RuntimeEvent, System};
		// check that asset is created
		assert_eq!(ForeignAssets::total_balance(our_location(), &ALICE), 0);

		assert!(System::events().iter().any(|r| {
			matches!(
				r.event,
				RuntimeEvent::MsgQueue(mock_msg_queue::Event::ExecutedDownward { .. })
			)
		}));
		assert!(System::events().iter().any(|r| {
			matches!(
				r.event,
				RuntimeEvent::ForeignAssets(pallet_assets::Event::ForceCreated { .. })
			)
		}));
		// reset the events
		System::reset_events();
	});

	let amount = 100 * UNIT;
	EbicsPara::execute_with(|| {
		<StableCoin as Mutate<crate::AccountId>>::mint_into((&ALICE).into(), INITIAL_BALANCE)
			.unwrap();
		<StableCoin as Mutate<crate::AccountId>>::mint_into((&BOB).into(), INITIAL_BALANCE)
			.unwrap();

		assert_ok!(FiatRamps::teleport_to_asset_hub(
			RuntimeOrigin::signed(ALICE.into()),
			amount,
			Some(ALICE.into())
		));

		System::reset_events();
	});

	AssetHub::execute_with(|| {
		use asset_hub::{RuntimeEvent, System};

		assert!(System::events().iter().any(|r| {
			matches!(r.event, RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. }))
		}));

		// try to round up the amount to the nearest second, since we don't know how much fee was
		// charged
		let alice_balance = ForeignAssets::total_balance(our_location(), &ALICE);
		assert!(amount / alice_balance > 0);

		// now send it back
		let destination = MultiLocation { parents: 1, interior: X1(Arc::new([Parachain(2000)])) };

		assert_ok!(AssetHubXcm::teleport_assets(
			asset_hub::RuntimeOrigin::signed(ALICE.into()),
			Box::new(destination.into()),
			Box::new(
				MultiLocation {
					parents: 0,
					interior: AccountId32 { network: Some(NetworkId::ByGenesis(WESTEND_GENESIS_HASH)), id: BOB.into() }
						.into()
				}
				.into(),
			),
			Box::new(
				vec![MultiAsset {
					id: AssetId(MultiLocation {
						parents: 1,
						interior: X1(Arc::new([Parachain(2000)]))
					}),
					fun: Fungible(alice_balance)
				},]
				.into()
			),
			0,
		),);

		System::reset_events();
	});

	EbicsPara::execute_with(|| {
		use crate::{RuntimeEvent, System};

		assert!(System::events().iter().any(|r| {
			matches!(r.event, RuntimeEvent::MockMsgQueue(mock_msg_queue::Event::Success { .. }))
		}));

		assert!(System::events().iter().any(|r| {
			matches!(r.event, RuntimeEvent::StableCoin(pallet_balances::Event::Minted { .. }))
		}));

		let bob_balance = EbicsParachainStablecoin::free_balance(AccountId::from(BOB));

		// we can't really know how much fee was charged, so we can only check that the balance
		// increased, and that the amount is at least the amount we sent
		assert!(bob_balance > INITIAL_BALANCE);
		assert!(amount / (bob_balance - INITIAL_BALANCE) > 0);
	});
}

#[test]
fn insufficient_balance_teleport_fails() {
	MockNet::reset();

	EbicsPara::execute_with(|| {
		assert_noop!(
			FiatRamps::teleport_to_asset_hub(
				RuntimeOrigin::signed(CHARLIE.into()),
				100 * UNIT,
				None
			),
			fiat_ramps::Error::<Runtime>::InsufficientBalance
		);

		<StableCoin as Mutate<crate::AccountId>>::mint_into((&CHARLIE).into(), INITIAL_BALANCE)
			.unwrap();

		assert_noop!(
			FiatRamps::teleport_to_asset_hub(
				RuntimeOrigin::signed(ALICE.into()),
				INITIAL_BALANCE + 1,
				None
			),
			fiat_ramps::Error::<Runtime>::InsufficientBalance
		);
	});
}

/// This tests following config values:
/// - `pallet_xcm::XcmExecuteFilter`
#[test]
fn local_xcm_execution_is_filtered_out() {
	EbicsPara::execute_with(|| {
		assert!(EbicsParachainXcm::execute(
			RuntimeOrigin::root(),
			Box::new(VersionedXcm::V5(Xcm(vec![
				WithdrawAsset(
					vec![MultiAsset { id: MultiLocation::here().into(), fun: Fungible(UNIT) }]
						.into(),
				),
				BuyExecution {
					fees: MultiAsset { id: MultiLocation::here().into(), fun: Fungible(UNIT) },
					weight_limit: Unlimited,
				},
				Transact {
					origin_kind: OriginKind::SovereignAccount,
					fallback_max_weight: Some(Weight::from_parts(1_000_000_000, 1024 * 1024)),
					call: vec![].into(),
				},
				DepositAsset {
					assets: Wild(All),
					beneficiary: MultiLocation {
						parents: 0,
						interior: AccountId32 { network: None, id: ALICE.into() }.into(),
					}
				},
			]))),
			Weight::from_parts(1_000_000_000, 1024 * 1024),
		)
		.is_err());
	});
}
