//! Implementations of traits and types for the pallet
use super::*;

use core::convert::TryInto;

use self::utils::{extract_value, parse_object};
use crate::types::*;
use sp_std::default::Default;

mod ext;
mod ocw;
mod types;

/// Utility functions
impl<T: Config> Pallet<T> {
	/// AccountId associated with Pallet
	pub(crate) fn account_id() -> T::AccountId {
		PALLET_ID.try_into_account().expect("Should never fail")
	}

	/// Returns the action to take
	pub(crate) fn choose_ocw_activity() -> OcwActivity {
		// Get last activity recorded
		let last_activity_ref = StorageValueRef::persistent(b"fiat_ramps:last_activity");

		// Fetch last activity and set new one
		// Activity order is following: Process statements -> Process burn requests -> Do nothing -> Repeat
		let last_activity =
			last_activity_ref.mutate(|val: Result<Option<OcwActivity>, StorageRetrievalError>| {
				match val {
					Ok(Some(activity)) => match activity {
						OcwActivity::FetchStatements => Ok(OcwActivity::VerifyAndProcessStatements),
						OcwActivity::VerifyAndProcessStatements =>
							Ok(OcwActivity::ProcessBurnRequests),
						OcwActivity::ProcessBurnRequests => Ok(OcwActivity::None),
						OcwActivity::None => Ok(OcwActivity::FetchStatements),
					},
					Ok(None) => Ok(OcwActivity::FetchStatements),
					_ => Ok(OcwActivity::None),
				}
			});

		match last_activity {
			Ok(activity) => activity,
			Err(MutateStorageError::ValueFunctionFailed(())) => OcwActivity::FetchStatements,
			Err(MutateStorageError::ConcurrentModification(_)) => OcwActivity::None,
		}
	}

	/// checks whether we should sync in the current timestamp
	pub(crate) fn should_sync() -> bool {
		/// A friendlier name for the error that is going to be returned in case we are in the grace
		/// period.
		const RECENTLY_SENT: () = ();

		let now = T::TimeProvider::now();
		let minimum_interval = T::MinimumInterval::get();

		// Start off by creating a reference to Local Storage value.
		let val = StorageValueRef::persistent(b"fiat_ramps::last_sync");

		// Retrieve the value from the storage.
		let res = val.mutate(|last_sync: Result<Option<u128>, StorageRetrievalError>| {
			match last_sync {
				// If we already have a value in storage and the block number is recent enough
				// we avoid sending another transaction at this time.
				Ok(Some(last_sync_at))
					if now.as_millis() < last_sync_at + minimum_interval as u128 =>
					Err(RECENTLY_SENT),
				// In every other case we attempt to acquire the lock and send a transaction.
				_ => Ok(now.as_millis()),
			}
		});

		match res {
			Ok(_now) => true,
			Err(MutateStorageError::ValueFunctionFailed(RECENTLY_SENT)) => false,
			Err(MutateStorageError::ConcurrentModification(_)) => false,
		}
	}

	/// Determines if we should process transactions for the statement
	///
	/// We should always sync statements if:
	/// - the balances on chain and on the bank do not match
	/// - iban account is not mapped to any account, we should create new account and sync transactions
	///
	/// # Arguments
	///
	/// `iban_account`: IbanAccount to check
	pub(crate) fn should_process_transactions(_iban_account: &BankAccountOf<T>) -> bool {
		true
	}

	/// Checks if iban is mapped to an account in the storage
	pub(crate) fn iban_exists(iban: &IbanOf<T>) -> bool {
		Accounts::<T>::iter().any(|(_, v)| &v.iban == iban)
	}

	/// Extract AccountId mapped to IbanOf<T>
	pub(crate) fn get_account_id(iban: &IbanOf<T>) -> Option<T::AccountId> {
		Accounts::<T>::iter().find(|(_, v)| &v.iban == iban).map(|(k, _)| k)
	}

	/// Ensures that an IBAN  is mapped to an account in the storage
	///
	/// If necessary, creates new account
	pub(crate) fn ensure_iban_is_mapped(iban: &IbanOf<T>) -> AccountIdOf<T> {
		// If iban is already mapped to account, return it
		if Self::iban_exists(iban) {
			log::debug!("IBAN already exists in the storage {:?}", &iban);
			Accounts::<T>::iter().find(|(_, v)| &v.iban == iban).unwrap().0
		} else {
			let bank_account: BankAccountOf<T> = iban.into();
			let hashed_iban = sp_io::hashing::blake2_256(iban.clone().into_inner().as_ref());

			// hash the iban to create new account id
			let new_account_id =
				T::AccountId::decode(&mut &hashed_iban[..]).expect("Decoding should not fail");

			// Map new account id to IBAN
			Accounts::<T>::insert(&new_account_id, bank_account);

			new_account_id
		}
	}

	// /// Send an XCM message as a sovereign account. Currently only supports transfer of
	// /// own assets on Asset Hub.
	// ///
	// /// # Arguments
	// ///
	// /// `who`: AccountId of the sender
	// /// `amount`: Amount to send
	// /// `dest`: Destination account
	// pub(crate) fn send_as_sovereign(
	// 	_who: &T::AccountId,
	// 	amount: BalanceOf<T>,
	// 	dest: T::AccountId,
	// ) -> DispatchResult {
	// 	let dest = T::AccountIdToMultiLocation::convert(dest);

	// 	// Absolute path to the asset on the current chain
	// 	// TODO: use `XCM` when transacting create asset
	// 	let this_chain_asset = MultiAsset {
	// 		id: (Parent, Parachain(2000)).into(),
	// 		fun: Fungible(amount.saturated_into::<u128>()),
	// 	};

	// 	// Absolute path to the asset for the fee in Asset Hub, i.e relay chain token
	// 	let weight_fee_asset =
	// 		MultiAsset { id: (Parent, Here).into(), fun: Fungible(T::UNIT.saturating_div(10)) };

	// 	let message = Xcm(vec![
	// 		WithdrawAsset(vec![weight_fee_asset.clone(), this_chain_asset.clone()].into()),
	// 		BuyExecution { fees: weight_fee_asset, weight_limit: Unlimited },
	// 		DepositAsset {
	// 			assets: Definite(vec![this_chain_asset.clone()].into()),
	// 			beneficiary: dest,
	// 		},
	// 		RefundSurplus,
	// 		DepositAsset { assets: Wild(All), beneficiary: (Parent, Parachain(2000)).into() },
	// 	]);

	// 	// Send the message
	// 	pallet_xcm::Pallet::<T>::send_xcm(Here, dest, message).map_err(|e| match e {
	// 		SendError::Unroutable => Error::<T>::UnroutableXcm,
	// 		_ => Error::<T>::XcmError,
	// 	})?;

	// 	Ok(())
	// }
}
