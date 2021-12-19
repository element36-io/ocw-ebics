//! A simple Treasury pallet that holds transfers that are yet to be confirmed by bank
//! 
//! Treasury has a pot of funds. Pot is not controlled by a keypair,
//! rather it is controlled by a pallet itself.
//! 
//! # Usage
//! * [`Treasury::deposit`](./struct.Treasury.html#method.deposit) - Deposit burn funds to the treasury.
//! * [`Treasury::confirm_burn`](./struct.Treasury.html#method.confirm_burn) - Confirm that an amount has been burned.
//! 
//! Funds can be confirmed as burned only by the `Root` account.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

use sp_runtime::traits::AccountIdConversion;

use frame_support::{ traits::{Currency}, PalletId };

/// Hardcoded pallet ID; used to create Treasury account.
/// Must be exactly 8 characters long
const PALLET_ID: PalletId = PalletId(*b"Treasury");

type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

// #[cfg(test)]
// mod tests;

#[frame_support::pallet]
pub mod pallet {
	use crate::BalanceOf;
	use frame_support::{
		dispatch::{DispatchResultWithPostInfo},
		pallet_prelude::*,
		traits::{Currency, ExistenceRequirement::{AllowDeath, self}, WithdrawReasons},
	};
	use frame_system::pallet_prelude::*;
	use sp_runtime::DispatchError;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// The currency type that the treasury
		type Currency: Currency<Self::AccountId>;
	}

	#[pallet::genesis_config]
	#[derive(Default)]
	pub struct GenesisConfig {}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			let _ = T::Currency::make_free_balance_be(
				&<Pallet<T>>::account_id(),
				T::Currency::minimum_balance(),
			);
		}
	}

	#[pallet::event]
	#[pallet::metadata(T::Balance = "Balance")]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// We have received a new Burn tx from user, we are now waiting for confirmation.
		/// `burner`, 'burn_amount', 'transaction_id'
		BurnRequest(T::AccountId, BalanceOf<T>, u128),
		/// We have detected a previous burn request has been confirmed.
		/// `burner`, 'burn_amount', 'transaction_id'
		BurnConfirmation(T::AccountId, BalanceOf<T>, u128),
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// User requests to burn some funds.
		/// 
		/// Treasury will hold it until bank confirms.
		#[pallet::weight(10_000)]
		pub fn request_burn(
			origin: OriginFor<T>, 
			amount: BalanceOf<T>, 
			transaction_id: u128
		) -> DispatchResultWithPostInfo {
			let burner = ensure_signed(origin)?;

			T::Currency::transfer(&burner, &Self::account_id(), amount, AllowDeath)
				.map_err(|_| DispatchError::Other("Can't burn funds"))?;
			
			// Record burn request
			<BurnRequests<T>>::insert(transaction_id, (burner.clone(), amount));

			Self::deposit_event(Event::BurnRequest(burner, amount, transaction_id));

			Ok(().into())
		}

		/// Confirm burn request from user.
		///
		/// This will be called by offchain worker when it detects an outgoing transaction
		/// in the bank statement of the user with the same transaction id.
		#[pallet::weight(10_000)]
		pub fn confirm_burn(
			origin: OriginFor<T>,
			burner: T::AccountId,
			amount: BalanceOf<T>,
			tx_id: u128
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			// Burn attempt
			let burn = T::Currency::burn(amount);

			match T::Currency::settle(
				&Self::account_id(),
				burn,
				WithdrawReasons::TRANSFER,
				ExistenceRequirement::AllowDeath,
			) {
				Ok(()) => {
					// Remove record of burn request
					<BurnRequests<T>>::remove(tx_id);
					Self::deposit_event(Event::BurnConfirmation(burner, amount, tx_id));
					Ok(().into())
				},
				Err(_e) => {
					log::error!("[Treasury] Burn failed");
					Err(DispatchError::Other("Burn failed").into())
				},
			}
		}
	}

	/// Mapping between tx_id and burner, amount
	#[pallet::storage]
	#[pallet::getter(fn burn_request)]
	pub (super) type BurnRequests<T: Config> = StorageMap<_, Blake2_128Concat, u128, (T::AccountId, BalanceOf<T>), ValueQuery>;
}

impl<T: Config> Pallet<T> {
	/// The account ID that holds the Charity's funds
	pub fn account_id() -> T::AccountId {
		PALLET_ID.into_account()
	}
}
